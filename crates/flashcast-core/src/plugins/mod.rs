//! 随应用提供的官方功能插件（ADR §6）。
//!
//! v0.1.0 随应用提供三个官方功能插件：备忘录（本 ticket）、剪贴板历史（ticket 09）
//! 与 Chrome 书签（ticket 13）。它们与内置主题一样由应用代码提供，而「有哪些插件、
//! 是否启用」由配置工作区的 `manifest.json` 唯一记录（见 [`crate::manifest`]）。
//!
//! 每个插件遵守同一条契约，ticket 09/13 照抄这条路径：
//!
//! 1. 声明 [`PluginManifest`]（标识、种类、版本、关键词别名、所需能力）；
//! 2. 通过 [`crate::plugin::FeaturePlugin::contributes_to_home`] 声明首屏搜索贡献；
//! 3. 只读取宿主共享的**只读快照**（例如 [`crate::memo::MemoBook`]），不接触工作区
//!    路径或原生能力；
//! 4. 需要原生能力的动作由宿主执行（例如复制走 `Host::execute` 里的权限校验与
//!    `ClipboardAccess`），插件的默认操作只是声明「回车会做什么」。

pub mod memo;

use std::sync::Arc;

use crate::memo::MemoBook;
use crate::registry::PluginRegistry;

pub use memo::{MemoPlugin, MEMO_PLUGIN_ID};

/// 注册全部官方功能插件。重复注册同一 id 时注册表保留先注册的那个。
pub fn register_official(registry: &PluginRegistry, memos: &Arc<MemoBook>) {
    registry.register(Arc::new(MemoPlugin::new(Arc::clone(memos))));
}
