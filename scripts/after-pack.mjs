/**
 * electron-builder afterPack 钩子：给 macOS 产物补一个 ad-hoc 签名。
 *
 * 背景：仓库没有 Apple 开发者证书，流水线用 CSC_IDENTITY_AUTO_DISCOVERY=false 跳过签名。
 * Electron 预构建的二进制只带 linker 签名、bundle 里没有 _CodeSignature，arm64 上 amfid
 * 会拒绝加载 Electron Framework，应用启动即静默退出：
 *
 *   amfid: Flashcast.app/Contents/MacOS/Flashcast not valid:
 *     "The file is adhoc signed or signed by an unknown certificate chain"
 *
 * 所以打包完必须整体重签一次，dmg / zip 里才是能直接双击运行的 bundle。
 *
 * 只补"没有有效签名"的情况：本机存在证书时 electron-builder 会在本钩子之后用真实证书
 * 重签，这里提前补 ad-hoc 不会干扰它（已有有效签名直接跳过）。另外必须在 afterPack 而
 * 不是 afterSign 做，因为未签名时 afterSign 根本不会触发。
 */
import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'

/** `codesign --verify` 能过就认为签名完整可用（ad-hoc 与真实证书都算）。 */
function hasValidSignature(appPath) {
  try {
    execFileSync('codesign', ['--verify', '--deep', '--strict', appPath], { stdio: 'ignore' })
    return true
  } catch {
    return false
  }
}

export async function afterPack(context) {
  // Linux / Windows 产物不需要也不应该走 codesign。
  if (context.electronPlatformName !== 'darwin') return

  const appPath = join(context.appOutDir, `${context.packager.appInfo.productFilename}.app`)
  if (!existsSync(appPath)) {
    throw new Error(`afterPack: 找不到应用包 ${appPath}`)
  }

  if (hasValidSignature(appPath)) {
    console.log(`  • 已有有效签名，跳过 ad-hoc 签名：${appPath}`)
    return
  }

  console.log(`  • 补 ad-hoc 签名：${appPath}`)
  execFileSync('codesign', ['--force', '--deep', '--sign', '-', appPath], { stdio: 'inherit' })

  if (!hasValidSignature(appPath)) {
    throw new Error('afterPack: ad-hoc 签名后仍然校验失败，macOS 产物不可用')
  }
}

export default afterPack
