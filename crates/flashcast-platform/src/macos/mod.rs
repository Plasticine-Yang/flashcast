//! macOS 平台适配层实现。
//!
//! 模块划分镜像 Linux 侧：**纯逻辑**（应用包遍历与 `Info.plist` 解析）放在
//! [`bundle`]，不依赖任何 Apple API，因此它的夹具测试在 Linux 开发机与任意
//! CI runner 上都会真实执行；只有真正的 macOS 集成（图标渲染、启动、焦点、
//! 辅助功能权限、快捷键、能力探测）按 `cfg(target_os = "macos")` 条件编译，
//! 由 macOS runner 上的 `flashcast-platform-check` 报告。
//!
//! 本机是 Linux，无法运行 macOS 代码；所有 macOS 专有行为在 ticket 记录中
//! 一律标记为「未覆盖」，编译通过不作为行为证据。

pub mod bundle;

#[cfg(target_os = "macos")]
pub mod cap;
#[cfg(target_os = "macos")]
pub mod catalog;
#[cfg(target_os = "macos")]
pub mod focus;
#[cfg(target_os = "macos")]
pub mod hotkeys;
#[cfg(target_os = "macos")]
pub mod icons;
#[cfg(target_os = "macos")]
pub mod launcher;

#[cfg(target_os = "macos")]
pub use bundle::{default_roots_from_env, ScanOptions, ScanOutcome};
#[cfg(target_os = "macos")]
pub use cap::MacosCapabilityProbe;
#[cfg(target_os = "macos")]
pub use catalog::MacosAppCatalog;
#[cfg(target_os = "macos")]
pub use focus::{accessibility, MacosFocusTracker};
#[cfg(target_os = "macos")]
pub use hotkeys::MacosHotkeyManager;
#[cfg(target_os = "macos")]
pub use launcher::MacosLauncher;
