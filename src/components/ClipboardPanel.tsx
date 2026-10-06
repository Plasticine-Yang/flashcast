import { useEffect, useState } from "react";
import type { ClipboardStateView } from "../types";
interface Props {
  clipboard: ClipboardStateView | null;
  busy: boolean;
  onTogglePaused: (paused: boolean) => void;
  onEnable: () => void;
  onSaveLimits: (days: number, capacity: number) => void;
}
export function ClipboardPanel({
  clipboard: c,
  busy,
  onTogglePaused,
  onEnable,
  onSaveLimits,
}: Props) {
  const [days, setDays] = useState("30"),
    [capacity, setCapacity] = useState("500");
  const [limitError, setLimitError] = useState<string | null>(null);
  const daysValid = Number.isInteger(Number(days)) && Number(days) >= 1 && Number(days) <= 3650;
  const capacityValid = Number.isInteger(Number(capacity)) && Number(capacity) >= 1 && Number(capacity) <= 100000;
  useEffect(() => {
    if (c) {
      setDays(String(c.retentionDays));
      setCapacity(String(c.capacity));
    }
  }, [c?.retentionDays, c?.capacity]);
  return (
    <section className="settings-section" data-testid="clipboard-section">
      <h2 className="settings-section-title">剪切板</h2>
      <p className="settings-hint">记录复制过的内容，随时搜索和粘贴。</p>
      {c ? (
        <>
          <div className="config-status-row">
            <div>
              <strong>
                {!c.enabled ? "未启用" : c.paused ? "已暂停" : "正在记录"}
              </strong>
              <p className="settings-hint">
                {c.entries} 条内容 · 容量 {c.capacity} 条
              </p>
            </div>
            <button
              className="secondary-button"
              disabled={busy}
              data-testid="clipboard-pause"
              onClick={() =>
                c.enabled ? onTogglePaused(!c.paused) : onEnable()
              }
            >
              {!c.enabled ? "启用" : c.paused ? "继续记录" : "暂停记录"}
            </button>
          </div>
          {(c.storageError ?? c.lastError ?? c.capacityReached) ? (
            <p role="alert">
              {c.storageError ?? c.lastError ?? c.capacityReached}
            </p>
          ) : null}
          <form
            className="clipboard-limits"
            noValidate
            onSubmit={(e) => {
              e.preventDefault();
              if (!daysValid || !capacityValid) {
                setLimitError([
                  !daysValid ? "保留期限须为 1–3650 天的整数" : null,
                  !capacityValid ? "容量须为 1–100000 条的整数" : null,
                ].filter(Boolean).join("；"));
                return;
              }
              setLimitError(null);
              onSaveLimits(Number(days), Number(capacity));
            }}
          >
            <label>
              保留期限（天）
              <input
                type="number"
                min="1"
                max="3650"
                required
                aria-invalid={!!limitError && !daysValid}
                aria-describedby={limitError ? "clipboard-limits-error" : undefined}
                data-testid="clipboard-retention"
                value={days}
                onChange={(e) => { setDays(e.target.value); setLimitError(null); }}
              />
            </label>
            <label>
              容量（条）
              <input
                type="number"
                min="1"
                max="100000"
                required
                aria-invalid={!!limitError && !capacityValid}
                aria-describedby={limitError ? "clipboard-limits-error" : undefined}
                data-testid="clipboard-capacity"
                value={capacity}
                onChange={(e) => { setCapacity(e.target.value); setLimitError(null); }}
              />
            </label>
            <button
              className="primary-button"
              data-testid="clipboard-save-limits"
              disabled={busy}
            >
              保存范围
            </button>
            {limitError ? <p className="field-error" id="clipboard-limits-error" role="alert">{limitError}</p> : null}
          </form>
          <p className="settings-hint">
            历史内容在剪切板插件页面查看，不参与配置同步。
          </p>
        </>
      ) : (
        <p>正在读取…</p>
      )}
    </section>
  );
}
