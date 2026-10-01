//! Windows 平台适配层实现。
//!
//! 本模块分两层：
//!
//! - **纯逻辑**（`start_menu`、`shell_link`、`registry`、`identity`、`uwp`、
//!   `icons`、`launch_plan`、`version`、`chrome` 的路径推导）不触碰操作系统，
//!   在所有目标上编译，并由 `tests/windows_fixture.rs` 在 Linux 上以真实夹具验证；
//! - **系统调用层**（`catalog`、`launcher`、`focus`、`hotkeys`、`cap`、`session`）
//!   只在 `cfg(target_os = "windows")` 下编译，只能在 Windows runner 上被真实检查覆盖。
//!
//! 这条分界是刻意的：本仓库的开发机是 Linux，无法执行任何 Windows API，因此凡是
//! 能写成纯函数的判断（注册表过滤表、`.lnk` 字段映射、启动计划、稳定标识、图标
//! 反预乘、Chrome 候选路径）都必须落在纯逻辑一侧，才能在提交前被本地测试覆盖。

pub mod chrome;
pub mod icons;
pub mod identity;
pub mod launch_plan;
pub mod registry;
pub mod shell_link;
pub mod start_menu;
pub mod uwp;
pub mod version;

#[cfg(target_os = "windows")]
pub mod cap;
#[cfg(target_os = "windows")]
pub mod catalog;
#[cfg(target_os = "windows")]
pub mod clipboard;
#[cfg(target_os = "windows")]
pub mod focus;
#[cfg(target_os = "windows")]
pub mod hotkeys;
#[cfg(target_os = "windows")]
pub mod launcher;
#[cfg(target_os = "windows")]
pub mod paste;
#[cfg(target_os = "windows")]
pub mod session;

#[cfg(target_os = "windows")]
pub use cap::WindowsCapabilityProbe;
#[cfg(target_os = "windows")]
pub use catalog::WindowsAppCatalog;
#[cfg(target_os = "windows")]
pub use chrome::WindowsChromeProvider;
#[cfg(target_os = "windows")]
pub use clipboard::{WindowsClipboard, WindowsClipboardWatcher};
#[cfg(target_os = "windows")]
pub use focus::WindowsFocusTracker;
#[cfg(target_os = "windows")]
pub use hotkeys::WindowsHotkeyManager;
#[cfg(target_os = "windows")]
pub use launcher::WindowsLauncher;
#[cfg(target_os = "windows")]
pub use paste::WindowsPaster;
