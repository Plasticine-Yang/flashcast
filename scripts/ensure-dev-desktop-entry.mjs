#!/usr/bin/env node
/**
 * 为 `pnpm dev` 安装一个与 package.json#desktopName 同名的用户级 .desktop 文件。
 *
 * 为什么需要它：Wayland 会话下 Electron 通过 xdg-desktop-portal 的 GlobalShortcuts
 * 接口注册全局快捷键，portal 用应用身份（CHROME_DESKTOP，来自 desktopName）向 GNOME
 * 申请绑定。xdg-desktop-portal 1.21+ 只接受能解析到已安装 .desktop 文件的身份，
 * 否则 GNOME 50 会静默拒绝所有绑定 —— globalShortcut.register() 却仍返回 true。
 * 开发时没有 electron-builder 生成桌面项，所以这里补一个。
 */
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { homedir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')

if (process.platform !== 'linux') {
  process.exit(0)
}

const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
const desktopName = (pkg.desktopName ?? '').trim()
if (!desktopName) {
  console.warn('[flashcast] package.json 缺少 desktopName，跳过开发用桌面项')
  process.exit(0)
}

const fileName = desktopName.endsWith('.desktop') ? desktopName : `${desktopName}.desktop`
const applicationsDir = join(
  process.env.XDG_DATA_HOME?.trim() || join(homedir(), '.local', 'share'),
  'applications'
)
const target = join(applicationsDir, fileName)

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
  'Comment=Flashcast 开发构建，同时作为 Wayland 全局快捷键的应用身份',
  `TryExec=${electron}`,
  `Exec=${quote(electron)} ${quote(root)} %U`,
  `Path=${root}`,
  `Icon=${existsSync(icon) ? icon : 'flashcast'}`,
  'Terminal=false',
  'NoDisplay=true',
  `StartupWMClass=${fileName.replace(/\.desktop$/, '')}`,
  'X-Flashcast-Dev=true',
  ''
].join('\n')

const current = existsSync(target) ? readFileSync(target, 'utf8') : null
if (current === content) {
  process.exit(0)
}

mkdirSync(applicationsDir, { recursive: true })
writeFileSync(target, content, 'utf8')
try {
  execFileSync('update-desktop-database', [applicationsDir], { stdio: 'ignore' })
} catch {
  // 有缓存时图标/搜索更快，没有也不影响 portal 解析。
}
console.log(`[flashcast] 已写入开发用桌面项 ${target}`)
