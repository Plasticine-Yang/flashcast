//! freedesktop（XDG）相关实现。
//!
//! 该模块刻意不依赖 Linux 专有 API，因此可在任意平台上运行解析与扫描逻辑，
//! 让 `.desktop` 解析的行为在无桌面会话的 CI runner 上也能被验证。

pub mod desktop_entry;
pub mod icons;
pub mod scan;
pub mod xdg;

pub use desktop_entry::{
    desktop_file_id, locale_candidates_from_env, parse_desktop_entry, parse_exec, tokenize_exec,
    DesktopEntry, ParsedDesktop,
};
pub use icons::IconIndex;
pub use scan::{is_executable, scan_desktop_dirs, ScanOptions, ScanOutcome, SkipReason, Skipped};
pub use xdg::XdgDirs;
