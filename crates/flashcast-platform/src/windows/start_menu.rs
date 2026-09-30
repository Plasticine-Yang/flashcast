//! Windows 开始菜单 `.lnk` 扫描（纯逻辑）。
//!
//! 本文件不触碰任何 Windows API，也不依赖 `std::os::windows`：它只做目录遍历、
//! 文件名到显示名的映射与去重，因此在 Linux 上可以用真实夹具目录完整验证。
//! 真正需要 `IShellLinkW` 的目标解析在 [`super::shell_link`] 与 Windows 专属的
//! `catalog` 中完成。
//!
//! 设计取自研究笔记 §1.1：显示名必须是 **快捷方式文件名去掉扩展名**（这也是用户在
//! 开始菜单里看到、会键入的名字），而不是目标的 `FileDescription`；`.url` 是
//! INI 格式的另一种文件，必须跳过；`Programs\Startup` 是开机自启项，不是可搜索的
//! 安装入口，也跳过。

use std::path::{Path, PathBuf};

/// 开始菜单相对 `%APPDATA%` / `%ProgramData%` 的位置。
pub const START_MENU_COMPONENTS: [&str; 3] = ["Microsoft", "Windows", "Start Menu"];

/// 开机自启目录名；其中的快捷方式不是用户会搜索的安装入口。
const STARTUP_DIR: &str = "startup";

/// 一个待解析的开始菜单快捷方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutFile {
    /// `.lnk` 文件本身的路径。
    pub path: PathBuf,
    /// 展示名：文件名去掉 `.lnk` 扩展名。
    pub display_name: String,
    /// 相对开始菜单根的分组目录（例如 `Microsoft Office`）；位于根目录时为 `None`。
    pub group: Option<String>,
}

/// 由环境变量给出的开始菜单根目录列表。
///
/// 顺序即优先级：用户目录（`%APPDATA%`）在全部用户目录（`%ProgramData%`）之前，
/// 与开始菜单的显示顺序一致。`%LOCALAPPDATA%` 在 Windows 11 上是指向用户开始菜单的
/// 联接（junction），这里一并纳入并由 [`dedupe_roots`] 去掉重复，避免同一批条目
/// 被扫描两次。
pub fn roots_from_env(
    appdata: Option<&str>,
    programdata: Option<&str>,
    localappdata: Option<&str>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for base in [appdata, localappdata, programdata].into_iter().flatten() {
        if base.trim().is_empty() {
            continue;
        }
        let mut root = PathBuf::from(base);
        for component in START_MENU_COMPONENTS {
            root.push(component);
        }
        roots.push(root);
    }
    dedupe_roots(roots)
}

/// 按「解析后的真实路径、大小写不敏感」去重，保留先出现的根。
///
/// 无法解析（不存在）的根按小写字符串比较，以免把仍然有效的路径误判为重复。
pub fn dedupe_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for root in roots {
        let key = match std::fs::canonicalize(&root) {
            Ok(resolved) => normalize_key(&resolved.to_string_lossy()),
            Err(_) => normalize_key(&root.to_string_lossy()),
        };
        if seen.iter().any(|existing| existing == &key) {
            continue;
        }
        seen.push(key);
        out.push(root);
    }
    out
}

/// 比较用的路径键：分隔符统一为 `\`、折叠重复分隔符、去掉末尾分隔符、转小写。
///
/// 只用于比较，不会当成真实路径使用，因此 `\\server` 被折叠成 `\server` 也无妨 ——
/// 两边用同一套规则，仍然能正确识别「同一个根的不同写法」。
fn normalize_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_was_separator = false;
    for c in value.chars() {
        let c = if c == '/' { '\\' } else { c };
        if c == '\\' {
            if last_was_separator {
                continue;
            }
            last_was_separator = true;
        } else {
            last_was_separator = false;
        }
        out.extend(c.to_lowercase());
    }
    while out.ends_with('\\') {
        out.pop();
    }
    out
}

/// 是否为需要解析的快捷方式文件（大小写不敏感的 `.lnk`）。
///
/// `.url`（INI 格式的 Internet 快捷方式）、`desktop.ini` 与其它文件一律拒绝。
pub fn is_shortcut_file(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".lnk")
}

/// 显示名：文件名去掉 `.lnk` 扩展名。
///
/// 按 `\` 与 `/` 手工切分文件名，而不是 `Path::file_stem`：Windows 路径在 Linux 的
/// 夹具测试里 `\` 不是分隔符，用 `file_stem` 会把整条路径当成文件名。
pub fn display_name_for(path: &Path) -> Option<String> {
    let raw = path.to_string_lossy();
    let file_name = raw.rsplit(['\\', '/']).next().unwrap_or_default();
    let stem = if file_name.to_ascii_lowercase().ends_with(".lnk") {
        &file_name[..file_name.len() - ".lnk".len()]
    } else {
        file_name
    };
    let stem = stem.trim();
    (!stem.is_empty()).then(|| stem.to_string())
}

/// 相对根目录的分组目录名；直接在根下时为 `None`。
///
/// 分隔符统一为 `\`，保证在 Linux 上跑夹具测试时得到与 Windows 一致的字符串。
pub fn group_for(root: &Path, path: &Path) -> Option<String> {
    let raw = path.to_string_lossy().replace('/', "\\");
    let root = root.to_string_lossy().replace('/', "\\");
    let root = root.trim_end_matches('\\');
    let parent = raw.rsplit_once('\\')?.0;
    let relative = parent.strip_prefix(root)?;
    let text = relative.trim_matches('\\').to_string();
    (!text.is_empty()).then_some(text)
}

/// 递归发现 `root` 下的全部快捷方式，结果按路径稳定排序。
///
/// 目录联接（junction）与符号链接不跟随，避免 Windows 11 上
/// `%LOCALAPPDATA%` 联接导致的无限递归与条目重复。
pub fn discover_shortcuts(root: &Path) -> Vec<ShortcutFile> {
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort_by(|a, b| {
        normalize_key(&a.path.to_string_lossy()).cmp(&normalize_key(&b.path.to_string_lossy()))
    });
    out.dedup_by(|a, b| normalize_key(&a.path.to_string_lossy()) == normalize_key(&b.path.to_string_lossy()));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<ShortcutFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    children.sort_by_key(|path| normalize_key(&path.to_string_lossy()));

    for path in children {
        // 用 symlink_metadata：目录联接不被当成目录继续下钻。
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if metadata.is_dir() {
            if name.eq_ignore_ascii_case(STARTUP_DIR) {
                continue;
            }
            walk(root, &path, out);
        } else if is_shortcut_file(&name) {
            if let Some(display_name) = display_name_for(&path) {
                out.push(ShortcutFile {
                    path: path.clone(),
                    display_name,
                    group: group_for(root, &path),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "flashcast-winstart-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("创建夹具根目录");
        path
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("创建父目录");
        }
        std::fs::write(path, b"fixture").expect("写入夹具");
    }

    #[test]
    fn only_lnk_files_are_shortcuts() {
        assert!(is_shortcut_file("Word.lnk"));
        assert!(is_shortcut_file("WORD.LNK"));
        assert!(!is_shortcut_file("Word.url"));
        assert!(!is_shortcut_file("desktop.ini"));
        assert!(!is_shortcut_file("Word.lnk.bak"));
    }

    #[test]
    fn display_name_drops_only_the_extension() {
        let name = display_name_for(Path::new(r"C:\Start Menu\Programs\Microsoft Office\Word.lnk"));
        assert_eq!(name.as_deref(), Some("Word"));
        assert_eq!(
            display_name_for(Path::new("示例 应用.lnk")).as_deref(),
            Some("示例 应用")
        );
        assert_eq!(display_name_for(Path::new(".lnk")), None);
    }

    #[test]
    fn roots_are_built_from_env_and_deduped() {
        let user_base = r"C:\Users\me\AppData\Roaming";
        let common_base = r"C:\ProgramData";
        let roots = roots_from_env(Some(user_base), Some(common_base), None);
        assert_eq!(roots.len(), 2);
        let expected_user = Path::new(user_base)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu");
        let expected_common = Path::new(common_base)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu");
        assert_eq!(roots[0], expected_user);
        assert_eq!(roots[1], expected_common);

        // 同一个根以不同写法出现（大小写、尾部反斜杠、斜杠）必须只保留一个。
        let dupes = roots_from_env(
            Some(r"C:\Users\me\AppData\Roaming\"),
            None,
            Some(r"c:/users/me/appdata/roaming"),
        );
        assert_eq!(dupes.len(), 1, "重复根必须折叠：{dupes:?}");
        assert!(roots_from_env(Some(""), None, None).is_empty());
    }

    #[test]
    fn discovery_recurses_groups_and_skips_startup_and_url() {
        let root = temp_root("discover");
        touch(&root.join("Word.lnk"));
        touch(&root.join("Microsoft Office").join("Excel.lnk"));
        touch(&root.join("Programs").join("Startup").join("AutoStart.lnk"));
        touch(&root.join("Readme.url"));
        touch(&root.join("desktop.ini"));

        let found = discover_shortcuts(&root);
        let names: Vec<&str> = found.iter().map(|s| s.display_name.as_str()).collect();
        assert_eq!(names, vec!["Excel", "Word"], "只保留 .lnk，且按路径排序");

        let word = found.iter().find(|s| s.display_name == "Word").unwrap();
        assert_eq!(word.group, None);
        let excel = found.iter().find(|s| s.display_name == "Excel").unwrap();
        assert_eq!(excel.group.as_deref(), Some("Microsoft Office"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_root_yields_nothing_instead_of_error() {
        let missing = PathBuf::from("/definitely/missing/flashcast");
        assert!(discover_shortcuts(&missing).is_empty());
    }
}
