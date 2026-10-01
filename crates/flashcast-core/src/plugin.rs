//! 功能插件契约。对应 ADR §6。
//!
//! 宿主对每个插件的 `search` 施加超时与 panic 隔离：插件超时、报错或 panic 时
//! 本轮返回空结果并记录插件错误，不影响宿主自身结果与其他插件。

use serde::{Deserialize, Serialize};

use crate::model::{QueryScope, SearchItem};

/// 能力标识：写入系统剪贴板的文本。
///
/// 功能插件在清单里声明所需能力；宿主在**原生边界**（真正调用平台适配层之前）
/// 校验声明，未声明的插件拿不到原生能力（ADR §6）。
pub const CAP_CLIPBOARD_WRITE: &str = "clipboard.write";

/// 能力标识：读取系统剪贴板内容（剪贴板历史的捕获依据）。
///
/// 后台捕获对隐私敏感，因此宿主只在来源插件**声明并启用**时才启动轮询；
/// 「启用插件」与「授权读取剪贴板」是同一件事的两面（spec「剪贴板在用户启用后后台监听」）。
pub const CAP_CLIPBOARD_READ: &str = "clipboard.read";

/// 去掉输入里已经用于进入范围的关键词前缀。
///
/// 用户输入「备忘录」后通常会在同一个输入框里继续写查询，因此范围把关键词本身
/// 视为空查询，并把 `备忘录 回复` 之类的前缀剥掉。多个插件共用同一套判断，
/// 避免各自的实现出现细微差异。
pub fn strip_keyword(query: &str, keyword: &str) -> String {
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

/// 插件种类。v0.1.0 的功能插件使用 `Feature`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginKind {
    Feature,
    Theme,
}

/// 插件清单：标识、种类、版本、关键词别名与所需能力。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    /// 面向用户的名称。
    pub name: String,
    pub kind: PluginKind,
    pub version: String,
    /// 关键词别名，中英文均可。完整匹配后进入该插件范围。
    pub keywords: Vec<String>,
    /// 所需能力标识，用于宿主授权判定。
    pub capabilities: Vec<String>,
}

impl PluginManifest {
    pub fn feature(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: PluginKind::Feature,
            version: version.into(),
            keywords: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    pub fn with_keywords<I, S>(mut self, keywords: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    /// 声明所需能力。宿主在原生边界按它做权限校验。
    pub fn with_capabilities<I, S>(mut self, capabilities: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.capabilities = capabilities.into_iter().map(Into::into).collect();
        self
    }

    /// 是否声明了某项能力。
    pub fn requires(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|item| item == capability)
    }

    /// 关键词是否与该输入完整匹配（不区分大小写）。
    pub fn matches_keyword(&self, input: &str) -> Option<String> {
        let normalized = input.trim().to_lowercase();
        if normalized.is_empty() {
            return None;
        }
        self.keywords
            .iter()
            .find(|keyword| keyword.to_lowercase() == normalized)
            .cloned()
    }
}

/// 插件标识是否合法。功能插件与主题插件共用同一条规则：小写字母、数字、
/// 点、下划线与连字符，最长 64 个字符。写入清单的标识必须通过它。
pub fn is_valid_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_'
        })
}

/// 一次插件搜索的上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchContext {
    /// 用户原始输入。
    pub input: String,
    /// 已小写化并去空白的查询串。
    pub query: String,
    pub scope: QueryScope,
    /// 本次最多返回多少条。
    pub limit: usize,
}

impl SearchContext {
    pub fn new(input: &str, scope: QueryScope, limit: usize) -> Self {
        Self {
            input: input.to_string(),
            query: input.trim().to_lowercase(),
            scope,
            limit,
        }
    }
}

/// 插件关键词。用于请求进入某个插件的范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyword(pub String);

impl Keyword {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    #[error("{0}")]
    Failed(String),
}

impl PluginError {
    pub fn failed(message: impl Into<String>) -> Self {
        PluginError::Failed(message.into())
    }
}

/// 进入插件范围后用于检索的范围对象。
pub trait PluginScope: Send + Sync {
    fn plugin_id(&self) -> &str;

    /// 进入该范围时使用的关键词。
    fn keyword(&self) -> &str;

    fn search(&self, ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError>;
}

/// 功能插件。
pub trait FeaturePlugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;

    /// 首次写入插件清单时的默认启用状态。
    ///
    /// 默认启用。剪贴板历史把它改成 `false`：后台捕获用户复制的内容是隐私敏感行为，
    /// 必须由用户显式启用（spec「剪贴板在用户启用后后台监听」）。它的取值只在清单里
    /// **还没有**这条记录时生效；此后清单是唯一权威。
    fn default_enabled(&self) -> bool {
        true
    }

    /// 是否参与首屏搜索。
    fn contributes_to_home(&self) -> bool;

    fn search(&self, ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError>;

    /// 关键词完整匹配时返回该插件的范围对象。
    fn take_scope(&self, keyword: &Keyword) -> Option<Box<dyn PluginScope>>;
}

/// 一个不参与搜索的空实现基类，供只需要关键词入口的插件复用。
pub struct NoScope;

impl PluginScope for NoScope {
    fn plugin_id(&self) -> &str {
        "none"
    }

    fn keyword(&self) -> &str {
        ""
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        Ok(Vec::new())
    }
}
