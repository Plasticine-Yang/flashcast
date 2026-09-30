//! 主题插件：声明式数据（JSON），**不执行任何主题代码**（ADR §7）。
//!
//! 一个主题文档定义统一语义 token：颜色、字体、间距、圆角、阴影与状态语义
//! （选中、焦点、错误、禁用）。宿主把文档解析成一组语义值，再映射为 CSS 自定义
//! 属性交给 UI（[`ThemeTokens::css_vars`]）；UI 只消费属性，不认识任何具体主题。
//!
//! 主题的加载路径是「插件清单条目 → 主题文档」，见 [`crate::manifest`]：
//! 默认提供的浅色、深色与跟随系统同样是清单里的条目，不存在按 id 硬编码的分支。
//!
//! ## 为什么间距与字号被钉住
//!
//! token 里包含间距与字号，但 [`ThemeTokens::validate`] 要求它们等于宿主的基准几何。
//! 「切换主题不改变关键控件的位置与布局」因此是**结构保证**，而不是靠每个主题自觉：
//! 主题可以换颜色、字体族、圆角与阴影，不能挪动控件。README / ticket 06 的注释里
//! 记录了这条取舍。
//!
//! ## 无效主题
//!
//! 解析或校验失败时保留上一次可用外观，并把中文原因交给调用方
//! （[`ThemeError`]）；宿主不会因为一个坏主题而失去可用的界面。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 主题文档的格式版本。
pub const THEME_SCHEMA_VERSION: u32 = 1;

/// 内置浅色主题的标识。
pub const THEME_LIGHT: &str = "flashcast.theme.light";
/// 内置深色主题的标识。
pub const THEME_DARK: &str = "flashcast.theme.dark";
/// 内置「跟随系统」主题的标识。
pub const THEME_SYSTEM: &str = "flashcast.theme.system";

/// 内置主题标识前缀：这些主题随应用提供，不能被移除。
pub const BUILTIN_THEME_PREFIX: &str = "flashcast.theme.";

/// 基准字号：正文。主题不得更改。
pub const CANONICAL_FONT_BODY: &str = "14px";
/// 基准字号：搜索输入。主题不得更改。
pub const CANONICAL_FONT_INPUT: &str = "16px";
/// 基准字号：辅助信息。主题不得更改。
pub const CANONICAL_FONT_AUX: &str = "12px";
/// 基准间距：窗口左右留白。主题不得更改。
pub const CANONICAL_SPACE_WINDOW_PADDING: &str = "14px";
/// 基准间距：列表项纵向内边距。主题不得更改。
pub const CANONICAL_SPACE_ROW_PADDING: &str = "6px";
/// 基准间距：列表项图标与正文间距。主题不得更改。
pub const CANONICAL_SPACE_ROW_GAP: &str = "10px";
/// 基准间距：设置页分区间距。主题不得更改。
pub const CANONICAL_SPACE_SECTION_GAP: &str = "8px";
/// 基准行高。主题不得更改。
pub const CANONICAL_ROW_HEIGHT: &str = "44px";

/// 圆角允许的最小值（px）。
pub const MIN_RADIUS_PX: f64 = 0.0;
/// 圆角允许的最大值（px）。超过它会把窗口圆角切掉内容。
pub const MAX_RADIUS_PX: f64 = 24.0;
/// 阴影字符串的最大长度，避免主题塞进任意长文本。
pub const MAX_SHADOW_LEN: usize = 160;

/// 实际生效的外观。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    pub fn as_str(self) -> &'static str {
        match self {
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }

    pub fn label_zh(self) -> &'static str {
        match self {
            Appearance::Light => "浅色",
            Appearance::Dark => "深色",
        }
    }
}

/// 主题声明的外观偏好。「跟随系统」在解析时按当前系统外观落定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeAppearance {
    Light,
    Dark,
    System,
}

impl ThemeAppearance {
    pub fn label_zh(self) -> &'static str {
        match self {
            ThemeAppearance::Light => "浅色",
            ThemeAppearance::Dark => "深色",
            ThemeAppearance::System => "跟随系统",
        }
    }
}

/// 主题相关失败的中文原因。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("主题无效：{0}")]
    Invalid(String),
    #[error("找不到主题：{0}")]
    Unknown(String),
    #[error("主题「{0}」已停用，请先启用后再选择")]
    Disabled(String),
    #[error("内置主题不能{0}：{1}")]
    Builtin(&'static str, String),
    #[error("尚未关联配置工作区，无法{0}主题包")]
    NoWorkspace(&'static str),
    #[error("主题包不存在：{}", .0.display())]
    PackageNotFound(PathBuf),
    #[error("主题配置写入失败：{0}")]
    Workspace(String),
}

impl ThemeError {
    pub fn invalid(reason: impl Into<String>) -> Self {
        ThemeError::Invalid(reason.into())
    }
}

/// 一个已解析的 CSS 自定义属性。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CssVar {
    pub name: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// 颜色：解析、合成与对比度
// ---------------------------------------------------------------------------

/// 一个 sRGB 颜色（含 alpha）。用于可读性与状态可辨识的校验。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    /// 解析 `#rgb` / `#rrggbb` / `#rrggbbaa` / `rgb(...)` / `rgba(...)`。
    ///
    /// 只接受这几种写法：主题校验需要真实算出对比度，「任意 CSS 颜色」无法验证，
    /// 因此无法解析的写法会被拒绝并给出中文原因。
    pub fn parse(text: &str) -> Option<Rgba> {
        let value = text.trim();
        if let Some(hex) = value.strip_prefix('#') {
            return Self::parse_hex(hex);
        }
        let lower = value.to_ascii_lowercase();
        for (prefix, has_alpha) in [("rgba(", true), ("rgb(", false)] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                let body = rest.strip_suffix(')')?;
                return Self::parse_function(body, has_alpha);
            }
        }
        None
    }

    fn parse_hex(hex: &str) -> Option<Rgba> {
        let digits: Vec<u32> = hex
            .chars()
            .map(|c| c.to_digit(16))
            .collect::<Option<Vec<_>>>()?;
        let channel = |value: u32| value as f64 / 255.0;
        match digits.len() {
            3 => Some(Rgba {
                r: channel(digits[0] * 17),
                g: channel(digits[1] * 17),
                b: channel(digits[2] * 17),
                a: 1.0,
            }),
            6 => Some(Rgba {
                r: channel(digits[0] * 16 + digits[1]),
                g: channel(digits[2] * 16 + digits[3]),
                b: channel(digits[4] * 16 + digits[5]),
                a: 1.0,
            }),
            8 => Some(Rgba {
                r: channel(digits[0] * 16 + digits[1]),
                g: channel(digits[2] * 16 + digits[3]),
                b: channel(digits[4] * 16 + digits[5]),
                a: channel(digits[6] * 16 + digits[7]),
            }),
            _ => None,
        }
    }

    fn parse_function(body: &str, has_alpha: bool) -> Option<Rgba> {
        let parts: Vec<&str> = body
            .split(|c| c == ',' || c == '/' || c == ' ')
            .filter(|part| !part.is_empty())
            .collect();
        let expected = if has_alpha { 4 } else { 3 };
        if parts.len() != expected {
            return None;
        }
        let channel = |text: &str| -> Option<f64> {
            let value: f64 = text.trim().parse().ok()?;
            Some((value / 255.0).clamp(0.0, 1.0))
        };
        let alpha = |text: &str| -> Option<f64> {
            let value: f64 = text.trim().parse().ok()?;
            Some(value.clamp(0.0, 1.0))
        };
        Some(Rgba {
            r: channel(parts[0])?,
            g: channel(parts[1])?,
            b: channel(parts[2])?,
            a: if has_alpha { alpha(parts[3])? } else { 1.0 },
        })
    }

    /// 把半透明颜色叠在不透明背景上。背景自身的 alpha 被忽略。
    pub fn composite_over(self, behind: Rgba) -> Rgba {
        let mix = |front: f64, back: f64| front * self.a + back * (1.0 - self.a);
        Rgba {
            r: mix(self.r, behind.r),
            g: mix(self.g, behind.g),
            b: mix(self.b, behind.b),
            a: 1.0,
        }
    }

    fn linearize(channel: f64) -> f64 {
        if channel <= 0.03928 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    /// WCAG 相对亮度。
    pub fn relative_luminance(self) -> f64 {
        0.2126 * Self::linearize(self.r)
            + 0.7152 * Self::linearize(self.g)
            + 0.0722 * Self::linearize(self.b)
    }

    /// WCAG 对比度。两者都应是不透明颜色（先 `composite_over`）。
    pub fn contrast_ratio(self, other: Rgba) -> f64 {
        let a = self.relative_luminance();
        let b = other.relative_luminance();
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// 两个颜色的最大通道差（0..1）。用于判断状态之间是否可辨识。
    pub fn max_channel_delta(self, other: Rgba) -> f64 {
        (self.r - other.r)
            .abs()
            .max((self.g - other.g).abs())
            .max((self.b - other.b).abs())
    }
}

// ---------------------------------------------------------------------------
// 语义 token
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorTokens {
    pub page_background: String,
    pub surface: String,
    pub hover: String,
    pub border: String,
    pub border_strong: String,
    pub text: String,
    pub text_muted: String,
    pub text_disabled: String,
    pub accent: String,
    pub info_background: String,
    pub warning_background: String,
    pub warning_border: String,
    pub warning_text: String,
    pub icon_fallback_background: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontTokens {
    pub family: String,
    pub body: String,
    pub input: String,
    pub aux: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpaceTokens {
    pub window_padding: String,
    pub row_padding: String,
    pub row_gap: String,
    pub section_gap: String,
    pub row_height: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadiusTokens {
    pub window: String,
    pub item: String,
    pub control: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShadowTokens {
    pub window: String,
    pub overlay: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedState {
    pub background: String,
    pub border: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FocusState {
    pub ring: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorState {
    pub background: String,
    pub border: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisabledState {
    pub text: String,
    pub opacity: f64,
}

/// 状态语义：选中、焦点、错误、禁用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateTokens {
    pub selected: SelectedState,
    pub focus: FocusState,
    pub error: ErrorState,
    pub disabled: DisabledState,
}

/// 统一语义 token 集合。**没有动画 / 过渡字段**：主题无法让高频操作产生动画，
/// `deny_unknown_fields` 会让试图写入 `transition` / `animation` 的主题直接报错。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeTokens {
    pub color: ColorTokens,
    pub font: FontTokens,
    pub space: SpaceTokens,
    pub radius: RadiusTokens,
    pub shadow: ShadowTokens,
    pub state: StateTokens,
}

impl ThemeTokens {
    /// 宿主基准几何：主题若改变它就会被拒绝。
    pub fn canonical_font() -> FontTokens {
        FontTokens {
            family: system_font_family(),
            body: CANONICAL_FONT_BODY.to_string(),
            input: CANONICAL_FONT_INPUT.to_string(),
            aux: CANONICAL_FONT_AUX.to_string(),
        }
    }

    pub fn canonical_space() -> SpaceTokens {
        SpaceTokens {
            window_padding: CANONICAL_SPACE_WINDOW_PADDING.to_string(),
            row_padding: CANONICAL_SPACE_ROW_PADDING.to_string(),
            row_gap: CANONICAL_SPACE_ROW_GAP.to_string(),
            section_gap: CANONICAL_SPACE_SECTION_GAP.to_string(),
            row_height: CANONICAL_ROW_HEIGHT.to_string(),
        }
    }

    /// 校验结构、几何与可读性。失败原因可直接展示给用户。
    pub fn validate(&self) -> Result<(), ThemeError> {
        self.validate_geometry()?;
        self.validate_colors()?;
        self.validate_readability()?;
        Ok(())
    }

    fn validate_geometry(&self) -> Result<(), ThemeError> {
        let canonical_font = Self::canonical_font();
        if self.font.body != canonical_font.body {
            return Err(ThemeError::invalid(format!(
                "字号 font.body 必须是 {}，当前为 {}；主题不得改变布局几何",
                canonical_font.body, self.font.body
            )));
        }
        if self.font.input != canonical_font.input {
            return Err(ThemeError::invalid(format!(
                "字号 font.input 必须是 {}，当前为 {}；主题不得改变布局几何",
                canonical_font.input, self.font.input
            )));
        }
        if self.font.aux != canonical_font.aux {
            return Err(ThemeError::invalid(format!(
                "字号 font.aux 必须是 {}，当前为 {}；主题不得改变布局几何",
                canonical_font.aux, self.font.aux
            )));
        }
        if self.font.family.trim().is_empty() {
            return Err(ThemeError::invalid("字体族 font.family 不能为空"));
        }

        let canonical_space = Self::canonical_space();
        let space_fields = [
            ("space.windowPadding", &self.space.window_padding, &canonical_space.window_padding),
            ("space.rowPadding", &self.space.row_padding, &canonical_space.row_padding),
            ("space.rowGap", &self.space.row_gap, &canonical_space.row_gap),
            ("space.sectionGap", &self.space.section_gap, &canonical_space.section_gap),
            ("space.rowHeight", &self.space.row_height, &canonical_space.row_height),
        ];
        for (name, actual, expected) in space_fields {
            if actual != expected {
                return Err(ThemeError::invalid(format!(
                    "间距 {name} 必须是 {expected}，当前为 {actual}；主题不得改变控件位置"
                )));
            }
        }

        for (name, value) in [
            ("radius.window", &self.radius.window),
            ("radius.item", &self.radius.item),
            ("radius.control", &self.radius.control),
        ] {
            let px = parse_px(value).ok_or_else(|| {
                ThemeError::invalid(format!("圆角 {name} 必须是 px 数值，当前为 {value}"))
            })?;
            if !(MIN_RADIUS_PX..=MAX_RADIUS_PX).contains(&px) {
                return Err(ThemeError::invalid(format!(
                    "圆角 {name} 必须在 {MIN_RADIUS_PX} 到 {MAX_RADIUS_PX}px 之间，当前为 {value}"
                )));
            }
        }

        for (name, value) in [
            ("shadow.window", &self.shadow.window),
            ("shadow.overlay", &self.shadow.overlay),
        ] {
            if value.trim().is_empty() {
                return Err(ThemeError::invalid(format!("阴影 {name} 不能为空")));
            }
            if value.len() > MAX_SHADOW_LEN {
                return Err(ThemeError::invalid(format!(
                    "阴影 {name} 过长（最多 {MAX_SHADOW_LEN} 字节）"
                )));
            }
        }

        if !(0.0..=1.0).contains(&self.state.disabled.opacity) {
            return Err(ThemeError::invalid(format!(
                "禁用状态透明度 state.disabled.opacity 必须在 0 到 1 之间，当前为 {}",
                self.state.disabled.opacity
            )));
        }
        Ok(())
    }

    /// 需要计算的每个颜色字段。
    fn color_fields(&self) -> Vec<(&'static str, &str)> {
        vec![
            ("color.pageBackground", &self.color.page_background),
            ("color.surface", &self.color.surface),
            ("color.hover", &self.color.hover),
            ("color.border", &self.color.border),
            ("color.borderStrong", &self.color.border_strong),
            ("color.text", &self.color.text),
            ("color.textMuted", &self.color.text_muted),
            ("color.textDisabled", &self.color.text_disabled),
            ("color.accent", &self.color.accent),
            ("color.infoBackground", &self.color.info_background),
            ("color.warningBackground", &self.color.warning_background),
            ("color.warningBorder", &self.color.warning_border),
            ("color.warningText", &self.color.warning_text),
            (
                "color.iconFallbackBackground",
                &self.color.icon_fallback_background,
            ),
            ("state.selected.background", &self.state.selected.background),
            ("state.selected.border", &self.state.selected.border),
            ("state.focus.ring", &self.state.focus.ring),
            ("state.error.background", &self.state.error.background),
            ("state.error.border", &self.state.error.border),
            ("state.error.text", &self.state.error.text),
            ("state.disabled.text", &self.state.disabled.text),
        ]
    }

    fn validate_colors(&self) -> Result<(), ThemeError> {
        for (name, value) in self.color_fields() {
            if value.trim().is_empty() {
                return Err(ThemeError::invalid(format!("颜色 {name} 不能为空")));
            }
            if Rgba::parse(value).is_none() {
                return Err(ThemeError::invalid(format!(
                    "颜色 {name} 的写法无法识别（{value}）；支持 #rgb / #rrggbb / #rrggbbaa / rgb() / rgba()"
                )));
            }
        }
        Ok(())
    }

    /// 可读性与状态可辨识：正文、辅助文字要能读，选中与焦点要能区分。
    fn validate_readability(&self) -> Result<(), ThemeError> {
        let surface = self.color_value("color.surface", &self.color.surface)?;
        let page = self.color_value("color.pageBackground", &self.color.page_background)?;

        self.require_contrast("color.text", &self.color.text, surface, 4.5)?;
        self.require_contrast("color.textMuted", &self.color.text_muted, surface, 4.5)?;
        self.require_contrast("color.text", &self.color.text, page, 4.5)?;
        // 禁用文字按 WCAG 属于豁免项，但至少要与背景区分得开。
        self.require_contrast("color.textDisabled", &self.color.text_disabled, surface, 3.0)?;

        let selection_background =
            self.color_value("state.selected.background", &self.state.selected.background)?;
        self.require_contrast(
            "state.selected.background 上的 color.text",
            &self.color.text,
            selection_background.composite_over(surface),
            4.5,
        )?;

        let error_background =
            self.color_value("state.error.background", &self.state.error.background)?;
        self.require_contrast(
            "state.error.text",
            &self.state.error.text,
            error_background.composite_over(surface),
            4.5,
        )?;

        // 选中与焦点必须彼此可辨识，且都要与普通表面不同：否则在某个主题里
        // 「当前选中项」或「焦点在哪」会看不出来。
        let selected_on_surface = selection_background.composite_over(surface);
        if selected_on_surface.max_channel_delta(surface) < 0.03 {
            return Err(ThemeError::invalid(
                "state.selected.background 与 color.surface 太接近，选中状态无法辨识",
            ));
        }
        let focus_ring = self.color_value("state.focus.ring", &self.state.focus.ring)?;
        let selected_border =
            self.color_value("state.selected.border", &self.state.selected.border)?;
        if focus_ring
            .composite_over(surface)
            .max_channel_delta(selected_border.composite_over(surface))
            < 0.03
        {
            return Err(ThemeError::invalid(
                "state.focus.ring 与 state.selected.border 太接近，焦点与选中状态无法区分",
            ));
        }
        if error_background.composite_over(surface).max_channel_delta(surface) < 0.03 {
            return Err(ThemeError::invalid(
                "state.error.background 与 color.surface 太接近，错误状态无法辨识",
            ));
        }
        Ok(())
    }

    fn color_value(&self, name: &str, value: &str) -> Result<Rgba, ThemeError> {
        Rgba::parse(value)
            .ok_or_else(|| ThemeError::invalid(format!("颜色 {name} 的写法无法识别（{value}）")))
    }

    fn require_contrast(
        &self,
        name: &str,
        value: &str,
        behind: Rgba,
        minimum: f64,
    ) -> Result<(), ThemeError> {
        let color = self.color_value(name, value)?;
        let ratio = color.composite_over(behind).contrast_ratio(behind);
        if ratio < minimum {
            return Err(ThemeError::invalid(format!(
                "{name} 与背景的对比度只有 {ratio:.2}:1，低于要求的 {minimum:.1}:1"
            )));
        }
        Ok(())
    }

    /// 映射为 UI 消费的 CSS 自定义属性。UI 不认识任何具体主题。
    pub fn css_vars(&self) -> Vec<CssVar> {
        let pairs: [(&str, String); 32] = [
            ("--fc-page-bg", self.color.page_background.clone()),
            ("--fc-surface", self.color.surface.clone()),
            ("--fc-hover-bg", self.color.hover.clone()),
            ("--fc-border", self.color.border.clone()),
            ("--fc-border-strong", self.color.border_strong.clone()),
            ("--fc-text", self.color.text.clone()),
            ("--fc-text-muted", self.color.text_muted.clone()),
            ("--fc-text-disabled", self.color.text_disabled.clone()),
            ("--fc-accent", self.color.accent.clone()),
            ("--fc-info-bg", self.color.info_background.clone()),
            ("--fc-warning-bg", self.color.warning_background.clone()),
            ("--fc-warning-border", self.color.warning_border.clone()),
            ("--fc-warning-text", self.color.warning_text.clone()),
            (
                "--fc-icon-fallback-bg",
                self.color.icon_fallback_background.clone(),
            ),
            (
                "--fc-selection-bg",
                self.state.selected.background.clone(),
            ),
            ("--fc-selection-border", self.state.selected.border.clone()),
            ("--fc-focus-ring", self.state.focus.ring.clone()),
            ("--fc-error-bg", self.state.error.background.clone()),
            ("--fc-error-border", self.state.error.border.clone()),
            ("--fc-error-text", self.state.error.text.clone()),
            (
                "--fc-disabled-opacity",
                format_float(self.state.disabled.opacity),
            ),
            ("--fc-font-family", self.font.family.clone()),
            ("--fc-font-body", self.font.body.clone()),
            ("--fc-font-input", self.font.input.clone()),
            ("--fc-font-aux", self.font.aux.clone()),
            (
                "--fc-space-window-padding",
                self.space.window_padding.clone(),
            ),
            ("--fc-space-row-padding", self.space.row_padding.clone()),
            ("--fc-space-row-gap", self.space.row_gap.clone()),
            ("--fc-space-section-gap", self.space.section_gap.clone()),
            ("--fc-row-height", self.space.row_height.clone()),
            ("--fc-radius-window", self.radius.window.clone()),
            ("--fc-shadow", self.shadow.window.clone()),
        ];
        let mut vars: Vec<CssVar> = pairs
            .into_iter()
            .map(|(name, value)| CssVar {
                name: name.to_string(),
                value,
            })
            .collect();
        vars.push(CssVar {
            name: "--fc-radius-item".to_string(),
            value: self.radius.item.clone(),
        });
        vars.push(CssVar {
            name: "--fc-radius-control".to_string(),
            value: self.radius.control.clone(),
        });
        vars.push(CssVar {
            name: "--fc-shadow-overlay".to_string(),
            value: self.shadow.overlay.clone(),
        });
        vars.push(CssVar {
            name: "--fc-state-disabled-text".to_string(),
            value: self.state.disabled.text.clone(),
        });
        vars
    }

    /// token 的字段名清单。测试用它断言「每个内置主题的 token 集合都是完整的」。
    pub fn field_names() -> Vec<&'static str> {
        vec![
            "color.pageBackground",
            "color.surface",
            "color.hover",
            "color.border",
            "color.borderStrong",
            "color.text",
            "color.textMuted",
            "color.textDisabled",
            "color.accent",
            "color.infoBackground",
            "color.warningBackground",
            "color.warningBorder",
            "color.warningText",
            "color.iconFallbackBackground",
            "font.family",
            "font.body",
            "font.input",
            "font.aux",
            "space.windowPadding",
            "space.rowPadding",
            "space.rowGap",
            "space.sectionGap",
            "space.rowHeight",
            "radius.window",
            "radius.item",
            "radius.control",
            "shadow.window",
            "shadow.overlay",
            "state.selected.background",
            "state.selected.border",
            "state.focus.ring",
            "state.error.background",
            "state.error.border",
            "state.error.text",
            "state.disabled.text",
            "state.disabled.opacity",
        ]
    }
}

/// 主题文档里两种外观各自的 token 集合（`appearance: "system"` 时使用）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemePalettes {
    pub light: ThemeTokens,
    pub dark: ThemeTokens,
}

/// 一个主题包的全部内容。**纯数据**：解析器不会执行其中任何内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeDocument {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub appearance: ThemeAppearance,
    /// `appearance` 为浅色 / 深色时提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<ThemeTokens>,
    /// `appearance` 为跟随系统时提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palettes: Option<ThemePalettes>,
}

impl ThemeDocument {
    /// 从本地主题包读取文档：目录里的 `theme.json`，或直接的 JSON 文件。
    pub fn from_package_path(path: &std::path::Path) -> Result<Self, ThemeError> {
        if !path.exists() {
            return Err(ThemeError::PackageNotFound(path.to_path_buf()));
        }
        let file = if path.is_dir() {
            path.join("theme.json")
        } else {
            path.to_path_buf()
        };
        if !file.exists() {
            return Err(ThemeError::PackageNotFound(file));
        }
        let text = std::fs::read_to_string(&file).map_err(|error| {
            ThemeError::invalid(format!("无法读取主题包 {}：{error}", file.display()))
        })?;
        Self::from_json(&text)
    }

    pub fn from_json(text: &str) -> Result<Self, ThemeError> {
        let document: ThemeDocument = serde_json::from_str(text)
            .map_err(|error| ThemeError::invalid(format!("主题 JSON 解析失败：{error}")))?;
        document.validate()?;
        Ok(document)
    }

    pub fn to_json(&self) -> Result<String, ThemeError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| ThemeError::invalid(format!("主题序列化失败：{error}")))
    }

    /// 标识是否是随应用提供的内置主题。
    pub fn is_builtin(&self) -> bool {
        is_builtin_theme_id(&self.id)
    }

    pub fn validate(&self) -> Result<(), ThemeError> {
        if self.schema_version != THEME_SCHEMA_VERSION {
            return Err(ThemeError::invalid(format!(
                "主题格式版本必须是 {THEME_SCHEMA_VERSION}，当前为 {}",
                self.schema_version
            )));
        }
        if !is_valid_theme_id(&self.id) {
            return Err(ThemeError::invalid(format!(
                "主题标识不合法（{}）：只能使用小写字母、数字、点、下划线与连字符，最长 64 个字符",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(ThemeError::invalid("主题名称不能为空"));
        }
        if self.version.trim().is_empty() {
            return Err(ThemeError::invalid("主题版本不能为空"));
        }
        match self.appearance {
            ThemeAppearance::System => {
                if self.tokens.is_some() {
                    return Err(ThemeError::invalid(
                        "跟随系统的主题不能提供 tokens，请提供 palettes.light 与 palettes.dark",
                    ));
                }
                let palettes = self.palettes.as_ref().ok_or_else(|| {
                    ThemeError::invalid("跟随系统的主题必须提供 palettes.light 与 palettes.dark")
                })?;
                palettes.light.validate().map_err(|error| {
                    ThemeError::invalid(format!("palettes.light 无效：{error}"))
                })?;
                palettes.dark.validate().map_err(|error| {
                    ThemeError::invalid(format!("palettes.dark 无效：{error}"))
                })?;
            }
            ThemeAppearance::Light | ThemeAppearance::Dark => {
                if self.palettes.is_some() {
                    return Err(ThemeError::invalid(
                        "浅色 / 深色主题不能提供 palettes，请提供 tokens",
                    ));
                }
                let tokens = self
                    .tokens
                    .as_ref()
                    .ok_or_else(|| ThemeError::invalid("主题必须提供 tokens"))?;
                tokens.validate()?;
            }
        }
        Ok(())
    }

    /// 按当前系统外观解析出生效的 token 集合。
    pub fn resolve(&self, system_appearance: Appearance) -> Result<ThemeTokens, ThemeError> {
        self.validate()?;
        Ok(match self.appearance {
            ThemeAppearance::Light => self
                .tokens
                .clone()
                .ok_or_else(|| ThemeError::invalid("主题缺少 tokens"))?,
            ThemeAppearance::Dark => self
                .tokens
                .clone()
                .ok_or_else(|| ThemeError::invalid("主题缺少 tokens"))?,
            ThemeAppearance::System => {
                let palettes = self
                    .palettes
                    .clone()
                    .ok_or_else(|| ThemeError::invalid("跟随系统的主题缺少 palettes"))?;
                match system_appearance {
                    Appearance::Light => palettes.light,
                    Appearance::Dark => palettes.dark,
                }
            }
        })
    }

    /// 这个主题实际生效的外观（跟随系统时用 `system_appearance` 落定）。
    pub fn resolved_appearance(&self, system_appearance: Appearance) -> Appearance {
        match self.appearance {
            ThemeAppearance::Light => Appearance::Light,
            ThemeAppearance::Dark => Appearance::Dark,
            ThemeAppearance::System => system_appearance,
        }
    }
}

/// 标识是否是内置主题。
pub fn is_builtin_theme_id(id: &str) -> bool {
    id.starts_with(BUILTIN_THEME_PREFIX)
}

/// 标识是否合法。与功能插件共用 [`crate::plugin::is_valid_plugin_id`] 的规则。
pub fn is_valid_theme_id(id: &str) -> bool {
    crate::plugin::is_valid_plugin_id(id)
}

/// 系统字体族（与 `src/styles.css` 的 `:root` 默认值一致）。
pub fn system_font_family() -> String {
    [
        "system-ui",
        "-apple-system",
        "\"Noto Sans CJK SC\"",
        "\"Source Han Sans SC\"",
        "\"PingFang SC\"",
        "\"Microsoft YaHei\"",
        "sans-serif",
    ]
    .join(", ")
}

fn parse_px(value: &str) -> Option<f64> {
    value.trim().strip_suffix("px")?.trim().parse().ok()
}

fn format_float(value: f64) -> String {
    let text = format!("{value}");
    text
}

// ---------------------------------------------------------------------------
// 主题库：内置主题 + 已安装的本地主题包
// ---------------------------------------------------------------------------

/// 工作区里 `theme.json` 的内容：当前选中的主题。
///
/// 主题配置是**可迁移偏好**，跟随配置工作区；系统外观属于设备/系统状态，不写进文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeSelection {
    pub selected: String,
}

impl ThemeSelection {
    pub fn new(selected: impl Into<String>) -> Self {
        Self {
            selected: selected.into(),
        }
    }

    pub fn from_json(text: &str) -> Result<Self, ThemeError> {
        serde_json::from_str(text)
            .map_err(|error| ThemeError::invalid(format!("主题配置 JSON 解析失败：{error}")))
    }

    pub fn to_json(&self) -> Result<String, ThemeError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| ThemeError::invalid(format!("主题配置序列化失败：{error}")))
    }
}

/// 主题列表里的一个条目：由插件清单 + 主题库共同决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    /// 清单里的启用状态。
    pub enabled: bool,
    /// 是否是当前选中的主题。
    pub selected: bool,
    /// 是否是随应用提供的内置主题（内置主题不能被移除）。
    pub builtin: bool,
    /// 主题声明的外观偏好。
    pub appearance: ThemeAppearance,
    /// 主题文档当前是否能解析并通过校验。
    pub usable: bool,
    /// 选中的主题不可用时给出的中文原因。
    pub error: Option<String>,
}

/// 宿主发给 UI 的主题状态：选中的主题、解析后的 token（含 CSS 自定义属性）
/// 与可选主题列表。UI 只消费它，不认识任何具体主题。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeState {
    /// 当前选中的主题 id。
    pub selected: String,
    /// 当前选中的主题名称。
    pub selected_name: String,
    /// 选中主题声明的外观偏好。
    pub preference: ThemeAppearance,
    /// 解析后实际生效的外观。
    pub appearance: Appearance,
    /// 当前系统外观（「跟随系统」据此解析）。
    pub system_appearance: Appearance,
    /// 当前生效的语义 token。
    pub tokens: ThemeTokens,
    /// UI 直接写入根元素的 CSS 自定义属性。
    pub css_vars: Vec<CssVar>,
    /// 可选主题列表（来自插件清单）。
    pub themes: Vec<ThemeEntry>,
    /// 最近一次无效主题的中文原因；此时 `tokens` 仍是上一次可用外观。
    pub error: Option<String>,
}

/// 主题库。内置主题随应用提供；本地主题包从配置工作区的 `themes/<id>/` 读入。
///
/// 这里**只保存已解析的文档**；「哪些主题存在、是否启用」由插件清单决定
/// （[`crate::manifest`]），因此禁用某个主题不需要动主题库。
#[derive(Debug, Clone, Default)]
pub struct ThemeLibrary {
    builtin: Vec<ThemeDocument>,
    installed: BTreeMap<String, ThemeDocument>,
    /// 无法加载的主题：id → 中文原因。
    broken: BTreeMap<String, String>,
}

impl ThemeLibrary {
    /// 带内置浅色 / 深色 / 跟随系统的主题库。
    pub fn with_builtins() -> Self {
        Self {
            builtin: builtin_themes(),
            installed: BTreeMap::new(),
            broken: BTreeMap::new(),
        }
    }

    pub fn builtin(&self) -> &[ThemeDocument] {
        &self.builtin
    }

    /// 登记一个已解析的本地主题包（同 id 覆盖）。
    pub fn install(&mut self, document: ThemeDocument) {
        self.broken.remove(&document.id);
        self.installed.insert(document.id.clone(), document);
    }

    pub fn remove(&mut self, id: &str) {
        self.installed.remove(id);
        self.broken.remove(id);
    }

    /// 记录一个无法加载的主题及其中文原因。
    pub fn mark_broken(&mut self, id: &str, reason: impl Into<String>) {
        self.installed.remove(id);
        self.broken.insert(id.to_string(), reason.into());
    }

    pub fn broken_reason(&self, id: &str) -> Option<&str> {
        self.broken.get(id).map(String::as_str)
    }

    /// 找到主题文档。本地主题包优先于同名内置主题（内置 id 不可被安装覆盖）。
    pub fn document(&self, id: &str) -> Result<&ThemeDocument, ThemeError> {
        if let Some(reason) = self.broken.get(id) {
            return Err(ThemeError::invalid(reason.clone()));
        }
        if let Some(document) = self.installed.get(id) {
            return Ok(document);
        }
        if let Some(document) = self.builtin.iter().find(|document| document.id == id) {
            return Ok(document);
        }
        Err(ThemeError::Unknown(id.to_string()))
    }

    /// 全部已知主题文档（内置 + 已安装）。
    pub fn documents(&self) -> Vec<&ThemeDocument> {
        self.builtin
            .iter()
            .chain(self.installed.values())
            .collect()
    }

    /// 已安装主题包的 id。
    pub fn installed_ids(&self) -> Vec<String> {
        self.installed.keys().cloned().collect()
    }

    /// 已安装主题包与加载失败的快照。宿主用它判断一次重读是否真的改变了什么。
    pub fn snapshot(&self) -> BTreeMap<String, String> {
        let mut snapshot = BTreeMap::new();
        for (id, document) in &self.installed {
            snapshot.insert(format!("pkg:{id}"), document.version.clone());
        }
        for (id, reason) in &self.broken {
            snapshot.insert(format!("err:{id}"), reason.clone());
        }
        snapshot
    }
}

/// 随应用提供的三个默认主题：浅色、深色、跟随系统。
///
/// 它们由清单条目加载（见 [`crate::manifest::PluginManifestFile::defaults`]），
/// 主题库只负责在给定 id 时提供文档。
pub fn builtin_themes() -> Vec<ThemeDocument> {
    vec![
        ThemeDocument {
            schema_version: THEME_SCHEMA_VERSION,
            id: THEME_LIGHT.to_string(),
            name: "浅色".to_string(),
            version: "0.1.0".to_string(),
            appearance: ThemeAppearance::Light,
            tokens: Some(light_tokens()),
            palettes: None,
        },
        ThemeDocument {
            schema_version: THEME_SCHEMA_VERSION,
            id: THEME_DARK.to_string(),
            name: "深色".to_string(),
            version: "0.1.0".to_string(),
            appearance: ThemeAppearance::Dark,
            tokens: Some(dark_tokens()),
            palettes: None,
        },
        ThemeDocument {
            schema_version: THEME_SCHEMA_VERSION,
            id: THEME_SYSTEM.to_string(),
            name: "跟随系统".to_string(),
            version: "0.1.0".to_string(),
            appearance: ThemeAppearance::System,
            tokens: None,
            palettes: Some(ThemePalettes {
                light: light_tokens(),
                dark: dark_tokens(),
            }),
        },
    ]
}

/// 内置浅色主题的 token（与 `src/styles.css` 的默认值一致）。
pub fn light_tokens() -> ThemeTokens {
    ThemeTokens {
        color: ColorTokens {
            page_background: "rgba(250, 250, 251, 0.98)".to_string(),
            surface: "#ffffff".to_string(),
            hover: "rgba(15, 23, 42, 0.04)".to_string(),
            border: "rgba(15, 23, 42, 0.12)".to_string(),
            border_strong: "rgba(15, 23, 42, 0.24)".to_string(),
            text: "#16181d".to_string(),
            text_muted: "#6b7280".to_string(),
            text_disabled: "#8b919c".to_string(),
            accent: "#2563eb".to_string(),
            info_background: "rgba(15, 23, 42, 0.05)".to_string(),
            warning_background: "rgba(217, 119, 6, 0.12)".to_string(),
            warning_border: "rgba(217, 119, 6, 0.45)".to_string(),
            warning_text: "#92400e".to_string(),
            icon_fallback_background: "rgba(15, 23, 42, 0.08)".to_string(),
        },
        font: ThemeTokens::canonical_font(),
        space: ThemeTokens::canonical_space(),
        radius: RadiusTokens {
            window: "14px".to_string(),
            item: "8px".to_string(),
            control: "6px".to_string(),
        },
        shadow: ShadowTokens {
            window: "0 8px 28px rgba(15, 23, 42, 0.16)".to_string(),
            overlay: "0 12px 32px rgba(15, 23, 42, 0.22)".to_string(),
        },
        state: StateTokens {
            selected: SelectedState {
                background: "rgba(37, 99, 235, 0.12)".to_string(),
                border: "rgba(37, 99, 235, 0.55)".to_string(),
            },
            focus: FocusState {
                ring: "rgba(37, 99, 235, 0.65)".to_string(),
            },
            error: ErrorState {
                background: "rgba(220, 38, 38, 0.1)".to_string(),
                border: "rgba(220, 38, 38, 0.5)".to_string(),
                text: "#991b1b".to_string(),
            },
            disabled: DisabledState {
                text: "#8b919c".to_string(),
                opacity: 0.5,
            },
        },
    }
}

/// 内置深色主题的 token。
pub fn dark_tokens() -> ThemeTokens {
    ThemeTokens {
        color: ColorTokens {
            page_background: "rgba(24, 25, 28, 0.98)".to_string(),
            surface: "#202226".to_string(),
            hover: "rgba(255, 255, 255, 0.06)".to_string(),
            border: "rgba(255, 255, 255, 0.14)".to_string(),
            border_strong: "rgba(255, 255, 255, 0.28)".to_string(),
            text: "#ecedf0".to_string(),
            text_muted: "#a1a7b3".to_string(),
            text_disabled: "#767c88".to_string(),
            accent: "#7aa2f7".to_string(),
            info_background: "rgba(255, 255, 255, 0.06)".to_string(),
            warning_background: "rgba(245, 158, 11, 0.18)".to_string(),
            warning_border: "rgba(245, 158, 11, 0.5)".to_string(),
            warning_text: "#fcd34d".to_string(),
            icon_fallback_background: "rgba(255, 255, 255, 0.1)".to_string(),
        },
        font: ThemeTokens::canonical_font(),
        space: ThemeTokens::canonical_space(),
        radius: RadiusTokens {
            window: "14px".to_string(),
            item: "8px".to_string(),
            control: "6px".to_string(),
        },
        shadow: ShadowTokens {
            window: "0 8px 28px rgba(0, 0, 0, 0.45)".to_string(),
            overlay: "0 12px 32px rgba(0, 0, 0, 0.55)".to_string(),
        },
        state: StateTokens {
            selected: SelectedState {
                background: "rgba(122, 162, 247, 0.24)".to_string(),
                border: "rgba(122, 162, 247, 0.75)".to_string(),
            },
            focus: FocusState {
                ring: "rgba(122, 162, 247, 0.95)".to_string(),
            },
            error: ErrorState {
                background: "rgba(248, 113, 113, 0.16)".to_string(),
                border: "rgba(248, 113, 113, 0.6)".to_string(),
                text: "#fecaca".to_string(),
            },
            disabled: DisabledState {
                text: "#767c88".to_string(),
                opacity: 0.5,
            },
        },
    }
}
