//! 从远端 Git 仓库克隆配置工作区（ticket 14）。
//!
//! 设计要点（ADR §9；API 序列见 `notes/research/git-workspace.md`）：
//!
//! - **进度**来自 `git2` 的真实回调：`RemoteCallbacks::transfer_progress` /
//!   `sideband_progress` 与 `CheckoutBuilder::progress`（git2 0.21 **没有**
//!   `build_checkout_callbacks`）。UI 轮询 [`CloneControl::progress`]。
//! - **取消**由 `Arc<AtomicBool>` 表达；回调返回 `false` 即中断传输 / 检出。
//! - **不覆盖**：目标目录非空一律拒绝；失败时回滚本次创建的内容
//!   （沿用 ticket 05 的「先创建后回滚」模式）。
//! - **凭证**复用用户已有配置：ssh-agent、`~/.ssh` 下的密钥、系统 git 的
//!   credential helper（`Config::open_default()` 只读系统 + 全局配置，
//!   工作区里的 `.git/config` 无法劫持它）以及设备本地令牌。
//! - **脱敏**：任何离开本模块的错误文本都会去掉 URL 里的 userinfo、抹掉本次
//!   使用过的口令与常见令牌形状。凭证绝不写进工作区、配置或日志。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::device::StoredToken;
use crate::workspace::{WorkspaceError, WorkspaceRemote, WorkspaceStatus};

/// 克隆阶段。UI 据此显示中文状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClonePhase {
    /// 尚未开始。
    Idle,
    /// 正在连接远端。
    Connecting,
    /// 正在接收对象。
    Receiving,
    /// 正在解析增量。
    Resolving,
    /// 正在检出工作区文件。
    CheckingOut,
    /// 完成。
    Done,
    /// 失败。
    Failed,
    /// 已取消。
    Cancelled,
}

impl ClonePhase {
    /// 面向用户的中文标签。
    pub fn label_zh(self) -> &'static str {
        match self {
            ClonePhase::Idle => "尚未开始",
            ClonePhase::Connecting => "正在连接远端",
            ClonePhase::Receiving => "正在接收数据",
            ClonePhase::Resolving => "正在解析数据",
            ClonePhase::CheckingOut => "正在检出文件",
            ClonePhase::Done => "克隆完成",
            ClonePhase::Failed => "克隆失败",
            ClonePhase::Cancelled => "已取消",
        }
    }
}

/// 一次克隆的进度快照。数值全部来自 git2 的真实回调。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgress {
    pub phase: ClonePhase,
    /// 已接收对象数。
    pub received_objects: usize,
    /// 远端声明的对象总数。
    pub total_objects: usize,
    /// 已写入本地对象库的对象数。
    pub indexed_objects: usize,
    /// 已接收字节数。
    pub received_bytes: usize,
    /// 当前检出的文件（相对工作区）。
    pub checkout_path: Option<String>,
    /// 已检出文件数。
    pub checkout_completed: usize,
    /// 需要检出的文件总数。
    pub checkout_total: usize,
    /// 回调触发次数。为 0 说明进度不是来自真实回调。
    pub updates: u64,
    /// 中文说明：失败原因或结束语。
    pub message: Option<String>,
}

impl Default for CloneProgress {
    fn default() -> Self {
        Self {
            phase: ClonePhase::Idle,
            received_objects: 0,
            total_objects: 0,
            indexed_objects: 0,
            received_bytes: 0,
            checkout_path: None,
            checkout_completed: 0,
            checkout_total: 0,
            updates: 0,
            message: None,
        }
    }
}

impl CloneProgress {
    /// 供 UI 显示的百分比；总数未知时为 `None`。
    pub fn percent(&self) -> Option<u8> {
        let (done, total) = match self.phase {
            ClonePhase::CheckingOut if self.checkout_total > 0 => {
                (self.checkout_completed, self.checkout_total)
            }
            _ if self.total_objects > 0 => (self.indexed_objects, self.total_objects),
            _ => return None,
        };
        if total == 0 {
            return None;
        }
        Some(((done.min(total) * 100) / total) as u8)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 一次克隆操作的进度与取消信号。克隆调用与 UI 轮询分属不同线程。
#[derive(Debug, Clone, Default)]
pub struct CloneControl {
    progress: Arc<Mutex<CloneProgress>>,
    cancel: Arc<AtomicBool>,
}

impl CloneControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// 重置为「尚未开始」，供新一次尝试使用。
    pub fn reset(&self) {
        self.cancel.store(false, Ordering::SeqCst);
        *lock(&self.progress) = CloneProgress::default();
    }

    /// 当前进度快照。
    pub fn progress(&self) -> CloneProgress {
        lock(&self.progress).clone()
    }

    /// 请求取消。进行中的回调在下一次触发时中断。
    pub fn cancel(&self) {
        self.cancel.fetch_or(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// 开始一次尝试：进入「正在连接远端」。
    pub fn start(&self) {
        self.reset();
        self.update(|progress| {
            progress.phase = ClonePhase::Connecting;
            progress.message = None;
        });
    }

    /// 结束一次尝试。
    pub fn finish(&self, phase: ClonePhase, message: Option<String>) {
        self.update(|progress| {
            progress.phase = phase;
            progress.message = message;
        });
    }

    pub(crate) fn update(&self, change: impl FnOnce(&mut CloneProgress)) {
        change(&mut lock(&self.progress));
    }
}

/// 克隆使用的凭证来源。只保存**本次**可用的信息，不写入工作区。
#[derive(Debug, Clone, Default)]
pub struct CredentialProvider {
    token: Option<StoredToken>,
}

impl CredentialProvider {
    /// 不提供任何设备本地令牌：只走 ssh-agent / `~/.ssh` / credential helper。
    pub fn new() -> Self {
        Self::default()
    }

    /// 带上设备本地为该主机保存的 https 令牌。
    pub fn with_token(token: Option<StoredToken>) -> Self {
        Self { token }
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    /// git2 的凭证回调。`secrets` 收集本次交给 libgit2 的明文口令，供脱敏使用。
    fn credential(
        &self,
        url: &str,
        username_from_url: Option<&str>,
        allowed: git2::CredentialType,
        secrets: &Mutex<Vec<String>>,
    ) -> Result<git2::Cred, git2::Error> {
        let user = username_from_url.unwrap_or("git");

        // 1. ssh：agent 优先，其次 ~/.ssh 下的常用密钥。
        if allowed.contains(git2::CredentialType::SSH_KEY) {
            if let Some(credential) = ssh_agent_credential(user) {
                return Ok(credential);
            }
            if let Some(credential) = ssh_key_file_credential(user) {
                return Ok(credential);
            }
        }

        // 2. https：设备本地令牌，其次系统 git 的 credential helper。
        if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            if let Some(token) = &self.token {
                if token_url_matches(url, token) {
                    lock(secrets).push(token.token.clone());
                    return git2::Cred::userpass_plaintext(&token.username, &token.token);
                }
            }
            // Config::open_default() 只读系统 + 全局 + XDG 配置，**不读**工作区的
            // .git/config，因此恶意工作区无法改写凭证 helper。
            if let Ok(config) = git2::Config::open_default() {
                if let Ok(credential) = git2::Cred::credential_helper(&config, url, Some(user)) {
                    return Ok(credential);
                }
            }
        }

        if allowed.contains(git2::CredentialType::USERNAME) {
            return git2::Cred::username(user);
        }

        // 放弃：交给调用方转成面向用户的鉴权指引。
        git2::Cred::default()
    }
}

/// 设备本地令牌是否适用于该远端地址。
fn token_url_matches(url: &str, token: &StoredToken) -> bool {
    host_of(url).is_some_and(|host| host.eq_ignore_ascii_case(&token.host))
}

/// 用 ssh-agent 鉴权。libgit2 只认进程环境里的 `SSH_AUTH_SOCK`，
/// 而从图形界面启动的应用通常不继承它，因此这里主动探测常见位置。
fn ssh_agent_credential(user: &str) -> Option<git2::Cred> {
    ensure_ssh_agent_socket()?;
    git2::Cred::ssh_key_from_agent(user).ok()
}

/// 保证 `SSH_AUTH_SOCK` 指向一个真实存在的代理套接字。
fn ensure_ssh_agent_socket() -> Option<PathBuf> {
    if let Some(socket) = std::env::var_os("SSH_AUTH_SOCK") {
        let path = PathBuf::from(socket);
        if path.exists() {
            return Some(path);
        }
    }
    let candidate = probe_ssh_agent_socket()?;
    // 安全：设置进程环境变量只在探测到真实存在的套接字时发生。
    std::env::set_var("SSH_AUTH_SOCK", &candidate);
    Some(candidate)
}

/// 探测 ssh-agent 套接字的常见位置（GUI 启动的应用拿不到 shell 环境）。
fn probe_ssh_agent_socket() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        for dir in ["/run/user", "/tmp"] {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            let mut found: Vec<PathBuf> = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                // /run/user/<uid>/keyring/ssh
                let keyring = path.join("keyring/ssh");
                if keyring.exists() {
                    found.push(keyring);
                }
                // /tmp/ssh-XXXX/agent.NNNN
                if name.starts_with("ssh-") && path.is_dir() {
                    if let Ok(inner) = std::fs::read_dir(&path) {
                        for item in inner.flatten() {
                            let item_name = item.file_name().to_string_lossy().into_owned();
                            if item_name.starts_with("agent.") {
                                found.push(item.path());
                            }
                        }
                    }
                }
            }
            // 目录顺序在不同机器上不稳定，固定排序让行为可复现。
            found.sort();
            if let Some(path) = found.into_iter().next() {
                return Some(path);
            }
        }
    }
    // ~/.ssh/agent/s.*（部分发行版的 systemd 用户会话把套接字放这里）
    let home = std::env::var_os("HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    })?;
    let dir = home.join(".ssh/agent");
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.exists())
        .collect();
    found.sort();
    found.into_iter().next()
}

/// 用 `~/.ssh` 下的常用私钥鉴权。带口令的密钥需要口令，首版不在此处索取。
fn ssh_key_file_credential(user: &str) -> Option<git2::Cred> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))?;
    for name in ["id_ed25519", "id_ecdsa", "id_rsa"] {
        let private = home.join(".ssh").join(name);
        if !private.exists() {
            continue;
        }
        let public = home.join(".ssh").join(format!("{name}.pub"));
        let public = public.exists().then_some(public);
        if let Ok(credential) = git2::Cred::ssh_key(user, public.as_deref(), &private, None) {
            return Some(credential);
        }
    }
    None
}

/// 克隆成功后的结果：新工作区状态、远端关系，以及本机对工作区所选主题与插件的
/// 实际覆盖情况（ticket 06/07 的主题与插件清单落地前，如实报告而不过度承诺）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneOutcome {
    /// 克隆并关联后的工作区状态。
    pub workspace: WorkspaceStatus,
    /// 远端 ↔ 工作区关系，供 ticket 16 的同步复用。
    pub remote: WorkspaceRemote,
    /// 工作区记录为「停用」但本机没有对应实现的插件 id。
    pub unavailable_plugins: Vec<String>,
    /// 工作区 `theme.json` 记录的主题名（主题支持由 ticket 06 落地）。
    pub recorded_theme: Option<String>,
}

/// 一次已完成的克隆。失败路径上由调用方决定何时回滚。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClonedTarget {
    path: PathBuf,
    created: bool,
}

impl ClonedTarget {
    /// 目标目录路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 回滚本次克隆：本次创建的目录整个删除；原本已存在的空目录只清空内容。
    ///
    /// 绝不删除本次之前就存在的文件（非空目录在克隆前已被拒绝）。
    pub fn rollback(&self) {
        if self.created {
            let _ = std::fs::remove_dir_all(&self.path);
            return;
        }
        let Ok(entries) = std::fs::read_dir(&self.path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// 从远端克隆到目标目录。
///
/// 目标目录非空时拒绝；失败（含取消）时**自动回滚**，不留半成品目录。
/// 成功时返回 [`ClonedTarget`]，由调用方在后续校验失败时回滚。
pub fn clone_repository(
    url: &str,
    target: &Path,
    control: &CloneControl,
    provider: &CredentialProvider,
) -> Result<ClonedTarget, WorkspaceError> {
    let target = target.to_path_buf();
    if target.exists() && !target.is_dir() {
        return Err(WorkspaceError::NotADirectory(target));
    }
    if target.is_dir() {
        let mut entries = std::fs::read_dir(&target)?;
        if entries.next().is_some() {
            return Err(WorkspaceError::CloneTargetNotEmpty(target));
        }
    }
    let created = !target.exists();
    std::fs::create_dir_all(&target)?;

    let result = run_clone(url, &target, control, provider);
    match result {
        Ok(()) => Ok(ClonedTarget {
            path: target,
            created,
        }),
        Err(error) => {
            let cloned = ClonedTarget {
                path: target,
                created,
            };
            cloned.rollback();
            Err(error)
        }
    }
}

/// 真正的 `git2` 克隆调用。所有错误都已脱敏并附上中文指引。
fn run_clone(
    url: &str,
    target: &Path,
    control: &CloneControl,
    provider: &CredentialProvider,
) -> Result<(), WorkspaceError> {
    if control.is_cancelled() {
        control.finish(ClonePhase::Cancelled, Some("克隆已取消".to_string()));
        return Err(WorkspaceError::CloneCancelled);
    }

    let secrets: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials({
        let provider = provider.clone();
        let secrets = Arc::clone(&secrets);
        move |url, username, allowed| provider.credential(url, username, allowed, &secrets)
    });
    callbacks.transfer_progress({
        let control = control.clone();
        move |stats| {
            control.update(|progress| {
                progress.updates += 1;
                progress.received_objects = stats.received_objects();
                progress.total_objects = stats.total_objects();
                progress.indexed_objects = stats.indexed_objects();
                progress.received_bytes = stats.received_bytes();
                progress.phase = if stats.total_objects() > 0
                    && stats.indexed_objects() >= stats.total_objects()
                {
                    ClonePhase::Resolving
                } else {
                    ClonePhase::Receiving
                };
            });
            !control.is_cancelled()
        }
    });
    callbacks.sideband_progress({
        let control = control.clone();
        move |_data| {
            control.update(|progress| {
                progress.updates += 1;
                if progress.phase == ClonePhase::Connecting {
                    progress.phase = ClonePhase::Receiving;
                }
            });
            !control.is_cancelled()
        }
    });

    let mut fetch = git2::FetchOptions::new();
    fetch
        .remote_callbacks(callbacks)
        .download_tags(git2::AutotagOption::All)
        .follow_redirects(git2::RemoteRedirect::All)
        .update_fetchhead(true);

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.progress({
        let control = control.clone();
        move |path, completed, total| {
            control.update(|progress| {
                progress.updates += 1;
                progress.phase = ClonePhase::CheckingOut;
                progress.checkout_path = path.map(|path| path.display().to_string());
                progress.checkout_completed = completed;
                progress.checkout_total = total;
            });
        }
    });

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch).with_checkout(checkout);
    let cloned = builder.clone(url, target);

    match cloned {
        Ok(_repository) => {
            control.finish(ClonePhase::Done, Some("克隆完成".to_string()));
            Ok(())
        }
        Err(error) => {
            let error = clone_error(&error, control, &lock(&secrets));
            Err(error)
        }
    }
}

/// 把 `git2` 的错误转成脱敏后的中文错误，并在能识别时给出可操作指引。
fn clone_error(error: &git2::Error, control: &CloneControl, secrets: &[String]) -> WorkspaceError {
    if control.is_cancelled() {
        control.finish(ClonePhase::Cancelled, Some("克隆已取消".to_string()));
        return WorkspaceError::CloneCancelled;
    }
    let message = redact_secrets(&redact(error.message()), secrets);
    let hint = error_hint(error);
    control.finish(
        ClonePhase::Failed,
        Some(match &hint {
            Some(hint) => format!("{message}（{hint}）"),
            None => message.clone(),
        }),
    );
    WorkspaceError::Clone(match hint {
        Some(hint) => format!("{message}（{hint}）"),
        None => message,
    })
}

/// 面向用户的下一步指引。先把错误分到（网络 / 鉴权 / 证书 / 本地）桶里，
/// 再用消息文本在两三个候选说法之间挑选，绝不把原始消息当成唯一依据。
pub fn error_hint(error: &git2::Error) -> Option<String> {
    use git2::{ErrorClass, ErrorCode};

    let message = error.message().to_ascii_lowercase();
    let network_words = [
        "could not resolve host",
        "failed to resolve",
        "connection refused",
        "connection timed out",
        "network is unreachable",
        "failed to connect",
        "no route to host",
        "operation timed out",
    ];
    let auth_words = [
        "permission denied",
        "authentication failed",
        "no supported authentication",
        "401",
        "403",
        "invalid credentials",
        "username",
    ];

    if error.code() == ErrorCode::Certificate || error.class() == ErrorClass::Ssl {
        return Some(
            "TLS 证书校验失败：请检查系统时间与根证书；自签名证书不受支持".to_string(),
        );
    }
    if error.code() == ErrorCode::Timeout {
        return Some("连接远端超时：请稍后重试".to_string());
    }
    if network_words.iter().any(|word| message.contains(word)) {
        return Some("请检查网络、代理与远端地址是否正确".to_string());
    }
    if error.code() == ErrorCode::Auth || error.class() == ErrorClass::Http {
        if auth_words.iter().any(|word| message.contains(word)) || error.code() == ErrorCode::Auth {
            return Some(
                "鉴权失败：https 请在设置中为该主机填写访问令牌，或确认系统 git 的凭证 helper 可用"
                    .to_string(),
            );
        }
    }
    if error.class() == ErrorClass::Ssh
        || message.contains("publickey")
        || message.contains("ssh")
    {
        return Some(
            "鉴权失败：请确认 ssh-agent 正在运行（SSH_AUTH_SOCK），或把私钥放在 ~/.ssh 下；\
             libgit2 不读取 ~/.ssh/config 的 Host 别名，请在地址中直接写主机名"
                .to_string(),
        );
    }
    if error.class() == ErrorClass::Callback {
        return Some(
            "无法取得远端凭证：请确认 ssh-agent（SSH_AUTH_SOCK）或系统 git 的凭证 helper 可用"
                .to_string(),
        );
    }
    if error.class() == ErrorClass::Os || error.class() == ErrorClass::Filesystem {
        return Some("请检查目标目录的权限与磁盘空间".to_string());
    }
    None
}

/// 去掉文本里的 URL userinfo、已知口令与常见令牌形状（防御性，多一层保险）。
pub fn redact(input: &str) -> String {
    let stripped = strip_url_userinfo(input);
    redact_token_shapes(&stripped)
}

/// 用本次实际使用过的口令再扫一遍。
pub fn redact_secrets(input: &str, secrets: &[String]) -> String {
    let mut out = input.to_string();
    for secret in secrets {
        // 太短的串替换会误伤正常文本。
        if secret.len() < 3 || secret.eq_ignore_ascii_case("git") || !out.contains(secret.as_str()) {
            continue;
        }
        out = out.replace(secret.as_str(), "***");
    }
    out
}

/// 去掉 URL 中的 `user:password@` / `user@` 部分，保留协议与主机路径。
pub fn strip_userinfo(url: &str) -> String {
    let Some(scheme_at) = url.find("://") else {
        // scp 形式 user@host:path
        if let Some(at) = url.find('@') {
            if !url[..at].contains('/') {
                return url[at + 1..].to_string();
            }
        }
        return url.to_string();
    };
    let authority_start = scheme_at + 3;
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map(|offset| authority_start + offset)
        .unwrap_or(url.len());
    let authority = &url[authority_start..authority_end];
    match authority.rfind('@') {
        Some(at) => format!(
            "{}{}{}",
            &url[..authority_start],
            &authority[at + 1..],
            &url[authority_end..]
        ),
        None => url.to_string(),
    }
}

/// 返回远端地址里的口令（`user:password@` 中的 password）。没有则为 `None`。
pub fn url_password(url: &str) -> Option<String> {
    let scheme_at = url.find("://")?;
    let authority_start = scheme_at + 3;
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map(|offset| authority_start + offset)
        .unwrap_or(url.len());
    let authority = &url[authority_start..authority_end];
    let at = authority.rfind('@')?;
    let userinfo = &authority[..at];
    let colon = userinfo.find(':')?;
    let password = &userinfo[colon + 1..];
    (!password.is_empty()).then(|| password.to_string())
}

/// 远端地址里的主机（含端口，便于区分本机不同端口的服务）。
pub fn host_of(url: &str) -> Option<String> {
    if let Some(scheme_at) = url.find("://") {
        let authority_start = scheme_at + 3;
        let authority_end = url[authority_start..]
            .find(['/', '?', '#'])
            .map(|offset| authority_start + offset)
            .unwrap_or(url.len());
        let authority = &url[authority_start..authority_end];
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let host = host.trim_end_matches('/');
        return (!host.is_empty()).then(|| host.to_ascii_lowercase());
    }
    // scp 形式 user@host:path
    let (_, rest) = url.split_once('@')?;
    let host = rest.split([':', '/']).next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// 把文本中 URL 的 userinfo 替换成 `***`（保留主机，便于定位问题）。
fn strip_url_userinfo(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(scheme_at) = rest.find("://") {
        let scheme_start = rest[..scheme_at]
            .rfind(|character: char| !(character.is_ascii_alphanumeric() || "+-.".contains(character)))
            .map(|index| index + 1)
            .unwrap_or(0);
        out.push_str(&rest[..scheme_start]);
        let authority_start = scheme_at + 3;
        let authority_end = rest[authority_start..]
            .find(['/', ' ', '\n', '\t', '?', '#'])
            .map(|offset| authority_start + offset)
            .unwrap_or(rest.len());
        let authority = &rest[authority_start..authority_end];
        let sanitized = match authority.rfind('@') {
            Some(at) => format!("***@{}", &authority[at + 1..]),
            None => authority.to_string(),
        };
        out.push_str(&rest[scheme_start..authority_start]);
        out.push_str(&sanitized);
        rest = &rest[authority_end..];
    }
    out.push_str(rest);
    out
}

/// 抹掉常见令牌形状（GitHub / GitLab）与 `key=value` 形式的秘密值。
fn redact_token_shapes(input: &str) -> String {
    let mut out = redact_key_values(input);
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "glpat-"] {
        let mut search_from = 0usize;
        while let Some(found) = out[search_from..].find(prefix) {
            let start = search_from + found;
            let end = out[start..]
                .find(|character: char| !(character.is_ascii_alphanumeric() || "_-".contains(character)))
                .map(|offset| start + offset)
                .unwrap_or(out.len());
            if end - start >= 20 {
                out.replace_range(start..end, "***");
                search_from = start + 3;
            } else {
                search_from = end;
            }
        }
    }
    out
}

/// 抹掉 `token=...`、`password: ...` 之类赋值里的值。
fn redact_key_values(input: &str) -> String {
    const KEYS: [&str; 9] = [
        "token",
        "password",
        "passwd",
        "passphrase",
        "secret",
        "authorization",
        "bearer",
        "apikey",
        "api_key",
    ];
    let mut out = input.to_string();
    let lowered = out.to_ascii_lowercase();
    let bytes = lowered.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let mut matched: Option<(usize, &str)> = None;
        for key in KEYS {
            if lowered[index..].starts_with(key) {
                matched = Some((key.len(), key));
                break;
            }
        }
        let Some((key_len, _)) = matched else {
            index += 1;
            continue;
        };
        let mut cursor = index + key_len;
        // 允许 `api key`、`api_key` 之后跟分隔符与空白。
        while cursor < bytes.len() && (bytes[cursor] as char).is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || !matches!(bytes[cursor] as char, '=' | ':') {
            index += key_len;
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && (bytes[cursor] as char).is_ascii_whitespace() {
            cursor += 1;
        }
        let value_end = out[cursor..]
            .find(|character: char| {
                character.is_ascii_whitespace() || matches!(character, ',' | ';' | ')' | '"' | '\'')
            })
            .map(|offset| cursor + offset)
            .unwrap_or(out.len());
        if value_end > cursor {
            out.replace_range(cursor..value_end, "***");
        }
        index = cursor + 3;
    }
    out
}

/// 克隆时的连接/传输超时，避免网络不可达时长时间卡住界面。
pub const CLONE_TIMEOUT: Duration = Duration::from_secs(60);
