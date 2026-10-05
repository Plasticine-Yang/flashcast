//! 用独立 D-Bus 总线穿透真实 LinuxHotkeyManager，验证协议与注销生命周期。
#![cfg(target_os = "linux")]
use ashpd::zbus::{
    self,
    message::Header,
    zvariant::{OwnedObjectPath, OwnedValue, Str},
    Connection,
};
use flashcast_platform::hotkey::HotkeySpec;
use flashcast_platform::linux::LinuxHotkeyManager;
use flashcast_platform::{HotkeyManager, SessionType};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

fn string(value: &str) -> OwnedValue {
    Str::from(value).into()
}
struct Session(Arc<AtomicBool>);
#[zbus::interface(name = "org.freedesktop.portal.Session", crate = "ashpd::zbus")]
impl Session {
    fn close(&self) {
        self.0.store(false, Ordering::Release);
    }
}
struct Registry {
    registered: Arc<AtomicBool>,
    desktop_entry: std::path::PathBuf,
}
#[zbus::interface(name = "org.freedesktop.host.portal.Registry", crate = "ashpd::zbus")]
impl Registry {
    #[zbus(property)]
    fn version(&self) -> u32 {
        1
    }
    fn register(
        &self,
        app_id: String,
        _options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        assert_eq!(app_id, "dev.flashcast.launcher");
        // 真实门户也通过 GIO 加载应用信息。只有文件存在不能证明身份有效。
        if gio::DesktopAppInfo::from_filename(&self.desktop_entry).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "Could not register app ID: App info not found for '{app_id}'"
            )));
        }
        self.registered.store(true, Ordering::Release);
        Ok(())
    }
}
struct Portal {
    mode: String,
    active: Arc<AtomicBool>,
    registered: Arc<AtomicBool>,
}
#[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts", crate = "ashpd::zbus")]
impl Portal {
    #[zbus(property)]
    fn version(&self) -> u32 {
        1
    }
    async fn create_session(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert!(
            self.registered.load(Ordering::Acquire),
            "门户调用前必须声明应用身份"
        );
        let sender = header
            .sender()
            .unwrap()
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let token = <&str>::try_from(options.get("handle_token").unwrap()).unwrap();
        let session_token = <&str>::try_from(options.get("session_handle_token").unwrap()).unwrap();
        let request = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
        let session = format!("/org/freedesktop/portal/desktop/session/{sender}/{session_token}");
        conn.object_server()
            .at(session.clone(), Session(self.active.clone()))
            .await
            .unwrap();
        let conn = conn.clone();
        let path = request.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            let results = HashMap::from([("session_handle", string(&session))]);
            conn.emit_signal(
                None::<&str>,
                path,
                "org.freedesktop.portal.Request",
                "Response",
                &(0u32, results),
            )
            .await
            .unwrap();
        });
        Ok(OwnedObjectPath::try_from(request).unwrap())
    }
    async fn bind_shortcuts(
        &self,
        session: OwnedObjectPath,
        shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
        _parent: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(shortcuts[0].0, "summon");
        assert_eq!(
            <&str>::try_from(shortcuts[0].1.get("preferred_trigger").unwrap()).unwrap(),
            "CTRL+ALT+space"
        );
        let sender = header
            .sender()
            .unwrap()
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let token = <&str>::try_from(options.get("handle_token").unwrap()).unwrap();
        let request = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
        let conn = conn.clone();
        let path = request.clone();
        let mode = self.mode.clone();
        let active = self.active.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            let bound: Vec<(String, HashMap<&str, OwnedValue>)> = if mode == "empty" {
                vec![]
            } else {
                vec![(
                    "summon".into(),
                    HashMap::from([
                        ("description", string("打开 Flashcast")),
                        ("trigger_description", string("Ctrl+Alt+Space")),
                    ]),
                )]
            };
            let results = HashMap::from([(
                "shortcuts",
                OwnedValue::try_from(zbus::zvariant::Value::from(bound)).unwrap(),
            )]);
            conn.emit_signal(
                None::<&str>,
                path,
                "org.freedesktop.portal.Request",
                "Response",
                &(if mode == "cancel" { 1u32 } else { 0u32 }, results),
            )
            .await
            .unwrap();
            if mode != "ok" {
                return;
            }
            let foreign = OwnedObjectPath::try_from("/foreign/session").unwrap();
            conn.emit_signal(
                None::<&str>,
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.GlobalShortcuts",
                "Activated",
                &(
                    foreign,
                    "summon",
                    1u64,
                    HashMap::from([("activation_token", string("wrong-token"))]),
                ),
            )
            .await
            .unwrap();
            while active.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(40)).await;
                if !active.load(Ordering::Acquire) {
                    break;
                }
                conn.emit_signal(
                    None::<&str>,
                    "/org/freedesktop/portal/desktop",
                    "org.freedesktop.portal.GlobalShortcuts",
                    "Activated",
                    &(
                        &session,
                        "summon",
                        2u64,
                        HashMap::from([("activation_token", string("test-token"))]),
                    ),
                )
                .await
                .unwrap();
            }
        });
        Ok(OwnedObjectPath::try_from(request).unwrap())
    }
}

#[test]
fn wayland_registration_dispatch_and_denial() {
    for (mode, fixture) in [
        ("ok", "invalid"),
        ("ok", "absent"),
        ("ok", "stale"),
        ("ok", "appimage"),
        ("ok", "valid"),
        ("ok", "system"),
        ("cancel", "valid"),
        ("empty", "valid"),
        ("missing", "valid"),
    ] {
        let config =
            std::env::temp_dir().join(format!("flashcast-test-bus-{}.conf", std::process::id()));
        std::fs::write(&config, "<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context='default'><allow send_destination='*'/><allow receive_sender='*'/><allow own='*'/></policy></busconfig>").unwrap();
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("CI 与桌面需有 dbus-daemon");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let data = std::env::temp_dir().join(format!("flashcast-portal-test-{}", daemon.id()));
        let user_data = data.join("user");
        let system_data = data.join("system");
        let appimage = data.join("Flashcast portable.AppImage");
        if fixture == "appimage" {
            std::fs::create_dir_all(&data).unwrap();
            std::os::unix::fs::symlink(std::env::current_exe().unwrap(), &appimage).unwrap();
        }
        let relative_entry = "applications/dev.flashcast.launcher.desktop";
        let desktop_entry = if fixture == "system" {
            system_data.join(relative_entry)
        } else {
            user_data.join(relative_entry)
        };
        std::fs::create_dir_all(desktop_entry.parent().unwrap()).unwrap();
        let original = if matches!(fixture, "absent" | "appimage") {
            None
        } else {
            let executable = match fixture {
                "invalid" => "flashcast-missing-executable-for-portal-test".into(),
                "stale" => data.join("removed/flashcast"),
                _ => std::env::current_exe().unwrap(),
            };
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=Custom Flashcast\nExec=\"{}\"\nNoDisplay=true\n",
                executable.display()
            );
            std::fs::write(&desktop_entry, &content).unwrap();
            Some(content)
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let active = Arc::new(AtomicBool::new(true));
        let registered = Arc::new(AtomicBool::new(false));
        let conn = rt.block_on(async {
            if mode == "missing" {
                return None;
            }
            Some(
                zbus::connection::Builder::address(address.trim())
                    .unwrap()
                    .name("org.freedesktop.portal.Desktop")
                    .unwrap()
                    .serve_at(
                        "/org/freedesktop/portal/desktop",
                        Portal {
                            mode: mode.into(),
                            active: active.clone(),
                            registered: registered.clone(),
                        },
                    )
                    .unwrap()
                    .serve_at(
                        "/org/freedesktop/portal/desktop",
                        Registry {
                            registered: registered.clone(),
                            desktop_entry: desktop_entry.clone(),
                        },
                    )
                    .unwrap()
                    .build()
                    .await
                    .unwrap(),
            )
        });
        let mut client = Command::new(std::env::current_exe().unwrap());
        client
            .args(["--ignored", "--exact", "portal_client", "--nocapture"])
            .env("XDG_DATA_HOME", &user_data)
            .env("XDG_DATA_DIRS", &system_data)
            .env_remove("APPIMAGE")
            .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
            .env("FLASHCAST_PORTAL_TEST_MODE", mode);
        if fixture == "appimage" {
            client.env("APPIMAGE", &appimage);
        }
        let result = client.output().unwrap();
        let _ = daemon.kill();
        let _ = daemon.wait();
        drop(conn);
        let _ = std::fs::remove_file(config);
        let preserved = !matches!(fixture, "valid" | "system")
            || std::fs::read_to_string(&desktop_entry).unwrap() == original.unwrap();
        let no_user_override = fixture != "system" || !user_data.join(relative_entry).exists();
        let appimage_used = fixture != "appimage"
            || gio::DesktopAppInfo::from_filename(&desktop_entry)
                .and_then(|info| gio::prelude::AppInfoExt::commandline(&info))
                .and_then(|line| gio::glib::shell_parse_argv(line).ok())
                .and_then(|argv| argv.into_iter().next())
                .map(|executable| std::path::PathBuf::from(executable) == appimage)
                .unwrap_or(false);
        let _ = std::fs::remove_dir_all(data);
        assert!(
            result.status.success(),
            "{mode}/{fixture}: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(preserved, "有效桌面入口不得重写：{mode}/{fixture}");
        assert!(no_user_override, "有效系统入口不得创建用户覆盖");
        assert!(appimage_used, "便携应用身份必须使用 AppImage 启动路径");
        if mode != "missing" {
            assert!(
                !active.load(Ordering::Acquire),
                "失败与注销都必须关闭会话：{mode}"
            );
        }
    }
}

#[test]
#[ignore = "由父测试在隔离总线下执行"]
fn portal_client() {
    let mode = std::env::var("FLASHCAST_PORTAL_TEST_MODE").unwrap();
    let manager = LinuxHotkeyManager::with_session(SessionType::Wayland, false);
    let spec = HotkeySpec::parse("Ctrl+Alt+Space").unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let result = manager.register(
        &spec,
        Arc::new(move |activation| {
            let _ = tx.send(activation.token);
        }),
    );
    if mode != "ok" {
        assert!(result.is_err(), "未授权或没有绑定不得报告成功");
        return;
    }
    let handle = result.unwrap();
    assert_eq!(
        handle.trigger_description.as_deref(),
        Some("Ctrl+Alt+Space")
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).unwrap().as_deref(),
        Some("test-token")
    );
    manager.unregister(&handle).unwrap();
    manager.unregister(&handle).unwrap();
    while rx.try_recv().is_ok() {}
    assert!(
        rx.recv_timeout(Duration::from_millis(150)).is_err(),
        "注销后不再派发"
    );
}
