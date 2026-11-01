import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { execFileSync, spawn } from 'node:child_process'
import { join, resolve } from 'node:path'

const root = process.cwd()
const build = resolve(root, 'build')
const temp = resolve(root, '.agent-tmp/icons')
await mkdir(build, { recursive: true })
await mkdir(temp, { recursive: true })

const appIconSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#21365C"/>
      <stop offset="0.55" stop-color="#101B30"/>
      <stop offset="1" stop-color="#070D18"/>
    </linearGradient>
    <linearGradient id="bolt" x1="0.2" y1="0" x2="0.8" y2="1">
      <stop offset="0" stop-color="#D9ECFF"/>
      <stop offset="0.45" stop-color="#6FA8FF"/>
      <stop offset="1" stop-color="#2563EB"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.46" r="0.5">
      <stop offset="0" stop-color="#4C8DFF" stop-opacity="0.3"/>
      <stop offset="1" stop-color="#4C8DFF" stop-opacity="0"/>
    </radialGradient>
    <filter id="soft" x="-30%" y="-30%" width="160%" height="160%">
      <feGaussianBlur stdDeviation="26"/>
    </filter>
  </defs>
  <rect x="32" y="32" width="960" height="960" rx="224" fill="url(#bg)"/>
  <rect x="33.5" y="33.5" width="957" height="957" rx="222.5" fill="none" stroke="#D8EAFF" stroke-opacity="0.16" stroke-width="3"/>
  <circle cx="512" cy="490" r="310" fill="url(#glow)"/>
  <path d="M566 150 L300 558 H470 L402 874 L746 434 H554 L654 150 Z" fill="#4C8DFF" opacity="0.38" filter="url(#soft)"/>
  <path d="M566 150 L300 558 H470 L402 874 L746 434 H554 L654 150 Z" fill="url(#bolt)"/>
  <path d="M566 150 L300 558 H470 L402 874" fill="none" stroke="#EAF4FF" stroke-opacity="0.5" stroke-width="4" stroke-linejoin="round"/>
</svg>`

const traySvg = (color) => `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 1024 1024">
  <path d="M566 120 L286 556 H470 L392 904 L750 420 H548 L654 120 Z" fill="${color}"/>
</svg>`

const videoCoverSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720" viewBox="0 0 1280 720">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#13223C"/>
      <stop offset="0.55" stop-color="#0C1729"/>
      <stop offset="1" stop-color="#091120"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.42" r="0.55">
      <stop offset="0" stop-color="#4C8DFF" stop-opacity="0.22"/>
      <stop offset="1" stop-color="#4C8DFF" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1280" height="720" fill="url(#bg)"/>
  <rect width="1280" height="720" fill="url(#glow)"/>
  <circle cx="640" cy="332" r="58" fill="rgba(111,168,255,0.16)" stroke="rgba(111,168,255,0.55)" stroke-width="1.5"/>
  <path d="M628 306 L666 332 L628 358 Z" fill="#9AC4FF"/>
  <text x="640" y="438" text-anchor="middle" fill="#F2F6FF" font-family="Inter, system-ui, sans-serif" font-size="28" font-weight="500">视频封面</text>
  <text x="640" y="474" text-anchor="middle" fill="#B2C0D9" font-family="Inter, system-ui, sans-serif" font-size="18" letter-spacing="2">FLASHCAST · VIDEO PREVIEW</text>
</svg>`

await writeFile(join(build, 'icon.svg'), appIconSvg)
await writeFile(join(build, 'tray.svg'), traySvg('#5B9DFF'))
await writeFile(join(build, 'trayTemplate.svg'), traySvg('#000000'))
await writeFile(join(build, 'video-cover.svg'), videoCoverSvg)

const chrome = ['google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser'].find((command) => {
  try {
    execFileSync('which', [command], { stdio: 'ignore' })
    return true
  } catch {
    return false
  }
})

if (!chrome) {
  console.warn('Chrome/Chromium not found; SVG sources were written but PNG icons were not generated.')
  process.exit(0)
}

const delay = (ms) => new Promise((resolveDelay) => setTimeout(resolveDelay, ms))

/**
 * 用 CDP 的 Emulation.setDeviceMetricsOverride + Page.captureScreenshot 精确截取
 * 指定尺寸的画布。
 *
 * 为什么不能用 `--screenshot --window-size=W,H`：headless=new 的 `--window-size`
 * 描述的是浏览器窗口而不是视口，视口高度会比 H 少掉一段（本机 Chrome 153 上稳定
 * 少 87px）。小尺寸图标因此被裁掉底部，只剩顶部一条 —— 96×96 和 128×128 就是
 * 这样被 GNOME 应用网格选中后显示成“半个图标”的。
 */
async function launchChrome() {
  if (typeof WebSocket === 'undefined') {
    throw new Error('Node 缺少全局 WebSocket（需要 >= 22.4），无法精确渲染图标')
  }
  const profile = join(temp, 'chrome-profile')
  const child = spawn(
    chrome,
    [
      '--headless=new',
      '--no-sandbox',
      '--disable-gpu',
      '--hide-scrollbars',
      '--disable-dev-shm-usage',
      '--force-device-scale-factor=1',
      '--remote-debugging-port=0',
      '--remote-allow-origins=*',
      `--user-data-dir=${profile}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-extensions',
      '--mute-audio',
      'about:blank'
    ],
    { stdio: ['ignore', 'ignore', 'pipe'] }
  )

  let stderr = ''
  child.stderr.on('data', (chunk) => {
    stderr += chunk
  })

  const portFile = join(profile, 'DevToolsActivePort')
  for (let attempt = 0; attempt < 200 && !existsSync(portFile); attempt += 1) {
    if (child.exitCode !== null) break
    await delay(50)
  }
  if (!existsSync(portFile)) {
    child.kill('SIGKILL')
    throw new Error(`Chrome 未能启动（端口文件缺失）${stderr.trim() ? `：${stderr.trim()}` : ''}`)
  }

  const [port] = (await readFile(portFile, 'utf8')).trim().split('\n')
  let targets = []
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`)
      targets = await response.json()
      if (targets.some((target) => target.type === 'page' && target.webSocketDebuggerUrl)) break
    } catch {
      // 调试端口还没就绪，继续等。
    }
    await delay(50)
  }

  const page =
    targets.find((target) => target.type === 'page' && target.webSocketDebuggerUrl) ??
    targets.find((target) => target.type === 'other' && target.webSocketDebuggerUrl)
  if (!page) {
    child.kill('SIGKILL')
    throw new Error('Chrome 调试端口没有可用的页面目标')
  }

  const socket = new WebSocket(page.webSocketDebuggerUrl)
  await new Promise((resolveOpen, rejectOpen) => {
    socket.addEventListener('open', resolveOpen, { once: true })
    socket.addEventListener('error', () => rejectOpen(new Error('无法连接 Chrome 调试端口')), { once: true })
  })

  const pending = new Map()
  const listeners = new Map()
  let nextId = 0

  socket.addEventListener('message', (event) => {
    const message = JSON.parse(event.data)
    if (typeof message.id === 'number') {
      const request = pending.get(message.id)
      if (!request) return
      pending.delete(message.id)
      if (message.error) request.reject(new Error(`${request.method}: ${message.error.message}`))
      else request.resolve(message.result)
      return
    }
    const callbacks = listeners.get(message.method)
    if (!callbacks) return
    listeners.delete(message.method)
    for (const callback of callbacks) callback(message.params)
  })

  const send = (method, params = {}) =>
    new Promise((resolveSend, rejectSend) => {
      const id = (nextId += 1)
      pending.set(id, { resolve: resolveSend, reject: rejectSend, method })
      socket.send(JSON.stringify({ id, method, params }))
    })

  const once = (method, timeoutMs = 10_000) =>
    new Promise((resolveEvent, rejectEvent) => {
      const timer = setTimeout(() => rejectEvent(new Error(`等待 ${method} 超时`)), timeoutMs)
      const callback = (params) => {
        clearTimeout(timer)
        resolveEvent(params)
      }
      listeners.set(method, [...(listeners.get(method) ?? []), callback])
    })

  await send('Page.enable')
  await send('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } })

  return {
    send,
    once,
    async close() {
      socket.close()
      child.kill('SIGKILL')
    }
  }
}

const client = await launchChrome()

try {
  async function render(svg, name, width, height = width, dir = build) {
    const htmlPath = join(temp, `${name}.html`)
    const html = `<!doctype html><html><head><meta charset="utf-8"><style>html,body{margin:0;padding:0;background:transparent;overflow:hidden}svg{display:block;width:${width}px;height:${height}px}</style></head><body>${svg}</body></html>`
    await writeFile(htmlPath, html)

    await client.send('Emulation.setDeviceMetricsOverride', {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false
    })
    const loaded = client.once('Page.loadEventFired')
    await client.send('Page.navigate', { url: `file://${htmlPath}` })
    await loaded
    // 等两帧，确保 SVG 的渐变/滤镜绘制完成后再截图。
    await client.send('Runtime.evaluate', {
      expression: 'new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(1))))',
      awaitPromise: true
    })

    const { data } = await client.send('Page.captureScreenshot', {
      format: 'png',
      captureBeyondViewport: false,
      clip: { x: 0, y: 0, width, height, scale: 1 }
    })
    const output = join(dir, `${name}.png`)
    await writeFile(output, Buffer.from(data, 'base64'))
    console.log(`generated ${output.replace(`${root}/`, '')} (${width}×${height})`)
  }

  await render(appIconSvg, 'icon', 1024)

  // Linux 桌面图标需要一整套标准尺寸：hicolor 的 index.theme 只索引到 512x512，
  // 只装一个 1024x1024 的 PNG 会被 GNOME 当成“没有图标”而回退到通用齿轮图标。
  const ICON_SIZES = [16, 22, 24, 32, 36, 48, 64, 72, 96, 128, 192, 256, 512]
  await mkdir(join(build, 'icons'), { recursive: true })
  for (const size of ICON_SIZES) {
    await render(appIconSvg, `${size}x${size}`, size, size, join(build, 'icons'))
  }
  await render(traySvg('#5B9DFF'), 'tray', 64)
  await render(traySvg('#000000'), 'trayTemplate', 32)
  await render(traySvg('#000000'), 'trayTemplate@2x', 64)
  await render(videoCoverSvg, 'video-cover', 1280, 720)
} finally {
  await client.close()
}
