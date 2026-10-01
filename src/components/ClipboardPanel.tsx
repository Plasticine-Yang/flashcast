import { useEffect, useState } from "react";
import type { ClipboardEntryView, ClipboardStateView } from "../types";

interface Props {
  /** 剪贴板历史状态；尚未加载时为 null。 */
  clipboard: ClipboardStateView | null;
  busy: boolean;
  onTogglePaused: (paused: boolean) => void;
  onSaveLimits: (retentionDays: number, capacity: number) => void;
  onPin: (id: string, pinned: boolean) => void;
  onDelete: (id: string) => void;
  onClear: () => void;
  /** 显式为某个原文件保存本机副本（ticket 12）。 */
  onSaveCopy: (id: string, attachmentId: string) => void;
}

/** 相对时间：与宿主副标题的粒度一致（刚刚 / 分钟 / 小时 / 天）。 */
function describeAge(capturedAtMs: number): string {
  const delta = Date.now() - capturedAtMs;
  if (delta < 60_000) return "刚刚";
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)} 分钟前`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)} 小时前`;
  return `${Math.floor(delta / 86_400_000)} 天前`;
}

/**
 * 人类可读的字节数。与宿主 `flashcast_core::plugins::clipboard::describe_bytes` 同口径：
 * 界面与预览不能对同一个文件给出不同的数字。
 */
function describeBytes(bytes: number): string {
  const KB = 1024;
  const MB = 1024 * KB;
  const GB = 1024 * MB;
  if (bytes >= GB) return `${(bytes / GB).toFixed(1)} GB`;
  if (bytes >= MB) return `${(bytes / MB).toFixed(1)} MB`;
  if (bytes >= KB) return `${(bytes / KB).toFixed(1)} KB`;
  return `${bytes} B`;
}

/** 结果副标题里的文件计数，与宿主 `clipboard_subtitle` 口径一致。 */
function fileCountLabel(entry: ClipboardEntryView): string {
  const parts = [`${entry.references} 个引用`];
  if (entry.fileCopies > 0) {
    parts.push(`${entry.fileCopies} 个已保存副本`);
  }
  return parts.join(" · ");
}

/**
 * 设置页里的剪贴板历史管理：保留范围、暂停开关与条目列表。
 *
 * 状态显示刻意分成几件事分别说明，而不是一句「正常」：存储是否可用、容量是否触顶、
 * 最近一次捕获是否失败、后台是否正在捕获。任何一种失败都必须看得见。
 *
 * 文件条目（ticket 12）在紧凑列表里额外给出名称、类型、引用/副本标记与当前是否可恢复；
 * 只有引用才有「保存本机副本」，且它必须由用户显式点击。
 */
export function ClipboardPanel({
  clipboard,
  busy,
  onTogglePaused,
  onSaveLimits,
  onPin,
  onDelete,
  onClear,
  onSaveCopy,
}: Props) {
  const [retentionDraft, setRetentionDraft] = useState("30");
  const [capacityDraft, setCapacityDraft] = useState("500");
  const [confirmingClear, setConfirmingClear] = useState(false);

  // 宿主状态变化（切换工作区、外部重载）时同步草稿，但不覆盖用户正在输入的值：
  // 只有与当前值不同的情况下才重排。
  useEffect(() => {
    if (!clipboard) return;
    setRetentionDraft((current) =>
      current === "" || Number(current) === clipboard.retentionDays ? current : String(clipboard.retentionDays),
    );
    setCapacityDraft((current) =>
      current === "" || Number(current) === clipboard.capacity ? current : String(clipboard.capacity),
    );
  }, [clipboard?.retentionDays, clipboard?.capacity]);

  if (!clipboard) {
    return (
      <section className="settings-section" data-testid="clipboard-section">
        <h2 className="settings-section-title">剪贴板历史</h2>
        <p className="settings-hint" data-testid="clipboard-loading">
          正在读取剪贴板历史状态…
        </p>
      </section>
    );
  }

  return (
    <section className="settings-section" data-testid="clipboard-section">
      <h2 className="settings-section-title">剪贴板历史</h2>

      <p className="settings-hint" data-testid="clipboard-state-summary">
        状态：
        {clipboard.enabled ? "已启用" : "未启用（默认关闭，启用后才会后台记录）"}
        {" · "}
        {clipboard.paused ? "已暂停记录" : "正在记录"}
        {" · "}
        {clipboard.captureActive ? "后台捕获运行中" : "后台捕获未运行"}
        {" · "}
        本机条目 {clipboard.entries}/{clipboard.capacity}
      </p>

      {/* 存储失败、容量触顶与最近一次捕获失败各有独立提示，绝不静默成功。 */}
      {!clipboard.storageOk && clipboard.storageError ? (
        <p className="settings-hint" data-testid="clipboard-storage-error" role="alert">
          存储不可用：{clipboard.storageError}
        </p>
      ) : null}
      {clipboard.capacityReached ? (
        <p className="settings-hint" data-testid="clipboard-capacity-reached" role="status">
          {clipboard.capacityReached}
        </p>
      ) : null}
      {clipboard.lastError && clipboard.storageOk ? (
        <p className="settings-hint" data-testid="clipboard-last-error" role="alert">
          最近一次捕获失败：{clipboard.lastError}
        </p>
      ) : null}

      <div className="settings-row">
        <button
          type="button"
          className="secondary-button"
          data-testid="clipboard-pause"
          disabled={busy || !clipboard.enabled}
          onClick={() => onTogglePaused(!clipboard.paused)}
        >
          {clipboard.paused ? "恢复记录" : "暂停记录"}
        </button>
        <button
          type="button"
          className="secondary-button"
          data-testid="clipboard-clear"
          disabled={busy || clipboard.entries === 0}
          onClick={() => {
            if (!confirmingClear) {
              setConfirmingClear(true);
              return;
            }
            setConfirmingClear(false);
            onClear();
          }}
        >
          {confirmingClear ? "确认清空？" : "清空历史"}
        </button>
      </div>

      <div className="settings-row">
        <label className="settings-label" htmlFor="clipboard-retention">
          保留期限（天）
        </label>
        <input
          id="clipboard-retention"
          className="path-input"
          data-testid="clipboard-retention"
          type="number"
          min={1}
          max={3650}
          value={retentionDraft}
          onChange={(event) => setRetentionDraft(event.target.value)}
        />
        <label className="settings-label" htmlFor="clipboard-capacity">
          容量（条）
        </label>
        <input
          id="clipboard-capacity"
          className="path-input"
          data-testid="clipboard-capacity"
          type="number"
          min={1}
          max={100000}
          value={capacityDraft}
          onChange={(event) => setCapacityDraft(event.target.value)}
        />
        <button
          type="button"
          className="primary-button"
          data-testid="clipboard-save-limits"
          disabled={busy}
          onClick={() => onSaveLimits(Number(retentionDraft), Number(capacityDraft))}
        >
          保存范围
        </button>
      </div>

      {clipboard.items.length === 0 ? (
        <p className="settings-hint" data-testid="clipboard-empty">
          还没有记录到内容。启用插件并在任意应用里复制文字或文件，历史会出现在「剪贴板」或
          「剪切板」关键词下。文件条目默认只是对原文件的引用，需要时可显式保存本机副本。
        </p>
      ) : (
        <ul className="theme-list" data-testid="clipboard-list">
          {clipboard.items.map((entry) => (
            <li
              className={`theme-item${entry.files.length > 0 ? " clipboard-item-files" : ""}`}
              data-testid="clipboard-item"
              data-entry-id={entry.id}
              data-pinned={entry.pinned ? "true" : "false"}
              data-files={entry.files.length}
              key={entry.id}
            >
              {entry.imageDataUrl ? (
                /* 图片条目在管理列表里也要能看到缩略图与类型尺寸（ticket 10）。 */
                <img
                  className="icon thumbnail"
                  data-testid="clipboard-thumbnail"
                  src={entry.imageDataUrl}
                  alt=""
                  aria-hidden="true"
                />
              ) : null}
              <span className="theme-name">
                {entry.summary}
                <span className="theme-meta">
                  {entry.formats.join("/")}
                    {entry.imageSize ? ` · ${entry.imageSize}` : ""}
                    {entry.files.length > 0 ? ` · ${fileCountLabel(entry)}` : ""}
                  {entry.source ? ` · 来自 ${entry.source}` : ""} · {describeAge(entry.capturedAtMs)}
                  {entry.copies > 1 ? ` · 复制过 ${entry.copies} 次` : ""}
                </span>
              </span>
              {entry.pinned ? <span className="theme-badge">已置顶</span> : null}
              <button
                type="button"
                className="secondary-button"
                data-testid="clipboard-pin"
                disabled={busy}
                onClick={() => onPin(entry.id, !entry.pinned)}
              >
                {entry.pinned ? "取消置顶" : "置顶"}
              </button>
              <button
                type="button"
                className="secondary-button"
                data-testid="clipboard-delete"
                disabled={busy}
                onClick={() => onDelete(entry.id)}
              >
                删除
              </button>

              {entry.files.length > 0 ? (
                <ul className="clipboard-files" data-testid="clipboard-file-list">
                  {entry.files.map((file) => (
                    <li
                      className="clipboard-file"
                      data-testid="clipboard-file"
                      data-attachment-id={file.attachmentId}
                      data-kind={file.kind.kind}
                      data-recoverable={file.recoverable ? "true" : "false"}
                      key={file.attachmentId}
                    >
                      <span className="theme-name">
                        {file.name}
                        <span className="theme-meta">
                          {file.mime ?? "未知类型"} · {describeBytes(file.bytes)}
                          {file.recoverable
                            ? ""
                            : ` · 不可恢复：${file.problem ?? "原因未知"}`}
                        </span>
                      </span>
                      <span className="theme-badge" data-testid="clipboard-file-kind">
                        {file.kindLabel}
                      </span>
                      {!file.recoverable ? (
                        <span
                          className="theme-badge theme-badge-error"
                          data-testid="clipboard-file-unrecoverable"
                        >
                          不可恢复
                        </span>
                      ) : null}
                      {file.kind.kind === "fileReference" && file.recoverable ? (
                        <button
                          type="button"
                          className="secondary-button"
                          data-testid="clipboard-save-copy"
                          disabled={busy}
                          onClick={() => onSaveCopy(entry.id, file.attachmentId)}
                        >
                          保存本机副本
                        </button>
                      ) : null}
                    </li>
                  ))}
                </ul>
              ) : null}
            </li>
          ))}
        </ul>
      )}

      <p className="settings-hint">
        历史与索引保存在本机数据目录（
        <code>{clipboard.storagePath}</code>
        ），不进入配置工作区；只有「暂停、保留期限、容量」这些可迁移偏好写在
        settings.toml 里。重复内容会自动去重，Flashcast 自己的粘贴写入不会被再次记录。
      </p>

      <p className="settings-hint">
        文件条目默认只是对原文件的引用：原文件被移动或删除后就无法恢复。
        「保存本机副本」会把文件复制到本机数据目录（受单份与总容量限制，本机专用），
        原文件只会被读取，不会被移动或删除；删除、清空与过期回收只清理不再被引用的副本。
      </p>
    </section>
  );
}
