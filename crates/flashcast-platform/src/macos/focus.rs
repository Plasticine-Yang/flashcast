//! macOS 焦点追踪：`NSWorkspace.frontmostApplication` 采集，
//! `activateWithOptions:` / `activateFromApplication:options:` 恢复。
//!
//! 采集时机与 X11 一样关键：`frontmostApplication` 必须在启动器窗口显示**之前**
//! 读取，否则读到的就是 Flashcast 自己。宿主的 `src-tauri/src/summon.rs` 已经是
//! 「先 capture，再 show/set_focus」，在 macOS 上同样正确。
//!
//! 关于强制激活：`NSApplicationActivateIgnoringOtherApps` 在 macOS 14 起**已废弃
//! 且无效**，不能依赖它。可用的手段只有两种：
//! - macOS 14+ 的 `activateFromApplication:options:` —— 由当前前台应用（也就是
//!   Flashcast 自己）把焦点交还回去，正是本产品的场景；
//! - 空 options 的 `activateWithOptions:`，由系统决定，配合「请求时我们就是前台
//!   应用」这一事实通常可以成功。
//!
//! 另外：向其他应用注入按键（自动粘贴）需要「辅助功能」权限，因此本模块把权限
//! 状态与设置入口一并暴露出来，供 UI 引导用户，而不是让粘贴静默失败。

use crate::focus::FocusedApp;

#[cfg(target_os = "macos")]
use crate::focus::{FocusError, FocusTracker};
#[cfg(target_os = "macos")]
use std::path::Path;

/// 辅助功能权限在系统设置中的位置（面向用户的中文说明）。
pub const ACCESSIBILITY_SETTINGS_LABEL: &str = "系统设置 → 隐私与安全性 → 辅助功能";

/// 直接打开辅助功能权限面板的 URL。
pub const ACCESSIBILITY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/// 恢复焦点时的查找依据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreTarget {
    /// 优先按进程号：同一个 bundle id 可能有多个实例。
    Pid(i32),
    /// 没有 pid 时按 bundle id 查找。
    BundleId(String),
}

/// 从采集到的应用身份决定恢复策略。纯函数，便于在任意平台验证。
pub fn restore_target(app: &FocusedApp) -> Option<RestoreTarget> {
    if let Some(pid) = app.pid.and_then(|pid| i32::try_from(pid).ok()) {
        if pid > 0 {
            return Some(RestoreTarget::Pid(pid));
        }
    }
    app.wm_class
        .as_deref()
        .filter(|class| !class.is_empty())
        .map(|class| RestoreTarget::BundleId(class.to_string()))
}

#[cfg(target_os = "macos")]
pub struct MacosFocusTracker;

#[cfg(target_os = "macos")]
impl Default for MacosFocusTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl MacosFocusTracker {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
impl FocusTracker for MacosFocusTracker {
    fn capture(&self) -> Result<FocusedApp, FocusError> {
        use objc2_app_kit::NSWorkspace;

        let workspace = NSWorkspace::sharedWorkspace();
        let Some(frontmost) = workspace.frontmostApplication() else {
            return Err(FocusError::NoActiveWindow);
        };

        let name = frontmost
            .localizedName()
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty());
        let bundle_id = frontmost
            .bundleIdentifier()
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty());
        let executable = frontmost
            .executableURL()
            .and_then(|url| url.path())
            .map(|value| value.to_string());
        let pid = u32::try_from(frontmost.processIdentifier())
            .ok()
            .filter(|pid| *pid > 0);

        let id = bundle_id
            .clone()
            .or_else(|| executable.clone())
            .or_else(|| name.clone())
            .ok_or(FocusError::NoActiveWindow)?;
        Ok(FocusedApp {
            name: name.unwrap_or_else(|| id.clone()),
            id,
            wm_class: bundle_id,
            pid,
            // `NSRunningApplication` 不是可持久化的句柄；恢复时按 pid / bundle id
            // 重新查找，因此这里不使用 platform-native 窗口句柄。
            window: None,
        })
    }

    fn restore(&self, app: &FocusedApp) -> Result<(), FocusError> {
        use objc2::rc::Retained;
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
        use objc2_foundation::NSString;

        let target = restore_target(app).ok_or_else(|| FocusError::Unavailable {
            reason: "唤起前的应用身份既没有 pid 也没有 bundle id，无法恢复焦点".to_string(),
        })?;

        let running: Option<Retained<NSRunningApplication>> = match target {
            RestoreTarget::Pid(pid) => {
                NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            }
            RestoreTarget::BundleId(bundle_id) => {
                let identifier = NSString::from_str(&bundle_id);
                let applications =
                    NSRunningApplication::runningApplicationsWithBundleIdentifier(&identifier);
                applications
                    .to_vec()
                    .into_iter()
                    .find(|candidate| !candidate.isTerminated())
            }
        };
        let Some(running) = running else {
            return Err(FocusError::Unavailable {
                reason: format!("唤起前的应用（{}）已退出，无法恢复焦点", app.name),
            });
        };
        if running.isTerminated() {
            return Err(FocusError::Unavailable {
                reason: format!("唤起前的应用（{}）已退出，无法恢复焦点", app.name),
            });
        }
        if running.isActive() {
            return Ok(());
        }
        if running.isHidden() {
            // 激活一个隐藏应用没有可见效果，先取消隐藏。
            running.unhide();
        }

        // 空 options：由系统决定是否接受这次激活请求。不能依赖
        // NSApplicationActivateIgnoringOtherApps —— 它在 macOS 14+ 是空操作。
        let options = NSApplicationActivationOptions(0);
        let mut activated = running.activateWithOptions(options);
        if !activated && objc2::available!(macos = 14.0) {
            // macOS 14+ 的正规做法：当前前台应用（Flashcast）把焦点交还回去。
            let current = NSRunningApplication::currentApplication();
            activated = running.activateFromApplication_options(&current, options);
        }
        if activated {
            Ok(())
        } else {
            Err(FocusError::Unavailable {
                reason: format!(
                    "系统拒绝了把焦点还给「{}」的请求（macOS 14+ 已移除强制激活标志）",
                    app.name
                ),
            })
        }
    }
}

/// 当前进程是否已获得「辅助功能」权限。
#[cfg(target_os = "macos")]
pub fn accessibility_granted() -> bool {
    use objc2_application_services::AXIsProcessTrusted;

    unsafe { AXIsProcessTrusted() }
}

/// 请求辅助功能权限：系统会异步弹出授权提示，返回值仍是**当前**状态。
#[cfg(target_os = "macos")]
pub fn accessibility_prompt() -> bool {
    use objc2_application_services::{kAXTrustedCheckOptionPrompt, AXIsProcessTrustedWithOptions};
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFType};

    unsafe {
        let key: &CFType = kAXTrustedCheckOptionPrompt.as_ref();
        let value: &CFType = CFBoolean::new(true).as_ref();
        let options = CFDictionary::<CFType, CFType>::from_slices(&[key], &[value]);
        AXIsProcessTrustedWithOptions(Some(options.as_ref()))
    }
}

/// 打开「隐私与安全性 → 辅助功能」面板，供 UI 引导用户授权。
#[cfg(target_os = "macos")]
pub fn open_accessibility_settings() -> Result<(), String> {
    let status = std::process::Command::new(super::launcher::OPEN_BIN)
        .arg(ACCESSIBILITY_SETTINGS_URL)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| format!("无法打开辅助功能设置面板：{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "无法打开辅助功能设置面板（open 退出码 {:?}），请手动进入 {ACCESSIBILITY_SETTINGS_LABEL}",
            status.code()
        ))
    }
}

/// 该路径是否指向一个正在运行的应用（诊断用）。
#[cfg(target_os = "macos")]
pub fn is_running_bundle(bundle: &Path) -> bool {
    use objc2_app_kit::NSWorkspace;

    let text = bundle.to_string_lossy();
    let workspace = NSWorkspace::sharedWorkspace();
    let applications = workspace.runningApplications();
    applications.to_vec().into_iter().any(|application| {
        application
            .bundleURL()
            .and_then(|url| url.path())
            .map(|path| path.to_string() == text)
            .unwrap_or(false)
    })
}
