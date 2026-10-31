import { app, BrowserWindow, Notification, screen, shell } from 'electron'
import { EventEmitter } from 'node:events'
import { join } from 'node:path'
import { IPC } from '@shared/ipc'
import type { PlatformId, WindowRole } from '@shared/types'
import { assetsDir, isDev } from './paths'
import { log } from './logger'

interface WindowCallbacks {
  onPasteAfterHide: () => Promise<{ ok: boolean; error?: string }>
}

export class WindowManager extends EventEmitter {
  private launcher: BrowserWindow | null = null
  private settings: BrowserWindow | null = null
  private launcherReady = false
  private pendingShow = false
  private pendingPlugin: string | null = null
  private closing = false
  private closeTimer: NodeJS.Timeout | null = null
  private showGuardUntil = 0
  private pasteAfterHide = false

  constructor(
    private readonly platform: PlatformId,
    private readonly callbacks: WindowCallbacks
  ) {
    super()
  }

  get launcherWindow(): BrowserWindow | null {
    return this.launcher
  }

  get settingsWindow(): BrowserWindow | null {
    return this.settings
  }

  createLauncher(): BrowserWindow {
    if (this.launcher && !this.launcher.isDestroyed()) return this.launcher
    const display = screen.getDisplayNearestPoint(screen.getCursorScreenPoint())
    const workArea = display.workArea
    const window = new BrowserWindow({
      x: workArea.x,
      y: workArea.y,
      width: workArea.width,
      height: workArea.height,
      show: false,
      frame: false,
      transparent: true,
      backgroundColor: '#00000000',
      hasShadow: false,
      resizable: false,
      movable: false,
      minimizable: false,
      maximizable: false,
      fullscreenable: false,
      skipTaskbar: true,
      alwaysOnTop: true,
      acceptFirstMouse: true,
      title: 'Flashcast',
      icon: join(assetsDir(), 'icon.png'),
      webPreferences: {
        preload: join(__dirname, '../preload/index.js'),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: false,
        spellcheck: false,
        backgroundThrottling: false,
        webviewTag: false,
        devTools: isDev()
      }
    })

    window.setAlwaysOnTop(true, 'screen-saver')
    if (this.platform === 'darwin') {
      window.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true })
      window.setFullScreenable(false)
    }

    window.webContents.setWindowOpenHandler(({ url }) => {
      void shell.openExternal(url)
      return { action: 'deny' }
    })
    window.webContents.on('will-navigate', (event, url) => {
      const current = window.webContents.getURL()
      if (url !== current) event.preventDefault()
    })

    window.on('blur', () => {
      if (Date.now() < this.showGuardUntil) return
      if (this.closing || !window.isVisible()) return
      setTimeout(() => {
        if (!this.launcher || this.launcher.isDestroyed()) return
        if (this.launcher.isFocused() || this.launcher.webContents.isDevToolsFocused()) return
        if (this.launcher.isVisible()) this.requestClose()
      }, 100)
    })

    window.on('closed', () => {
      this.launcher = null
      this.launcherReady = false
    })

    window.webContents.on('did-finish-load', () => {
      this.launcherReady = true
      if (this.pendingShow) {
        this.pendingShow = false
        this.showLauncherNow(this.pendingPlugin)
        this.pendingPlugin = null
      }
    })

    this.loadRenderer(window, 'launcher')
    this.launcher = window
    return window
  }

  createSettings(): BrowserWindow {
    if (this.settings && !this.settings.isDestroyed()) return this.settings
    const window = new BrowserWindow({
      width: 920,
      height: 640,
      minWidth: 760,
      minHeight: 540,
      show: false,
      title: 'Flashcast 设置',
      backgroundColor: '#18181B',
      autoHideMenuBar: true,
      titleBarStyle: this.platform === 'darwin' ? 'hiddenInset' : 'default',
      icon: join(assetsDir(), 'icon.png'),
      webPreferences: {
        preload: join(__dirname, '../preload/index.js'),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: false,
        spellcheck: false,
        devTools: isDev()
      }
    })
    window.setMenuBarVisibility(false)
    window.webContents.setWindowOpenHandler(({ url }) => {
      void shell.openExternal(url)
      return { action: 'deny' }
    })
    window.once('ready-to-show', () => {
      window.show()
      window.focus()
    })
    window.on('closed', () => {
      this.settings = null
    })
    this.loadRenderer(window, 'settings')
    this.settings = window
    return window
  }

  private loadRenderer(window: BrowserWindow, role: WindowRole): void {
    const devUrl = process.env['ELECTRON_RENDERER_URL']
    if (isDev() && devUrl) {
      const url = new URL(devUrl)
      url.searchParams.set('window', role)
      void window.loadURL(url.toString())
      return
    }
    void window.loadFile(join(__dirname, '../renderer/index.html'), {
      query: { window: role }
    })
  }

  showLauncher(pluginId?: string): void {
    const window = this.launcher && !this.launcher.isDestroyed() ? this.launcher : this.createLauncher()
    if (!this.launcherReady) {
      this.pendingShow = true
      this.pendingPlugin = pluginId ?? this.pendingPlugin
      return
    }
    this.showLauncherNow(pluginId)
  }

  private showLauncherNow(pluginId?: string | null): void {
    const window = this.launcher
    if (!window || window.isDestroyed()) return
    this.closing = false
    if (this.closeTimer) {
      clearTimeout(this.closeTimer)
      this.closeTimer = null
    }
    const display = screen.getDisplayNearestPoint(screen.getCursorScreenPoint())
    window.setBounds(display.workArea)
    this.showGuardUntil = Date.now() + 300
    window.setAlwaysOnTop(true, 'screen-saver')
    window.show()
    window.focus()
    window.webContents.send(IPC.shown)
    if (pluginId) {
      const id = pluginId
      setTimeout(() => {
        if (!this.launcher || this.launcher.isDestroyed()) return
        this.launcher.webContents.send(IPC.openPlugin, { pluginId: id })
      }, 90)
    }
  }

  toggleLauncher(): void {
    if (!this.launcher) {
      this.showLauncher()
      return
    }
    if (this.launcher.isVisible() && !this.closing) {
      this.requestClose()
    } else {
      this.showLauncher()
    }
  }

  openClipboardPlugin(): void {
    if (this.launcher?.isVisible() && !this.closing) {
      this.launcher.webContents.send(IPC.openPlugin, { pluginId: 'clipboard' })
      return
    }
    this.showLauncher('clipboard')
  }

  requestClose(paste = false): void {
    if (!this.launcher || !this.launcher.isVisible() || this.closing) {
      if (paste) this.hideImmediately(true)
      return
    }
    this.pasteAfterHide = this.pasteAfterHide || paste
    this.closing = true
    this.launcher.webContents.send(IPC.closeRequest)
    this.closeTimer = setTimeout(() => {
      this.hideImmediately(this.pasteAfterHide)
    }, 420)
  }

  hideImmediately(paste = false): void {
    if (this.closeTimer) clearTimeout(this.closeTimer)
    this.closeTimer = null
    this.closing = false
    const shouldPaste = paste || this.pasteAfterHide
    this.pasteAfterHide = false
    if (this.launcher && !this.launcher.isDestroyed()) {
      this.launcher.hide()
    }
    if (shouldPaste) {
      setTimeout(async () => {
        try {
          const result = await this.callbacks.onPasteAfterHide()
          if (!result.ok) this.notifyPasteFailure(result.error ?? '无法触发粘贴')
        } catch (error) {
          this.notifyPasteFailure((error as Error).message)
        }
      }, 100)
    }
  }

  openSettings(): void {
    const existing = this.settings && !this.settings.isDestroyed() ? this.settings : null
    if (existing) {
      existing.show()
      existing.focus()
      return
    }
    this.createSettings()
  }

  broadcast(channel: string, payload: unknown): void {
    for (const window of BrowserWindow.getAllWindows()) {
      if (!window.isDestroyed()) window.webContents.send(channel, payload)
    }
  }

  getRole(senderId: number): WindowRole | null {
    if (this.launcher?.webContents.id === senderId) return 'launcher'
    if (this.settings?.webContents.id === senderId) return 'settings'
    return null
  }

  notifyPasteFailure(error: string): void {
    log.warn('paste after hide failed:', error)
    try {
      new Notification({
        title: 'Flashcast 无法自动粘贴',
        body: `${error}。内容已复制到剪贴板，可手动粘贴。`
      }).show()
    } catch {
      // Notifications are best-effort.
    }
  }

  closeSettings(): void {
    this.settings?.close()
  }

  destroy(): void {
    this.launcher?.destroy()
    this.settings?.destroy()
    this.launcher = null
    this.settings = null
  }
}
