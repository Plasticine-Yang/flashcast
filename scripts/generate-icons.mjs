import { mkdir, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { execFile } from 'node:child_process'
import { join, resolve } from 'node:path'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
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
  return (process.env.PATH ?? '').split(':').some((dir) => existsSync(join(dir, command)))
})

if (!chrome) {
  console.warn('Chrome/Chromium not found; SVG sources were written but PNG icons were not generated.')
  process.exit(0)
}

async function render(svg, name, width, height = width) {
  const html = `<!doctype html><html><head><meta charset="utf-8"><style>html,body{margin:0;padding:0;background:transparent;overflow:hidden}svg{display:block;width:${width}px;height:${height}px}</style></head><body>${svg}</body></html>`
  const htmlPath = join(temp, `${name}.html`)
  await writeFile(htmlPath, html)
  const profile = join(temp, 'chrome-profile')
  await execFileAsync(
    chrome,
    [
      '--headless=new',
      '--no-sandbox',
      '--disable-gpu',
      '--hide-scrollbars',
      '--disable-dev-shm-usage',
      '--force-device-scale-factor=1',
      `--window-size=${width},${height}`,
      '--default-background-color=00000000',
      `--user-data-dir=${profile}`,
      `--screenshot=${join(build, `${name}.png`)}`,
      `file://${htmlPath}`
    ],
    { timeout: 30000 }
  )
  console.log(`generated build/${name}.png (${width}×${height})`)
}

await render(appIconSvg, 'icon', 1024)
await render(traySvg('#5B9DFF'), 'tray', 64)
await render(traySvg('#000000'), 'trayTemplate', 32)
await render(traySvg('#000000'), 'trayTemplate@2x', 64)
await render(videoCoverSvg, 'video-cover', 1280, 720)
