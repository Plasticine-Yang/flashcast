import { app, dialog, protocol, session } from 'electron'
import { mkdirSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { CLIPBOARD_MEDIA_SCHEME, type PlatformId } from '@shared/types'
import { FlashcastApplication } from './application'
import { log } from './logger'

app.setName('Flashcast')

if (process.env.FLASHCAST_USER_DATA) {
  const userData = resolve(process.env.FLASHCAST_USER_DATA)
  mkdirSync(userData, { recursive: true })
  app.setPath('userData', userData)
}

try {
  const pkg = JSON.parse(readFileSync(resolve(__dirname, '../../package.json'), 'utf8')) as {
    version?: string
  }
  // Available at runtime but absent from Electron 45's public typings.
  ;(
    app as unknown as { setVersion?: (version: string) => void }
  ).setVersion?.(pkg.version ?? '0.1.0')
} catch {
  // Packaged builds read the version from their own manifest.
}

// Transparent frameless windows need an ARGB visual on X11. On Wayland the
// compositor handles alpha natively.
if (process.platform === 'linux') {
  app.commandLine.appendSwitch('enable-transparent-visuals')
  app.commandLine.appendSwitch('enable-features', 'GlobalShortcutsPortal')
}

protocol.registerSchemesAsPrivileged([
  {
    scheme: CLIPBOARD_MEDIA_SCHEME,
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      stream: true,
      corsEnabled: true
    }
  }
])

if (process.platform === 'win32') {
  app.setAppUserModelId('com.flashcast.app')
}

const gotLock = app.requestSingleInstanceLock()

let application: FlashcastApplication | null = null

if (gotLock) {
  app.on('second-instance', () => {
    application?.showLauncher()
  })

  app.whenReady().then(async () => {
    session.defaultSession.setPermissionRequestHandler((_webContents, _permission, callback) => {
      callback(false)
    })
    session.defaultSession.setPermissionCheckHandler(() => false)

    if (process.platform === 'darwin') {
      app.dock?.hide()
    }

    try {
      const platform = (['darwin', 'win32', 'linux'].includes(process.platform)
        ? process.platform
        : 'linux') as PlatformId
      application = new FlashcastApplication(platform)
      await application.init()
      log.info('Flashcast ready')
    } catch (error) {
      log.error('startup failed:', error)
      dialog.showErrorBox(
        'Flashcast 启动失败',
        `${(error as Error).message}\n\n请检查工作区目录权限，或设置 FLASHCAST_WORKSPACE 指向可写目录。`
      )
      app.quit()
    }
  })
} else {
  app.quit()
}

app.on('activate', () => {
  application?.showLauncher()
})

app.on('window-all-closed', () => {
  // Flashcast lives in the tray/menu bar, like Raycast.
})

app.on('before-quit', () => {
  application?.dispose()
})

process.on('uncaughtException', (error) => {
  log.error('uncaught exception:', error)
})

process.on('unhandledRejection', (reason) => {
  log.error('unhandled rejection:', reason)
})
