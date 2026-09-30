//! freedesktop `.desktop` 条目解析。纯文本处理，不依赖平台，可在任意平台测试。

use std::collections::BTreeMap;
use std::path::Path;

/// 从 `.desktop` 文件中读到的原始字段（未做图标解析，也未做路径归属判断）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    pub entry_type: Option<String>,
    pub name: Option<String>,
    pub comment: Option<String>,
    pub icon: Option<String>,
    pub exec: Option<String>,
    pub try_exec: Option<String>,
    pub path: Option<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
    pub startup_wm_class: Option<String>,
}

impl DesktopEntry {
    /// 是否为可展示、可启动的应用条目。
    pub fn is_launchable_application(&self) -> bool {
        self.entry_type.as_deref() == Some("Application")
            && !self.no_display
            && !self.hidden
            && self.name.is_some()
    }
}

/// 解析结果。`fields` 之外还保留解析过程中的告警，供诊断报告使用。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedDesktop {
    pub fields: DesktopEntry,
    pub warnings: Vec<String>,
}

/// 解析 `.desktop` 文件内容。
///
/// 只读取 `[Desktop Entry]` 分组；`locale_candidates` 为按优先级排列的语言标记，
/// 例如 `["zh_CN", "zh"]`，用于挑选 `Name[zh_CN]` 这类本地化取值。
pub fn parse_desktop_entry(content: &str, locale_candidates: &[String]) -> ParsedDesktop {
    let mut warnings = Vec::new();
    let mut in_main = false;
    // 本地化键 -> 值，后出现的同名键覆盖先出现的。
    let mut localized: BTreeMap<String, String> = BTreeMap::new();
    let mut fields = DesktopEntry::default();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let section = line.trim_start_matches('[').trim_end_matches(']').trim();
            in_main = section == "Desktop Entry";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            warnings.push(format!("忽略无法解析的行：{line}"));
            continue;
        };
        let key = key.trim();
        let value = unescape_string(value.trim());

        // 本地化键形如 `Name[zh_CN]` 或 `Name[zh_CN.UTF-8]`。
        if let Some((base, locale)) = split_localized_key(key) {
            let locale = strip_encoding(&locale);
            if base == "Name" || base == "Comment" || base == "Keywords" {
                localized.insert(format!("{base}\u{1}{locale}"), value);
            }
            continue;
        }

        match key {
            "Type" => fields.entry_type = Some(value),
            "Name" => fields.name = Some(value),
            "Comment" => fields.comment = Some(value),
            "Icon" => fields.icon = Some(value),
            "Exec" => fields.exec = Some(value),
            "TryExec" => fields.try_exec = Some(value),
            "Path" => fields.path = Some(value),
            "Keywords" => fields.keywords = split_list(&value),
            "Categories" => fields.categories = split_list(&value),
            "NoDisplay" => fields.no_display = parse_bool(&value).unwrap_or(false),
            "Hidden" => fields.hidden = parse_bool(&value).unwrap_or(false),
            "Terminal" => fields.terminal = parse_bool(&value).unwrap_or(false),
            "StartupWMClass" => fields.startup_wm_class = Some(value),
            _ => {}
        }
    }

    // 本地化覆盖：第一个命中的候选语言生效。
    if let Some(name) = pick_localized(&localized, "Name", locale_candidates) {
        fields.name = Some(name);
    }
    if let Some(comment) = pick_localized(&localized, "Comment", locale_candidates) {
        fields.comment = Some(comment);
    }
    if fields.keywords.is_empty() {
        if let Some(keywords) = pick_localized(&localized, "Keywords", locale_candidates) {
            fields.keywords = split_list(&keywords);
        }
    }

    ParsedDesktop { fields, warnings }
}

/// 从路径推导 desktop file ID：相对于所属 `applications` 目录的路径，`/` 换成 `-`。
pub fn desktop_file_id(path: &Path, applications_dir: &Path) -> Option<String> {
    let relative = path.strip_prefix(applications_dir).ok()?;
    let mut id = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("-");
    if id.is_empty() {
        return None;
    }
    if !id.ends_with(".desktop") {
        id.push_str(".desktop");
    }
    Some(id)
}

/// 展开 `Exec=` 为最终 argv。
///
/// 按 Desktop Entry Specification 的规则分词，并展开字段码：
/// - `%f` `%F` `%u` `%U` 与已废弃的 `%d` `%D` `%n` `%N` `%v` `%m` 在本产品中无对应参数，整段丢弃；
/// - `%i` 展开为 `--icon <Icon>` 两个参数；
/// - `%c` 展开为本地化名称，`%k` 展开为该 `.desktop` 文件路径；
/// - `%%` 展开为字面量 `%`。
///
/// 返回值不含 shell，调用方必须直接使用 argv 启动进程。
pub fn parse_exec(
    exec: &str,
    desktop_file: Option<&Path>,
    icon: Option<&str>,
    name: Option<&str>,
) -> Vec<String> {
    let tokens = tokenize_exec(exec);
    let mut argv = Vec::with_capacity(tokens.len());
    for token in tokens {
        expand_token(
            &token,
            desktop_file,
            icon,
            name,
            &mut argv,
        );
    }
    argv
}

/// 按 freedesktop 规则把 `Exec=` 拆成参数。
///
/// 双引号内的空白不分割参数；`\` 可转义 `"`、`` ` ``、`$`、`\`。
pub fn tokenize_exec(exec: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_current = false;
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if has_current {
                    tokens.push(std::mem::take(&mut current));
                    has_current = false;
                }
            }
            '"' => {
                has_current = true;
                while let Some(inner) = chars.next() {
                    match inner {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some(escaped @ ('"' | '`' | '$' | '\\')) => current.push(escaped),
                            Some(other) => {
                                current.push('\\');
                                current.push(other);
                            }
                            None => current.push('\\'),
                        },
                        other => current.push(other),
                    }
                }
            }
            '\\' => {
                has_current = true;
                match chars.next() {
                    Some(escaped @ ('"' | '`' | '$' | '\\')) => current.push(escaped),
                    Some(other) => {
                        current.push('\\');
                        current.push(other);
                    }
                    None => current.push('\\'),
                }
            }
            other => {
                has_current = true;
                current.push(other);
            }
        }
    }
    if has_current {
        tokens.push(current);
    }
    tokens
}

fn expand_token(
    token: &str,
    desktop_file: Option<&Path>,
    icon: Option<&str>,
    name: Option<&str>,
    out: &mut Vec<String>,
) {
    // 整段就是 %i 时展开为两个参数。
    if token == "%i" {
        if let Some(icon) = icon {
            if !icon.is_empty() {
                out.push("--icon".to_string());
                out.push(icon.to_string());
            }
        }
        return;
    }

    let mut result = String::new();
    let mut produced = false;
    let mut chars = token.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            produced = true;
            continue;
        }
        match chars.next() {
            Some('%') => {
                result.push('%');
                produced = true;
            }
            // 无对应参数的文件/URL 字段码：整段丢弃。
            Some('f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm') => {}
            Some('i') => {
                if let Some(icon) = icon {
                    if !icon.is_empty() {
                        result.push_str(icon);
                        produced = true;
                    }
                }
            }
            Some('c') => {
                if let Some(name) = name {
                    result.push_str(name);
                    produced = true;
                }
            }
            Some('k') => {
                if let Some(path) = desktop_file {
                    result.push_str(&path.to_string_lossy());
                    produced = true;
                }
            }
            // 未知字段码按字面保留，避免静默改变命令行语义。
            Some(other) => {
                result.push('%');
                result.push(other);
                produced = true;
            }
            None => {
                result.push('%');
                produced = true;
            }
        }
    }
    if produced && !result.is_empty() {
        out.push(result);
    }
}

fn split_localized_key(key: &str) -> Option<(String, String)> {
    let open = key.find('[')?;
    if !key.ends_with(']') {
        return None;
    }
    let base = key[..open].to_string();
    let locale = key[open + 1..key.len() - 1].to_string();
    if base.is_empty() || locale.is_empty() {
        return None;
    }
    Some((base, locale))
}

fn strip_encoding(locale: &str) -> String {
    match locale.split_once('.') {
        Some((lang, _encoding)) => lang.to_string(),
        None => locale.to_string(),
    }
}

fn pick_localized(
    localized: &BTreeMap<String, String>,
    base: &str,
    candidates: &[String],
) -> Option<String> {
    for candidate in candidates {
        if let Some(value) = localized.get(&format!("{base}\u{1}{candidate}")) {
            if !value.is_empty() {
                return Some(value.clone());
            }
        }
    }
    None
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// 展开 freedesktop 字符串值中的转义序列。
fn unescape_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// 按 `LANG` / `LC_MESSAGES` 推导本地化候选语言列表。
///
/// 例如 `LANG=zh_CN.UTF-8` 得到 `["zh_CN", "zh"]`。
pub fn locale_candidates_from_env(
    lang: Option<&str>,
    lc_messages: Option<&str>,
    lc_all: Option<&str>,
) -> Vec<String> {
    let raw = lc_all
        .filter(|v| !v.is_empty() && *v != "C" && *v != "POSIX")
        .or_else(|| lc_messages.filter(|v| !v.is_empty() && *v != "C" && *v != "POSIX"))
        .or_else(|| lang.filter(|v| !v.is_empty() && *v != "C" && *v != "POSIX"));
    let Some(raw) = raw else {
        return Vec::new();
    };
    let base = raw.split_once('.').map(|(b, _)| b).unwrap_or(raw);
    let base = base.split_once('@').map(|(b, _)| b).unwrap_or(base);
    let mut candidates = Vec::new();
    if !base.is_empty() {
        candidates.push(base.to_string());
    }
    if let Some((lang, _country)) = base.split_once('_') {
        if !lang.is_empty() {
            candidates.push(lang.to_string());
        }
    }
    candidates.dedup();
    candidates
}
