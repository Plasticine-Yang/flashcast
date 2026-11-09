import {
  ArrowDownToLine,
  ExternalLink,
  FolderOpen,
  RefreshCw,
  RotateCw,
  X
} from 'lucide-react'
import { useMemo, useState } from 'react'
import type { BootstrapData, UpdateState } from '@shared/types'
import { relativeTime } from '@shared/time'
import { formatBytes, plainReleaseNotes, updatePercent } from '@shared/update'
import { api } from '../bridge'
import { InlineSpinner, Logo, StatusChip } from './components'

type Tone = 'default' | 'success' | 'warning' | 'danger'

const CHIP_TONE: Record<UpdateState['phase'], 'success' | 'warning' | 'danger' | 'default' | 'accent'> = {
  unsupported: 'default',
  idle: 'default',
  checking: 'accent',
  latest: 'success',
  available: 'warning',
  downloading: 'accent',
  ready: 'success',
  installing: 'accent',
  error: 'danger'
}

const STEP_LABELS = ['检查', '下载', '安装'] as const

function phaseTitle(state: UpdateState): string {
  const version = state.latestVersion ? `v${state.latestVersion}` : '新版本'
  switch (state.phase) {
    case 'unsupported':
      return '当前构建不支持自动更新'
    case 'idle':
      return '检查是否有新版本'
    case 'checking':
      return '正在检查更新'
    case 'latest':
      return '已是最新版本'
    case 'available':
      return `发现新版本 ${version}`
    case 'downloading':
      return `正在下载 ${version}`
    case 'ready':
      return `${version} 已就绪`
    case 'installing':
      return `正在安装 ${version}`
    case 'error':
      return '更新失败'
  }
}

/**
 * Update panel for the About page: check → download → restart-and-install as a
 * three-step rail, with a determinate download bar and explicit restart rather
 * than relaunching while the user is working.
 */
export function UpdateCard({
  data,
  update,
  onToast
}: {
  data: BootstrapData
  update: UpdateState | null
  onToast: (message: string, tone?: Tone) => void
}): React.JSX.Element {
  const [busy, setBusy] = useState<'check' | 'install' | null>(null)
  const percent = update ? updatePercent(update.progress) : 0
  const checkedAgo = update?.checkedAt ? `检查于 ${relativeTime(update.checkedAt)}` : '尚未检查更新'

  const run = async (action: 'check' | 'download' | 'cancel' | 'install' | 'release' | 'reveal'): Promise<void> => {
    try {
      if (action === 'check') {
        setBusy('check')
        const next = await api.checkForUpdates()
        if (next.phase === 'available') onToast(`发现新版本 v${next.latestVersion}`, 'warning')
        else if (next.phase === 'latest') onToast(`当前已是最新版本 v${next.currentVersion}`, 'success')
        else if (next.phase === 'unsupported') onToast(next.message, 'default')
        else onToast(next.error ?? next.message, 'danger')
      } else if (action === 'download') {
        const next = await api.downloadUpdate()
        if (next.phase === 'ready') onToast('更新已下载，重启后完成安装', 'success')
        else if (next.phase === 'error') onToast(next.error ?? '下载更新失败', 'danger')
      } else if (action === 'cancel') {
        await api.cancelUpdate()
      } else if (action === 'install') {
        setBusy('install')
        const result = await api.installUpdate()
        if (!result.ok) onToast(result.error ?? '安装更新失败', 'danger')
      } else if (action === 'release') {
        await api.openReleasePage()
      } else {
        await api.showUpdateFile()
      }
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      if (action === 'check' || action === 'install') setBusy(null)
    }
  }

  const phase = update?.phase ?? 'idle'
  const steps = useMemo(() => {
    if (
      phase !== 'available' &&
      phase !== 'downloading' &&
      phase !== 'ready' &&
      phase !== 'installing'
    ) {
      return null
    }
    const active = phase === 'ready' || phase === 'installing' ? 2 : 1
    return STEP_LABELS.map((label, index) => ({
      label,
      state: index < active ? 'done' : index === active ? 'current' : 'todo'
    }))
  }, [phase])

  const meta = (() => {
    if (!update) return '正在读取更新状态…'
    if (update.phase === 'available' && update.latestVersion) {
      const published = update.publishedAt ? ` · 发布于 ${relativeTime(update.publishedAt)}` : ''
      return `当前 v${update.currentVersion}${published}`
    }
    if (update.phase === 'downloading') {
      // The web fallback cannot know the asset size up front, so the total may
      // still be 0 until the first Content-Length arrives.
      const size = update.totalBytes || update.assetSize || 0
      return size > 0
        ? `当前 v${update.currentVersion} · 更新包 ${formatBytes(size)}`
        : `当前 v${update.currentVersion}`
    }
    if (update.phase === 'ready' && update.downloadPath) {
      return update.installKind === 'manual'
        ? '安装包已下载，请在系统安装器中完成安装'
        : '重启后自动替换当前版本'
    }
    if (update.phase === 'error') return update.error ?? '请稍后重试'
    if (update.phase === 'checking') return '正在连接更新源…'
    if (update.phase === 'unsupported') return update.message
    return `当前 v${update.currentVersion} · ${checkedAgo}`
  })()

  const primary = (() => {
    if (!update) return null
    if (update.phase === 'available') {
      return {
        key: 'download' as const,
        label: update.canInstall ? '下载并安装' : '下载安装包',
        icon: <ArrowDownToLine size={13} strokeWidth={2} aria-hidden="true" />
      }
    }
    if (update.phase === 'ready') {
      return {
        key: 'install' as const,
        label: update.installKind === 'manual' ? '打开安装包' : '重启并安装',
        icon:
          update.installKind === 'manual' ? (
            <FolderOpen size={13} strokeWidth={2} aria-hidden="true" />
          ) : (
            <RotateCw size={13} strokeWidth={2} aria-hidden="true" />
          )
      }
    }
    if (update.phase === 'error') {
      // A failed download still has an asset to retry; a failed check does not.
      const retryDownload = Boolean(update.assetName)
      return {
        key: retryDownload ? ('download' as const) : ('check' as const),
        label: retryDownload ? '重试下载' : '重试',
        icon: retryDownload ? (
          <ArrowDownToLine size={13} strokeWidth={2} aria-hidden="true" />
        ) : (
          <RefreshCw size={13} strokeWidth={2} aria-hidden="true" />
        )
      }
    }
    return null
  })()

  return (
    <div className="settings-update-card" data-phase={phase}>
      <div className="settings-update-head">
        <div className="settings-update-mark" aria-hidden="true">
          <Logo size={40} />
        </div>
        <div className="settings-update-copy">
          <div className="settings-update-title">
            {phaseTitle(update ?? placeholderState(data))}
            {update?.latestVersion ? (
              <StatusChip tone={CHIP_TONE[phase]}>v{update.latestVersion}</StatusChip>
            ) : null}
          </div>
          <div className="settings-update-meta" aria-live="polite">
            {meta}
          </div>
        </div>
        {phase === 'checking' || phase === 'installing' ? (
          <InlineSpinner label={phase === 'checking' ? '检查中' : '安装中'} />
        ) : (
          <span className="settings-update-phase">{phaseLabel(phase)}</span>
        )}
      </div>

      {steps ? (
        <ol className="settings-update-steps" aria-label="更新进度">
          {steps.map((step) => (
            <li className="settings-update-step" data-state={step.state} key={step.label}>
              <span className="settings-update-step-mark" aria-hidden="true" />
              <span className="settings-update-step-label">{step.label}</span>
              <span className="sr-only">
                {step.state === 'done' ? '已完成' : step.state === 'current' ? '进行中' : '待进行'}
              </span>
            </li>
          ))}
        </ol>
      ) : null}

      {update?.phase === 'downloading' ? (
        <div className="settings-update-progress">
          <div
            className="settings-progress"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent}
            aria-valuetext={`${percent}%，${formatBytes(update.downloadedBytes)}${
              update.totalBytes || update.assetSize
                ? ` / ${formatBytes(update.totalBytes || update.assetSize || 0)}`
                : ''
            }`}
            aria-label="更新下载进度"
          >
            <span
              className="settings-progress-fill"
              style={{ transform: `scaleX(${Math.max(0.01, percent / 100)})` }}
            />
          </div>
          <div className="settings-update-progress-meta">
            <span>{percent}%</span>
            <span>
              {formatBytes(update.downloadedBytes)}
              {update.totalBytes || update.assetSize
                ? ` / ${formatBytes(update.totalBytes || update.assetSize || 0)}`
                : ''}
            </span>
          </div>
        </div>
      ) : null}

      {update?.releaseNotes && (phase === 'available' || phase === 'ready') ? (
        <div className="settings-update-notes">
          <div className="settings-update-notes-head">更新内容</div>
          <p>{plainReleaseNotes(update.releaseNotes)}</p>
        </div>
      ) : null}

      <div className="settings-update-actions">
        {primary ? (
          <button
            type="button"
            className="fc-btn fc-btn--primary"
            onClick={() => void run(primary.key)}
            disabled={busy !== null || (primary.key === 'install' && !update?.canInstall)}
          >
            {primary.icon}
            {primary.label}
          </button>
        ) : null}

        {(phase === 'idle' || phase === 'latest' || phase === 'unsupported') && (
          <button
            type="button"
            className="fc-btn"
            onClick={() => void run('check')}
            disabled={busy === 'check'}
          >
            {busy === 'check' ? (
              <span className="fc-spinner" aria-hidden="true" />
            ) : (
              <RefreshCw size={13} strokeWidth={2} aria-hidden="true" />
            )}
            检查更新
          </button>
        )}

        {phase === 'downloading' ? (
          <button type="button" className="fc-btn fc-btn--ghost" onClick={() => void run('cancel')}>
            <X size={13} strokeWidth={2} aria-hidden="true" />
            取消下载
          </button>
        ) : null}

        {phase === 'ready' ? (
          <button type="button" className="fc-btn fc-btn--ghost" onClick={() => void run('reveal')}>
            <FolderOpen size={13} strokeWidth={2} aria-hidden="true" />
            打开安装包位置
          </button>
        ) : null}

        {(update?.releaseUrl ?? '') !== '' ? (
          <button
            type="button"
            className="settings-link-button"
            onClick={() => void run('release')}
          >
            <ExternalLink size={12} strokeWidth={1.5} aria-hidden="true" />
            发布说明
          </button>
        ) : null}

        <span className="settings-update-hint">
          {update?.phase === 'ready' && !update.canInstall
            ? '开发构建不支持自动安装，请使用正式安装包'
            : ''}
        </span>
      </div>
    </div>
  )
}

function phaseLabel(phase: UpdateState['phase']): string {
  switch (phase) {
    case 'unsupported':
      return '手动更新'
    case 'idle':
      return '待检查'
    case 'checking':
      return '检查中'
    case 'latest':
      return '已最新'
    case 'available':
      return '有新版'
    case 'downloading':
      return '下载中'
    case 'ready':
      return '待重启'
    case 'installing':
      return '安装中'
    case 'error':
      return '需重试'
  }
}

/** Placeholder state used before the first `getUpdateState()` resolves. */
function placeholderState(data: BootstrapData): UpdateState {
  return {
    phase: 'idle',
    currentVersion: data.version,
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
