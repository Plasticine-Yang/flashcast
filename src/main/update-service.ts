import { app, net, shell } from 'electron'
import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createReadStream, createWriteStream, existsSync } from 'node:fs'
import { access, chmod, copyFile, mkdir, rename, rm, writeFile } from 'node:fs/promises'
import { constants } from 'node:fs'
import { join, resolve } from 'node:path'
import type { ActionResult, PlatformId, UpdateState } from '@shared/types'
import { log } from './logger'
import { appVersion, isDev } from './paths'
import {
  checksumFor,
  installKindFor,
  isNewerVersion,
  normalizeReleaseNotes,
  parseVersion,
  pickUpdateAsset,
  type ReleaseAsset
} from './update-assets'

const DEFAULT_REPO = 'Plasticine-Yang/flashcast'
const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000
const MIN_CHECK_GAP_MS = 60 * 1000
const START_DELAY_MS = 6000
const PROGRESS_THROTTLE_MS = 120
const MAX_JSON_BYTES = 4 * 1024 * 1024
const MAX_CHECKSUMS_BYTES = 256 * 1024

interface ReleaseInfo {
  version: string
  name: string
  notes: string | null
  url: string
  publishedAt: number | null
  asset: ReleaseAsset | null
  checksums: ReleaseAsset | null
}

function repoSlug(): string {
  return process.env.FLASHCAST_UPDATE_REPO?.trim() || DEFAULT_REPO
}

export function releaseFeed(): string {
  const override = process.env.FLASHCAST_UPDATE_FEED?.trim()
  if (override) return override
  return `https://api.github.com/repos/${repoSlug()}/releases/latest`
}

function toAsset(value: unknown): ReleaseAsset | null {
  if (!value || typeof value !== 'object') return null
  const record = value as Record<string, unknown>
  if (
    typeof record.name !== 'string' ||
    typeof record.size !== 'number' ||
    typeof record.browser_download_url !== 'string'
  ) {
    return null
  }
  return { name: record.name, size: record.size, url: record.browser_download_url }
}

function describeError(error: unknown): string {
  if (error instanceof Error && error.message) return error.message
  return String(error)
}

function contentLength(value: unknown): number {
  if (typeof value === 'number') return value
  if (typeof value === 'string') return Number(value) || 0
  return 0
}

/**
 * Check, download and install Flashcast releases published on GitHub.
 *
 * The feed is the public Releases API (or `FLASHCAST_UPDATE_FEED`), so the
 * updater needs no release server of its own. Downloads are verified against
 * the release's `SHA256SUMS.txt` before anything is unpacked or executed.
 */
export class UpdateService {
  private state: UpdateState
  private release: ReleaseInfo | null = null
  private checking: Promise<UpdateState> | null = null
  private activeRequest: Electron.ClientRequest | null = null
  private cancelled = false
  private startTimer: NodeJS.Timeout | null = null
  private intervalTimer: NodeJS.Timeout | null = null

  constructor(
    private readonly platform: PlatformId,
    private readonly emit: (state: UpdateState) => void
  ) {
    this.state = {
      phase: 'idle',
      currentVersion: appVersion(),
      latestVersion: null,
      releaseName: null,
      releaseNotes: null,
      releaseUrl: null,
      publishedAt: null,
      assetName: null,
      assetSize: null,
      downloadedBytes: 0,
      totalBytes: 0,
      progress: 0,
      downloadPath: null,
      installKind: 'none',
      canInstall: false,
      message: '尚未检查更新',
      error: null,
      checkedAt: null
    }
  }

  getState(): UpdateState {
    return this.state
  }

  /** Background check on launch and every few hours; silent when it fails. */
  start(): void {
    if (isDev()) return
    this.startTimer = setTimeout(() => {
      void this.check({ silent: true })
    }, START_DELAY_MS)
    this.intervalTimer = setInterval(() => {
      void this.check({ silent: true })
    }, CHECK_INTERVAL_MS)
  }

  dispose(): void {
    if (this.startTimer) clearTimeout(this.startTimer)
    if (this.intervalTimer) clearInterval(this.intervalTimer)
    this.startTimer = null
    this.intervalTimer = null
    this.activeRequest?.abort()
    this.activeRequest = null
  }

  /* ------------------------------- checking ------------------------------ */

  async check(options: { force?: boolean; silent?: boolean } = {}): Promise<UpdateState> {
    if (this.checking) return this.checking
    // A downloaded update stays ready until the user restarts; a background
    // check must not throw that work away.
    if (
      this.state.phase === 'downloading' ||
      this.state.phase === 'installing' ||
      this.state.phase === 'ready'
    ) {
      return this.state
    }
    const fresh =
      this.state.checkedAt !== null && Date.now() - this.state.checkedAt < MIN_CHECK_GAP_MS
    if (!options.force && fresh && this.state.phase !== 'error') return this.state

    this.setState({ phase: 'checking', message: '正在检查更新…', error: null })
    this.checking = this.runCheck(options.silent === true).finally(() => {
      this.checking = null
    })
    return this.checking
  }

  private async runCheck(silent: boolean): Promise<UpdateState> {
    try {
      const payload = await this.fetchJson(releaseFeed())
      const release = this.parseRelease(payload)
      if (!release) throw new Error('更新源返回了无法识别的数据')
      this.release = release

      if (!isNewerVersion(release.version, this.state.currentVersion)) {
        this.setState({
          phase: 'latest',
          latestVersion: release.version,
          releaseName: release.name,
          releaseUrl: release.url,
          checkedAt: Date.now(),
          message: `已是最新版本 v${this.state.currentVersion}`,
          error: null
        })
        return this.state
      }

      const asset = release.asset
      if (!asset) {
        this.setState({
          phase: 'unsupported',
          latestVersion: release.version,
          releaseName: release.name,
          releaseNotes: release.notes,
          releaseUrl: release.url,
          publishedAt: release.publishedAt,
          checkedAt: Date.now(),
          message: `v${release.version} 没有适用于当前平台的安装包`,
          error: null
        })
        return this.state
      }

      const installKind = installKindFor(asset.name, this.platform)
      this.setState({
        phase: 'available',
        latestVersion: release.version,
        releaseName: release.name,
        releaseNotes: release.notes,
        releaseUrl: release.url,
        publishedAt: release.publishedAt,
        assetName: asset.name,
        assetSize: asset.size,
        totalBytes: asset.size,
        downloadedBytes: 0,
        progress: 0,
        downloadPath: null,
        installKind,
        canInstall: !isDev() && installKind !== 'none',
        checkedAt: Date.now(),
        message: `发现新版本 v${release.version}`,
        error: null
      })
      return this.state
    } catch (error) {
      const message = describeError(error)
      log.warn(`update check failed${silent ? ' (background)' : ''}:`, message)
      this.setState({
        phase: 'error',
        checkedAt: Date.now(),
        message: '检查更新失败',
        error: message
      })
      return this.state
    }
  }

  /* ------------------------------ downloading ---------------------------- */

  async download(): Promise<UpdateState> {
    const asset = this.release?.asset
    if (!asset) return this.state
    if (this.state.phase !== 'available' && this.state.phase !== 'error') return this.state

    const directory = join(app.getPath('userData'), 'updates')
    const target = join(directory, asset.name)
    const partial = `${target}.part`

    this.cancelled = false
    this.setState({
      phase: 'downloading',
      downloadedBytes: 0,
      totalBytes: asset.size,
      progress: 0,
      downloadPath: null,
      message: '正在下载更新…',
      error: null
    })

    try {
      await mkdir(directory, { recursive: true })
      await rm(partial, { force: true })
      await this.downloadTo(asset.url, partial, asset.size)
      const verified = await this.verifyChecksum(asset.name, partial)
      if (!verified) throw new Error('安装包校验失败，请稍后重试')
      await rm(target, { force: true })
      await rename(partial, target)
      const installKind = installKindFor(asset.name, this.platform)
      this.setState({
        phase: 'ready',
        downloadedBytes: asset.size,
        totalBytes: asset.size,
        progress: 1,
        downloadPath: target,
        installKind,
        canInstall: !isDev() && installKind !== 'none',
        message:
          installKind === 'manual'
            ? '安装包已下载，可在系统安装器中完成安装'
            : '更新已就绪，重启后完成安装',
        error: null
      })
      return this.state
    } catch (error) {
      await rm(partial, { force: true }).catch(() => undefined)
      if (this.cancelled) {
        this.setState({
          phase: 'available',
          downloadedBytes: 0,
          totalBytes: asset.size,
          progress: 0,
          message: '已取消下载',
          error: null
        })
        return this.state
      }
      const message = describeError(error)
      log.warn('update download failed:', message)
      this.setState({
        phase: 'error',
        progress: 0,
        downloadedBytes: 0,
        message: '下载更新失败',
        error: message
      })
      return this.state
    }
  }

  async cancel(): Promise<UpdateState> {
    if (this.state.phase !== 'downloading') return this.state
    this.cancelled = true
    this.activeRequest?.abort()
    this.activeRequest = null
    return this.state
  }

  private downloadTo(url: string, path: string, expectedSize: number): Promise<void> {
    return new Promise<void>((resolvePromise, rejectPromise) => {
      const output = createWriteStream(path)
      let received = 0
      let lastEmit = 0
      let settled = false

      const fail = (error: Error): void => {
        if (settled) return
        settled = true
        output.destroy()
        rejectPromise(error)
      }

      output.on('error', (error: Error) => fail(error))
      output.on('finish', () => {
        if (settled) return
        settled = true
        this.activeRequest = null
        resolvePromise()
      })

      const request = net.request({ url, method: 'GET', redirect: 'follow' })
      this.activeRequest = request
      request.setHeader('Accept', 'application/octet-stream')
      request.setHeader('User-Agent', `Flashcast/${this.state.currentVersion}`)

      request.on('response', (response) => {
        const status = response.statusCode ?? 0
        if (status !== 200) {
          response.on('data', () => undefined)
          response.on('end', () => fail(new Error(`下载更新失败（HTTP ${status}）`)))
          response.on('error', (error: Error) => fail(error))
          return
        }
        const total = expectedSize > 0 ? expectedSize : contentLength(response.headers['content-length'])
        response.on('data', (chunk: Buffer) => {
          received += chunk.length
          output.write(chunk)
          const now = Date.now()
          if (now - lastEmit >= PROGRESS_THROTTLE_MS) {
            lastEmit = now
            this.setState({
              downloadedBytes: received,
              totalBytes: total || received,
              progress: total > 0 ? Math.min(0.999, received / total) : 0
            })
          }
        })
        response.on('end', () => output.end())
        response.on('error', (error: Error) => fail(error))
      })

      request.on('error', (error: Error) => fail(error))
      request.on('abort', () => fail(new Error('下载已取消')))
      request.end()
    })
  }

  private async verifyChecksum(name: string, path: string): Promise<boolean> {
    const checksums = this.release?.checksums
    if (!checksums) {
      log.warn('release has no SHA256SUMS.txt; skipping checksum verification')
      return true
    }
    try {
      const text = await this.fetchText(checksums.url, MAX_CHECKSUMS_BYTES)
      const expected = checksumFor(text, name)
      if (!expected) {
        log.warn(`no checksum entry for ${name}; skipping verification`)
        return true
      }
      const actual = await sha256File(path)
      if (actual !== expected) {
        log.error(`checksum mismatch for ${name}: expected ${expected}, got ${actual}`)
        return false
      }
      return true
    } catch (error) {
      log.warn('checksum verification skipped:', describeError(error))
      return true
    }
  }

  /* ------------------------------- installing ---------------------------- */

  async install(): Promise<ActionResult> {
    const path = this.state.downloadPath
    if (this.state.phase !== 'ready' || !path) {
      return { ok: false, error: '还没有可安装的更新' }
    }
    if (isDev()) {
      return { ok: false, error: '开发构建不支持自动安装，请使用正式安装包' }
    }
    if (!existsSync(path)) {
      this.setState({ phase: 'error', message: '安装包已不存在', error: '请重新下载更新' })
      return { ok: false, error: '安装包已不存在，请重新下载' }
    }

    this.setState({ phase: 'installing', message: '正在安装更新…', error: null })
    try {
      if (this.state.installKind === 'manual') {
        const opened = await shell.openPath(path)
        if (opened) throw new Error(opened)
        this.setState({ phase: 'ready', message: '已打开系统安装器，完成后重新打开 Flashcast' })
        return { ok: true, toast: { message: '已打开系统安装器', tone: 'default' } }
      }
      if (this.platform === 'darwin') {
        await this.installOnMac(path)
        return { ok: true, toast: { message: '正在重启并安装更新', tone: 'success' } }
      }
      if (this.platform === 'win32') {
        await this.installOnWindows(path)
        return { ok: true, toast: { message: '正在重启并安装更新', tone: 'success' } }
      }
      await this.installOnLinux(path)
      return { ok: true, toast: { message: '正在重启并安装更新', tone: 'success' } }
    } catch (error) {
      const message = describeError(error)
      log.warn('update install failed:', message)
      this.setState({ phase: 'error', message: '安装更新失败', error: message })
      return { ok: false, error: message }
    }
  }

  private async installOnMac(zipPath: string): Promise<void> {
    const bundle = macBundlePath()
    if (!bundle) throw new Error('无法定位当前应用，请手动安装')
    try {
      await access(bundle, constants.W_OK)
    } catch {
      throw new Error('没有权限替换当前应用，请手动安装新版本')
    }
    const staging = join(app.getPath('userData'), 'updates', 'staging')
    await rm(staging, { recursive: true, force: true })
    await mkdir(staging, { recursive: true })
    await run('/usr/bin/ditto', ['-x', '-k', zipPath, staging])
    const source = join(staging, 'Flashcast.app')
    if (!existsSync(join(source, 'Contents', 'MacOS', 'Flashcast'))) {
      throw new Error('更新包内容不完整，请重新下载')
    }

    const script = join(app.getPath('temp'), `flashcast-update-${process.pid}.sh`)
    await writeFile(
      script,
      [
        '#!/bin/sh',
        '# Wait for Flashcast to exit, then swap the app bundle in place.',
        '# The old bundle is kept as a backup until the new one launches so a',
        '# broken copy cannot leave the user without an app.',
        'while kill -0 "$FC_PID" 2>/dev/null; do sleep 0.2; done',
        'sleep 0.4',
        'rm -rf "$FC_BACKUP"',
        'mv "$FC_TARGET" "$FC_BACKUP" || exit 1',
        'if /usr/bin/ditto "$FC_SOURCE" "$FC_TARGET"; then',
        '  xattr -dr com.apple.quarantine "$FC_TARGET" >/dev/null 2>&1 || true',
        '  if open "$FC_TARGET" >/dev/null 2>&1; then',
        '    rm -rf "$FC_STAGING" "$FC_ZIP" "$FC_BACKUP" >/dev/null 2>&1 || true',
        '    exit 0',
        '  fi',
        'fi',
        'rm -rf "$FC_TARGET"',
        'mv "$FC_BACKUP" "$FC_TARGET"',
        'open "$FC_TARGET" >/dev/null 2>&1 || true',
        ''
      ].join('\n'),
      { mode: 0o755 }
    )
    await spawnDetached('/bin/sh', [script], {
      FC_PID: String(process.pid),
      FC_TARGET: bundle,
      FC_SOURCE: source,
      FC_STAGING: staging,
      FC_ZIP: zipPath,
      FC_BACKUP: `${bundle}.old`
    })
    this.quitForInstall()
  }

  private async installOnWindows(installerPath: string): Promise<void> {
    await spawnDetached(installerPath, ['/S', '--updated'], {}, { windowsHide: true })
    this.quitForInstall()
  }

  private async installOnLinux(downloadPath: string): Promise<void> {
    const target = process.env.APPIMAGE?.trim()
    if (!target || !downloadPath.toLowerCase().endsWith('.appimage')) {
      const opened = await shell.openPath(downloadPath)
      if (opened) throw new Error(opened)
      this.setState({ phase: 'ready', message: '已打开系统安装器，完成后重新打开 Flashcast' })
      return
    }
    try {
      await access(target, constants.W_OK)
    } catch {
      throw new Error('没有权限替换当前 AppImage，请手动安装新版本')
    }
    const staged = `${target}.new`
    await rm(staged, { force: true })
    await copyFile(downloadPath, staged)
    await chmod(staged, 0o755)

    const script = join(app.getPath('temp'), `flashcast-update-${process.pid}.sh`)
    await writeFile(
      script,
      [
        '#!/bin/sh',
        'while kill -0 "$FC_PID" 2>/dev/null; do sleep 0.2; done',
        'sleep 0.4',
        'mv -f "$FC_SOURCE" "$FC_TARGET" || exit 1',
        'chmod +x "$FC_TARGET"',
        '"$FC_TARGET" >/dev/null 2>&1 &',
        ''
      ].join('\n'),
      { mode: 0o755 }
    )
    await spawnDetached('/bin/sh', [script], {
      FC_PID: String(process.pid),
      FC_TARGET: target,
      FC_SOURCE: staged
    })
    this.quitForInstall()
  }

  /** The helper script (macOS/Linux) or the installer relaunches the app. */
  private quitForInstall(): void {
    setTimeout(() => app.quit(), 500)
  }

  /* --------------------------------- helpers ----------------------------- */

  private setState(patch: Partial<UpdateState>): void {
    this.state = { ...this.state, ...patch }
    this.emit(this.state)
  }

  private parseRelease(payload: unknown): ReleaseInfo | null {
    if (!payload || typeof payload !== 'object') return null
    const record = payload as Record<string, unknown>
    const tag = typeof record.tag_name === 'string' ? record.tag_name : ''
    const version = tag.replace(/^v/i, '')
    if (!parseVersion(version)) return null
    const assets = Array.isArray(record.assets) ? record.assets : []
    const available = assets.map(toAsset).filter((asset): asset is ReleaseAsset => asset !== null)
    const published =
      typeof record.published_at === 'string' ? Date.parse(record.published_at) : Number.NaN
    return {
      version,
      name: typeof record.name === 'string' && record.name.trim() ? record.name : `v${version}`,
      notes: normalizeReleaseNotes(typeof record.body === 'string' ? record.body : null),
      url:
        typeof record.html_url === 'string'
          ? record.html_url
          : `https://github.com/${repoSlug()}/releases/tag/v${version}`,
      publishedAt: Number.isFinite(published) ? published : null,
      asset: pickUpdateAsset(available, this.platform, process.arch),
      checksums: available.find((asset) => /^sha256sums\.txt$/i.test(asset.name)) ?? null
    }
  }

  private fetchJson(url: string): Promise<unknown> {
    return this.fetchText(url, MAX_JSON_BYTES).then((text) => JSON.parse(text) as unknown)
  }

  private fetchText(url: string, limit: number): Promise<string> {
    return new Promise<string>((resolvePromise, rejectPromise) => {
      const request = net.request({
        url,
        method: 'GET',
        redirect: 'follow',
        headers: {
          Accept: 'application/vnd.github+json',
          'User-Agent': `Flashcast/${this.state.currentVersion}`
        }
      })
      let settled = false
      const timer = setTimeout(() => {
        request.abort()
        rejectPromise(new Error('请求超时，请检查网络连接'))
      }, 20_000)

      const settle = (fn: () => void): void => {
        if (settled) return
        settled = true
        clearTimeout(timer)
        fn()
      }

      request.on('response', (response) => {
        let body = ''
        response.on('data', (chunk: Buffer) => {
          body += chunk.toString('utf8')
          if (body.length > limit) {
            request.abort()
            settle(() => rejectPromise(new Error('更新源返回的数据过大')))
          }
        })
        response.on('end', () => {
          const status = response.statusCode ?? 0
          if (status < 200 || status >= 300) {
            settle(() => {
              rejectPromise(
                new Error(status === 404 ? '尚未发布任何版本' : `更新源响应异常（HTTP ${status}）`)
              )
            })
            return
          }
          settle(() => resolvePromise(body))
        })
        response.on('error', (error: Error) => settle(() => rejectPromise(error)))
      })
      request.on('error', (error: Error) => settle(() => rejectPromise(error)))
      request.end()
    })
  }
}

/* -------------------------------------------------------------------------- */
/* Platform helpers                                                            */
/* -------------------------------------------------------------------------- */

function macBundlePath(): string | null {
  if (process.platform !== 'darwin') return null
  const bundle = resolve(app.getPath('exe'), '../../..')
  return bundle.endsWith('.app') ? bundle : null
}

function sha256File(path: string): Promise<string> {
  const hash = createHash('sha256')
  return new Promise<string>((resolvePromise, rejectPromise) => {
    const stream = createReadStream(path)
    stream.on('data', (chunk) => hash.update(chunk))
    stream.on('end', () => resolvePromise(hash.digest('hex')))
    stream.on('error', rejectPromise)
  })
}

function run(command: string, args: string[]): Promise<void> {
  return new Promise<void>((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, { stdio: ['ignore', 'ignore', 'pipe'] })
    let stderr = ''
    child.stderr.on('data', (chunk) => {
      stderr += String(chunk)
    })
    child.on('error', rejectPromise)
    child.on('exit', (code) => {
      if (code === 0) resolvePromise()
      else rejectPromise(new Error(`${command} 退出码 ${code}${stderr ? `：${stderr.trim()}` : ''}`))
    })
  })
}

function spawnDetached(
  command: string,
  args: string[],
  env: Record<string, string>,
  options: { windowsHide?: boolean } = {}
): Promise<void> {
  return new Promise<void>((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, {
      detached: true,
      stdio: 'ignore',
      windowsHide: options.windowsHide === true,
      env: { ...process.env, ...env }
    })
    child.once('error', rejectPromise)
    child.once('spawn', () => {
      child.unref()
      resolvePromise()
    })
  })
}
