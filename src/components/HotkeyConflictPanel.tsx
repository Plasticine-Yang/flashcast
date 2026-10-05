import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { HotkeyConflictReport, HotkeyStatus } from "../types";
import { Glyph } from "./Glyph";
import "./hotkey-conflict.css";

interface Props {
  desired: string;
  hotkey: HotkeyStatus | null;
  active: boolean;
  busy: boolean;
  onBusyChange: (busy: boolean) => void;
  onReportChange: (report: HotkeyConflictReport | null) => void;
  onSaveHotkey: (hotkey: string) => void;
}

export function HotkeyConflictPanel({ desired, hotkey, active, busy, onBusyChange, onReportChange, onSaveHotkey }: Props) {
  const [report, setReport] = useState<HotkeyConflictReport | null>(null);
  const [working, setWorking] = useState(false);
  const [manual, setManual] = useState(false);
  const [message, setMessage] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [completed, setCompleted] = useState(false);
  const [verified, setVerified] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const guide = useRef<HTMLElement>(null);
  const request = useRef(0);
  const operation = useRef(false);
  const mounted = useRef(true);
  useEffect(() => { onReportChange(report); }, [report, onReportChange]);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; request.current++; }; }, []);

  useEffect(() => {
    setConfirmed(false);
    setCompleted(false);
    setVerified(false);
    setMessage("");
    setManual(false);
    setReport(null);
    request.current++;
  }, [desired]);

  useEffect(() => {
    if (!active) return;
    const refresh = () => {
      if (operation.current) return;
      const id = ++request.current;
      void api.get_hotkey_conflict().then(value => {
        if (mounted.current && id === request.current) setReport(value);
      }).catch(() => {
        if (mounted.current && id === request.current) setReport({ status: "unknown", canResolve: false, canUndo: false, effective: null, message: "暂时无法读取系统快捷键，请在系统设置中检查。" });
      });
    };
    refresh();
    window.addEventListener("focus", refresh);
    return () => { if (!operation.current) request.current++; window.removeEventListener("focus", refresh); };
  }, [active, desired, hotkey?.pending]);

  useEffect(() => {
    if (confirmed) dialog.current?.showModal();
    else dialog.current?.close();
  }, [confirmed]);

  const perform = async (action: "check" | "resolve" | "undo") => {
    if (operation.current) return;
    setConfirmed(false);
    operation.current = true;
    setWorking(true);
    onBusyChange(true);
    setMessage("");
    const id = ++request.current;
    try {
      const next = action === "check" ? await api.get_hotkey_conflict() : await api.resolve_hotkey_conflict(action === "undo");
      if (!mounted.current || id !== request.current) return;
      setReport(next);
      setVerified(false);
      setCompleted(action === "resolve" && next.status === "clear");
      if (action === "check") setMessage(next.status === "clear" ? "系统占用已解除。请按一次 Alt+Space 确认可以唤起。" : "仍需处理系统占用或绑定，请检查下面的步骤。");
      if (action === "undo") setMessage("已恢复修改前的系统快捷键。");
    } catch (error) {
      if (!mounted.current || id !== request.current) return;
      setMessage(String(error));
      setManual(true);
      setCompleted(false);
      // 错误之后重新读取，避免继续显示旧的成功状态。
      try { const next = await api.get_hotkey_conflict(); if (mounted.current && id === request.current) setReport(next); } catch { /* 保留可见错误与手动步骤 */ }
    } finally {
      operation.current = false;
      if (mounted.current) setWorking(false);
      onBusyChange(false);
    }
  };

  if (!active || !report || report.status === "not-applicable") return null;
  const needsHelp = ["conflict", "mismatch", "unknown"].includes(report.status);
  const showManual = manual || (needsHelp && !report.canResolve);
  const disabled = busy || working || Boolean(hotkey?.pending);
  return <div className="hotkey-conflict-panel">
    {working ? <div className="hotkey-working" role="status"><span className="hotkey-spinner"/>正在处理系统快捷键…</div> : needsHelp ?
      <section className="hotkey-notice" data-testid="hotkey-conflict">
        <div className="hotkey-notice-title"><Glyph name="hotkey"/><strong>{report.status === "conflict" ? "Alt+Space 被 GNOME 窗口菜单占用" : report.status === "mismatch" ? "系统绑定与 Alt+Space 不一致" : "暂时无法检测系统快捷键"}</strong></div>
        <p>{report.status === "conflict" ? "按下它会打开窗口菜单。解除占用后，再用它唤起 Flashcast。" : report.status === "mismatch" ? "系统仍使用其他按键打开 Flashcast，可以重新绑定为 Alt+Space。" : "请在系统设置中检查 Alt+Space 是否被占用。"}</p>
        {report.message && <p>{report.message}</p>}
        <div className="hotkey-actions">{report.canResolve && <button className="primary-button" disabled={disabled} onClick={() => setConfirmed(true)}>{report.status === "mismatch" ? "重新绑定" : "解除冲突"}</button>}<button className="ghost-button" disabled={disabled} onClick={() => { setManual(true); requestAnimationFrame(() => guide.current?.scrollIntoView({ block: "start", behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" })); }} aria-expanded={showManual}>查看手动步骤</button></div>
      </section> : (completed || report.canUndo) && <section className="hotkey-notice hotkey-success" role="status">
        <strong>{verified ? "快捷键可以正常唤起" : "系统设置已更新"}</strong>
        <p>{hotkey?.pending ? "正在核验 Flashcast 的系统绑定…" : hotkey?.error ? "系统设置已保存，但快捷键注册未完成。请查看上方原因并重新检测。" : verified ? "随时按 Alt+Space 打开 Flashcast。" : "请按一次 Alt+Space，确认 Flashcast 能打开并输入。"}</p>
        {!verified && hotkey?.registered && !hotkey.pending && !hotkey.error && <button className="ghost-button" onClick={() => setVerified(true)}>已确认可以唤起</button>}
      </section>}
    {message && <p className="settings-hint" role="status" data-testid="hotkey-conflict-message">{message}</p>}
    {showManual && <section ref={guide} className="hotkey-manual" aria-label="手动解除冲突">
      <strong>在 GNOME 中手动处理</strong>
      <ol>
        <li><strong>找到窗口快捷键</strong><p>系统「设置 → 键盘 → 查看及自定义快捷键 → 窗口」。不同版本的名称可能略有不同。</p></li>
        <li><strong>停用「激活窗口菜单」</strong><p>选中它，按 Backspace 清除 Alt+Space。右键标题栏仍可打开窗口菜单。</p></li>
        <li><strong>给 Flashcast 分配 Alt+Space</strong><p>在系统「应用 → Flashcast → 全局快捷键」中修改「打开 Flashcast」。如果没有此项，可添加运行 flashcast 的自定义快捷键；便携版使用 AppImage 的绝对路径。</p></li>
      </ol>
      <button className="secondary-button" disabled={disabled} onClick={() => void perform("check")}>我已修改，重新检测</button>
    </section>}

    <div className="hotkey-actions hotkey-secondary-actions">
      <button className="ghost-button" disabled={disabled} onClick={() => void perform("check")}>重新检测</button>
      {report.canUndo && <button className="ghost-button" disabled={disabled} onClick={() => void perform("undo")}>撤销系统修改</button>}
      {needsHelp && <button className="ghost-button" disabled={disabled} onClick={() => onSaveHotkey("Ctrl+Alt+Space")}>改用 Ctrl+Alt+Space</button>}
    </div>
    <dialog ref={dialog} className="hotkey-confirm" onCancel={event => { event.preventDefault(); setConfirmed(false); }} aria-labelledby="hotkey-confirm-title">
      <Glyph name="hotkey"/>
      <h2 id="hotkey-confirm-title">把 Alt+Space 留给 Flashcast？</h2>
      <p>将调整以下两项系统设置：</p>
      <dl><div><dt>GNOME 窗口菜单</dt><dd>移除 Alt+Space，保留其他按键</dd></div><div><dt>打开 Flashcast</dt><dd>{report.effective ?? "当前绑定"} → Alt+Space</dd></div></dl>
      <p>右键标题栏仍可打开窗口菜单。修改后可撤销。</p>
      <div className="hotkey-actions"><button className="secondary-button" autoFocus onClick={() => setConfirmed(false)}>暂不修改</button><button className="primary-button" onClick={() => void perform("resolve")}>解除占用并绑定</button></div>
    </dialog>
  </div>;
}
