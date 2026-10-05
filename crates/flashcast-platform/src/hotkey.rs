//! 全局快捷键的规格解析。纯函数，不依赖任何平台后端，便于在没有桌面会话的
//! 环境中测试，也保证各平台实现与测试替身使用同一套语法。

use serde::{Deserialize, Serialize};

/// 修饰键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Modifier {
    Control,
    Alt,
    Shift,
    Super,
}

impl Modifier {
    pub fn label(self) -> &'static str {
        match self {
            Modifier::Control => "Ctrl",
            Modifier::Alt => "Alt",
            Modifier::Shift => "Shift",
            Modifier::Super => "Super",
        }
    }
}

/// 快捷键的主键。只覆盖启动器实际会用的键位，避免引入未被使用的映射表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Key {
    /// `a`–`z`（小写规范形式）。
    Letter(char),
    /// `0`–`9`。
    Digit(u8),
    Space,
    Enter,
    Escape,
    Tab,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    /// `F1`–`F24`。
    Function(u8),
}

impl Key {
    /// 规范名称，用于 [`HotkeySpec::canonical`]。
    pub fn canonical(self) -> String {
        match self {
            Key::Letter(c) => c.to_ascii_uppercase().to_string(),
            Key::Digit(d) => d.to_string(),
            Key::Space => "Space".to_string(),
            Key::Enter => "Enter".to_string(),
            Key::Escape => "Escape".to_string(),
            Key::Tab => "Tab".to_string(),
            Key::ArrowUp => "ArrowUp".to_string(),
            Key::ArrowDown => "ArrowDown".to_string(),
            Key::ArrowLeft => "ArrowLeft".to_string(),
            Key::ArrowRight => "ArrowRight".to_string(),
            Key::Function(n) => format!("F{n}"),
        }
    }
}

/// 已解析的快捷键规格。
///
/// `modifiers` 已去重并按固定顺序排列，`canonical()` 为可稳定比较与持久化的形式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeySpec {
    /// 用户输入的原始形式，原样保留以便设置界面回显。
    pub raw: String,
    pub modifiers: Vec<Modifier>,
    pub key: Key,
}

impl HotkeySpec {
    /// 解析 `Ctrl+Alt+Space`、`Super+Space`、`Ctrl+Shift+A` 这类写法。
    ///
    /// 规则：
    /// - 以 `+` 分隔，最后一段是主键，其余是修饰键；
    /// - 至少需要一个修饰键，避免注册会吞掉普通输入的全局快捷键；
    /// - 修饰键别名：`Ctrl`/`Control`/`Ctl`、`Alt`/`Option`/`Opt`、`Shift`、`Super`/`Meta`/`Cmd`/`Command`/`Win`；
    /// - `CmdOrCtrl` 解析为 `Super`（本产品在 macOS 上以 Command 为准）。
    pub fn parse(raw: &str) -> Result<Self, HotkeySpecError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(HotkeySpecError::Empty);
        }
        let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err(HotkeySpecError::Malformed {
                raw: trimmed.to_string(),
            });
        }
        let (key_part, modifier_parts) = parts.split_last().expect("split 至少产生一段");

        let key = parse_key(key_part).ok_or_else(|| HotkeySpecError::UnknownKey {
            token: (*key_part).to_string(),
        })?;

        let mut modifiers = Vec::new();
        for part in modifier_parts {
            let modifier = parse_modifier(part).ok_or_else(|| HotkeySpecError::UnknownModifier {
                token: (*part).to_string(),
            })?;
            if !modifiers.contains(&modifier) {
                modifiers.push(modifier);
            }
        }
        if modifiers.is_empty() {
            return Err(HotkeySpecError::MissingModifier {
                raw: trimmed.to_string(),
            });
        }
        modifiers.sort();

        Ok(HotkeySpec {
            raw: trimmed.to_string(),
            modifiers,
            key,
        })
    }

    /// 规范字符串，例如 `Ctrl+Alt+Space`。
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        for modifier in &self.modifiers {
            out.push_str(modifier.label());
            out.push('+');
        }
        out.push_str(&self.key.canonical());
        out
    }

    pub fn has_modifier(&self, modifier: Modifier) -> bool {
        self.modifiers.contains(&modifier)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeySpecError {
    #[error("快捷键不能为空")]
    Empty,
    #[error("快捷键格式无法识别：{raw}")]
    Malformed { raw: String },
    #[error("无法识别的修饰键：{token}")]
    UnknownModifier { token: String },
    #[error("无法识别的主键：{token}")]
    UnknownKey { token: String },
    #[error("全局快捷键至少需要一个修饰键（Ctrl / Alt / Super / Shift）：{raw}")]
    MissingModifier { raw: String },
}

fn parse_modifier(token: &str) -> Option<Modifier> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "ctl" => Some(Modifier::Control),
        "alt" | "option" | "opt" => Some(Modifier::Alt),
        "shift" => Some(Modifier::Shift),
        // CmdOrCtrl 在 macOS 上表示 Command；Super 在其他平台表示 Win 键。
        "super" | "meta" | "cmd" | "command" | "cmdorctrl" | "win" | "windows" => {
            Some(Modifier::Super)
        }
        _ => None,
    }
}

fn parse_key(token: &str) -> Option<Key> {
    let lower = token.to_ascii_lowercase();
    match lower.as_str() {
        "space" | "spacebar" => return Some(Key::Space),
        "enter" | "return" => return Some(Key::Enter),
        "esc" | "escape" => return Some(Key::Escape),
        "tab" => return Some(Key::Tab),
        "up" | "arrowup" => return Some(Key::ArrowUp),
        "down" | "arrowdown" => return Some(Key::ArrowDown),
        "left" | "arrowleft" => return Some(Key::ArrowLeft),
        "right" | "arrowright" => return Some(Key::ArrowRight),
        _ => {}
    }
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u8>() {
            if (1..=24).contains(&n) {
                return Some(Key::Function(n));
            }
        }
    }
    let mut chars = lower.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_lowercase() => Some(Key::Letter(c)),
        (Some(c), None) if c.is_ascii_digit() => Some(Key::Digit(c.to_digit(10)? as u8)),
        _ => None,
    }
}

/// 默认全局快捷键：Alt + 空格（macOS 对应 Option + 空格）。
pub const DEFAULT_HOTKEY: &str = "Alt+Space";
