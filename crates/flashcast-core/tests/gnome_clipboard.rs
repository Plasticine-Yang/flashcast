#![cfg(target_os = "linux")]
//! 由 scripts/gnome/check-clipboard-bridge.sh 在私有总线启动真实生产桥接。
//! 只替换前台应用、写入与其它平台能力；监听器穿透真实 D-Bus / Mutter Selection。
mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use flashcast_core::{Host, HostDeps, PluginRegistry};
use flashcast_platform::clipboard::ClipboardWatcher;
use flashcast_platform::fake::{
    FakeAppCatalog, FakeCapabilityProbe, FakeClipboard, FakeFocusTracker, FakeLauncher, FakePaster,
};
use flashcast_platform::linux::LinuxClipboardWatcher;
use flashcast_platform::SessionType;
use gio::glib::variant::ToVariant;

fn fixture(method: &str, mode: Option<&str>) {
    let bus = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).unwrap();
    bus.call_sync(
        Some("org.gnome.Shell.Extensions.FlashcastClipboard"),
        "/org/flashcast/ClipboardFixture",
        "org.flashcast.ClipboardFixture",
        method,
        mode.map(|mode| (mode,).to_variant()).as_ref(),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        1500,
        None::<&gio::Cancellable>,
    )
    .unwrap();
}

fn wait_for(message: &str, predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !predicate() {
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "需要 scripts/gnome/check-clipboard-bridge.sh 启动私有 GNOME 桥接"]
fn gnome_bridge_captures_formats_through_host_and_obeys_pause_disable_and_own_writes() {
    assert_eq!(
        std::env::var("FLASHCAST_GNOME_CLIPBOARD_CHECK").as_deref(),
        Ok("1"),
        "请通过 scripts/gnome/check-clipboard-bridge.sh 在私有总线运行"
    );
    let device = tempfile::tempdir().unwrap();
    let watcher = Arc::new(LinuxClipboardWatcher::with_session(
        SessionType::Wayland,
        false,
    ));
    assert!(watcher.check_background_support().is_ok());
    let host = Host::new(
        HostDeps {
            catalog: Arc::new(FakeAppCatalog::with_apps(Vec::new())),
            launcher: Arc::new(FakeLauncher::always_succeeds()),
            capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
            clipboard: Arc::new(FakeClipboard::new()),
            clipboard_watcher: watcher.clone(),
            chrome: support::no_chrome(),
            focus: Arc::new(FakeFocusTracker::default()),
            paster: Arc::new(FakePaster::new()),
            plugins: Arc::new(PluginRegistry::new()),
            device_dir: device.path().to_path_buf(),
        },
        support::fast_settings(),
    );
    host.install_official_plugins();
    host.set_plugin_enabled("clipboard", true).unwrap();
    // 激活订阅后才生成复制事件。
    watcher.poll().unwrap();
    fixture("Copy", Some("桥接文字"));
    wait_for("文字必须入库", || {
        host.clipboard_entries(Some("桥接文字")).len() == 1
    });
    let response = support::plugin_query(&host, "剪贴板");
    assert_eq!(response.items.len(), 1, "记录必须能经宿主查询显示");
    let event = host.clipboard_entries(Some("桥接文字")).remove(0);
    assert_eq!(event.source.unwrap().app_id, "fixture.desktop");

    fixture("Copy", Some("rich"));
    wait_for("富文本必须入库", || {
        host.clipboard_entries(Some("桥接富文本")).len() == 1
    });
    let event = host.clipboard_entries(Some("桥接富文本")).remove(0);
    for kind in ["text", "html", "rtf"] {
        assert!(event.formats.iter().any(|f| f.tag() == kind), "缺少 {kind}");
    }
    fixture("Copy", Some("image"));
    wait_for("图片必须入库", || {
        host.clipboard_entries(None)
            .iter()
            .any(|e| e.formats.iter().any(|f| f.tag() == "image"))
    });
    fixture("Copy", Some("files"));
    wait_for("文件列表必须入库", || {
        host.clipboard_entries(None)
            .iter()
            .any(|e| e.formats.iter().any(|f| f.tag() == "files"))
    });

    watcher.note_own_write("own-bridge");
    fixture("Copy", Some("own-bridge"));
    std::thread::sleep(Duration::from_millis(600));
    assert!(host.clipboard_entries(Some("own-bridge")).is_empty());
    let before_pause = host.clipboard_entries(None).len();
    host.set_clipboard_paused(true).unwrap();
    fixture("Copy", Some("暂停复制不保存"));
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(host.clipboard_entries(None).len(), before_pause);
    host.set_clipboard_paused(false).unwrap();
    watcher.poll().unwrap();
    fixture("Copy", Some("恢复后新复制"));
    wait_for("恢复后必须记录", || {
        host.clipboard_entries(Some("恢复后新复制")).len() == 1
    });
    assert!(host.clipboard_entries(Some("暂停复制不保存")).is_empty());

    host.set_plugin_enabled("clipboard", false).unwrap();
    assert!(!host.clipboard_capture_active());
    fixture("Copy", Some("停用复制不保存"));
    host.set_plugin_enabled("clipboard", true).unwrap();
    watcher.poll().unwrap();
    fixture("Copy", Some("重新启用后复制"));
    wait_for("重新启用后必须记录", || {
        host.clipboard_entries(Some("重新启用后复制")).len() == 1
    });
    assert!(host.clipboard_entries(Some("停用复制不保存")).is_empty());

    fixture("Quit", None);
    wait_for("扩展断开必须报告原因", || {
        host.clipboard_state().last_error.is_some()
    });
    assert!(host.clipboard_state().last_error.unwrap().contains("GNOME"));
    assert_eq!(
        support::plugin_query(&host, "剪贴板").items.len(),
        6,
        "断开后保留已有历史"
    );
    host.stop_clipboard_capture();
}
