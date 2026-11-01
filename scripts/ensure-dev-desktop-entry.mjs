#!/usr/bin/env node
/**
 * 为 `pnpm dev` 安装开发版自己的用户级 .desktop 文件（package.json#devDesktopName）。
 *
 * 为什么需要它：Wayland 会话下 Electron 通过 xdg-desktop-portal 的 GlobalShortcuts
 * 接口注册全局快捷键，portal 用应用身份（CHROME_DESKTOP，由主进程的
 * app.setDesktopName 决定）向 GNOME 申请绑定。xdg-desktop-portal 1.21+ 只接受能
 * 解析到已安装 .desktop 文件的身份，否则 GNOME 50 会静默拒绝所有绑定 ——
 * 而 globalShortcut.register() 仍返回 true。
 *
 * 为什么不再沿用打包版的文件名：~/.local/share/applications 在 XDG 查找顺序里排在
 * /usr/share/applications 之前，同名条目会遮住 deb 安装的生产版，带 NoDisplay=true
 * 时更会让它在应用网格与搜索里彻底消失。开发版因此使用独立身份，两个条目并存。
 */
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { homedir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')

if (process.platform !== 'linux') {
  process.exit(0)
}

const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))

const toFileName = (name, fallback) => {
  const trimmed = typeof name === 'string' ? name.trim() : ''
  const value = trimmed || fallback
  return value.endsWith('.desktop') ? value : `${value}.desktop`
}

const packagedName = toFileName(pkg.desktopName, 'flashcast.desktop')
const devName = toFileName(pkg.devDesktopName, 'com.flashcast.app.dev.desktop')

if (devName === packagedName) {
  console.warn(
    `[flashcast] package.json 的 devDesktopName 与 desktopName 相同（${devName}），` +
      '开发版会遮住生产版，已跳过写入'
  )
  process.exit(0)
}

const applicationsDir = join(
  process.env.XDG_DATA_HOME?.trim() || join(homedir(), '.local', 'share'),
  'applications'
)
const target = join(applicationsDir, devName)

let electron = ''
try {
  electron = createRequire(import.meta.url)('electron')
} catch {
  electron = join(root, 'node_modules', 'electron', 'dist', 'electron')
}

const quote = (value) => `"${value.replace(/(["\\$`])/g, '\\$1')}"`
const icon = join(root, 'build', 'icon.png')
const content = [
  '[Desktop Entry]',
  'Type=Application',
  'Name=Flashcast (Dev)',
  'GenericName=Launcher',
  'Comment=Flashcast 开发构建；启动最近一次构建的产物，数据与快捷键均与生产版分开',
  `TryExec=${electron}`,
  // 与 `pnpm dev` 一致地关掉 SUID sandbox：开发机上的 chrome-sandbox 没有 4755 权限。
  `Exec=${quote(electron)} --no-sandbox ${quote(root)} %U`,
  `Path=${root}`,
  `Icon=${existsSync(icon) ? icon : 'flashcast'}`,
  'Terminal=false',
  'NoDisplay=false',
  `StartupWMClass=${devName.replace(/\.desktop$/, '')}`,
  'Categories=Utility;',
  'Keywords=flashcast;launcher;dev;',
  'X-Flashcast-Dev=true',
  ''
].join('\n')

// 旧版本把开发版条目写成打包版的同名文件，它会遮住 deb 安装的生产版。
const legacy = join(applicationsDir, packagedName)
let removedLegacy = false
if (existsSync(legacy)) {
  let owned = false
  try {
    owned = readFileSync(legacy, 'utf8').includes('X-Flashcast-Dev=true')
  } catch {
    // 读不到就不动它：只清理确认由本脚本写入的条目。
  }
  if (owned) {
    try {
      unlinkSync(legacy)
      removedLegacy = true
    } catch (error) {
      console.warn(`[flashcast] 无法移除旧开发条目 ${legacy}：${error.message}`)
    }
  }
}

const current = existsSync(target) ? readFileSync(target, 'utf8') : null
const changed = current !== content

if (changed) {
  mkdirSync(applicationsDir, { recursive: true })
  writeFileSync(target, content, 'utf8')
}

if (changed || removedLegacy) {
  try {
    execFileSync('update-desktop-database', [applicationsDir], { stdio: 'ignore' })
  } catch {
    // 有缓存时图标/搜索更快，没有也不影响 portal 解析。
  }
}

if (changed) {
  console.log(`[flashcast] 已写入开发版桌面项 ${target}`)
}

if (removedLegacy) {
  console.log(`[flashcast] 已移除会遮住生产版的旧开发条目 ${legacy}`)
}
