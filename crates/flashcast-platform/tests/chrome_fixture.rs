//! Chrome 发现的真实夹具检查。
//!
//! 这里不启动任何浏览器、也不接触用户的真实 Chrome profile：所有输入都是本测试自己
//! 写在临时目录里的 `Local State` 与 `Bookmarks` 样本，发现逻辑走的是
//! `chrome::discover_from_paths` 与 `chrome::enumerate_profiles` 的**真实实现**。
//!
//! 未覆盖：真正的 Chrome 进程启动与 `--user-data-dir` 传给真实 Chrome 的行为。参数
//! 向量由 `crates/flashcast-core/tests/chrome.rs` 断言，真实启动在 Linux 机器上另行检查
//! 并在 ticket 13 的 Comments 里如实记录。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use flashcast_platform::chrome::{
    discover_from_paths, BinaryCandidate, ChromeBrand, ChromeError, UserDataCandidate,
    UserDataOrigin,
};

/// 每个夹具一个进程内唯一的临时目录。
fn unique_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let index = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "flashcast-chrome-fixture-{prefix}-{}-{index}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("创建父目录");
    }
    std::fs::write(path, text).expect("写入夹具文件");
}

/// 一份真实的 `Local State` 片段：两个 profile，其中一个是工作 profile。
const LOCAL_STATE: &str = r#"{
   "os_crypt": { "encrypted_key": "本测试不会读取它" },
   "profile": {
      "info_cache": {
         "Default": { "name": "个人", "user_name": "me@example.com" },
         "Profile 1": {
            "name": "工作",
            "is_managed": true,
            "hosted_domain": "corp.example"
         }
      }
   }
}"#;

/// 一份带目录、非 ASCII 标题与查询串的 `Bookmarks` 样本。
const BOOKMARKS: &str = r#"{
   "checksum": "5a1f7c0d9e2b4a6f8c1d3e5f7a9b0c2d",
   "checksum_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
   "roots": {
      "bookmark_bar": {
         "children": [ {
            "date_added": "13341234567890123",
            "guid": "3a5b7c9d-1e2f-4a6b-8c0d-2e4f6a8b0c1d",
            "id": "7",
            "name": "Rust 官网",
            "type": "url",
            "url": "https://www.rust-lang.org/"
         } ],
         "id": "1",
         "name": "书签栏",
         "type": "folder"
      },
      "other": { "children": [], "id": "2", "name": "其他书签", "type": "folder" },
      "synced": { "children": [], "id": "3", "name": "移动设备书签", "type": "folder" }
   },
   "sync_metadata": "ignored",
   "version": 1
}"#;

/// 构造一台「Chrome 已安装」的机器：可执行文件 + 用户数据目录 + 两个 profile。
fn machine(prefix: &str) -> (PathBuf, Vec<BinaryCandidate>, Vec<UserDataCandidate>) {
    let root = unique_dir(prefix);
    let binary = root.join("bin").join("google-chrome");
    write(&binary, "#!/bin/sh\n");
    let udd = root.join("udd");
    write(&udd.join("Local State"), LOCAL_STATE);
    write(&udd.join("Default").join("Bookmarks"), BOOKMARKS);
    write(&udd.join("Profile 1").join("Preferences"), "{}");
    (
        root,
        vec![BinaryCandidate::new(ChromeBrand::Chrome, binary)],
        vec![UserDataCandidate::new(
            ChromeBrand::Chrome,
            udd,
            UserDataOrigin::Default,
        )],
    )
}

#[test]
fn discovery_finds_binary_profiles_and_display_names() {
    let (root, binaries, user_data) = machine("discover");
    let environment = discover_from_paths(&binaries, &user_data).expect("必须发现 Chrome");

    assert_eq!(environment.brand, ChromeBrand::Chrome);
    assert!(environment.binary.is_file());
    assert_eq!(environment.user_data_dir, root.join("udd"));
    assert!(
        !environment.pass_user_data_dir,
        "默认目录不需要 --user-data-dir"
    );
    assert!(
        environment.warnings.is_empty(),
        "干净的夹具不应有警告：{:?}",
        environment.warnings
    );

    let default = environment.profile("Default").expect("Default profile");
    assert_eq!(default.name, "个人");
    assert_eq!(default.user_name.as_deref(), Some("me@example.com"));
    assert!(!default.managed);
    assert!(default.has_bookmarks && default.bookmarks_readable);

    let work = environment.profile("Profile 1").expect("Profile 1");
    assert_eq!(work.name, "工作", "显示名来自 profile.info_cache");
    assert!(work.managed, "hosted_domain 必须视为企业管理");
    assert!(
        !work.has_bookmarks,
        "只有 Preferences 的 profile 还没有 Bookmarks，这是正常空状态"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn profile_directory_without_local_state_entry_is_still_listed() {
    let (root, binaries, user_data) = machine("no-info");
    // info_cache 里没有它，但目录里有 Preferences：必须仍然被枚举出来。
    let udd = root.join("udd");
    write(&udd.join("Profile 9").join("Preferences"), "{}");
    write(
        &udd.join("Local State"),
        r#"{"profile":{"info_cache":{"Default":{"name":"个人"}}}}"#,
    );
    let environment = discover_from_paths(&binaries, &user_data).expect("必须发现 Chrome");
    let extra = environment
        .profile("Profile 9")
        .expect("目录枚举必须补上它");
    assert_eq!(
        extra.name, "Profile 9",
        "info_cache 里没有名字时退化为目录名"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn corrupt_local_state_is_a_warning_and_directory_scan_still_works() {
    let (root, binaries, user_data) = machine("corrupt-local-state");
    write(&root.join("udd").join("Local State"), "{ 这不是 JSON");
    let environment = discover_from_paths(&binaries, &user_data).expect("必须发现 Chrome");
    assert!(
        environment
            .warnings
            .iter()
            .any(|warning| warning.contains("Local State")),
        "Local State 损坏必须如实报告：{:?}",
        environment.warnings
    );
    assert!(
        environment.profile("Default").is_some(),
        "退回到目录枚举后仍然要能找到 Default"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_binary_reports_searched_paths() {
    let root = unique_dir("no-binary");
    let binaries = vec![BinaryCandidate::new(
        ChromeBrand::Chrome,
        root.join("bin/google-chrome"),
    )];
    let user_data = vec![UserDataCandidate::new(
        ChromeBrand::Chrome,
        root.join("udd"),
        UserDataOrigin::Default,
    )];
    match discover_from_paths(&binaries, &user_data) {
        Err(ChromeError::NotInstalled { searched }) => {
            assert!(
                searched.contains("google-chrome"),
                "未安装的提示要列出搜索过的路径：{searched}"
            );
        }
        other => panic!("必须报告未安装，实际 {other:?}"),
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn installed_but_never_started_is_an_empty_state_with_a_warning() {
    let root = unique_dir("no-udd");
    let binary = root.join("bin/google-chrome");
    write(&binary, "#!/bin/sh\n");
    let binaries = vec![BinaryCandidate::new(ChromeBrand::Chrome, binary)];
    let user_data = vec![UserDataCandidate::new(
        ChromeBrand::Chrome,
        root.join("udd"),
        UserDataOrigin::Default,
    )];
    let environment = discover_from_paths(&binaries, &user_data).expect("Chrome 已安装");
    assert!(environment.profiles.is_empty());
    assert!(
        environment
            .warnings
            .iter()
            .any(|warning| warning.contains("用户数据目录")),
        "没有用户数据目录时必须给出说明：{:?}",
        environment.warnings
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn custom_user_data_dir_requires_the_switch() {
    let (root, binaries, _) = machine("custom");
    let user_data = vec![UserDataCandidate::new(
        ChromeBrand::Chrome,
        root.join("udd"),
        UserDataOrigin::Custom,
    )];
    let environment = discover_from_paths(&binaries, &user_data).expect("必须发现 Chrome");
    assert!(
        environment.pass_user_data_dir,
        "默认位置之外的目录必须显式传 --user-data-dir"
    );
    std::fs::remove_dir_all(&root).ok();
}
