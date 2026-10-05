//! 只读真实捕获检查：不打印剪贴板内容，不写剪贴板。
fn main() {
    let watcher = flashcast_platform::current().clipboard_watcher;
    match watcher.poll() {
        Ok(_) => println!("capture read completed"),
        Err(error) => println!("capture unavailable: {error}"),
    }
}
