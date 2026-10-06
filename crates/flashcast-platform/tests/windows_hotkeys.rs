//! Windows 原生注册回归；只验证 API 与回调状态，不代表物理按键唤起实测。
#![cfg(target_os = "windows")]

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use flashcast_platform::{hotkey_backend, HotkeyError, HotkeySpec};

#[test]
fn native_registration_preserves_owner_callback_and_old_binding_on_failure() {
    let original = HotkeySpec::parse("Ctrl+Alt+Shift+F8").unwrap();
    let replacement = HotkeySpec::parse("Ctrl+Alt+Shift+F9").unwrap();
    let external = HotkeySpec::parse("Ctrl+Alt+Shift+F10").unwrap();
    let worker_key = HotkeySpec::parse("Ctrl+Alt+Shift+F11").unwrap();
    let presses = Arc::new(AtomicUsize::new(0));
    let counter = presses.clone();
    let handle = hotkey_backend::register(
        &original,
        Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .expect("原线程必须能够注册测试组合");

    // 同一进程的重复请求不能覆盖原回调。
    assert!(matches!(
        hotkey_backend::register(&original, Arc::new(|_| {})),
        Err(HotkeyError::AlreadyRegistered { .. })
    ));
    let unchanged = hotkey_backend::update(&handle, &original).unwrap();
    assert_eq!(unchanged.id, handle.id);

    // 后台线程不能创建第二个隐藏窗口，也不能把注销失败当成成功。
    let old = handle.clone();
    let (registration, removal) = std::thread::spawn(move || {
        (
            hotkey_backend::register(&worker_key, Arc::new(|_| {})),
            hotkey_backend::unregister(&old),
        )
    })
    .join()
    .unwrap();
    assert!(matches!(
        registration,
        Err(HotkeyError::BackendUnavailable { .. })
    ));
    assert!(matches!(
        removal,
        Err(HotkeyError::BackendUnavailable { .. })
    ));
    handle.fire();
    assert_eq!(presses.load(Ordering::SeqCst), 1);

    // 独立 HWND 的占用必须报告为系统冲突，不能指认为 Flashcast 自身。
    let other = global_hotkey::GlobalHotKeyManager::new().unwrap();
    let occupied = hotkey_backend::to_backend_hotkey(&external).unwrap();
    other.register(occupied).unwrap();
    let failure = hotkey_backend::update(&handle, &external).unwrap_err();
    assert!(matches!(failure, HotkeyError::Conflict { .. }));
    assert!(!failure.to_string().contains("已被 Flashcast 注册"));
    handle.fire();
    assert_eq!(presses.load(Ordering::SeqCst), 2);
    other.unregister(occupied).unwrap();

    let updated = hotkey_backend::update(&handle, &replacement).unwrap();
    updated.fire();
    assert_eq!(presses.load(Ordering::SeqCst), 3);
    handle.fire();
    assert_eq!(presses.load(Ordering::SeqCst), 3, "旧组合已释放回调");
    hotkey_backend::unregister(&updated).unwrap();
    hotkey_backend::unregister(&updated).unwrap();
}
