//! Tauri 命令：把 UI 的请求转发给 `flashcast_core::Host`。
//!
//! 这里只做三件事：调用宿主入口、把结果整理成 UI 需要的展示结构、
//! 把错误与提示原样带回。命令中不出现业务分支。

use std::path::Path;
use std::sync::Arc;

use flashcast_core::{
    ActionOutcome, Appearance, BackOutcome, CloneOutcome, CloneProgress, CommitOutcome,
    DefaultAction, ItemKind, Notice, PluginFailure, Preview, PullOutcome, PushOutcome,
    QueryResponse, QueryScope, Score, SearchItem, Settings, SyncProgress, SyncStatus, ThemeState,
    WorkspaceChanges, WorkspaceStatus,
};
use tauri::{AppHandle, Manager, State};

use crate::icon::icon_data_url;
use crate::state::{lock, AppState};
use crate::summon;
use crate::watch::WorkspaceEvent;

/// 展示用的结果条目：在宿主模型之上附加可直接显示的图标 data URL。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon_data_url: Option<String>,
    pub source: String,
    pub kind: ItemKind,
    pub default_action: DefaultAction,
    /// 操作栏显示的默认操作中文名，例如「打开」。
    pub default_action_label: String,
    pub score: Score,
}

/// 展示用的查询响应。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryView {
    pub seq: u64,
    pub scope: QueryScope,
    pub scope_label: String,
    pub input: String,
    pub items: Vec<ItemView>,
    pub selection: usize,
    pub notice: Option<Notice>,
    pub plugin_failures: Vec<PluginFailure>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackView {
    pub restored: bool,
    pub response: QueryView,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatusView {
    pub label: String,
    pub error: Option<String>,
    /// 是否已成功注册。
    pub registered: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    pub previous_app: Option<flashcast_platform::FocusedApp>,
    pub hotkey: HotkeyStatusView,
    pub capabilities: flashcast_platform::Capabilities,
    pub plugins: Vec<PluginView>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub keywords: Vec<String>,
    pub enabled: bool,
}

fn item_view(state: &AppState, item: &SearchItem) -> ItemView {
    let icon_data_url = item.icon.as_ref().and_then(|icon| {
        let path = icon.path.clone()?;
        let mut cache = lock(&state.icons);
        cache
            .entry(path.clone())
            .or_insert_with(|| icon_data_url(&path))
            .clone()
    });
    ItemView {
        id: item.id.clone(),
        title: item.title.clone(),
        subtitle: item.subtitle.clone(),
        icon_data_url,
        source: item.source.clone(),
        kind: item.kind,
        default_action: item.default_action,
        default_action_label: item.default_action.label_zh().to_string(),
        score: item.score,
    }
}

fn query_view(state: &AppState, response: &QueryResponse) -> QueryView {
    QueryView {
        seq: response.seq,
        scope: response.scope.clone(),
        scope_label: response.scope.label_zh(),
        input: response.input.clone(),
        items: response
            .items
            .iter()
            .map(|item| item_view(state, item))
            .collect(),
        selection: response.selection,
        notice: response.notice.clone(),
        plugin_failures: response.plugin_failures.clone(),
    }
}

/// 查询入口。
#[tauri::command]
pub fn query(state: State<'_, AppState>, input: String) -> QueryView {
    let response = state.host.query(&input);
    query_view(&state, &response)
}

/// 命令入口。UI 传条目 id，由宿主从最近一次结果中还原完整条目。
#[tauri::command(rename_all = "snake_case")]
pub fn execute(state: State<'_, AppState>, item_id: String) -> ActionOutcome {
    match state.host.item_by_id(&item_id) {
        Some(item) => state.host.execute(&item),
        None => {
            ActionOutcome::failed("结果已过期：请重新输入查询后再执行（列表可能已被重新扫描刷新）")
        }
    }
}

/// 键盘选择：移动 `delta`。
#[tauri::command(rename_all = "snake_case")]
pub fn move_selection(state: State<'_, AppState>, delta: isize) -> QueryView {
    let response = state.host.move_selection(delta);
    query_view(&state, &response)
}

/// 键盘选择：直接设置为指定下标。
#[tauri::command(rename_all = "snake_case")]
pub fn set_selection(state: State<'_, AppState>, index: usize) -> QueryView {
    let response = state.host.set_selection(index);
    query_view(&state, &response)
}

/// 返回上一查询范围；`restored` 为 false 时 UI 应关闭窗口。
#[tauri::command]
pub fn back(state: State<'_, AppState>) -> BackView {
    let BackOutcome { restored, response } = state.host.back();
    BackView {
        restored,
        response: query_view(&state, &response),
    }
}

/// 重新扫描软件目录。
#[tauri::command]
pub fn rescan(state: State<'_, AppState>) -> QueryView {
    let response = state.host.rescan();
    query_view(&state, &response)
}

/// 当前状态快照（不重新搜索），用于窗口重新显示时同步。
#[tauri::command(rename_all = "snake_case")]
pub fn refresh_state(state: State<'_, AppState>) -> QueryView {
    let response = state.host.snapshot();
    query_view(&state, &response)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_capabilities(state: State<'_, AppState>) -> flashcast_platform::Capabilities {
    state.host.capabilities()
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.host.settings()
}

/// 写入设置。快捷键变化时重新注册，注册失败会作为可展示的错误返回。
///
/// 宿主先把设置写进当前工作区文件，写入失败时保留上一次可用状态并返回中文原因。
#[tauri::command(rename_all = "snake_case")]
pub fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<HotkeyStatusView, String> {
    let previous = state.host.settings();
    let applied = state
        .host
        .update_settings(settings)
        .map_err(|error| error.to_string())?;
    if applied.hotkey != previous.hotkey {
        crate::hotkey::apply(&app, &state, &applied.hotkey);
    }
    Ok(hotkey_status(&state))
}

/// 当前主题状态：选中主题、CSS 自定义属性与可选主题列表。
#[tauri::command(rename_all = "snake_case")]
pub fn get_theme(state: State<'_, AppState>) -> ThemeState {
    state.host.theme_state()
}

/// 选择主题。失败时返回中文原因，且当前外观不变。
#[tauri::command(rename_all = "snake_case")]
pub fn select_theme(state: State<'_, AppState>, id: String) -> Result<ThemeState, String> {
    state
        .host
        .select_theme(&id)
        .map_err(|error| error.to_string())
}

/// 启用或停用插件（功能插件与主题插件共用同一张清单）。
#[tauri::command(rename_all = "snake_case")]
pub fn set_plugin_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .host
        .set_plugin_enabled(&id, enabled)
        .map_err(|error| error.to_string())
}

/// 上报当前系统外观；「跟随系统」的主题据此在运行时切换。
#[tauri::command(rename_all = "snake_case")]
pub fn set_system_appearance(state: State<'_, AppState>, appearance: Appearance) -> ThemeState {
    state.host.set_system_appearance(appearance)
}

/// 校验并安装一个本地主题包（目录或 JSON 文件）。失败时返回中文原因，
/// 已安装内容与当前外观保持不变。
#[tauri::command(rename_all = "snake_case")]
pub fn install_theme(state: State<'_, AppState>, path: String) -> Result<ThemeState, String> {
    state
        .host
        .install_theme_package(Path::new(&path))
        .map_err(|error| error.to_string())
}

/// 移除一个已安装的本地主题包。内置主题会被拒绝。
#[tauri::command(rename_all = "snake_case")]
pub fn remove_theme(state: State<'_, AppState>, id: String) -> Result<ThemeState, String> {
    state
        .host
        .remove_theme(&id)
        .map_err(|error| error.to_string())
}

/// 当前配置工作区与它的有效性。
#[tauri::command(rename_all = "snake_case")]
pub fn get_workspace(state: State<'_, AppState>) -> WorkspaceStatus {
    state.host.workspace_status()
}

/// 关联一个已存在的本地仓库 / 目录为配置工作区。失败时返回中文原因，
/// 并且不改变当前工作区与有效设置。
#[tauri::command(rename_all = "snake_case")]
pub fn select_workspace(
    state: State<'_, AppState>,
    path: String,
) -> Result<WorkspaceStatus, String> {
    state
        .host
        .select_workspace(Path::new(&path))
        .map_err(|error| error.to_string())
}

/// 在指定目录初始化新的配置工作区及其 Git 仓库。目标目录非空时拒绝。
#[tauri::command(rename_all = "snake_case")]
pub fn init_workspace(state: State<'_, AppState>, path: String) -> Result<WorkspaceStatus, String> {
    state
        .host
        .init_workspace(Path::new(&path))
        .map_err(|error| error.to_string())
}

/// 克隆请求里携带的 https 令牌。只写进**设备本地**目录，不进入工作区或日志。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TokenInput {
    pub username: String,
    pub token: String,
}

/// 从远端克隆配置工作区，成功后关联并恢复其中的设置与插件选择。
///
/// libgit2 是阻塞的 C 库，放到阻塞线程池执行，避免卡住 Tauri 运行时；
/// UI 通过 `clone_progress` 轮询进度、用 `cancel_clone` 取消。
#[tauri::command(rename_all = "snake_case")]
pub async fn clone_workspace(
    state: State<'_, AppState>,
    url: String,
    target: String,
    token: Option<TokenInput>,
) -> Result<CloneOutcome, String> {
    let host = host_of(&state);
    if let Some(token) = token.filter(|token| !token.token.trim().is_empty()) {
        host.remember_git_token(&url, &token.username, token.token.trim())
            .map_err(|error| error.to_string())?;
    }
    tauri::async_runtime::spawn_blocking(move || {
        host.clone_workspace(&url, Path::new(&target))
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 最近一次克隆的进度快照。
#[tauri::command(rename_all = "snake_case")]
pub fn clone_progress(state: State<'_, AppState>) -> CloneProgress {
    state.host.clone_progress()
}

/// 请求取消正在进行的克隆。
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_clone(state: State<'_, AppState>) {
    state.host.cancel_clone();
}

/// 当前工作区的 Git 变更：状态分类、分支与逐文件真实差异。
///
/// 只读入口，不修改仓库；工作区不是 Git 仓库或读取失败时，
/// 结果里的 `error` 给出中文原因（ADR §3 的补充入口）。
#[tauri::command(rename_all = "snake_case")]
pub fn get_git_changes(state: State<'_, AppState>) -> WorkspaceChanges {
    state.host.workspace_changes()
}

/// 创建 Git 提交。提交范围**只包含** `paths` 里显式给出的路径。
///
/// 失败（空提交说明、没有选择、身份未配置、工作区异常、索引被占用）时返回中文原因；
/// 宿主保证此时不产生提交，也不改动工作区文件与用户已有的暂存状态。
#[tauri::command(rename_all = "snake_case")]
pub fn commit_changes(
    state: State<'_, AppState>,
    message: String,
    paths: Vec<String>,
) -> Result<CommitOutcome, String> {
    state
        .host
        .commit_workspace(&message, &paths)
        .map_err(|error| error.to_string())
}

/// 重新读取工作区配置（外部修改未触发监听时的兜底入口）。
#[tauri::command(rename_all = "snake_case")]
pub fn reload_workspace(state: State<'_, AppState>) -> WorkspaceEvent {
    let reload = state.host.reload_workspace();
    WorkspaceEvent {
        status: state.host.workspace_status(),
        settings: state.host.settings(),
        theme: state.host.theme_state(),
        reload: Some(reload),
    }
}

/// 当前工作区的同步状态（分支、远端、领先 / 落后、阻塞原因与指引）。
///
/// 只读入口：不发网络请求，也不改动仓库。业务判断全在宿主。
#[tauri::command(rename_all = "snake_case")]
pub fn get_sync_status(state: State<'_, AppState>) -> SyncStatus {
    state.host.sync_status()
}

/// 重新检测同步状态：用户在应用外部处理完阻塞后调用它恢复同步。
#[tauri::command(rename_all = "snake_case")]
pub fn redetect_sync_state(state: State<'_, AppState>) -> SyncStatus {
    state.host.redetect_sync_state()
}

/// 仅快进拉取当前工作区。
///
/// libgit2 是阻塞的 C 库，放到阻塞线程池执行；UI 用 `sync_progress` 轮询进度、
/// 用 `cancel_sync` 取消。阻塞状态（未提交修改、分叉、冲突、进行中操作）在
/// 宿主里先于网络操作返回，工作区与索引保持不变。
#[tauri::command(rename_all = "snake_case")]
pub async fn pull_workspace(state: State<'_, AppState>) -> Result<PullOutcome, String> {
    let host = host_of(&state);
    tauri::async_runtime::spawn_blocking(move || {
        host.pull_workspace().map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 显式推送当前分支到它的上游（永不 force）。
#[tauri::command(rename_all = "snake_case")]
pub async fn push_workspace(state: State<'_, AppState>) -> Result<PushOutcome, String> {
    let host = host_of(&state);
    tauri::async_runtime::spawn_blocking(move || {
        host.push_workspace().map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 最近一次同步的进度快照。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_progress(state: State<'_, AppState>) -> SyncProgress {
    state.host.sync_progress()
}

/// 请求取消正在进行的同步。
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_sync(state: State<'_, AppState>) {
    state.host.cancel_sync();
}

// ---------------------------------------------------------------------------
// 备忘录（ticket 07）
// ---------------------------------------------------------------------------

/// 展示用的备忘录条目。字段与 `flashcast_core::Memo` 一一对应。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoView {
    /// 稳定标识，同时是 `memos/<id>.md` 的文件名。
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub body: String,
}

impl MemoView {
    fn from_memo(memo: flashcast_core::Memo) -> Self {
        Self {
            id: memo.id,
            title: memo.title,
            tags: memo.tags,
            body: memo.body,
        }
    }
}

/// 展示用的「无法读取的备忘录文件」。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoProblemView {
    pub path: String,
    /// 面向用户的中文原因。
    pub reason: String,
}

/// 当前生效的备忘录（按标识排序，与目录读取顺序无关）。
#[tauri::command]
pub fn memos(state: State<'_, AppState>) -> Vec<MemoView> {
    state
        .host
        .memos()
        .into_iter()
        .map(MemoView::from_memo)
        .collect()
}

/// 无法读取的备忘录文件：宿主保留可用内容，并如实报告原因。
#[tauri::command(rename_all = "snake_case")]
pub fn memo_problems(state: State<'_, AppState>) -> Vec<MemoProblemView> {
    state
        .host
        .memo_problems()
        .into_iter()
        .map(|problem| MemoProblemView {
            path: problem.path.to_string_lossy().into_owned(),
            reason: problem.reason,
        })
        .collect()
}

/// 新建一条备忘录。未关联工作区或插件已停用时返回中文原因。
#[tauri::command(rename_all = "snake_case")]
pub fn create_memo(
    state: State<'_, AppState>,
    title: String,
    tags: Vec<String>,
    body: String,
) -> Result<MemoView, String> {
    state
        .host
        .create_memo(&title, &tags, &body)
        .map(MemoView::from_memo)
        .map_err(|error| error.to_string())
}

/// 修改一条已存在的备忘录（标识不变）。
#[tauri::command(rename_all = "snake_case")]
pub fn update_memo(
    state: State<'_, AppState>,
    id: String,
    title: String,
    tags: Vec<String>,
    body: String,
) -> Result<MemoView, String> {
    state
        .host
        .update_memo(&id, &title, &tags, &body)
        .map(MemoView::from_memo)
        .map_err(|error| error.to_string())
}

/// 删除一条备忘录（同时删除工作区里的文件）。
#[tauri::command(rename_all = "snake_case")]
pub fn delete_memo(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .host
        .delete_memo(&id)
        .map_err(|error| error.to_string())
}

/// 预览某条结果。备忘录按**当前**内容返回完整正文；未知 id 返回 `null`。
#[tauri::command(rename_all = "snake_case")]
pub fn preview(state: State<'_, AppState>, item_id: String) -> Option<Preview> {
    state.host.preview(&item_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_status(state: State<'_, AppState>) -> StatusView {
    let plugins = state
        .host
        .plugin_manifests()
        .into_iter()
        .map(|(manifest, enabled)| PluginView {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            keywords: manifest.keywords,
            enabled,
        })
        .collect();
    StatusView {
        previous_app: state.previous_app(),
        hotkey: hotkey_status(&state),
        capabilities: state.host.capabilities(),
        plugins,
    }
}

/// 隐藏窗口（Escape 路径由 UI 决定，最终调用这里）。
#[tauri::command(rename_all = "snake_case")]
pub fn hide_window(app: AppHandle) {
    summon::hide(&app);
}

fn hotkey_status(state: &AppState) -> HotkeyStatusView {
    let hotkey = lock(&state.hotkey);
    HotkeyStatusView {
        label: hotkey.label.clone(),
        error: hotkey.error.clone(),
        registered: hotkey.handle.is_some(),
    }
}

/// 供托盘「重新扫描软件」使用：扫描后把新状态推送给 UI。
pub fn rescan_and_push(app: &AppHandle) {
    let state = app.state::<AppState>();
    let response = state.host.rescan();
    let view = query_view(&state, &response);
    let _ = tauri::Emitter::emit(app, "flashcast://state", view);
}

/// 供内部复用：把 Arc<Host> 交给外壳。
pub fn host_of(state: &AppState) -> Arc<flashcast_core::Host> {
    Arc::clone(&state.host)
}
