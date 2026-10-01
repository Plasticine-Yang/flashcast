//! 备忘录功能插件（ticket 07）。
//!
//! 与内置主题走同一条「清单驱动」路径：插件实现随应用编译进来，清单条目记录标识、
//! 版本、关键词别名、所需能力与启用状态。停用后既不贡献结果也不产生后台活动
//! （注册表在搜索前就过滤掉停用的插件，见 [`crate::registry`]）。
//!
//! 插件只读取宿主共享的 [`MemoBook`] 只读快照：它不持有工作区路径，也不接触剪贴板
//! 等原生能力。复制这类动作由宿主在 `Host::execute` 里做权限校验并执行（ADR §6）。

use std::sync::Arc;

use crate::memo::{Memo, MemoBook};
use crate::model::{DefaultAction, ItemKind, MatchTier, Preview, Score, SearchItem};
use crate::plugin::{
    FeaturePlugin, Keyword, PluginError, PluginManifest, PluginScope, SearchContext,
    CAP_CLIPBOARD_WRITE,
};
use crate::ranking::score_match;

/// 备忘录插件的标识。清单、结果来源与本机实现都以它为准。
pub const MEMO_PLUGIN_ID: &str = "memo";

/// 中文关键词别名。
pub const MEMO_KEYWORD_ZH: &str = "备忘录";

/// 一次插件搜索最多返回多少条。宿主也会限制总数。
const SCOPE_LIMIT: usize = 50;

/// 备忘录插件的实现。
pub struct MemoPlugin {
    book: Arc<MemoBook>,
    manifest: PluginManifest,
}

impl MemoPlugin {
    pub fn new(book: Arc<MemoBook>) -> Self {
        Self {
            book,
            manifest: PluginManifest::feature(MEMO_PLUGIN_ID, "备忘录", env!("CARGO_PKG_VERSION"))
                .with_keywords([MEMO_KEYWORD_ZH, "memo", "memos"])
                .with_capabilities([CAP_CLIPBOARD_WRITE]),
        }
    }
}

impl FeaturePlugin for MemoPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    /// 备忘录参与首屏搜索：**按标签**命中（spec「首屏范围为软件与备忘录标签」）。
    ///
    /// 关键词与标签冲突时两边都不会静默消失，而判定权在宿主：插件**始终**按标签贡献
    /// 候选（哪怕查询正好等于自己的关键词），由 `Host::search` 决定是进入范围还是留在
    /// 首屏并同时给出「插件入口 + 标签命中」（ADR §4）。此前这里在关键词完全匹配时
    /// 提前返回空结果，结果是「标签命中被关键词吞掉」，与 ADR 相反。
    fn contributes_to_home(&self) -> bool {
        true
    }

    fn search(&self, ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        if ctx.query.is_empty() {
            return Ok(Vec::new());
        }
        let snapshot = self.book.snapshot();
        let mut items: Vec<SearchItem> = snapshot
            .memos
            .iter()
            .filter(|memo| memo.tags.iter().any(|tag| tag.to_lowercase() == ctx.query))
            .map(|memo| {
                let relevance = exact_tag_relevance(memo, &ctx.query);
                memo_item(memo, Score::new(MatchTier::KeywordOrTagExact, relevance))
            })
            .collect();
        items.truncate(ctx.limit.min(SCOPE_LIMIT));
        Ok(items)
    }

    fn take_scope(&self, keyword: &Keyword) -> Option<Box<dyn PluginScope>> {
        if self.manifest.matches_keyword(keyword.as_str()).is_none() {
            return None;
        }
        Some(Box::new(MemoScope {
            book: Arc::clone(&self.book),
            keyword: keyword.as_str().to_string(),
        }))
    }
}

/// 标签精确匹配的相关度：标签越短越贴近用户输入的意图。
fn exact_tag_relevance(memo: &Memo, query: &str) -> u8 {
    let tag_length = memo
        .tags
        .iter()
        .find(|tag| tag.to_lowercase() == query)
        .map(|tag| tag.chars().count())
        .unwrap_or(query.chars().count());
    90u8.saturating_sub((tag_length.min(20) as u8).saturating_mul(2))
        .max(40)
}

/// 备忘录范围：标题、标签与正文都可检索。
struct MemoScope {
    book: Arc<MemoBook>,
    keyword: String,
}

impl PluginScope for MemoScope {
    fn plugin_id(&self) -> &str {
        MEMO_PLUGIN_ID
    }

    fn keyword(&self) -> &str {
        &self.keyword
    }

    fn search(&self, ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        let query = strip_keyword(&ctx.query, self.keyword());
        let snapshot = self.book.snapshot();
        let mut items: Vec<SearchItem> = Vec::new();
        if query.is_empty() {
            // 刚进入范围：列出全部备忘录，用户可以直接上下选择并预览。
            for memo in &snapshot.memos {
                items.push(memo_item(memo, Score::unordered()));
            }
        } else {
            for memo in &snapshot.memos {
                let metadata: Vec<&str> = memo
                    .tags
                    .iter()
                    .map(String::as_str)
                    .chain(std::iter::once(memo.body.as_str()))
                    .collect();
                if let Some(score) = score_match(&query, &memo.title, &metadata) {
                    items.push(memo_item(memo, score));
                }
            }
        }
        items.truncate(ctx.limit.min(SCOPE_LIMIT));
        Ok(items)
    }
}

/// 去掉输入里已经用于进入范围的关键词前缀。
///
/// 用户输入「备忘录」后通常会在同一个输入框里继续写查询，因此范围把关键词本身
/// 视为空查询，并把 `备忘录 回复` 之类的前缀剥掉。
fn strip_keyword(query: &str, keyword: &str) -> String {
    let query = query.trim().to_lowercase();
    let keyword = keyword.trim().to_lowercase();
    if query == keyword {
        return String::new();
    }
    match query.strip_prefix(&keyword) {
        Some(rest) => rest.trim().to_string(),
        None => query,
    }
}

/// 一条备忘录结果的统一构造：稳定 id、来源、默认操作与完整预览。
pub fn memo_item(memo: &Memo, score: Score) -> SearchItem {
    SearchItem {
        id: memo_item_id(&memo.id),
        title: memo.title.clone(),
        subtitle: Some(subtitle_for(memo)),
        icon: None,
        source: MEMO_PLUGIN_ID.to_string(),
        kind: ItemKind::Memo,
        // 备忘录的默认操作是粘贴（ADR §4）；ticket 07 先复制并提示手动粘贴，
        // 自动粘贴由 ticket 08 完成。
        default_action: DefaultAction::Paste,
        preview: Preview::Text {
            title: Some(memo.title.clone()),
            // 预览区按需展开，因此这里给出完整正文：用户粘贴前要能确认内容。
            body: memo.body.clone(),
        },
        score,
    }
}

/// 结果条目的稳定标识。宿主据此在执行时还原备忘录。
pub fn memo_item_id(memo_id: &str) -> String {
    format!("{MEMO_PLUGIN_ID}:{memo_id}")
}

/// 从结果标识还原备忘录标识。
pub fn memo_id_from_item_id(item_id: &str) -> Option<&str> {
    item_id.strip_prefix("memo:").filter(|id| !id.is_empty())
}

/// 结果副标题：标签（带来源可见性）或「无标签」。
fn subtitle_for(memo: &Memo) -> String {
    if memo.tags.is_empty() {
        "无标签".to_string()
    } else {
        format!("标签：{}", memo.tags.join("、"))
    }
}
