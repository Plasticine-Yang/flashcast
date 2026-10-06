//! 只读真实捕获检查：不打印剪贴板内容，不写剪贴板。
fn main() {
    let watcher = flashcast_platform::current().clipboard_watcher;
    if let Some(expected) = std::env::args()
        .nth(1)
        .filter(|arg| arg == "--expect-text")
        .and_then(|_| std::env::args().nth(2))
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
        while std::time::Instant::now() < deadline {
            match watcher.poll() {
                Ok(flashcast_platform::ClipboardPoll::Changed(capture))
                    if capture.text.as_deref() == Some(&expected) =>
                {
                    watcher.stop_background_capture();
                    println!(
                        "PASS: expected fixture text captured without printing clipboard content"
                    );
                    return;
                }
                Ok(_) => {}
                Err(error) => panic!("capture unavailable: {error}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        watcher.stop_background_capture();
        panic!("expected fixture copy was not captured");
    }
    match watcher.poll() {
        Ok(_) => println!("capture read completed"),
        Err(error) => println!("capture unavailable: {error}"),
    }
}
