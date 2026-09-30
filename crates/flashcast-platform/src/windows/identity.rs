//! 稳定标识与去重（纯逻辑）。
//!
//! 取自研究笔记 §1.8：Windows 上不存在单一完美主键，因此按优先级选取，
//! 并把「同一软件的多个入口」归并到同一个 id：
//!
//! 1. **AUMID** —— 操作系统认可的稳定标识（打包应用、以及注册了 AUMID 的 Win32 应用）；
//! 2. **小写化的目标路径 + 参数** —— 开始菜单 `.lnk` 与注册表推导目标的实际主键；
//! 3. **注册表子键路径** —— 只有注册表来源的条目才用它；
//! 4. **`.lnk` 路径** —— 连目标都解析不出来时的兜底。
//!
//! 去重时必须保留「更靠前」的那一个：用户与全体用户各有一个快捷方式、
//! MSI 修复又留下副本时，应保留开始菜单根目录里的那个，把其余路径记为别名
//! （别名持久化由后续 ticket 负责，这里只保证不产生重复条目）。

use std::path::Path;

use crate::catalog::AppEntry;

/// 所有 Windows 条目的 id 前缀，避免与 Linux 的 desktop file id 冲突。
pub const WINDOWS_ID_PREFIX: &str = "win:";

/// 路径归一化：统一分隔符、去掉多余的反斜杠、转小写。
pub fn normalize_path(path: &str) -> String {
    let mut normalized = path.replace('/', "\\").to_lowercase();
    while normalized.len() > 3 && normalized.ends_with('\\') {
        normalized.pop();
    }
    normalized
}

/// 目标 + 参数的稳定 id。
pub fn exe_id(target: &str, arguments: Option<&str>) -> String {
    let args = arguments.map(str::trim).unwrap_or_default();
    format!("{}exe:{}\u{0}{args}", WINDOWS_ID_PREFIX, normalize_path(target))
}

/// AUMID 的稳定 id。
pub fn aumid_id(aumid: &str) -> String {
    format!("{}aumid:{}", WINDOWS_ID_PREFIX, aumid.trim().to_lowercase())
}

/// 注册表条目的稳定 id：子键路径 + 显示名。
pub fn registry_id(hive: &str, key_name: &str, display_name: &str) -> String {
    format!(
        "{}reg:{}|{}|{}",
        WINDOWS_ID_PREFIX,
        hive.trim().to_lowercase(),
        key_name.trim().to_lowercase(),
        display_name.trim().to_lowercase()
    )
}

/// 连目标都解析不出来时的兜底 id。
pub fn lnk_id(lnk_path: &Path) -> String {
    format!(
        "{}lnk:{}",
        WINDOWS_ID_PREFIX,
        normalize_path(&lnk_path.to_string_lossy())
    )
}

/// 按 id 去重：同 id 时保留 `rank` 最小的一项，并保持首次出现的顺序。
///
/// `rank` 是调用方给出的「优先级」：数值越小越优先（例如开始菜单根目录为 0、
/// 子目录为 1、注册表为 2）。
pub fn dedupe_entries(entries: Vec<(u32, AppEntry)>) -> Vec<AppEntry> {
    let mut order: Vec<String> = Vec::new();
    let mut best: Vec<(u32, AppEntry)> = Vec::new();
    for (rank, entry) in entries {
        match order.iter().position(|id| id == &entry.id) {
            Some(index) => {
                if rank < best[index].0 {
                    best[index] = (rank, entry);
                }
            }
            None => {
                order.push(entry.id.clone());
                best.push((rank, entry));
            }
        }
    }
    best.into_iter().map(|(_, entry)| entry).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AppSource, IconRef};

    fn entry(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: name.to_string(),
            comment: None,
            icon: Some(IconRef::unresolved(name)),
            exec: vec![r"C:\Tools\app.exe".to_string()],
            desktop_file: None,
            working_dir: None,
            wm_class: None,
            terminal: false,
            keywords: Vec::new(),
            source: AppSource::StartMenu,
        }
    }

    #[test]
    fn ids_are_stable_and_case_insensitive() {
        assert_eq!(
            exe_id(r"C:\Program Files\Foo\Foo.exe", Some("--flag")),
            exe_id(r"c:/program files/foo/FOO.EXE", Some(" --flag "))
        );
        assert_ne!(
            exe_id(r"C:\Program Files\Foo\Foo.exe", None),
            exe_id(r"C:\Program Files\Foo\Bar.exe", None)
        );
        assert_eq!(
            aumid_id("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"),
            "win:aumid:microsoft.windowscalculator_8wekyb3d8bbwe!app"
        );
        assert_eq!(
            registry_id("hklm", "{GUID}", "Example App"),
            "win:reg:hklm|{guid}|example app"
        );
        assert!(lnk_id(Path::new(r"C:\Start Menu\A.lnk")).starts_with("win:lnk:"));
        assert_ne!(
            exe_id(r"C:\a.exe", None),
            exe_id(r"C:\a.exe", Some("--x")),
            "参数不同必须是不同条目"
        );
    }

    #[test]
    fn dedupe_keeps_the_best_rank_and_the_first_appearance_order() {
        let entries = vec![
            (1, entry("win:exe:c:\\a.exe\u{0}", "A（子目录）")),
            (0, entry("win:exe:c:\\a.exe\u{0}", "A（根目录）")),
            (2, entry("win:exe:c:\\b.exe\u{0}", "B")),
            (0, entry("win:exe:c:\\a.exe\u{0}", "A（更靠前但排名 0 之外的重复项）")),
        ];
        let deduped = dedupe_entries(entries);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].name, "A（根目录）", "保留 rank 最小的一项");
        assert_eq!(deduped[1].name, "B", "保持首次出现的顺序");
    }
}
