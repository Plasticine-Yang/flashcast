import { dialog, shell } from 'electron'
import { EventEmitter } from 'node:events'
import { existsSync, writeFileSync } from 'node:fs'
import type { WorkspaceSummary } from '@shared/types'
import { launcherStatePath, readLauncherState, resolveInitialWorkspace, workspaceLayout, type WorkspaceLayout } from './paths'
import {
  commitWorkspace,
  ensureWorkspaceRepo,
  initializeFirstCommit,
  readDiff,
  readGitStatus
} from './git-service'
import type { ConfigStore } from './config-store'
import { log } from './logger'

export class WorkspaceService extends EventEmitter {
  private layout: WorkspaceLayout
  private summary: WorkspaceSummary
  private initializedRepo = false
  private commitTimer: NodeJS.Timeout | null = null
  private statusTimer: NodeJS.Timeout | null = null

  constructor(
    private readonly config: ConfigStore,
    initialPath = resolveInitialWorkspace()
  ) {
    super()
    this.layout = workspaceLayout(initialPath)
    this.summary = this.emptySummary(initialPath)
  }

  async init(): Promise<void> {
    await this.open(this.layout.root, { persist: false, initializing: true })
    this.config.on('persist', () => this.scheduleCommit())
  }

  get root(): string {
    return this.layout.root
  }

  get workspaceLayout(): WorkspaceLayout {
    return this.layout
  }

  getSummary(): WorkspaceSummary {
    return { ...this.summary }
  }

  private emptySummary(path: string): WorkspaceSummary {
    return {
      path,
      exists: false,
      isRepo: false,
      branch: null,
      lastCommit: null,
      dirtyCount: 0,
      conflict: false,
      ahead: 0,
      behind: 0,
      hasRemote: false,
      autoCommit: this.config.getPrefs().autoCommit,
      syncState: 'not-a-repo',
      lastError: null
    }
  }

  async open(
    path: string,
    options: { persist: boolean; initializing?: boolean } = { persist: true }
  ): Promise<WorkspaceSummary> {
    try {
      const info = await ensureWorkspaceRepo(path)
      this.layout = workspaceLayout(info.root)
      this.initializedRepo = info.initialized
      if (options.persist) {
        const state = readLauncherState()
        writeFileSync(
          launcherStatePath(),
          `${JSON.stringify({ ...state, activeWorkspace: info.root, hasLaunched: true }, null, 2)}\n`,
          'utf8'
        )
      }

      await this.config.setWorkspace(this.layout)

      await this.refreshStatus()
      return this.getSummary()
    } catch (error) {
      const message = (error as Error).message
      log.error('workspace open failed:', message)
      this.summary = {
        ...this.summary,
        exists: existsSync(path),
        isRepo: false,
        syncState: 'error',
        lastError: message
      }
      this.emit('changed', this.getSummary())
      return this.getSummary()
    }
  }

  async finalizeInitialCommit(): Promise<void> {
    if (!this.initializedRepo) return
    const committed = await initializeFirstCommit(this.layout.root)
    if (committed) await this.refreshStatus()
  }

  async refreshStatus(emit = true): Promise<WorkspaceSummary> {
    const status = await readGitStatus(this.layout.root)
    const autoCommit = this.config.getPrefs().autoCommit
    let syncState: WorkspaceSummary['syncState'] = 'synced'
    if (!status.isRepo) syncState = 'not-a-repo'
    else if (status.lastError) syncState = 'error'
    else if (status.conflict) syncState = 'conflict'
    else if (status.dirtyCount > 0) syncState = 'dirty'
    else if (!status.hasRemote) syncState = 'no-remote'

    this.summary = {
      path: this.layout.root,
      exists: existsSync(this.layout.root),
      isRepo: status.isRepo,
      branch: status.branch,
      lastCommit: status.lastCommit,
      dirtyCount: status.dirtyCount,
      conflict: status.conflict,
      ahead: status.ahead,
      behind: status.behind,
      hasRemote: status.hasRemote,
      autoCommit,
      syncState,
      lastError: status.lastError
    }
    if (emit) this.emit('changed', this.getSummary())
    return this.getSummary()
  }

  async chooseWorkspace(parentWindow: Electron.BrowserWindow | null): Promise<{ canceled: boolean; workspace: WorkspaceSummary }> {
    const options = {
      title: '选择 Flashcast 工作区',
      message: '选择一个文件夹作为配置工作区。如果不是 Git 仓库，Flashcast 会自动初始化。',
      properties: ['openDirectory', 'createDirectory'] as Array<'openDirectory' | 'createDirectory'>,
      buttonLabel: '关联此文件夹'
    }
    const result = parentWindow
      ? await dialog.showOpenDialog(parentWindow, options)
      : await dialog.showOpenDialog(options)
    if (result.canceled || result.filePaths.length === 0) {
      return { canceled: true, workspace: this.getSummary() }
    }
    const summary = await this.open(result.filePaths[0], { persist: true })
    return { canceled: false, workspace: summary }
  }

  async openFolder(): Promise<void> {
    const error = await shell.openPath(this.layout.root)
    if (error) throw new Error(error)
  }

  async diff(): Promise<{ status: string; diff: string; error: string | null }> {
    return readDiff(this.layout.root)
  }

  private scheduleCommit(): void {
    if (!this.config.getPrefs().autoCommit) return
    if (this.commitTimer) clearTimeout(this.commitTimer)
    this.commitTimer = setTimeout(() => {
      this.commitTimer = null
      void this.commitNow()
    }, 1200)
  }

  private async commitNow(): Promise<void> {
    if (!this.config.getPrefs().autoCommit) return
    const result = await commitWorkspace(
      this.layout.root,
      'config: update Flashcast workspace',
      true
    )
    if (result.error) {
      this.summary = { ...this.summary, lastError: result.error, syncState: 'error' }
      this.emit('changed', this.getSummary())
    } else if (result.committed) {
      await this.refreshStatus()
    }
  }

  async setAutoCommit(enabled: boolean): Promise<WorkspaceSummary> {
    await this.config.updatePrefs({ autoCommit: enabled })
    if (enabled) this.scheduleCommit()
    return this.refreshStatus()
  }

  async setWorkspace(path: string): Promise<WorkspaceSummary> {
    return this.open(path, { persist: true })
  }

  startStatusPolling(): void {
    if (this.statusTimer) return
    this.statusTimer = setInterval(() => {
      void this.refreshStatus()
    }, 20_000)
    this.statusTimer.unref?.()
  }

  stop(): void {
    if (this.commitTimer) clearTimeout(this.commitTimer)
    if (this.statusTimer) clearInterval(this.statusTimer)
    this.commitTimer = null
    this.statusTimer = null
  }

}
