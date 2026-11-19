import { app, systemPreferences } from 'electron'
import { execFile } from 'node:child_process'
import { join } from 'node:path'
import { promisify } from 'node:util'
import type {
  PermissionResetResult,
  PermissionStatus,
  PlatformId,
  SystemSettingsPane
} from '@shared/types'
import { readJson, writeJsonAtomic } from './json-store'
import { log } from './logger'
import { nativePaste } from './native-paste'
import { userDataDir } from './paths'

const execFileAsync = promisify(execFile)

interface StoredPermissions {
  lastGrantedVersion: string | null
  lastGrantedAt: number | null
}

const EMPTY: StoredPermissions = { lastGrantedVersion: null, lastGrantedAt: null }

/**
 * macOS TCC cannot be queried for "is this grant stale?"; it only answers
 * whether the current process is allowed. Keeping the last version that was
 * granted lets the UI distinguish "never granted" from "the app was updated
 * and macOS no longer matches the old code signature".
 */
export class PermissionService {
  private stored: StoredPermissions = EMPTY
  private loaded = false
  private recordedVersion: string | null = null

  constructor(private readonly platform: PlatformId) {}

  private get version(): string {
    return app.getVersion()
  }

  private get file(): string {
    return join(userDataDir(), 'permissions.json')
  }

  private async init(): Promise<void> {
    if (this.loaded) return
    this.loaded = true
    const stored = await readJson<Partial<StoredPermissions>>(this.file, {})
    this.stored = {
      lastGrantedVersion: stored.lastGrantedVersion ?? null,
      lastGrantedAt: stored.lastGrantedAt ?? null
    }
  }

  async status(): Promise<PermissionStatus> {
    await this.init()
    const version = this.version

    if (this.platform !== 'darwin') {
      return {
        platform: this.platform,
        accessibility: true,
        postEvent: true,
        needsAttention: false,
        stale: false,
        previouslyGranted: false,
        version,
        lastGrantedVersion: null
      }
    }

    const addon = nativePaste()
    const accessibility = this.accessibilityGranted()
    // The AppleScript fallback does not post CGEvents; when the addon is
    // missing, Accessibility is the only relevant grant.
    const postEvent = addon ? this.safeBool(() => addon.postEventTrusted(), accessibility) : accessibility
    const needsAttention = !accessibility || !postEvent
    const previouslyGranted = Boolean(this.stored.lastGrantedVersion)
    const stale =
      needsAttention &&
      previouslyGranted &&
      this.stored.lastGrantedVersion !== version

    if (!needsAttention) this.recordGranted(version)

    return {
      platform: this.platform,
      accessibility,
      postEvent,
      needsAttention,
      stale,
      previouslyGranted,
      version,
      lastGrantedVersion: this.stored.lastGrantedVersion
    }
  }

  /**
   * `tccutil` can only reset; it cannot grant. Resetting removes the stale /
   * duplicate entry so the system prompt (Accessibility) or the list (Full
   * Disk Access) can register the current build cleanly.
   */
  async reset(pane: SystemSettingsPane): Promise<PermissionResetResult> {
    await this.init()
    if (this.platform !== 'darwin') {
      return { ok: true, status: await this.status() }
    }

    const service =
      pane === 'accessibility'
        ? 'Accessibility'
        : pane === 'full-disk-access'
          ? 'SystemPolicyAllFiles'
          : 'AppleEvents'
    const addon = nativePaste()
    const bundleId = addon ? this.safeString(() => addon.bundleId()) : ''
    if (!bundleId) {
      return {
        ok: false,
        error: '无法确定应用标识，请在系统设置里手动移除后重新添加 Flashcast',
        status: await this.status()
      }
    }

    try {
      await execFileAsync('/usr/bin/tccutil', ['reset', service, bundleId], { timeout: 5000 })
    } catch (error) {
      log.warn('tccutil reset failed:', service, bundleId, (error as Error).message)
      return {
        ok: false,
        error: (error as Error).message,
        status: await this.status()
      }
    }

    if (pane === 'accessibility') {
      // Ask the system to register the now-unlisted app and show the standard
      // "open System Settings" prompt. Opening the pane is the caller's job.
      try {
        systemPreferences.isTrustedAccessibilityClient(true)
      } catch {
        // Best effort; the pane still lets the user add it manually.
      }
    }

    return { ok: true, status: await this.status() }
  }

  private accessibilityGranted(): boolean {
    try {
      return systemPreferences.isTrustedAccessibilityClient(false)
    } catch {
      // Unknown: do not cry wolf.
      return true
    }
  }

  private recordGranted(version: string): void {
    if (this.recordedVersion === version) return
    this.recordedVersion = version
    if (this.stored.lastGrantedVersion === version) return
    this.stored = { lastGrantedVersion: version, lastGrantedAt: Date.now() }
    void writeJsonAtomic(this.file, this.stored).catch((error) => {
      log.warn('failed to persist permission state:', (error as Error).message)
    })
  }

  private safeBool(task: () => boolean, fallback: boolean): boolean {
    try {
      return task()
    } catch {
      return fallback
    }
  }

  private safeString(task: () => string): string {
    try {
      return task()
    } catch {
      return ''
    }
  }
}
