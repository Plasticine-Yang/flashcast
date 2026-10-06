//! GNOME Shell 扩展的剪贴板桥接。会话 D-Bus 传递有界载荷，不创建窗口。
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use gio::glib::variant::ToVariant;
use gio::glib::Variant;

use crate::clipboard::{
    check_files, file_entries, image_from_bytes, parse_uri_list, ClipboardCapture, ClipboardError,
    ClipboardFormatKind, ClipboardPoll, ClipboardSourceApp, IMAGE_MIME_PNG, MAX_IMAGE_BYTES,
    MAX_TEXT_BYTES,
};

const SERVICE: &str = "org.gnome.Shell.Extensions.FlashcastClipboard";
const OBJECT: &str = "/org/gnome/Shell/Extensions/FlashcastClipboard";
type Reply = (
    String,
    u64,
    HashMap<String, Vec<u8>>,
    String,
    String,
    String,
);

#[derive(Default)]
pub(super) struct GnomeClipboard {
    cursor: Mutex<(String, u64)>,
    paused: AtomicBool,
}

fn call(method: &str, parameters: Option<&Variant>) -> Result<Variant, ClipboardError> {
    let bus = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
        .map_err(|e| ClipboardError::ReadFailed(format!("无法连接会话 D-Bus：{e}")))?;
    bus.call_sync(
        Some(SERVICE), OBJECT, SERVICE, method, parameters, None,
        gio::DBusCallFlags::NO_AUTO_START, 1500, None::<&gio::Cancellable>,
    ).map_err(|_| ClipboardError::Unsupported {
        reason: "GNOME 剪贴板桥接未运行。请安装并启用 Flashcast Clipboard Bridge 扩展；首次安装后重新登录，再重启 Flashcast。安装命令：scripts/gnome/install-clipboard-bridge.sh。已有历史仍可使用。".into(),
    })
}

impl GnomeClipboard {
    pub fn available(&self) -> bool {
        call("Ping", None).ok().and_then(|v| v.get::<(u32,)>()) == Some((1,))
    }

    pub fn poll(&self) -> Result<ClipboardPoll, ClipboardError> {
        if self.paused.load(Ordering::SeqCst) {
            return Ok(ClipboardPoll::Unchanged);
        }
        let mut cursor = self.cursor.lock().unwrap_or_else(|p| p.into_inner());
        let reply: Reply = call("Poll", Some(&cursor.clone().to_variant()))?
            .get()
            .ok_or_else(|| ClipboardError::ReadFailed("GNOME 剪贴板桥接响应格式不兼容".into()))?;
        // 暂停请求可能与一次 IPC 并发。暂停时即使已返回数据也不消费它。
        if self.paused.load(Ordering::SeqCst) {
            return Ok(ClipboardPoll::Unchanged);
        }
        let (epoch, sequence, payloads, app_id, app_name, problem) = reply;
        if *cursor == (epoch.clone(), sequence) {
            return Ok(ClipboardPoll::Unchanged);
        }
        *cursor = (epoch, sequence);
        capture(payloads, app_id, app_name, problem)
    }

    pub fn stop(&self) {
        // 与 poll 共用锁：Stop 必须排在已发出的 Poll 后面，防止暂停后重新续租。
        let _cursor = self.cursor.lock().unwrap_or_else(|p| p.into_inner());
        let _ = call("Stop", None);
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }
}

fn text(
    payloads: &HashMap<String, Vec<u8>>,
    mimes: &[&str],
) -> Result<Option<String>, ClipboardError> {
    let Some(bytes) = mimes.iter().find_map(|mime| payloads.get(*mime)) else {
        return Ok(None);
    };
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(ClipboardError::ReadFailed(
            "GNOME 剪贴板文本超过 4 MiB，未保存".into(),
        ));
    }
    String::from_utf8(bytes.clone())
        .map(|s| (!s.is_empty()).then_some(s))
        .map_err(|_| ClipboardError::ReadFailed("GNOME 剪贴板文本不是有效 UTF-8".into()))
}

fn capture(
    payloads: HashMap<String, Vec<u8>>,
    app_id: String,
    app_name: String,
    problem: String,
) -> Result<ClipboardPoll, ClipboardError> {
    let source = (!app_id.is_empty() || !app_name.is_empty()).then_some(ClipboardSourceApp {
        app_id,
        title: (!app_name.is_empty()).then_some(app_name),
    });
    if let Some(uris) = text(
        &payloads,
        &["text/uri-list", "x-special/gnome-copied-files"],
    )? {
        let paths = parse_uri_list(&uris);
        if !paths.is_empty() {
            check_files(&paths)?;
            return Ok(ClipboardPoll::Changed(ClipboardCapture {
                formats: vec![ClipboardFormatKind::Files],
                text: None,
                image: None,
                image_problem: (!problem.is_empty()).then_some(problem),
                html: None,
                rtf: None,
                files: file_entries(&paths),
                source,
            }));
        }
    }
    let text = text(
        &payloads,
        &["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"],
    )?;
    // 富文本尽力读取，与 wl-paste 后端保持一致；单个富文本格式失败不丢掉有效正文。
    let html = self::text(&payloads, &["text/html"]).ok().flatten();
    let rtf = self::text(&payloads, &["text/rtf", "application/rtf"])
        .ok()
        .flatten();
    let (image, image_problem) = match payloads.get(IMAGE_MIME_PNG) {
        Some(bytes) if bytes.len() > MAX_IMAGE_BYTES => {
            (None, Some("PNG 超过 16 MiB，未保存".into()))
        }
        Some(bytes) => image_from_bytes(IMAGE_MIME_PNG, Some(bytes.clone())),
        None => (None, None),
    };
    let image_problem = if problem.is_empty() {
        image_problem
    } else {
        Some(match image_problem {
            Some(image) => format!("{problem} {image}"),
            None => problem,
        })
    };
    let mut formats = Vec::new();
    if text.is_some() {
        formats.push(ClipboardFormatKind::Text);
    }
    if html.is_some() {
        formats.push(ClipboardFormatKind::Html);
    }
    if rtf.is_some() {
        formats.push(ClipboardFormatKind::Rtf);
    }
    if image.is_some() {
        formats.push(ClipboardFormatKind::Image);
    }
    if formats.is_empty() && image_problem.is_none() {
        return Ok(ClipboardPoll::Unchanged);
    }
    Ok(ClipboardPoll::Changed(ClipboardCapture {
        formats,
        text,
        image,
        image_problem,
        html,
        rtf,
        files: Vec::new(),
        source,
    }))
}
