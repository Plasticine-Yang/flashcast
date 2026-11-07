import { ArrowDownToLine, Loader2, RotateCw } from 'lucide-react'
import type { UpdateState } from '@shared/types'
import { isUpdateActionable, updatePercent } from '@shared/update'

/**
 * Compact update entry in the launcher top bar.
 *
 * It only renders while an update is actionable, so an idle launcher keeps the
 * exact same layout as before. Progress is drawn as a scaled bar (not width) to
 * stay off the layout path, and the whole chip is a single button whose
 * accessible name states the current step.
 */
export function UpdateChip({
  state,
  onAction
}: {
  state: UpdateState | null
  onAction: () => void
}): React.JSX.Element | null {
  if (!state || !isUpdateActionable(state.phase)) return null

  const percent = updatePercent(state.progress)
  const version = state.latestVersion ? `v${state.latestVersion}` : '新版本'

  let label = `更新 ${version}`
  let title = `发现新版本 ${version}，点击下载并安装`
  let icon = <ArrowDownToLine size={13} strokeWidth={2} aria-hidden="true" />
  let disabled = false

  if (state.phase === 'downloading') {
    label = `${percent}%`
    title = `正在下载更新 ${percent}%`
    icon = <Loader2 className="fc-update-chip-spinner" size={13} strokeWidth={2} aria-hidden="true" />
    disabled = true
  } else if (state.phase === 'ready') {
    label = '重启安装'
    title = `更新已下载，点击重启并安装 ${version}`
    icon = <RotateCw size={13} strokeWidth={2} aria-hidden="true" />
  } else if (state.phase === 'installing') {
    label = '安装中…'
    title = '正在安装更新'
    icon = <Loader2 className="fc-update-chip-spinner" size={13} strokeWidth={2} aria-hidden="true" />
    disabled = true
  }

  const announcement =
    state.phase === 'available'
      ? `发现新版本 ${version}，按 Tab 可切换到更新按钮`
      : state.phase === 'ready'
        ? '更新已下载，重启后完成安装'
        : state.phase === 'installing'
          ? '正在安装更新'
          : ''

  return (
    <>
      <button
        type="button"
        className="fc-update-chip"
        data-phase={state.phase}
        aria-label={title}
        title={title}
        // `aria-disabled` instead of `disabled`: a disabled button that was
        // focused by keyboard drops focus to <body>, so typing would stop
        // reaching the search input while the download runs.
        aria-disabled={disabled || undefined}
        onClick={() => {
          if (!disabled) onAction()
        }}
      >
        <span className="fc-update-chip-icon">{icon}</span>
        <span className="fc-update-chip-label">{label}</span>
        {state.phase === 'downloading' ? (
          <span className="fc-update-chip-track" aria-hidden="true">
            <span
              className="fc-update-chip-fill"
              style={{ transform: `scaleX(${Math.max(0.02, percent / 100)})` }}
            />
          </span>
        ) : null}
      </button>
      <span className="sr-only" aria-live="polite">
        {announcement}
      </span>
    </>
  )
}
