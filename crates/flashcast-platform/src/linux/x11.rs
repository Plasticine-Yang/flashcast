//! X11 / EWMH 访问。
//!
//! 通过 `_NET_ACTIVE_WINDOW`、`_NET_CLIENT_LIST`、`WM_CLASS` 与 `_NET_WM_PID`
//! 读取窗口信息，并用 `_NET_ACTIVE_WINDOW` 客户端消息恢复焦点。
//!
//! 该模块只在 X11 会话下有实际意义；Wayland 会话下即便存在 XWayland 的
//! `DISPLAY`，也只能看到 X11 客户端窗口，不能代表整个桌面。

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, PropMode, Window,
};
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

/// 读取到的 X11 前台窗口信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11ActiveWindow {
    pub window: u64,
    /// `WM_CLASS` 的实例名与类名。
    pub instance: Option<String>,
    pub class: Option<String>,
    /// `_NET_WM_NAME`（UTF-8）或 `WM_NAME`。
    pub title: Option<String>,
    pub pid: Option<u32>,
}

impl X11ActiveWindow {
    /// 用于稳定标识的窗口类名：优先实例名，其次类名。
    pub fn identifier(&self) -> Option<String> {
        self.instance
            .clone()
            .or_else(|| self.class.clone())
            .filter(|s| !s.is_empty())
    }
}

/// X11 探测的可用性。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X11Availability {
    Available,
    NoDisplay,
    ConnectFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11Diagnostics {
    pub availability: X11Availability,
    /// 读取到的前台窗口（若有）。
    pub active: Option<X11ActiveWindow>,
    /// 窗口管理器名称（`_NET_SUPPORTING_WM_CHECK` 指向窗口的 `_NET_WM_NAME`）。
    pub window_manager: Option<String>,
    /// `_NET_CLIENT_LIST` 中的窗口数量。
    pub client_count: Option<usize>,
}

fn connect() -> Result<(x11rb::rust_connection::RustConnection, usize), x11rb::errors::ConnectError>
{
    x11rb::connect(None)
}

/// 当前 X11 前台窗口；没有 `DISPLAY`、连接失败或没有前台窗口时返回 `None`。
pub fn active_window() -> Option<X11ActiveWindow> {
    let (conn, screen_num) = connect().ok()?;
    let root = conn.setup().roots.get(screen_num)?.root;
    let net_active = intern(&conn, b"_NET_ACTIVE_WINDOW").ok()?;
    let net_wm_pid = intern(&conn, b"_NET_WM_PID").ok()?;

    let window = get_window_property(&conn, root, net_active)
        .and_then(|windows| windows.first().copied())
        .filter(|w| *w != 0)
        // 没有 EWMH 属性时退回 XGetInputFocus，并上溯到顶层窗口。
        .or_else(|| {
            conn.get_input_focus()
                .ok()?
                .reply()
                .ok()
                .map(|reply| reply.focus)
                .and_then(|focus| top_level_window(&conn, root, focus))
        })?;

    let (instance, class) = read_wm_class(&conn, window).unwrap_or((None, None));
    let title = read_text_property(&conn, window, b"_NET_WM_NAME")
        .or_else(|| read_text_property(&conn, window, b"WM_NAME"));
    let pid = get_u32_property(&conn, window, net_wm_pid, AtomEnum::CARDINAL)
        .and_then(|values| values.first().copied())
        .filter(|pid| *pid != 0);

    Some(X11ActiveWindow {
        window: window as u64,
        instance,
        class,
        title,
        pid,
    })
}

/// 请窗口管理器把焦点切换给 `window`。
pub fn activate_window(window: u64) -> Result<(), String> {
    let (conn, screen_num) = connect().map_err(|e| e.to_string())?;
    let root = conn
        .setup()
        .roots
        .get(screen_num)
        .map(|s| s.root)
        .ok_or_else(|| "无法获取 X11 根窗口".to_string())?;
    let net_active = intern(&conn, b"_NET_ACTIVE_WINDOW").map_err(|e| e.to_string())?;
    let net_current = intern(&conn, b"_NET_CURRENT_DESKTOP").map_err(|e| e.to_string())?;
    let target: Window = window
        .try_into()
        .map_err(|_| "窗口 id 超出 X11 范围".to_string())?;

    // 1. 直接写根窗口属性：对不实现 EWMH 的窗口管理器也有效。
    conn.change_property32(
        PropMode::REPLACE,
        root,
        net_active,
        AtomEnum::WINDOW,
        &[target],
    )
    .map_err(|e| e.to_string())?;

    // 2. 发送 EWMH 客户端消息：source indication = 2（分页器/用户显式请求）。
    let event = ClientMessageEvent::new(32, target, net_active, [2, 0, 0, 0, 0]);
    conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )
    .map_err(|e| e.to_string())?;

    // 3. 把目标窗口切到当前桌面，避免焦点发给其他桌面上的窗口。
    let _ = conn.get_property(false, root, net_current, AtomEnum::CARDINAL, 0, 1);
    conn.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// X11 服务器是否提供 XTEST 扩展。
///
/// 自动粘贴依赖 XTEST 注入按键（研究 §4.3），因此能力探测必须先问服务器，而不是假定
/// 「有 DISPLAY 就能注入」：没有该扩展时如实报告不支持，而不是在用户按下回车后才失败。
pub fn xtest_available() -> bool {
    let Ok((conn, _screen)) = connect() else {
        return false;
    };
    matches!(
        conn.extension_information(XTEST_EXTENSION).ok().flatten(),
        Some(_)
    )
}

/// XTEST 扩展名。这里不引入 x11rb 的 `xtest` feature：本模块只查询扩展是否存在，
/// 真正的注入由合成输入库完成。
const XTEST_EXTENSION: &str = "XTEST";

/// X11 诊断信息，供真实平台检查使用。
pub fn diagnostics() -> X11Diagnostics {
    let (conn, screen_num) = match connect() {
        Ok(value) => value,
        Err(error) => {
            return X11Diagnostics {
                availability: X11Availability::ConnectFailed(error.to_string()),
                active: None,
                window_manager: None,
                client_count: None,
            }
        }
    };
    let Some(root) = conn.setup().roots.get(screen_num).map(|s| s.root) else {
        return X11Diagnostics {
            availability: X11Availability::ConnectFailed("无法获取根窗口".to_string()),
            active: None,
            window_manager: None,
            client_count: None,
        };
    };
    let client_count = intern(&conn, b"_NET_CLIENT_LIST")
        .ok()
        .and_then(|atom| get_window_property(&conn, root, atom))
        .map(|windows| windows.len());
    let window_manager = intern(&conn, b"_NET_SUPPORTING_WM_CHECK")
        .ok()
        .and_then(|atom| get_window_property(&conn, root, atom))
        .and_then(|windows| windows.first().copied())
        .filter(|w| *w != 0)
        .and_then(|check| read_text_property(&conn, check, b"_NET_WM_NAME"));
    X11Diagnostics {
        availability: X11Availability::Available,
        active: active_window(),
        window_manager,
        client_count,
    }
}

fn intern(conn: &impl Connection, name: &[u8]) -> Result<u32, x11rb::errors::ReplyError> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

/// 读取类型为 `WINDOW` 的属性（`_NET_ACTIVE_WINDOW`、`_NET_CLIENT_LIST` 等）。
fn get_window_property(conn: &impl Connection, window: Window, atom: u32) -> Option<Vec<u32>> {
    get_u32_property(conn, window, atom, AtomEnum::WINDOW)
}

/// 读取 32 位属性，`requested_type` 必须与属性实际类型一致，否则回复为空。
fn get_u32_property(
    conn: &impl Connection,
    window: Window,
    atom: u32,
    requested_type: impl Into<Atom>,
) -> Option<Vec<u32>> {
    let reply = conn
        .get_property(false, window, atom, requested_type, 0, 1024)
        .ok()?
        .reply()
        .ok()?;
    Some(
        reply
            .value32()
            .map(|iter| iter.collect::<Vec<u32>>())
            .unwrap_or_default(),
    )
}

fn read_text_property(conn: &impl Connection, window: Window, name: &[u8]) -> Option<String> {
    let atom = intern(conn, name).ok()?;
    let reply = conn
        .get_property(false, window, atom, AtomEnum::ANY, 0, 1024)
        .ok()?
        .reply()
        .ok()?;
    if reply.value.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&reply.value)
        .trim_end_matches('\0')
        .to_string();
    (!text.is_empty()).then_some(text)
}

fn read_wm_class(
    conn: &impl Connection,
    window: Window,
) -> Option<(Option<String>, Option<String>)> {
    let atom = intern(conn, b"WM_CLASS").ok()?;
    let reply = conn
        .get_property(false, window, atom, AtomEnum::STRING, 0, 1024)
        .ok()?
        .reply()
        .ok()?;
    let mut parts = reply
        .value
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned());
    let instance = parts.next();
    let class = parts.next();
    Some((instance, class))
}

/// 从 `window` 沿父链上溯，直到直接挂在根窗口下的顶层窗口。
fn top_level_window(conn: &impl Connection, root: Window, window: Window) -> Option<Window> {
    let mut current = window;
    for _ in 0..32 {
        if current == 0 || current == root {
            return None;
        }
        let tree = conn.query_tree(current).ok()?.reply().ok()?;
        if tree.parent == root {
            return Some(current);
        }
        current = tree.parent;
    }
    None
}
