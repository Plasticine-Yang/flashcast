import { existsSync, readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { isAbsolute, join, resolve } from 'node:path'

/**
 * Wayland 会话下 Electron 不再直接抓键，而是通过 xdg-desktop-portal 的
 * GlobalShortcuts 接口注册全局快捷键。portal 用应用身份（Electron 从
 * package.json#desktopName 推导出的 CHROME_DESKTOP）申请绑定，并要求该身份能
 * 解析到已安装的 .desktop 文件；否则 xdg-desktop-portal 1.21+ 会拒绝会话
 * （GNOME 50 报 NotAllowed），而 globalShortcut.register() 仍返回 true。
 *
 * 这一组 helper 让主进程能在启动时发现这种“静默失败”，而不是让用户面对
 * 一个按了没反应的快捷键。
 */

/** 与 Chromium 的 SetOzonePlatformForLinuxIfNeeded 保持同样的判断顺序。 */
export function usesWayland(env: NodeJS.ProcessEnv = process.env): boolean {
  const hasWayland = Boolean(env.WAYLAND_DISPLAY || env.WAYLAND_SOCKET)
  const hasX11 = Boolean(env.DISPLAY)
  // 会话类型只在两者都可能可用时用于打破平局（例如 ssh -X、容器）。
  let wayland = env.XDG_SESSION_TYPE === 'wayland'
  if (hasWayland !== hasX11) wayland = hasWayland
  return wayland
}

/** 读取 Electron 实际会报告给桌面的身份（默认回退到 Flashcast 的 slug）。 */
export function packageDesktopName(baseDir: string = __dirname): string {
  try {
    const pkg = JSON.parse(readFileSync(join(baseDir, '../../package.json'), 'utf8')) as {
      desktopName?: string
    }
    const name = pkg.desktopName?.trim()
    if (name) return name.endsWith('.desktop') ? name : `${name}.desktop`
  } catch {
    // 打包后可读路径不同，忽略并回退。
  }
  return 'flashcast.desktop'
}

export function desktopEntryDirs(
  env: NodeJS.ProcessEnv = process.env,
  home: string = homedir()
): string[] {
  const dataHome = env.XDG_DATA_HOME?.trim() || join(home, '.local', 'share')
  const dataDirs = (env.XDG_DATA_DIRS?.trim() || '/usr/local/share:/usr/share')
    .split(':')
    .filter(Boolean)
  return [...new Set([dataHome, ...dataDirs].map((dir) => resolve(dir)))]
}

export function findDesktopEntry(
  fileName: string,
  dirs: string[],
  exists: (path: string) => boolean = existsSync
): string | null {
  if (!fileName || isAbsolute(fileName)) {
    // 绝对路径不属于 desktop id 语义，portal 也不会这样解析。
    return null
  }
  for (const dir of dirs) {
    const candidate = join(dir, 'applications', fileName)
    if (exists(candidate)) return candidate
  }
  return null
}
