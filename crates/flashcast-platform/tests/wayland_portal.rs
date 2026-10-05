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
struct Registry(Arc<AtomicBool>);
#[zbus::interface(name = "org.freedesktop.host.portal.Registry", crate = "ashpd::zbus")]
impl Registry {
    #[zbus(property)]
    fn version(&self) -> u32 {
        1
    }
    fn register(&self, app_id: String, _options: HashMap<String, OwnedValue>) {
        assert_eq!(app_id, "dev.flashcast.launcher");
        self.0.store(true, Ordering::Release);
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
    for mode in ["ok", "cancel", "empty", "missing"] {
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
                        Registry(registered.clone()),
                    )
                    .unwrap()
                    .build()
                    .await
                    .unwrap(),
            )
        });
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "portal_client", "--nocapture"])
            .env(
                "XDG_DATA_HOME",
                std::env::temp_dir().join(format!("flashcast-portal-test-{}", daemon.id())),
            )
            .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
            .env("FLASHCAST_PORTAL_TEST_MODE", mode)
            .output()
            .unwrap();
        let _ = daemon.kill();
        let _ = daemon.wait();
        drop(conn);
        let _ = std::fs::remove_file(config);
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("flashcast-portal-test-{}", daemon.id())),
        );
        assert!(
            result.status.success(),
            "{mode}: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
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
