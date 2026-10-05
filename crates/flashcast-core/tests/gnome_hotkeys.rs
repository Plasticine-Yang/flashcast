//! 隔离的真实 GSettings 和 D-Bus，经宿主入口检查冲突、回滚与撤销保护。
#![cfg(target_os = "linux")]
mod support;
use ashpd::zbus::{self, zvariant::OwnedValue};
use flashcast_platform::{linux::LinuxHotkeyManager, SessionType};
use gio::{glib::variant::ToVariant, prelude::*};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct Rebind {
    calls: Arc<AtomicUsize>,
    fail: bool,
}
#[zbus::interface(name = "org.gnome.GlobalShortcutsRebind", crate = "ashpd::zbus")]
impl Rebind {
    fn rebind_shortcuts(
        &self,
        app_id: String,
        shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
    ) -> zbus::fdo::Result<()> {
        assert_eq!(app_id, "dev.flashcast.launcher");
        assert!(shortcuts.iter().any(|(id, _)| id == "summon"));
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail && call == 0 {
            return Err(zbus::fdo::Error::Failed("模拟首次重新绑定失败".into()));
        }
        Ok(())
    }
}

#[test]
fn gnome_conflict_resolution_through_host() {
    let empty_schemas = tempfile::tempdir().unwrap();
    let schemas = tempfile::tempdir().unwrap();
    std::fs::write(schemas.path().join("hotkeys.gschema.xml"), r#"<schemalist>
      <schema id="org.gnome.desktop.wm.keybindings" path="/org/gnome/desktop/wm/keybindings/"><key name="activate-window-menu" type="as"><default>['&lt;Alt&gt;space']</default></key></schema>
      <schema id="org.gnome.settings-daemon.global-shortcuts.application"><key name="shortcuts" type="a(sa{sv})"><default>[]</default></key></schema>
    </schemalist>"#).unwrap();
    assert!(Command::new("glib-compile-schemas")
        .arg(schemas.path())
        .status()
        .unwrap()
        .success());
    for mode in [
        "success",
        "failure",
        "missing",
        "backup-failure",
        "stale-undo",
        "stale-app-undo",
        "mismatch",
        "empty",
        "nongnome",
        "missing-schema",
    ] {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let connection = rt.block_on(async {
            if mode == "missing" {
                return None;
            }
            Some(
                zbus::connection::Builder::address(address.trim())
                    .unwrap()
                    .name("org.freedesktop.impl.portal.desktop.gnome")
                    .unwrap()
                    .serve_at(
                        "/org/gnome/globalshortcuts",
                        Rebind {
                            calls: calls.clone(),
                            fail: mode == "failure",
                        },
                    )
                    .unwrap()
                    .build()
                    .await
                    .unwrap(),
            )
        });
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "gnome_client", "--nocapture"])
            .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
            .env("GSETTINGS_BACKEND", "memory")
            .env(
                "GSETTINGS_SCHEMA_DIR",
                if mode == "missing-schema" {
                    empty_schemas.path()
                } else {
                    schemas.path()
                },
            )
            .env("XDG_DATA_DIRS", empty_schemas.path())
            .env("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")
            .env("FLASHCAST_GNOME_MODE", mode)
            .output()
            .unwrap();
        drop(connection);
        let _ = daemon.kill();
        let _ = daemon.wait();
        assert!(
            output.status.success(),
            "{mode}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if mode == "failure" {
            assert_eq!(
                calls.load(Ordering::SeqCst),
                2,
                "失败后必须恢复并重新绑定原值"
            );
        }
        if mode == "backup-failure" {
            assert_eq!(calls.load(Ordering::SeqCst), 0, "备份失败不得修改系统");
        }
    }
}

#[test]
#[ignore = "由父测试在独立总线和内存 GSettings 中执行"]
fn gnome_client() {
    let mode = std::env::var("FLASHCAST_GNOME_MODE").unwrap();
    if mode == "missing-schema" {
        let (host, _, _) = support::host_with_device(vec![], flashcast_core::Settings::default());
        let manager = LinuxHotkeyManager::with_session(SessionType::Wayland, false);
        let report = host.hotkey_conflict(&manager);
        assert_eq!(report.status, "unknown");
        assert!(!report.can_resolve);
        assert!(host.resolve_hotkey_conflict(&manager, false).is_err());
        return;
    }
    let source = gio::SettingsSchemaSource::default().unwrap();
    let menu = gio::Settings::new_full(
        &source
            .lookup("org.gnome.desktop.wm.keybindings", true)
            .unwrap(),
        None::<&gio::SettingsBackend>,
        None,
    );
    let app = gio::Settings::new_full(
        &source
            .lookup(
                "org.gnome.settings-daemon.global-shortcuts.application",
                true,
            )
            .unwrap(),
        None::<&gio::SettingsBackend>,
        Some("/org/gnome/settings-daemon/global-shortcuts/dev.flashcast.launcher/"),
    );
    let menu_before = vec![
        "<Alt>space",
        "<Super>F10",
        "<Mod1>space",
        "<Shift><Alt>space",
    ];
    menu.set_strv(
        "activate-window-menu",
        if mode == "mismatch" {
            vec!["<Super>F10", "<Shift><Alt>space"]
        } else {
            menu_before.clone()
        },
    )
    .unwrap();
    let bindings = vec![
        (
            "summon",
            HashMap::from([
                ("description", "打开 Flashcast".to_variant()),
                ("shortcuts", vec!["<Control><Alt>space"].to_variant()),
            ]),
        ),
        (
            "other",
            HashMap::from([
                ("description", "保留其他操作".to_variant()),
                ("shortcuts", vec!["<Super>F9"].to_variant()),
            ]),
        ),
    ]
    .to_variant();
    app.set_value("shortcuts", &bindings).unwrap();
    if mode == "empty" {
        app.set_value(
            "shortcuts",
            &Vec::<(String, HashMap<String, gio::glib::Variant>)>::new().to_variant(),
        )
        .unwrap();
    }
    let before_menu = menu.value("activate-window-menu");
    let (host, _, device) = support::host_with_device(vec![], flashcast_core::Settings::default());
    let manager = LinuxHotkeyManager::with_session(SessionType::Wayland, false);
    if mode == "nongnome" {
        std::env::set_var("XDG_CURRENT_DESKTOP", "KDE");
        assert_eq!(host.hotkey_conflict(&manager).status, "not-applicable");
        assert!(host.resolve_hotkey_conflict(&manager, false).is_err());
        assert_eq!(app.value("shortcuts"), bindings);
        return;
    }
    let initial = host.hotkey_conflict(&manager);
    if mode == "empty" {
        assert_eq!(initial.status, "conflict");
        assert!(!initial.can_resolve);
        assert!(host.resolve_hotkey_conflict(&manager, false).is_err());
        assert_eq!(menu.value("activate-window-menu"), before_menu);
        return;
    }
    assert_eq!(
        initial.status,
        if mode == "mismatch" {
            "mismatch"
        } else {
            "conflict"
        }
    );
    assert_eq!(initial.effective.as_deref(), Some("Ctrl+Alt+Space"));
    assert!(!initial.can_undo);
    assert_eq!(app.value("shortcuts"), bindings, "检测必须只读");
    if mode == "missing" {
        assert!(!initial.can_resolve);
        assert!(host.resolve_hotkey_conflict(&manager, false).is_err());
    } else {
        if mode == "backup-failure" {
            std::fs::create_dir_all(&device).unwrap();
            std::fs::create_dir(device.join("gnome-hotkey-backup.tmp")).unwrap();
        }
        let result = host.resolve_hotkey_conflict(&manager, false);
        if mode == "failure" || mode == "backup-failure" {
            assert!(result.is_err());
        } else {
            let report = result.unwrap();
            assert_eq!(report.status, "clear");
            assert!(report.can_undo);
            assert_eq!(
                menu.strv("activate-window-menu")
                    .iter()
                    .map(|v| v.as_str())
                    .collect::<Vec<_>>(),
                vec!["<Super>F10", "<Shift><Alt>space"]
            );
            let after: Vec<(String, HashMap<String, gio::glib::Variant>)> =
                app.value("shortcuts").get().unwrap();
            assert_eq!(
                after[1].1["shortcuts"].get::<Vec<String>>().unwrap(),
                vec!["<Super>F9"]
            );
            assert!(device.join("gnome-hotkey-backup.json").is_file());
            // 模拟重启：新平台管理器仍能读取本机撤销记录。
            let restarted = LinuxHotkeyManager::with_session(SessionType::Wayland, false);
            if mode == "stale-app-undo" {
                let mut edited = after.clone();
                edited[1]
                    .1
                    .insert("shortcuts".into(), vec!["<Super>F8"].to_variant());
                let edited = edited.to_variant();
                app.set_value("shortcuts", &edited).unwrap();
                assert!(!host.hotkey_conflict(&restarted).can_undo);
                assert!(host.resolve_hotkey_conflict(&restarted, true).is_err());
                assert_eq!(app.value("shortcuts"), edited);
                return;
            }
            if mode == "stale-undo" {
                menu.set_strv("activate-window-menu", ["<Super>F11"])
                    .unwrap();
                assert!(!host.hotkey_conflict(&restarted).can_undo);
                assert!(host
                    .resolve_hotkey_conflict(&restarted, true)
                    .unwrap_err()
                    .contains("再次修改"));
                assert_eq!(menu.strv("activate-window-menu")[0], "<Super>F11");
                return;
            }
            assert!(host.hotkey_conflict(&restarted).can_undo);
            host.resolve_hotkey_conflict(&restarted, true).unwrap();
            assert!(!device.join("gnome-hotkey-backup.json").exists());
        }
    }
    assert_eq!(menu.value("activate-window-menu"), before_menu);
    assert_eq!(
        app.value("shortcuts"),
        bindings,
        "其他操作与原绑定必须完整保留"
    );
    host.update_settings(flashcast_core::Settings {
        hotkey: "Ctrl+Alt+Space".into(),
        ..host.settings()
    })
    .unwrap();
    assert_eq!(host.hotkey_conflict(&manager).status, "not-applicable");
    assert!(host.resolve_hotkey_conflict(&manager, false).is_err());
}

/// 显式原生诊断；不在 CI 执行，不把系统绑定核验当作物理按键实测。
#[test]
#[ignore = "需真实 GNOME Wayland 会话，临时修改后恢复原设置"]
fn gnome_live_cycle() {
    assert!(std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap()
        .contains("GNOME"));
    let source = gio::SettingsSchemaSource::default().unwrap();
    let menu = gio::Settings::new_full(
        &source
            .lookup("org.gnome.desktop.wm.keybindings", true)
            .unwrap(),
        None::<&gio::SettingsBackend>,
        None,
    );
    let app = gio::Settings::new_full(
        &source
            .lookup(
                "org.gnome.settings-daemon.global-shortcuts.application",
                true,
            )
            .unwrap(),
        None::<&gio::SettingsBackend>,
        Some("/org/gnome/settings-daemon/global-shortcuts/dev.flashcast.launcher/"),
    );
    struct Restore {
        menu: gio::Settings,
        app: gio::Settings,
        old_menu: gio::glib::Variant,
        old_app: gio::glib::Variant,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            self.menu
                .set_value("activate-window-menu", &self.old_menu)
                .unwrap();
            self.app.set_value("shortcuts", &self.old_app).unwrap();
            gio::Settings::sync();
            let bus = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).unwrap();
            let params = gio::glib::Variant::tuple_from_iter([
                "dev.flashcast.launcher".to_variant(),
                self.old_app.clone(),
            ]);
            bus.call_sync(
                Some("org.freedesktop.impl.portal.desktop.gnome"),
                "/org/gnome/globalshortcuts",
                "org.gnome.GlobalShortcutsRebind",
                "RebindShortcuts",
                Some(&params),
                None,
                gio::DBusCallFlags::NONE,
                5000,
                None::<&gio::Cancellable>,
            )
            .unwrap();
            println!("已恢复诊断前全部系统快捷键");
        }
    }
    let _restore = Restore {
        old_menu: menu.value("activate-window-menu"),
        old_app: app.value("shortcuts"),
        menu: menu.clone(),
        app: app.clone(),
    };
    let mut keys: Vec<String> = menu
        .strv("activate-window-menu")
        .iter()
        .map(ToString::to_string)
        .collect();
    keys.push("<Alt>space".into());
    menu.set_strv("activate-window-menu", keys.as_slice())
        .unwrap();
    gio::Settings::sync();
    let (host, _, _) = support::host_with_device(vec![], flashcast_core::Settings::default());
    let manager = LinuxHotkeyManager::with_session(SessionType::Wayland, false);
    let initial = host.hotkey_conflict(&manager);
    println!("真实检测：{initial:?}");
    assert_eq!(initial.status, "conflict");
    assert!(initial.can_resolve);
    let result = host.resolve_hotkey_conflict(&manager, false).unwrap();
    println!("真实处理：{result:?}");
    assert_eq!(result.status, "clear");
    assert!(result.can_undo);
    let result = host.resolve_hotkey_conflict(&manager, true).unwrap();
    println!("真实撤销：{result:?}");
    assert_eq!(result.status, "conflict");
    assert_eq!(
        menu.strv("activate-window-menu")
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        keys
    );
}
