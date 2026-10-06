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
         "Default": {
            "name": "个人",
            "user_name": "me@example.com",
            "is_managed": 0,
            "hosted_domain": "NO_HOSTED_DOMAIN",
            "force_signin_profile_locked": false
         },
         "Profile 1": {
            "name": "工作",
            "is_managed": 1,
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
    assert!(
        !default.managed,
        "整数 0 + NO_HOSTED_DOMAIN 不能判成受管理（真实 Chrome 的形状）"
    );
    assert!(default.has_bookmarks && default.bookmarks_readable);

    let work = environment.profile("Profile 1").expect("Profile 1");
    assert_eq!(work.name, "工作", "显示名来自 profile.info_cache");
    assert!(work.managed, "整数 1 / 企业域必须视为企业管理");
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
fn account_only_profile_without_local_state_is_discovered() {
    let root = unique_dir("account-only-no-info");
    let udd = root.join("udd");
    write(&udd.join("Profile 2/AccountBookmarks"), BOOKMARKS);
    let (profiles, _warnings) = flashcast_platform::chrome::enumerate_profiles(&udd);
    let profile = profiles
        .iter()
        .find(|p| p.dir == "Profile 2")
        .expect("账号文件也是 profile 发现依据");
    assert!(profile.has_bookmarks && profile.bookmarks_readable);
    assert!(!profile.bookmarks.exists(), "保留旧的本地文件定位接口");
    std::fs::remove_dir_all(root).unwrap();
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

/// 真实机器上的发现检查（**默认忽略**）。
///
/// 只在 Linux 开发机上手动运行：它读取当前用户真实的 Chrome 用户数据目录与
/// `Local State`，但只取 `profile.info_cache`（显示名 / 账号 / 是否受管理），
/// **绝不读取** `os_crypt.encrypted_key`，也绝不写入任何 Chrome 文件。
/// 因此它不进 CI（runner 上没有真实 Chrome profile），也不会在普通 `cargo test` 里跑。
///
/// 运行方式：
///   cargo test -p flashcast-platform --test chrome_fixture real_machine_discovery -- --ignored --nocapture
#[cfg(target_os = "linux")]
#[test]
#[ignore = "需要开发机上的真实 Chrome profile；只读 profile.info_cache"]
fn real_machine_discovery() {
    use flashcast_platform::chrome::ChromeProvider;
    use flashcast_platform::linux::LinuxChromeProvider;

    let provider = LinuxChromeProvider::new();
    match provider.discover() {
        Ok(environment) => {
            println!("品牌: {:?}", environment.brand);
            println!("可执行文件: {}", environment.binary.display());
            println!(
                "用户数据目录: {}（来源 {:?}，需要 --user-data-dir: {}）",
                environment.user_data_dir.display(),
                environment.user_data_origin,
                environment.pass_user_data_dir
            );
            println!("profile 数: {}", environment.profiles.len());
            for profile in &environment.profiles {
                println!(
                    "  - 目录「{}」显示名「{}」账号 {:?} 管理 {} 有 Bookmarks {} 可读 {}",
                    profile.dir,
                    profile.name,
                    profile.user_name,
                    profile.managed,
                    profile.has_bookmarks,
                    profile.bookmarks_readable
                );
            }
            for warning in &environment.warnings {
                println!("警告: {warning}");
            }
        }
        Err(error) => println!("发现失败: {error}"),
    }
}

/// 真实 Chrome 的**启动**检查（默认忽略，Linux 开发机手动运行）。
///
/// 它调用的是产品代码路径 `chrome::spawn_chrome`（spawn 后立即返回、不等待、不看退出码），
/// 只证明「Chrome 进程被真实启动了」，**不**证明页面打开。为了不在桌面上弹出窗口，
/// 这里额外加了 `--headless=new`；`--dump-dom` 的页面加载证据见 ticket 13 的 Comments
/// 里单独记录的 CLI 检查。
///
/// 运行方式：
///   cargo test -p flashcast-platform --test chrome_fixture real_chrome_spawn -- --ignored --nocapture
#[cfg(target_os = "linux")]
#[test]
#[ignore = "会在真实机器上启动一次 headless Chrome（使用 /tmp 下的一次性 user-data-dir）"]
fn real_chrome_spawn_starts_a_process() {
    use flashcast_platform::chrome::{build_open_args, spawn_chrome, ChromeLaunchRequest};

    let binary = Path::new("/usr/bin/google-chrome");
    if !binary.is_file() {
        println!("跳过：本机没有 /usr/bin/google-chrome");
        return;
    }
    let root = unique_dir("real-spawn");
    let udd = root.join("udd");
    std::fs::create_dir_all(udd.join("Default")).expect("创建一次性 user-data-dir");

    // **必须**显式传 `--user-data-dir` 指向一次性目录：本检查绝不可以使用用户真实的
    // Chrome profile（第一次写这个用例时漏了它，Chrome 于是用了 ~/.config/google-chrome）。
    // 产品代码里「默认位置不传该开关」是对的（用户就是要用自己的 profile），但测试不是。
    let mut args = build_open_args("Default", Some(&udd), "about:blank");
    let url = args.pop().expect("URL 是最后一个元素");
    args.push("--headless=new".to_string());
    args.push("--disable-gpu".to_string());
    args.push(url);
    let request = ChromeLaunchRequest::new(binary, args);

    let launch = spawn_chrome(&request).expect("真实 Chrome 必须能被启动");
    println!("argv = {:?}", launch.argv);
    assert!(launch.pid.is_some(), "启动成功后应给出 pid");

    // 真实进程确实验证：给它时间写出用户数据目录里的文件。
    let marker = udd.join("Local State");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline && !marker.is_file() {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    println!(
        "一次性 user-data-dir 里是否出现 Local State: {}",
        marker.is_file()
    );
    assert!(
        marker.is_file(),
        "被启动的 Chrome 必须真的使用了这个 --user-data-dir"
    );
    let _ = std::fs::remove_dir_all(&root);
}
