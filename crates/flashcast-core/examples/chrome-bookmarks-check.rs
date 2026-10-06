//! 只读真实 profile 检查：只打印数量和状态，不输出书签标题、网址或账号信息。
use flashcast_core::BookmarkIndex;

fn main() {
    let profile = std::env::args_os()
        .nth(1)
        .expect("用法：chrome-bookmarks-check <profile目录>");
    let index = BookmarkIndex::new();
    index.set_path(Some(std::path::PathBuf::from(profile).join("Bookmarks")));
    index.refresh();
    println!("{}", index.status().label_zh());
    println!(
        "local={}, account={}",
        index
            .entries()
            .iter()
            .filter(|e| !e.id.starts_with("account:"))
            .count(),
        index
            .entries()
            .iter()
            .filter(|e| e.id.starts_with("account:"))
            .count()
    );
    assert!(index.status().is_ok(), "真实文件读取失败");
}
