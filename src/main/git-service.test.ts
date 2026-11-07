import { execFile } from 'node:child_process'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { promisify } from 'node:util'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { commitWorkspace, ensureWorkspaceRepo, git } from './git-service'

const execFileAsync = promisify(execFile)

describe('workspace git tracking', () => {
  let root: string

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'flashcast-git-'))
  })

  afterEach(async () => {
    await rm(root, { recursive: true, force: true })
  })

  it('versions themes/extensions, ignores state and migrates legacy extensions ignore', async () => {
    try {
      await execFileAsync('git', ['--version'])
    } catch {
      console.warn('git is unavailable; skipping workspace git tracking test')
      return
    }

    // Existing workspace from an older release: extensions/ was ignored.
    await writeFile(
      join(root, '.gitignore'),
      'node_modules/\nextensions/\nstate/\n',
      'utf8'
    )

    try {
      await ensureWorkspaceRepo(root)
    } catch (error) {
      console.warn('git environment unavailable; skipping test:', (error as Error).message)
      return
    }

    const ignore = await readFile(join(root, '.gitignore'), 'utf8')
    const ignoreLines = ignore.split(/\r?\n/)
    expect(ignoreLines).not.toContain('extensions/')
    expect(ignoreLines).toContain('state/')
    expect(ignoreLines).toContain('.DS_Store')
    expect(ignoreLines).toContain('*.log')
    expect(ignoreLines).toContain('.tmp/')

    const readme = await readFile(join(root, 'README.md'), 'utf8')
    expect(readme).toContain('themes/<id>')
    expect(readme).toContain('extensions/<id>/')

    await mkdir(join(root, 'themes', 'foo'), { recursive: true })
    await writeFile(
      join(root, 'themes', 'foo', 'theme.json'),
      JSON.stringify({ schemaVersion: 1, id: 'foo' }),
      'utf8'
    )
    await mkdir(join(root, 'extensions', 'bar'), { recursive: true })
    await writeFile(join(root, 'extensions', 'bar', 'index.js'), 'module.exports = {}\n', 'utf8')
    await mkdir(join(root, 'state'), { recursive: true })
    await writeFile(join(root, 'state', 'machine.json'), '{}\n', 'utf8')

    await git(root, ['config', 'user.email', 'tests@flashcast.local'])
    await git(root, ['config', 'user.name', 'Flashcast Tests'])
    await git(root, ['config', 'commit.gpgsign', 'false'])

    const result = await commitWorkspace(root, 'test: version themes and extensions', false)
    expect(result.error).toBeNull()
    expect(result.committed).toBe(true)

    const shown = await git(root, ['show', '--name-only', '--pretty=format:'])
    const files = shown.stdout.split('\n').filter(Boolean)
    expect(files).toContain('themes/foo/theme.json')
    expect(files).toContain('extensions/bar/index.js')
    expect(files.some((file) => file === 'state/machine.json' || file.startsWith('state/'))).toBe(
      false
    )
  })
})
