import { clipboard, shell } from 'electron'
import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import type { ActionResult, EditorWorkspace } from '@shared/types'
import type { VariantRecord } from './detect'

function run(command: string, args: string[], env?: NodeJS.ProcessEnv): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      detached: true,
      stdio: 'ignore',
      env: env ?? process.env
    })
    child.once('error', reject)
    child.once('spawn', () => {
      child.unref()
      resolve()
    })
  })
}

function workspaceArgs(workspace: EditorWorkspace, newWindow: boolean): string[] {
  const args: string[] = []
  if (newWindow) args.push('--new-window')
  if (workspace.remote || workspace.uri.startsWith('vscode-remote:')) {
    args.push(workspace.kind === 'workspace' ? '--file-uri' : '--folder-uri', workspace.uri)
  } else {
    args.push(workspace.path)
  }
  return args
}

/**
 * Opens a workspace with the variant it belongs to. Never goes through the
 * `vscode://` / `trae://` protocol handler: that would open whichever product
 * registered last, which is exactly what this plugin must avoid.
 */
export async function launchWorkspace(
  variant: VariantRecord,
  workspace: EditorWorkspace,
  options: { newWindow: boolean }
): Promise<ActionResult> {
  if (!workspace.remote && !existsSync(workspace.path)) {
    return { ok: false, error: '工作区路径不存在' }
  }

  const args = workspaceArgs(workspace, options.newWindow)

  try {
    if (variant.launchKind === 'cli' && variant.cliPath) {
      await run(variant.cliPath, args)
    } else if (variant.launchKind === 'exe' && variant.executable) {
      if (variant.cliScript) {
        await run(variant.executable, [variant.cliScript, ...args], {
          ...process.env,
          ELECTRON_RUN_AS_NODE: '1'
        })
      } else {
        await run(variant.executable, args)
      }
    } else if (variant.launchKind === 'open' && variant.appPath) {
      if (process.platform === 'darwin') {
        if (workspace.remote) {
          return { ok: false, error: `${variant.name} 缺少命令行工具，无法打开远程工作区` }
        }
        await run(
          'open',
          options.newWindow
            ? ['-a', variant.appPath, '--args', '--new-window', workspace.path]
            : ['-a', variant.appPath, workspace.path]
        )
      } else if (variant.executable) {
        await run(variant.executable, args)
      } else {
        return { ok: false, error: `无法启动 ${variant.name}` }
      }
    } else {
      return { ok: false, error: `无法启动 ${variant.name}` }
    }
  } catch (error) {
    return { ok: false, error: `启动 ${variant.name} 失败：${(error as Error).message}` }
  }

  return { ok: true, close: true }
}

/** Reveals a local workspace in the file manager, or copies a remote URI. */
export async function revealWorkspace(workspace: EditorWorkspace): Promise<ActionResult> {
  if (workspace.remote) {
    clipboard.writeText(workspace.uri)
    return {
      ok: true,
      toast: { message: '已复制远程工作区 URI', tone: 'success' }
    }
  }
  if (!existsSync(workspace.path)) {
    return { ok: false, error: '工作区路径不存在' }
  }
  shell.showItemInFolder(workspace.path)
  return { ok: true }
}
