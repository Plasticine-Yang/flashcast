//! macOS 平台适配层实现。
//!
//! 模块划分镜像 Linux 侧：**纯逻辑**（应用包遍历、`Info.plist` 解析、启动参数
//! 拼装、焦点恢复策略、能力结论）不依赖任何 Apple API，因此它的夹具测试在 Linux
//! 开发机与任意 CI runner 上都会真实执行；只有真正的 macOS 集成（图标渲染、
//! 进程启动、焦点激活、辅助功能权限、快捷键后端）按 `cfg(target_os = "macos")`
//! 条件编译，由 macOS runner 上的 `flashcast-platform-check` 报告。
//!
//! 本机是 Linux，无法运行也不能执行 macOS 代码；所有 macOS 专有行为在 ticket
//! 记录中一律标记为「未覆盖」，编译通过不作为行为证据。

pub mod bundle;
pub mod cap;
pub mod chrome;
pub mod clipboard;
pub mod focus;
pub mod hotkeys;
pub mod icons;
pub mod launcher;
pub mod paste;

#[cfg(target_os = "macos")]
pub mod catalog;

#[cfg(target_os = "macos")]
pub use cap::MacosCapabilityProbe;
#[cfg(target_os = "macos")]
pub use catalog::MacosAppCatalog;
#[cfg(target_os = "macos")]
pub use chrome::MacosChromeProvider;
pub use clipboard::{MacosClipboard, MacosClipboardWatcher};
#[cfg(target_os = "macos")]
pub use focus::{accessibility_granted, MacosFocusTracker};
#[cfg(target_os = "macos")]
pub use hotkeys::MacosHotkeyManager;
#[cfg(target_os = "macos")]
pub use launcher::MacosLauncher;
#[cfg(target_os = "macos")]
pub use paste::MacosPaster;
