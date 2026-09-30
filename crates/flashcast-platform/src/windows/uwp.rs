//! UWP / 打包应用（AUMID）的枚举结果解析（纯逻辑）。
//!
//! 枚举用 `Get-StartApps | ConvertTo-Json -Compress`（研究笔记 §1.3）：这是覆盖
//! 打包应用与「注册了 AUMID 的 Win32 应用」的最省事方式，冷启动 200–500ms，
//! 因此在真实实现里只在扫描时调用一次并缓存。解析本身与 JSON 结构强相关，
//! 所以放在纯逻辑一侧，用真实 `Get-StartApps` 输出形状做夹具。
//!
//! 启动用 `explorer.exe shell:AppsFolder\<AUMID>`（等价于
//! `ShellExecuteW("shell:AppsFolder\…")`），不需要 COM 与 CLSID。
//! 条目在 argv 里以 `aumid:` 前缀表示，由 [`super::launch_plan`] 还原为启动计划。

use serde_json::Value;

/// 表示打包应用的 argv 前缀。
pub const AUMID_SCHEME: &str = "aumid:";
/// `AppsFolder` 虚拟文件夹的 shell 路径前缀。
pub const APPS_FOLDER_PREFIX: &str = "shell:AppsFolder\\";

/// `Get-StartApps` 返回的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartApp {
    pub name: String,
    pub aumid: String,
}

/// 已知无法作为启动入口的 shell 宿主（它们是外壳的一部分，启动没有意义）。
const NON_LAUNCHABLE_AUMIDS: [&str; 4] = [
    "microsoft.windows.shellexperiencehost",
    "microsoft.windows.startmenuexperiencehost",
    "microsoft.windows.cortana",
    "microsoft.windows.cloudexperiencehost",
];

/// AUMID 是否值得作为可启动条目展示。
pub fn is_launchable_aumid(aumid: &str) -> bool {
    let trimmed = aumid.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    !NON_LAUNCHABLE_AUMIDS
        .iter()
        .any(|blocked| lower.starts_with(blocked))
}

/// 展示名是否可用。
///
/// `Get-StartApps` 偶尔会给出未解析的资源引用（`@{Microsoft.X_8wekyb3d8bbwe?ms-resource://…}`）
/// 或空名，这类条目没有可展示的名称，必须跳过而不是显示一串内部标识。
pub fn is_usable_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && !trimmed.starts_with("@{")
}

/// 解析 `Get-StartApps | ConvertTo-Json -Compress` 的输出。
///
/// `ConvertTo-Json` 在只有一个应用时输出**对象**而不是数组，两种形状都要接受；
/// 字段名以 `AppID` 为准（Microsoft Learn），同时容忍 `AppId` / `appId`。
pub fn parse_get_start_apps_json(json: &str) -> Vec<StartApp> {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return Vec::new();
    };
    let items: Vec<&Value> = match &value {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![&value],
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in items {
        let name = string_field(item, &["Name", "name"]);
        let aumid = string_field(item, &["AppID", "AppId", "appId", "appID"]);
        let (Some(name), Some(aumid)) = (name, aumid) else {
            continue;
        };
        if !is_usable_name(&name) || !is_launchable_aumid(&aumid) {
            continue;
        }
        out.push(StartApp { name, aumid });
    }
    dedupe_start_apps(out)
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(Value::String(text)) = object.get(*key) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// 按 AUMID（大小写不敏感）去重，保留先出现的一项。
pub fn dedupe_start_apps(apps: Vec<StartApp>) -> Vec<StartApp> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for app in apps {
        let key = app.aumid.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(app);
    }
    out
}

/// 打包应用在 `AppEntry.exec` 中的表示。
pub fn aumid_program(aumid: &str) -> String {
    format!("{AUMID_SCHEME}{aumid}")
}

/// 从 argv[0] 还原 AUMID。
pub fn aumid_from_program(program: &str) -> Option<&str> {
    let trimmed = program.trim();
    let aumid = trimmed.strip_prefix(AUMID_SCHEME)?;
    (!aumid.is_empty()).then_some(aumid)
}

/// `explorer.exe` 需要的 `shell:AppsFolder\<AUMID>` 参数。
pub fn apps_folder_argument(aumid: &str) -> String {
    format!("{APPS_FOLDER_PREFIX}{aumid}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_array_shape_get_start_apps_returns() {
        let json = r#"[{"Name":"计算器","AppID":"Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"},{"Name":"Google Chrome","AppID":"Chrome"}]"#;
        let apps = parse_get_start_apps_json(json);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "计算器");
        assert_eq!(apps[0].aumid, "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App");
        assert_eq!(apps[1].aumid, "Chrome");
    }

    #[test]
    fn parses_the_single_object_shape() {
        let json = r#"{"Name":"设置","AppID":"windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel"}"#;
        let apps = parse_get_start_apps_json(json);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "设置");
    }

    #[test]
    fn tolerates_alternate_field_spellings_and_empty_output() {
        let json = r#"[{"Name":"A","AppId":"a"},{"Name":"B","appId":"b"},{"Name":"C"}]"#;
        let apps = parse_get_start_apps_json(json);
        assert_eq!(
            apps.iter().map(|a| a.aumid.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"],
            "缺 AppID 的条目必须跳过"
        );
        assert!(parse_get_start_apps_json("").is_empty());
        assert!(parse_get_start_apps_json("   ").is_empty());
        assert!(
            parse_get_start_apps_json("not json at all").is_empty(),
            "解析失败必须是空结果，不能 panic"
        );
        assert!(parse_get_start_apps_json("null").is_empty());
    }

    #[test]
    fn unusable_names_and_shell_hosts_are_skipped() {
        let json = r#"[
            {"Name":"@{Microsoft.Foo_8wekyb3d8bbwe?ms-resource://Foo/Name}","AppID":"Foo!App"},
            {"Name":"   ","AppID":"Blank!App"},
            {"Name":"Shell Experience Host","AppID":"Microsoft.Windows.ShellExperienceHost_cw5n1h2txyewy!App"},
            {"Name":"开始菜单","AppID":"Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy!App"},
            {"Name":"记事本","AppID":"Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"}
        ]"#;
        let apps = parse_get_start_apps_json(json);
        assert_eq!(apps.len(), 1, "只应保留记事本：{apps:?}");
        assert_eq!(apps[0].aumid, "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App");
        assert!(!is_launchable_aumid(""));
        assert!(is_launchable_aumid("Chrome"));
    }

    #[test]
    fn aumid_program_and_apps_folder_round_trip() {
        let aumid = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
        let program = aumid_program(aumid);
        assert_eq!(program, format!("aumid:{aumid}"));
        assert_eq!(aumid_from_program(&program), Some(aumid));
        assert_eq!(aumid_from_program("aumid:"), None);
        assert_eq!(aumid_from_program(r"C:\Tools\app.exe"), None);
        assert_eq!(
            apps_folder_argument(aumid),
            r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
        );
    }

    #[test]
    fn duplicate_aumids_are_collapsed() {
        let apps = dedupe_start_apps(vec![
            StartApp {
                name: "Chrome".into(),
                aumid: "Chrome".into(),
            },
            StartApp {
                name: "Chrome 副本".into(),
                aumid: "chrome".into(),
            },
        ]);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Chrome");
    }
}
