//! 真实 freedesktop 解析器的夹具测试。
//!
//! 该测试**不依赖 Linux 专有 API**，因此在任意平台（含无桌面会话的 CI runner）
//! 上都会运行：它用临时目录中的真实 `.desktop` 文件驱动
//! `flashcast_platform::freedesktop` 的真实解析与扫描实现。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use flashcast_platform::freedesktop::scan::{scan_desktop_dirs, ScanOptions, SkipReason};
use flashcast_platform::freedesktop::xdg::XdgDirs;
use flashcast_platform::freedesktop::{parse_desktop_entry, parse_exec, tokenize_exec};
use flashcast_platform::hotkey::HotkeySpec;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 一个用完即删的临时目录。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "flashcast-test-{}-{}-{}",
            std::process::id(),
            label,
            unique
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, content: &str) -> PathBuf {
        let target = self.path.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("创建父目录");
        }
        std::fs::write(&target, content).expect("写入夹具文件");
        target
    }

    fn touch(&self, relative: &str) -> PathBuf {
        self.write(relative, "fixture")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn scan_options(apps_dir: &Path, extra: &[PathBuf]) -> ScanOptions {
    let mut applications_dirs = vec![apps_dir.to_path_buf()];
    applications_dirs.extend(extra.iter().cloned());
    ScanOptions {
        applications_dirs,
        icon_theme_roots: Vec::new(),
        icon_flat_dirs: Vec::new(),
        icon_themes: vec!["hicolor".to_string()],
        locale_candidates: vec!["zh_CN".to_string(), "zh".to_string()],
        path_env: Some(String::new()),
    }
}

/// 解析真实 `.desktop` 文件：本地化名称、说明、图标、窗口类名与关键词。
#[test]
fn parses_fixture_desktop_entries_with_localized_name() {
    let dir = TempDir::new("parse");
    dir.write(
        "applications/org.example.Editor.desktop",
        "\
[Desktop Entry]
Type=Application
Name=Example Editor
Name[zh_CN]=示例编辑器
Comment=Edit text files
Comment[zh_CN]=编辑文本文件
Exec=example-editor %U
Icon=example-editor
StartupWMClass=ExampleEditor
Keywords=editor;text;
Terminal=false
",
    );

    let outcome = scan_desktop_dirs(&scan_options(&dir.path().join("applications"), &[]));

    assert_eq!(outcome.files_seen, 1);
    assert_eq!(outcome.entries.len(), 1);
    let entry = &outcome.entries[0];
    assert_eq!(entry.id, "org.example.Editor.desktop");
    assert_eq!(entry.name, "示例编辑器", "必须优先使用 Name[zh_CN]");
    assert_eq!(entry.comment.as_deref(), Some("编辑文本文件"));
    assert_eq!(entry.wm_class.as_deref(), Some("ExampleEditor"));
    assert_eq!(entry.keywords, vec!["editor".to_string(), "text".to_string()]);
    assert!(!entry.terminal);
    assert_eq!(entry.icon.as_ref().map(|icon| icon.name.as_str()), Some("example-editor"));
    assert_eq!(entry.exec, vec!["example-editor".to_string()], "%U 无对应参数应丢弃");
}

/// 没有本地化条目时回退到基础 `Name`。
#[test]
fn falls_back_to_base_name_without_localized_entry() {
    let dir = TempDir::new("fallback");
    dir.write(
        "applications/plain.desktop",
        "[Desktop Entry]\nType=Application\nName=Plain App\nExec=plain\n",
    );

    let outcome = scan_desktop_dirs(&scan_options(&dir.path().join("applications"), &[]));

    assert_eq!(outcome.entries.len(), 1);
    assert_eq!(outcome.entries[0].name, "Plain App");
}

/// `NoDisplay` / `Hidden` / 非 Application / TryExec 缺失都必须跳过，并记录原因。
#[test]
fn skips_nodisplay_hidden_non_application_and_missing_tryexec() {
    let dir = TempDir::new("skips");
    let apps = dir.path().join("applications");
    dir.write(
        "applications/nodisplay.desktop",
        "[Desktop Entry]\nType=Application\nName=隐藏项\nExec=hidden-app\nNoDisplay=true\n",
    );
    dir.write(
        "applications/hidden.desktop",
        "[Desktop Entry]\nType=Application\nName=已删除\nExec=gone\nHidden=true\n",
    );
    dir.write(
        "applications/link.desktop",
        "Type=Link\nName=网页链接\nURL=https://example.com\n",
    );
    dir.write(
        "applications/tryexec.desktop",
        "[Desktop Entry]\nType=Application\nName=未安装\nExec=not-installed\nTryExec=definitely-not-here\n",
    );
    dir.write(
        "applications/ok.desktop",
        "[Desktop Entry]\nType=Application\nName=正常软件\nExec=ok-app\n",
    );

    let outcome = scan_desktop_dirs(&scan_options(&apps, &[]));

    assert_eq!(outcome.entries.len(), 1, "只有正常条目应保留");
    assert_eq!(outcome.entries[0].name, "正常软件");
    let reasons: Vec<SkipReason> = outcome.skipped.iter().map(|skip| skip.reason).collect();
    assert!(reasons.contains(&SkipReason::NoDisplay));
    assert!(reasons.contains(&SkipReason::Hidden));
    assert!(reasons.contains(&SkipReason::NotApplication));
    assert!(reasons.contains(&SkipReason::TryExecMissing));
}

/// 缺少可用 `Exec` 的条目不能启动，应被跳过。
#[test]
fn skips_entry_without_usable_exec() {
    let dir = TempDir::new("noexec");
    let apps = dir.path().join("applications");
    dir.write("applications/noexec.desktop", "[Desktop Entry]\nType=Application\nName=无命令\nExec=%U\n");

    let outcome = scan_desktop_dirs(&scan_options(&apps, &[]));

    assert!(outcome.entries.is_empty());
    assert_eq!(outcome.skipped.len(), 1);
    assert_eq!(outcome.skipped[0].reason, SkipReason::EmptyExec);
}

/// 同一个 desktop file ID 只保留优先级更高的目录中的那一份。
#[test]
fn duplicate_desktop_id_keeps_higher_priority_directory() {
    let dir = TempDir::new("dup");
    let high = dir.path().join("high/applications");
    let low = dir.path().join("low/applications");
    dir.write("high/applications/dup.desktop", "[Desktop Entry]\nType=Application\nName=高优先级\nExec=high\n");
    dir.write("low/applications/dup.desktop", "[Desktop Entry]\nType=Application\nName=低优先级\nExec=low\n");

    let outcome = scan_desktop_dirs(&scan_options(&high, &[low]));

    assert_eq!(outcome.entries.len(), 1);
    assert_eq!(outcome.entries[0].name, "高优先级");
    assert!(outcome
        .skipped
        .iter()
        .any(|skip| skip.reason == SkipReason::DuplicateId));
}

/// 子目录中的条目 id 用 `-` 连接，`Exec` 字段码按规范展开。
#[test]
fn nested_entry_id_and_exec_field_codes() {
    let dir = TempDir::new("fieldcodes");
    let apps = dir.path().join("applications");
    let file = dir.write(
        "applications/kde4/field.desktop",
        "[Desktop Entry]\nType=Application\nName=字段码\nIcon=field-icon\nExec=field-app --icon %i --name %c --file %f %U %%\n",
    );

    let outcome = scan_desktop_dirs(&scan_options(&apps, &[]));

    assert_eq!(outcome.entries.len(), 1);
    let entry = &outcome.entries[0];
    assert_eq!(entry.id, "kde4-field.desktop", "子目录用 - 连接");
    assert_eq!(
        entry.exec,
        vec![
            "field-app".to_string(),
            "--icon".to_string(),
            "field-icon".to_string(),
            "--name".to_string(),
            "字段码".to_string(),
            "--file".to_string(),
            "%".to_string(),
        ],
        "%i 展开为两个参数，%f/%U 整段丢弃，%% 展开为字面量 %"
    );
    assert_eq!(entry.desktop_file.as_deref(), Some(file.as_path()));
}

/// 分词规则：双引号内空白不分割，`\\` 可转义保留字符。
#[test]
fn exec_tokenization_follows_freedesktop_rules() {
    assert_eq!(
        tokenize_exec(r#"/usr/bin/app --title "两个 词" plain"#),
        vec!["/usr/bin/app", "--title", "两个 词", "plain"]
    );
    assert_eq!(
        tokenize_exec(r#"/usr/bin/app "带\"引号" \`转义\`"#),
        vec!["/usr/bin/app", "带\"引号", "`转义`"]
    );
    assert_eq!(tokenize_exec(""), Vec::<String>::new());
    assert_eq!(
        parse_exec("app %k", Some(Path::new("/tmp/x.desktop")), None, None),
        vec!["app".to_string(), "/tmp/x.desktop".to_string()]
    );
}

/// 图标解析顺序：主题目录 → 其他主题 → `/usr/share/pixmaps` 风格的平铺目录。
#[test]
fn icon_resolution_prefers_theme_then_hicolor_then_flat_dir() {
    let dir = TempDir::new("icons");
    let apps = dir.path().join("applications");
    let theme_root = dir.path().join("icons");
    let flat = dir.path().join("pixmaps");
    dir.touch("icons/hicolor/128x128/apps/themed.png");
    dir.touch("icons/hicolor/48x48/apps/themed.png");
    dir.touch("icons/hicolor/scalable/apps/vector.svg");
    dir.touch("icons/OtherTheme/128x128/apps/other.png");
    dir.touch("pixmaps/flat.png");
    dir.write(
        "applications/icons.desktop",
        "[Desktop Entry]\nType=Application\nName=图标软件\nExec=icons-app\nIcon=themed\n",
    );
    dir.write(
        "applications/vector.desktop",
        "[Desktop Entry]\nType=Application\nName=矢量图标\nExec=vector-app\nIcon=vector\n",
    );
    dir.write(
        "applications/flat.desktop",
        "[Desktop Entry]\nType=Application\nName=平铺图标\nExec=flat-app\nIcon=flat\n",
    );
    dir.write(
        "applications/absolute.desktop",
        &format!(
            "[Desktop Entry]\nType=Application\nName=绝对路径图标\nExec=abs-app\nIcon={}\n",
            dir.path().join("pixmaps/flat.png").display()
        ),
    );

    let options = ScanOptions {
        applications_dirs: vec![apps],
        icon_theme_roots: vec![theme_root],
        icon_flat_dirs: vec![flat],
        icon_themes: vec!["hicolor".to_string()],
        locale_candidates: Vec::new(),
        path_env: Some(String::new()),
    };
    let outcome = scan_desktop_dirs(&options);

    assert_eq!(outcome.entries.len(), 4);
    assert!(!outcome.icon_index_empty);

    let by_name = |name: &str| {
        outcome
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("找不到 {name}"))
    };
    assert_eq!(
        by_name("图标软件").icon.as_ref().unwrap().path.as_deref(),
        Some(dir.path().join("icons/hicolor/128x128/apps/themed.png").as_path()),
        "应优先选择尺寸更合适的主题图标"
    );
    assert_eq!(
        by_name("矢量图标").icon.as_ref().unwrap().path.as_deref(),
        Some(dir.path().join("icons/hicolor/scalable/apps/vector.svg").as_path())
    );
    assert_eq!(
        by_name("平铺图标").icon.as_ref().unwrap().path.as_deref(),
        Some(dir.path().join("pixmaps/flat.png").as_path())
    );
    assert_eq!(
        by_name("绝对路径图标").icon.as_ref().unwrap().path.as_deref(),
        Some(dir.path().join("pixmaps/flat.png").as_path())
    );
}

/// XDG 目录优先级：`$XDG_DATA_HOME` → `$XDG_DATA_DIRS` → 显式系统目录 → 导出目录，且去重。
#[test]
fn xdg_application_dirs_are_ordered_and_deduplicated() {
    let dirs = XdgDirs::from_values(
        Some("/home/u"),
        Some("/home/u/.local/share"),
        Some("/usr/share:/home/u/.local/share"),
        None,
        None,
    );

    let apps = dirs.application_dirs();

    assert_eq!(apps[0], PathBuf::from("/home/u/.local/share/applications"));
    assert_eq!(apps[1], PathBuf::from("/usr/share/applications"));
    assert_eq!(
        apps.iter()
            .filter(|path| **path == PathBuf::from("/home/u/.local/share/applications"))
            .count(),
        1,
        "重复路径必须去重"
    );
    assert!(apps.contains(&PathBuf::from("/usr/local/share/applications")));
    assert!(apps.contains(&PathBuf::from(
        "/var/lib/flatpak/exports/share/applications"
    )));
    assert!(apps.contains(&PathBuf::from(
        "/var/lib/snapd/desktop/applications"
    )));

    // 图标主题始终以 hicolor 兜底。
    let themes = dirs.icon_themes(None, Some("Adwaita:dark"));
    assert_eq!(themes.first().map(String::as_str), Some("Adwaita"));
    assert_eq!(themes.last().map(String::as_str), Some("hicolor"));
}

/// 本地化候选语言推导。
#[test]
fn locale_candidates_follow_lang_priority() {
    use flashcast_platform::freedesktop::locale_candidates_from_env;

    assert_eq!(
        locale_candidates_from_env(Some("zh_CN.UTF-8"), None, None),
        vec!["zh_CN".to_string(), "zh".to_string()]
    );
    assert_eq!(
        locale_candidates_from_env(Some("en_US.UTF-8"), Some("zh_CN.UTF-8"), None),
        vec!["zh_CN".to_string(), "zh".to_string()],
        "LC_MESSAGES 优先于 LANG"
    );
    assert!(locale_candidates_from_env(Some("C"), None, None).is_empty());
}

/// 快捷键语法：接受常见写法，拒绝无修饰键与未知键。
#[test]
fn hotkey_spec_parsing_accepts_and_rejects() {
    let spec = HotkeySpec::parse("ctrl+alt+space").expect("应可解析");
    assert_eq!(spec.canonical(), "Ctrl+Alt+Space");
    assert_eq!(spec.modifiers.len(), 2);
    assert_eq!(spec.key, flashcast_platform::Key::Space);

    assert_eq!(
        HotkeySpec::parse("Super+Space").expect("应可解析").canonical(),
        "Super+Space"
    );
    assert_eq!(
        HotkeySpec::parse("CmdOrCtrl+K").expect("应可解析").canonical(),
        "Super+K",
        "CmdOrCtrl 规范为 Super"
    );
    assert_eq!(
        HotkeySpec::parse("Ctrl+Shift+F12").expect("应可解析").canonical(),
        "Ctrl+Shift+F12"
    );

    assert!(HotkeySpec::parse("Space").is_err(), "缺少修饰键必须被拒绝");
    assert!(HotkeySpec::parse("").is_err());
    assert!(HotkeySpec::parse("Ctrl+").is_err());
    assert!(HotkeySpec::parse("Ctrl+Unknown").is_err());
    assert!(HotkeySpec::parse("Ctrl+F99").is_err());
}

/// 解析器直接面对文件内容时也只读 `[Desktop Entry]` 分组。
#[test]
fn parser_only_reads_the_desktop_entry_group() {
    let parsed = parse_desktop_entry(
        "\
[Desktop Action new]
Name=新建窗口
Exec=app --new

[Desktop Entry]
Type=Application
Name=主名称
Exec=app
",
        &[],
    );

    assert_eq!(parsed.fields.name.as_deref(), Some("主名称"));
    assert!(parsed.fields.is_launchable_application());
}
