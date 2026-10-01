//! 随应用提供的官方功能插件（ADR §6）。
//!
//! v0.1.0 随应用提供三个官方功能插件：备忘录（ticket 07）、剪贴板历史（ticket 09）
//! 与 Chrome 书签（ticket 13）。它们与内置主题一样由应用代码提供，而「有哪些插件、
//! 是否启用」由配置工作区的 `manifest.json` 唯一记录（见 [`crate::manifest`]）。
//!
//! 每个插件遵守同一条契约：
//!
//! 1. 声明 [`PluginManifest`]（标识、种类、版本、关键词别名、所需能力）；
//! 2. 通过 [`crate::plugin::FeaturePlugin::contributes_to_home`] 声明首屏搜索贡献，
//!    并用 [`crate::plugin::FeaturePlugin::default_enabled`] 声明首次写入清单时的默认启用状态；
//! 3. 只读取宿主共享的**只读快照**（例如 [`crate::memo::MemoBook`] 与
//!    [`crate::clipboard::ClipboardStore`]），不接触工作区路径或原生能力；
//! 4. 需要原生能力的动作由宿主执行（例如复制走 `Host::execute` 里的权限校验与
//!    `ClipboardAccess`），插件的默认操作只是声明「回车会做什么」。

pub mod chrome;
pub mod clipboard;
pub mod memo;

use std::sync::Arc;

use crate::chrome::BookmarkIndex;
use crate::clipboard::ClipboardStore;
use crate::memo::MemoBook;
use crate::registry::PluginRegistry;

pub use chrome::{
    ChromeBookmarksPlugin, CAP_CHROME_OPEN, CHROME_KEYWORD_EN, CHROME_KEYWORD_ZH, CHROME_PLUGIN_ID,
};
pub use clipboard::{
    ClipboardPlugin, CLIPBOARD_KEYWORD_ALT_ZH, CLIPBOARD_KEYWORD_EN, CLIPBOARD_KEYWORD_ZH,
    CLIPBOARD_PLUGIN_ID,
};
pub use memo::{MemoPlugin, MEMO_PLUGIN_ID};

/// 注册全部官方功能插件。重复注册同一 id 时注册表保留先注册的那个。
///
/// 「剪贴板」与「剪切板」在这里**只注册一次**：两个写法是同一个
/// [`ClipboardPlugin`] 的两个关键词别名（spec 明确不另建插件）。
pub fn register_official(
    registry: &PluginRegistry,
    memos: &Arc<MemoBook>,
    bookmarks: &Arc<BookmarkIndex>,
    clipboard: &Arc<ClipboardStore>,
) {
    registry.register(Arc::new(MemoPlugin::new(Arc::clone(memos))));
    registry.register(Arc::new(ChromeBookmarksPlugin::new(Arc::clone(bookmarks))));
    registry.register(Arc::new(ClipboardPlugin::new(Arc::clone(clipboard))));
}
