//! Chrome 书签功能插件（ticket 13）。
//!
//! 与备忘录插件走同一条「清单驱动」路径：实现随应用编译进来，清单记录标识、版本、
//! 关键词别名、所需能力与启用状态。停用后既不贡献结果也不产生后台活动。
//!
//! 与备忘录的差别只有两点，都由 spec 决定：
//!
//! - **不参与首屏搜索**：首屏只检索软件与备忘录标签，书签只在关键词范围里检索
//!   （「不在首屏默认检索全部剪贴板历史或书签」）；
//! - **默认操作是在 Chrome 打开**：真正的启动由宿主在 `Host::execute` 里做权限校验与
//!   URL 校验后调用平台适配层，插件只声明「回车会做什么」。
//!
//! 插件只读宿主共享的 [`BookmarkIndex`] 快照，因此拿不到工作区路径，也拿不到
//! Chrome 可执行文件路径或任何原生能力句柄。

use std::sync::Arc;

use flashcast_platform::IconRef;

use crate::chrome::{search_bookmarks, BookmarkEntry, BookmarkIndex};
use crate::model::{DefaultAction, ItemKind, Preview, Score, SearchItem};
use crate::plugin::{
    FeaturePlugin, Keyword, PluginError, PluginManifest, PluginScope, SearchContext,
};

/// 插件的标识。清单、结果来源与本机实现都以它为准。
pub const CHROME_PLUGIN_ID: &str = "chrome-bookmarks";

/// 英文关键词别名。
pub const CHROME_KEYWORD_EN: &str = "chrome bookmarks";

/// 中文关键词别名。
pub const CHROME_KEYWORD_ZH: &str = "chrome 书签";

/// 插件声明的能力：在选定的 Chrome profile 里打开链接。
///
/// 宿主在**原生边界**（真正调用 Chrome 适配层之前）校验它：没有声明的插件拿不到这个
/// 动作（ADR §6）。
pub const CAP_CHROME_OPEN: &str = "chrome.open";

/// 一次插件搜索最多返回多少条。宿主也会限制总数。
const SCOPE_LIMIT: usize = usize::MAX;

/// Chrome 书签插件的实现。
pub struct ChromeBookmarksPlugin {
    index: Arc<BookmarkIndex>,
    manifest: PluginManifest,
}

impl ChromeBookmarksPlugin {
    pub fn new(index: Arc<BookmarkIndex>) -> Self {
        Self {
            index,
            manifest: PluginManifest::feature(
                CHROME_PLUGIN_ID,
                "Chrome 书签",
                env!("CARGO_PKG_VERSION"),
            )
            .with_keywords([
                CHROME_KEYWORD_EN,
                CHROME_KEYWORD_ZH,
                "bookmark",
                "bookmarks",
                "书签",
            ])
            .with_capabilities([CAP_CHROME_OPEN]),
        }
    }
}

impl FeaturePlugin for ChromeBookmarksPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    /// 书签**不**参与首屏搜索（spec：首屏不检索全部书签）。
    fn contributes_to_home(&self) -> bool {
        false
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        Ok(Vec::new())
    }

    fn take_scope(&self, keyword: &Keyword) -> Option<Box<dyn PluginScope>> {
        if self.manifest.matches_keyword(keyword.as_str()).is_none() {
            return None;
        }
        Some(Box::new(ChromeBookmarksScope {
            index: Arc::clone(&self.index),
            keyword: keyword.as_str().to_string(),
        }))
    }
}

/// 书签范围：标题、网址与目录都可检索。
struct ChromeBookmarksScope {
    index: Arc<BookmarkIndex>,
    keyword: String,
}

impl PluginScope for ChromeBookmarksScope {
    fn plugin_id(&self) -> &str {
        CHROME_PLUGIN_ID
    }

    fn keyword(&self) -> &str {
        &self.keyword
    }

    fn search(&self, ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        // 变化后刷新。这里用「变化才重读」的版本：它只 stat 文件，解析失败也不会
        // 睡 500ms——插件搜索有超时（ADR §6），重试交给宿主入口。
        self.index.refresh_if_changed();
        let snapshot = self.index.snapshot();
        match &snapshot.status {
            // 正常空状态：还没有关联 profile、或该 profile 还没有 Bookmarks 文件。
            crate::chrome::BookmarksStatus::Ok { .. } => {}
            crate::chrome::BookmarksStatus::Missing => return Ok(Vec::new()),
            // 其余状态都要用户先处理，直接如实说明原因。
            other => return Err(PluginError::failed(other.label_zh())),
        }

        let query = ctx.query.clone();
        let mut items: Vec<SearchItem> = search_bookmarks(&snapshot.entries, &query)
            .into_iter()
            .map(|(entry, score)| bookmark_item(entry, score))
            .collect();
        items.truncate(ctx.limit.min(SCOPE_LIMIT));
        Ok(items)
    }
}

/// 一条书签结果的统一构造：稳定 id、来源、默认操作与预览。
///
/// 副标题给出**网址与目录**（ticket 要求结果里能看到标题 / 网址 / 目录），预览给出
/// 完整链接与目录路径。图标用 Chrome 的图标名；浏览器里没有图标文件时 UI 会退回到
/// 内置的浏览器字形（`ResultList` 按 `kind` 判断）。
pub fn bookmark_item(entry: &BookmarkEntry, score: Score) -> SearchItem {
    let subtitle = if entry.folder.trim().is_empty() {
        entry.url.clone()
    } else {
        format!("{} · 目录：{}", entry.url, entry.folder)
    };
    SearchItem {
        id: entry.item_id(),
        title: entry.title.clone(),
        subtitle: Some(subtitle),
        icon: Some(IconRef::unresolved("google-chrome")),
        source: CHROME_PLUGIN_ID.to_string(),
        kind: ItemKind::Bookmark,
        default_action: DefaultAction::OpenInChrome,
        preview: Preview::Text {
            title: Some(entry.title.clone()),
            body: format!("{}\n目录：{}", entry.url, entry.folder),
        },
        score,
    }
}
