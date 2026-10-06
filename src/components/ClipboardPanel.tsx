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
            onSubmit={(e) => {
              e.preventDefault();
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
                data-testid="clipboard-retention"
                value={days}
                onChange={(e) => setDays(e.target.value)}
              />
            </label>
            <label>
              容量（条）
              <input
                type="number"
                min="1"
                max="100000"
                required
                data-testid="clipboard-capacity"
                value={capacity}
                onChange={(e) => setCapacity(e.target.value)}
              />
            </label>
            <button
              className="primary-button"
              data-testid="clipboard-save-limits"
              disabled={busy}
            >
              保存范围
            </button>
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
