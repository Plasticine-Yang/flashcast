//! 宿主的结果模型。对应 ADR §3–§4 的查询入口与结果结构。

use std::path::PathBuf;

use flashcast_platform::IconRef;
use serde::{Deserialize, Serialize};

/// 来源标识。宿主自身的条目使用 [`HOST_SOURCE`]，插件条目使用插件 id。
pub type SourceId = String;

/// 宿主自身作为结果来源时使用的标识。
pub const HOST_SOURCE: &str = "flashcast";

/// 条目的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Application,
    Memo,
    ClipboardEntry,
    Bookmark,
    Command,
}

/// 按下回车时执行的默认操作。操作栏据此显示「打开 / 在 Chrome 打开 / 粘贴」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultAction {
    Open,
    OpenInChrome,
    Paste,
}

impl DefaultAction {
    /// 面向用户的中文操作名。
    pub fn label_zh(self) -> &'static str {
        match self {
            DefaultAction::Open => "打开",
            DefaultAction::OpenInChrome => "在 Chrome 打开",
            DefaultAction::Paste => "粘贴",
        }
    }
}

/// 匹配层级。数值越小优先级越高，对应 ADR §4 的稳定排序键第一项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchTier {
    /// 关键词完整匹配（插件关键词）或标签精确匹配。
    KeywordOrTagExact = 0,
    /// 标题前缀匹配（标题完全相等也属于本层级，由 relevance 区分）。
    TitlePrefix = 1,
    /// 标题子串匹配。
    TitleSubstring = 2,
    /// 正文、路径或元数据匹配。
    MetadataSubstring = 3,
}

/// 贡献者计算的分数。宿主排序使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub tier: MatchTier,
    /// 贡献者内部的相对分数，越大越靠前。
    pub relevance: u8,
}

impl Score {
    pub fn new(tier: MatchTier, relevance: u8) -> Self {
        Self { tier, relevance }
    }

    /// 不参与匹配的条目（例如空查询下的快速访问项）。
    pub fn unordered() -> Self {
        Self {
            tier: MatchTier::TitlePrefix,
            relevance: 0,
        }
    }
}

/// 预览内容。v0.1.0 只在需要阅读长内容或查看图片时展开。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Preview {
    None,
    Text { title: Option<String>, body: String },
    Image { path: PathBuf },
}

/// 查询范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QueryScope {
    /// 首屏：软件与备忘录标签。
    Home,
    /// 某个功能插件的范围。
    Plugin { id: String, keyword: String },
}

impl QueryScope {
    pub fn is_home(&self) -> bool {
        matches!(self, QueryScope::Home)
    }

    /// 面向用户的范围说明。
    pub fn label_zh(&self) -> String {
        match self {
            QueryScope::Home => "首屏".to_string(),
            QueryScope::Plugin { keyword, .. } => format!("{keyword} 范围"),
        }
    }
}

/// 提示级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// 面向用户的提示或错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
}

impl Notice {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Info,
            message: message.into(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Warning,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Error,
            message: message.into(),
        }
    }
}

/// 查询返回的候选条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchItem {
    /// 稳定标识，跨查询与重启一致。
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<IconRef>,
    pub source: SourceId,
    pub kind: ItemKind,
    pub default_action: DefaultAction,
    pub preview: Preview,
    pub score: Score,
}

/// 插件本轮失败的记录。插件失败不影响宿主自身结果与其他插件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFailure {
    pub plugin_id: String,
    pub reason: String,
    pub kind: PluginFailureKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginFailureKind {
    Error,
    Timeout,
    Panic,
}

/// 查询入口的返回。`seq` 单调递增，UI 丢弃 seq 小于已应用值的响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse {
    pub seq: u64,
    pub scope: QueryScope,
    /// 产生该响应时的查询输入，`back()` 恢复查询后据此回显。
    pub input: String,
    pub items: Vec<SearchItem>,
    /// 宿主维护的键盘选择。
    pub selection: usize,
    pub notice: Option<Notice>,
    /// 本轮失败的插件；空表示全部正常。
    pub plugin_failures: Vec<PluginFailure>,
}

impl QueryResponse {
    /// 空响应，用于错误路径上也返回结构完整的对象。
    pub fn empty(seq: u64) -> Self {
        Self {
            seq,
            scope: QueryScope::Home,
            input: String::new(),
            items: Vec::new(),
            selection: 0,
            notice: None,
            plugin_failures: Vec::new(),
        }
    }

    /// 当前选中的条目。
    pub fn selected(&self) -> Option<&SearchItem> {
        self.items.get(self.selection)
    }

    /// 该响应是否应被丢弃：已应用的 seq 大于等于它。
    pub fn is_stale(&self, applied_seq: u64) -> bool {
        self.seq <= applied_seq
    }
}

/// 命令入口的执行结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionStatus {
    Done,
    /// 已复制到剪贴板，但需要用户手动粘贴。
    CopiedNeedsManualPaste,
    Failed,
}

/// 命令入口的返回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcome {
    pub status: ActionStatus,
    /// 面向用户的中文反馈。
    pub message: Option<String>,
}

impl ActionOutcome {
    pub fn done(message: Option<String>) -> Self {
        Self {
            status: ActionStatus::Done,
            message,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: ActionStatus::Failed,
            message: Some(message.into()),
        }
    }

    pub fn is_failed(&self) -> bool {
        self.status == ActionStatus::Failed
    }
}

/// `back()` 的结果。`restored` 为 false 表示已经在最外层，UI 应关闭窗口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackOutcome {
    pub restored: bool,
    pub response: QueryResponse,
}

/// 宿主内置命令的标识前缀。
pub const COMMAND_PREFIX: &str = "flashcast.command.";

/// 重新扫描软件的命令 id。
pub const COMMAND_RESCAN: &str = "flashcast.command.rescan";

/// 查看平台能力的命令 id。
pub const COMMAND_CAPABILITIES: &str = "flashcast.command.capabilities";
