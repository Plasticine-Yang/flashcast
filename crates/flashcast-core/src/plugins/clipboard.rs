//! 剪贴板历史功能插件（ticket 09）。
//!
//! 与备忘录插件走同一条「清单驱动」路径：实现随应用编译进来，清单记录标识、版本、
//! 关键词别名、所需能力与启用状态。
//!
//! ## 两个关键词，一个插件
//!
//! 「剪贴板」与「剪切板」是**同一个插件**的两个输入别名（spec 的术语约定：剪切板只作为
//! 输入别名，不另建插件）。两者都声明在同一个 [`PluginManifest`] 里，因此
//! `Host::query("剪贴板")` 与 `Host::query("剪切板")` 进入的是同一个范围对象、
//! 同一份历史；结果条目也来自同一个来源标识。
//!
//! ## 插件不碰原生能力
//!
//! 插件只读取宿主共享的 [`ClipboardStore`] 只读视图（由于是本机 SQLite，读操作本身就是
//! 查询）。后台**捕获**与写入剪贴板都由宿主完成：捕获走 `Host` 的轮询管线，粘贴走
//! `Host::execute` 里的权限校验与 `ClipboardAccess`（ADR §6）。因此本模块既不轮询
//! 剪贴板，也不写剪贴板。
//!
//! ## 首屏不检索历史
//!
//! [`FeaturePlugin::contributes_to_home`] 返回 `false`：首屏只检索软件与备忘录标签，
//! 历史必须经关键词进入（spec「不在首屏默认检索全部剪贴板历史」）。
//!
//! ## 默认关闭
//!
//! [`FeaturePlugin::default_enabled`] 返回 `false`：捕获用户复制的内容是隐私敏感行为，
//! 必须由用户显式启用（spec「剪贴板在用户启用后后台监听」）。

use std::sync::Arc;

use crate::clipboard::{ClipboardEvent, ClipboardStore};
use crate::model::{DefaultAction, ItemKind, Preview, Score, SearchItem};
use crate::plugin::{
    strip_keyword, FeaturePlugin, Keyword, PluginError, PluginManifest, PluginScope, SearchContext,
    CAP_CLIPBOARD_READ, CAP_CLIPBOARD_WRITE,
};
use crate::ranking::score_match;

/// 剪贴板历史插件的标识。定义在数据模型一侧（`crate::clipboard`），
/// 存储、清单与结果来源共用同一个值。
pub use crate::clipboard::CLIPBOARD_PLUGIN_ID;

/// 中文关键词别名（规范写法）。
pub const CLIPBOARD_KEYWORD_ZH: &str = "剪贴板";

/// 中文关键词别名（常见误写）。它只是输入别名，不组成第二个插件。
pub const CLIPBOARD_KEYWORD_ALT_ZH: &str = "剪切板";

/// 英文关键词别名。
pub const CLIPBOARD_KEYWORD_EN: &str = "clipboard";

/// 一次插件搜索最多返回多少条。
const SCOPE_LIMIT: usize = 50;

/// 剪贴板历史插件的实现。
pub struct ClipboardPlugin {
    store: Arc<ClipboardStore>,
    manifest: PluginManifest,
}

impl ClipboardPlugin {
    pub fn new(store: Arc<ClipboardStore>) -> Self {
        Self {
            store,
            manifest: PluginManifest::feature(
                CLIPBOARD_PLUGIN_ID,
                "剪贴板历史",
                env!("CARGO_PKG_VERSION"),
            )
            // 三个别名进入**同一个**插件（关键需求）。
            .with_keywords([
                CLIPBOARD_KEYWORD_ZH,
                CLIPBOARD_KEYWORD_ALT_ZH,
                CLIPBOARD_KEYWORD_EN,
            ])
            // 读：后台捕获；写：把历史条目送回剪贴板以便粘贴。
            .with_capabilities([CAP_CLIPBOARD_READ, CAP_CLIPBOARD_WRITE]),
        }
    }
}

impl FeaturePlugin for ClipboardPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    /// 默认关闭：必须由用户显式启用后才开始后台捕获。
    fn default_enabled(&self) -> bool {
        false
    }

    /// 首屏不检索历史（spec「不在首屏默认检索全部剪贴板历史」）。
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
        Some(Box::new(ClipboardScope {
            store: Arc::clone(&self.store),
            keyword: keyword.as_str().to_string(),
        }))
    }
}

/// 剪贴板历史范围：刚进入时列出全部历史，输入查询后按文字、摘要与来源检索。
struct ClipboardScope {
    store: Arc<ClipboardStore>,
    keyword: String,
}

impl PluginScope for ClipboardScope {
    fn plugin_id(&self) -> &str {
        CLIPBOARD_PLUGIN_ID
    }

    fn keyword(&self) -> &str {
        &self.keyword
    }

    fn search(&self, ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        let query = strip_keyword(&ctx.query, self.keyword());
        let limit = ctx.limit.min(SCOPE_LIMIT);
        // 存储失败必须如实上报（宿主会把它记成一次插件失败并在界面上说明原因），
        // 而不是返回空列表假装「没有历史」。
        let events = self
            .store
            .list(if query.is_empty() { None } else { Some(&query) }, limit)
            .map_err(|error| PluginError::failed(error.to_string()))?;
        if query.is_empty() {
            return Ok(events
                .iter()
                .map(|event| clipboard_item(event, Score::unordered()))
                .collect());
        }
        let mut items = Vec::new();
        for event in &events {
            let metadata = metadata_for(event);
            let metadata_refs: Vec<&str> = metadata.iter().map(String::as_str).collect();
            if let Some(score) = score_match(&query, &event.summary, &metadata_refs) {
                items.push(clipboard_item(event, score));
            }
        }
        Ok(items)
    }
}

/// 参与匹配的元数据：完整文字、来源应用、格式名与**文件名称/类型**。
///
/// 文件列表没有可索引文字（spec「不承诺 OCR」），因此名称与元数据是唯一的检索入口：
/// 每个文件的名称、MIME 与路径都参与匹配。
fn metadata_for(event: &ClipboardEvent) -> Vec<String> {
    let mut metadata: Vec<String> = Vec::new();
    if let Some(text) = event.text.as_deref() {
        metadata.push(text.to_string());
    }
    if let Some(source) = &event.source {
        metadata.push(source.app_id.clone());
        if let Some(title) = source.title.as_deref() {
            metadata.push(title.to_string());
        }
    }
    metadata.extend(event.format_labels().into_iter().map(str::to_string));
    for view in event.file_views() {
        metadata.push(view.name.clone());
        if let Some(mime) = view.mime.clone() {
            metadata.push(mime);
        }
        metadata.push(view.path.to_string_lossy().into_owned());
    }
    metadata
}

/// 一条剪贴板历史结果的统一构造：稳定 id、来源、默认操作与完整预览。
pub fn clipboard_item(event: &ClipboardEvent, score: Score) -> SearchItem {
    SearchItem {
        id: event.item_id(),
        title: event.summary.clone(),
        subtitle: Some(clipboard_subtitle(event)),
        icon: None,
        source: CLIPBOARD_PLUGIN_ID.to_string(),
        kind: ItemKind::ClipboardEntry,
        // 默认操作是粘贴（ADR §4）；自动粘贴不可用时由宿主降级为
        // 「已复制，请手动粘贴」，反馈路径与备忘录完全一致（ticket 08）。
        default_action: DefaultAction::Paste,
        preview: clipboard_preview(event),
        score,
    }
}

/// 预览：文件列表逐条给出「引用 / 已保存副本」与当前是否可恢复，文字走原文。
///
/// 状态是**按当前文件系统**算出来的，不是捕获时的快照：原文件被删除后再次预览必须显示
/// 不可恢复（spec「原文件失效时引用显示不可恢复状态」）。
pub fn clipboard_preview(event: &ClipboardEvent) -> Preview {
    if event.is_file_list() {
        return Preview::Text {
            title: Some(event.summary.clone()),
            body: file_list_preview_body(event),
        };
    }
    Preview::Text {
        title: Some(event.summary.clone()),
        body: event
            .text
            .clone()
            .unwrap_or_else(|| "（该条目没有可显示的文字内容）".to_string()),
    }
}

/// 文件列表预览正文：第一行是数量汇总，之后每个文件一行状态。
pub fn file_list_preview_body(event: &ClipboardEvent) -> String {
    let views = event.file_views();
    let (references, copies) = event.file_counts();
    let mut lines = vec![format!(
        "{count} 个文件 · {references} 个引用{spec}",
        count = views.len(),
        spec = if copies > 0 {
            format!(" · {copies} 个已保存副本")
        } else {
            String::new()
        }
    )];
    for view in &views {
        let state = if view.recoverable {
            "可恢复".to_string()
        } else {
            format!(
                "不可恢复：{}",
                view.problem.as_deref().unwrap_or("原因未知")
            )
        };
        lines.push(format!(
            "{} — {}（{}）· {}",
            view.name,
            view.kind_label_zh(),
            state,
            describe_bytes(view.bytes)
        ));
    }
    lines.join("\n")
}

/// 人类可读的字节数。
pub fn describe_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// 结果副标题：置顶标记、格式、**引用/副本数量**、来源应用与相对时间。
pub fn clipboard_subtitle(event: &ClipboardEvent) -> String {
    let mut parts: Vec<String> = Vec::new();
    if event.pinned {
        parts.push("已置顶".to_string());
    }
    let labels = event.format_labels();
    if !labels.is_empty() {
        parts.push(labels.join("/"));
    }
    if event.is_file_list() {
        let (references, copies) = event.file_counts();
        let mut file_part = format!("{references} 个引用");
        if copies > 0 {
            file_part.push_str(&format!(" · {copies} 个已保存副本"));
        }
        parts.push(file_part);
    }
    if let Some(source) = &event.source {
        let name = source
            .title
            .clone()
            .unwrap_or_else(|| source.app_id.clone());
        parts.push(format!("来自 {name}"));
    }
    parts.push(crate::clipboard::describe_age(
        event.captured_at_ms,
        crate::clipboard::now_ms(),
    ));
    if event.copies > 1 {
        parts.push(format!("复制过 {} 次", event.copies));
    }
    parts.join(" · ")
}
