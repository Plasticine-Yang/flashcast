import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { appendFile, mkdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { promisify } from 'node:util'
import { log } from './logger'

const execFileAsync = promisify(execFile)

export interface GitResult {
  stdout: string
  stderr: string
}

interface GitOptions {
  timeout?: number
  allowFailure?: boolean
}

export async function git(root: string, args: string[], options: GitOptions = {}): Promise<GitResult> {
  try {
    const { stdout, stderr } = await execFileAsync('git', ['-C', root, ...args], {
      timeout: options.timeout ?? 15_000,
      maxBuffer: 8 * 1024 * 1024,
      env: {
        ...process.env,
        GIT_TERMINAL_PROMPT: '0',
        GIT_OPTIONAL_LOCKS: '0'
      }
    })
    return { stdout: String(stdout), stderr: String(stderr) }
  } catch (error) {
    if (options.allowFailure) return { stdout: '', stderr: (error as Error).message }
    throw error
  }
}

export interface RepoInfo {
  isRepo: boolean
  root: string
  initialized: boolean
}

export async function resolveRepoRoot(path: string): Promise<string | null> {
  try {
    const { stdout } = await git(path, ['rev-parse', '--show-toplevel'])
    return stdout.trim() || null
  } catch {
    return null
  }
}

const README = `# Flashcast workspace

This git repository keeps your Flashcast configuration:

- \`config.json\` — app preferences and hotkeys
- \`plugins/<id>/config.json\` — per-plugin settings
- \`README.md\` — this file

Machine-local runtime data lives in \`state/\` and is intentionally ignored by git.
Installed external plugin code lives in \`extensions/\` and is also ignored; the
per-plugin settings are still versioned under \`plugins/\`.
`

const GITIGNORE_ENTRIES = ['.DS_Store', 'state/', 'extensions/', '*.log', '.tmp/']

async function ensureGitignore(root: string): Promise<void> {
  const path = join(root, '.gitignore')
  let content = ''
  try {
    content = await readFile(path, 'utf8')
  } catch {
    content = ''
  }
  const missing = GITIGNORE_ENTRIES.filter((entry) => !content.split(/\r?\n/).includes(entry))
  if (missing.length > 0) {
    const prefix = content && !content.endsWith('\n') ? '\n' : ''
    await appendFile(path, `${prefix}${missing.join('\n')}\n`, 'utf8')
  }
}

export async function ensureWorkspaceRepo(path: string): Promise<RepoInfo> {
  await mkdir(path, { recursive: true })

  const existingRoot = await resolveRepoRoot(path)
  if (existingRoot) {
    await ensureWorkspaceSupportFiles(existingRoot)
    return { isRepo: true, root: existingRoot, initialized: false }
  }

  await git(path, ['init', '-b', 'main'], { allowFailure: true })
  await ensureWorkspaceSupportFiles(path)
  return { isRepo: true, root: path, initialized: true }
}

async function ensureWorkspaceSupportFiles(root: string): Promise<void> {
  const readmePath = join(root, 'README.md')
  if (!existsSync(readmePath)) await writeFile(readmePath, README, 'utf8')
  await ensureGitignore(root)
}

export interface GitStatus {
  isRepo: boolean
  branch: string | null
  lastCommit: { hash: string; message: string; date: number } | null
  dirtyCount: number
  conflict: boolean
  ahead: number
  behind: number
  hasRemote: boolean
  lastError: string | null
}

export async function readGitStatus(root: string): Promise<GitStatus> {
  const empty: GitStatus = {
    isRepo: false,
    branch: null,
    lastCommit: null,
    dirtyCount: 0,
    conflict: false,
    ahead: 0,
    behind: 0,
    hasRemote: false,
    lastError: null
  }

  if (!existsSync(root)) return { ...empty, lastError: '工作区目录不存在' }

  const inside = await git(root, ['rev-parse', '--is-inside-work-tree'], { allowFailure: true })
  if (inside.stdout.trim() !== 'true') {
    return { ...empty, lastError: '当前目录不是 Git 仓库' }
  }

  let branch: string | null = null
  let lastCommit: GitStatus['lastCommit'] = null
  let dirtyCount = 0
  let conflict = false
  let ahead = 0
  let behind = 0
  let hasRemote = false
  let lastError: string | null = null

  try {
    const branchResult = await git(root, ['rev-parse', '--abbrev-ref', 'HEAD'], {
      allowFailure: true
    })
    branch = branchResult.stdout.trim() || null

    const logResult = await git(root, ['log', '-1', '--format=%h%x1f%s%x1f%cI'], {
      allowFailure: true
    })
    const raw = logResult.stdout.trim()
    if (raw) {
      const [hash, message, date] = raw.split('\x1f')
      const parsed = Date.parse(date)
      lastCommit = {
        hash,
        message,
        date: Number.isFinite(parsed) ? parsed : Date.now()
      }
    }

    const statusResult = await git(root, ['status', '--porcelain=v1'], { allowFailure: true })
    const lines = statusResult.stdout.split('\n').filter(Boolean)
    dirtyCount = lines.length
    conflict = lines.some((line) => /^(UU|AA|DD|AU|UA|DU|UD)/.test(line))

    const remoteResult = await git(root, ['remote'], { allowFailure: true })
    hasRemote = remoteResult.stdout.trim().length > 0

    if (branch && branch !== 'HEAD') {
      const upstream = await git(root, ['rev-parse', '--abbrev-ref', '@{u}'], {
        allowFailure: true
      })
      if (upstream.stdout.trim()) {
        const counts = await git(
          root,
          ['rev-list', '--left-right', '--count', '@{u}...HEAD'],
          { allowFailure: true }
        )
        const [behindRaw, aheadRaw] = counts.stdout.trim().split(/\s+/)
        behind = Number.parseInt(behindRaw ?? '0', 10) || 0
        ahead = Number.parseInt(aheadRaw ?? '0', 10) || 0
      }
    }
  } catch (error) {
    lastError = (error as Error).message
  }

  return {
    isRepo: true,
    branch,
    lastCommit,
    dirtyCount,
    conflict,
    ahead,
    behind,
    hasRemote,
    lastError
  }
}

/** Commits only Flashcast-owned paths so unrelated files in the repo are never staged. */
export async function commitWorkspace(
  root: string,
  message: string,
  push: boolean
): Promise<{ committed: boolean; pushed: boolean; error: string | null }> {
  try {
    await git(root, ['add', '--all', '--', 'config.json', 'plugins', 'README.md', '.gitignore'], {
      allowFailure: true
    })
    const stagedList = await git(root, ['diff', '--cached', '--name-only'], { allowFailure: true })
    if (!stagedList.stdout.trim()) {
      return { committed: false, pushed: false, error: null }
    }
    await git(root, ['commit', '-m', message], { timeout: 20_000 })
    let pushed = false
    if (push) {
      const upstream = await git(root, ['rev-parse', '--abbrev-ref', '@{u}'], {
        allowFailure: true
      })
      if (upstream.stdout.trim()) {
        const pushResult = await git(root, ['push'], { timeout: 45_000, allowFailure: true })
        pushed = !pushResult.stderr.toLowerCase().includes('error')
      }
    }
    return { committed: true, pushed, error: null }
  } catch (error) {
    log.warn('workspace auto-commit failed:', (error as Error).message)
    return { committed: false, pushed: false, error: (error as Error).message }
  }
}

export async function readDiff(root: string): Promise<{ status: string; diff: string; error: string | null }> {
  try {
    const status = await git(root, ['status', '--short'], { allowFailure: true })
    const unstaged = await git(root, ['diff', '--stat'], { allowFailure: true })
    const staged = await git(root, ['diff', '--cached', '--stat'], { allowFailure: true })
    return {
      status: status.stdout.trim(),
      diff: [staged.stdout.trim(), unstaged.stdout.trim()].filter(Boolean).join('\n\n'),
      error: null
    }
  } catch (error) {
    return { status: '', diff: '', error: (error as Error).message }
  }
}

export async function gitHasCommits(root: string): Promise<boolean> {
  const result = await git(root, ['rev-parse', '--verify', 'HEAD'], { allowFailure: true })
  return result.stdout.trim().length > 0
}

export async function initializeFirstCommit(root: string): Promise<boolean> {
  if (await gitHasCommits(root)) return false
  await git(root, ['add', '--', 'config.json', 'README.md', '.gitignore', 'plugins'], {
    allowFailure: true
  })
  const result = await git(root, ['commit', '-m', 'chore: initialize Flashcast workspace'], {
    allowFailure: true
  })
  return result.stderr.length === 0
}
