import { Keycap, Keycaps } from '../components/ui'

export interface FooterHint {
  keys: string[]
  label: string
  tone?: 'danger'
}

export function Footer({
  hints,
  clearCount,
  clearKeys,
  onHelp
}: {
  hints: FooterHint[]
  clearCount: number | null
  clearKeys: string[]
  onHelp: () => void
}): React.JSX.Element {
  return (
    <footer className="fc-footer">
      {clearCount !== null ? (
        <div className="fc-confirm-bar" role="status" aria-live="assertive">
          <Keycaps parts={clearKeys} />
          <span>再按一次，清空全部 {clearCount} 项</span>
          <span className="fc-confirm-hint">按 esc 或其他键取消</span>
        </div>
      ) : (
        <>
          <div className="fc-footer-hints">
            {hints.map((hint, index) => (
              <span
                className={`fc-hint${hint.tone === 'danger' ? ' fc-hint--danger' : ''}`}
                key={`${hint.label}-${index}`}
              >
                <Keycaps parts={hint.keys} />
                <span>{hint.label}</span>
              </span>
            ))}
          </div>
          <div className="fc-footer-spacer" />
          <button
            type="button"
            className="fc-help-button"
            onClick={onHelp}
            aria-label="打开快捷键帮助"
            title="快捷键帮助"
          >
            <Keycap>?</Keycap>
            <span>快捷键</span>
          </button>
        </>
      )}
    </footer>
  )
}
