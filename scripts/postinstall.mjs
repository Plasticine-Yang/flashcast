#!/usr/bin/env node
/**
 * 本地安装时下载 Electron 运行时（`pnpm dev` / `pnpm start` 依赖 `node_modules/electron/dist`）。
 *
 * 打包阶段用不到这份二进制：electron-builder 会自己下载对应平台的 Electron。
 * CI 里两个都下等于每次都从 GitHub 的 release 资产 CDN 拉两遍（每个 job ~120MB），
 * v0.4.4 的 Linux 打包就是卡在这个多余的下载上（504）。所以 CI 直接跳过。
 *
 * 需要强制本地也跳过时，设置 FLASHCAST_SKIP_ELECTRON=1。
 */
import { execFileSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'

if (process.env.CI === 'true' || process.env.FLASHCAST_SKIP_ELECTRON === '1') {
  console.log('postinstall: CI 环境跳过 Electron 运行时下载（打包由 electron-builder 负责）')
  process.exit(0)
}

// 直接跑 electron 包自己的 install.js：跨平台，不依赖 npm/pnpm 生成的 .cmd shim。
const require = createRequire(import.meta.url)

try {
  const electronDir = dirname(require.resolve('electron/package.json'))
  execFileSync(process.execPath, [join(electronDir, 'install.js')], { stdio: 'inherit' })
} catch (error) {
  console.error('postinstall: Electron 运行时下载失败，请检查网络后重试 `pnpm install`。')
  process.exit(typeof error.status === 'number' ? error.status : 1)
}
