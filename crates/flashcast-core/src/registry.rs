//! 功能插件注册表。宿主对每个插件的搜索施加超时与 panic 隔离。

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::model::{PluginFailure, PluginFailureKind, SearchItem, SourceId};
use crate::plugin::{
    FeaturePlugin, Keyword, PluginError, PluginManifest, PluginScope, SearchContext,
};

struct RegisteredPlugin {
    plugin: Arc<dyn FeaturePlugin>,
    manifest: PluginManifest,
    enabled: bool,
}

/// 插件注册表。
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Mutex<Vec<RegisteredPlugin>>,
    /// 插件注册顺序，用于来源优先级。
    order: Mutex<HashMap<String, u32>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 某个插件搜索的产出。
pub struct PluginSearchOutcome {
    pub results: Vec<(SourceId, Vec<SearchItem>)>,
    pub failures: Vec<PluginFailure>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册插件。重复 id 会被忽略（保留先注册的那个）。
    pub fn register(&self, plugin: Arc<dyn FeaturePlugin>) {
        let manifest = plugin.manifest();
        let mut plugins = lock(&self.plugins);
        if plugins.iter().any(|p| p.manifest.id == manifest.id) {
            return;
        }
        let mut order = lock(&self.order);
        let next = order.len() as u32;
        order.entry(manifest.id.clone()).or_insert(next);
        plugins.push(RegisteredPlugin {
            plugin,
            manifest,
            enabled: true,
        });
    }

    /// 启用或停用插件。停用后不参与搜索、不产生后台活动。
    pub fn set_enabled(&self, plugin_id: &str, enabled: bool) -> bool {
        let mut plugins = lock(&self.plugins);
        match plugins.iter_mut().find(|p| p.manifest.id == plugin_id) {
            Some(entry) => {
                entry.enabled = enabled;
                true
            }
            None => false,
        }
    }

    pub fn is_enabled(&self, plugin_id: &str) -> bool {
        lock(&self.plugins)
            .iter()
            .find(|p| p.manifest.id == plugin_id)
            .map(|p| p.enabled)
            .unwrap_or(false)
    }

    /// 全部插件清单及启用状态。
    pub fn manifests(&self) -> Vec<(PluginManifest, bool)> {
        lock(&self.plugins)
            .iter()
            .map(|p| (p.manifest.clone(), p.enabled))
            .collect()
    }

    pub fn source_order(&self, plugin_id: &str) -> u32 {
        lock(&self.order)
            .get(plugin_id)
            .copied()
            .unwrap_or(u32::MAX)
    }

    /// 参与首屏搜索且已启用的插件。
    fn home_contributors(&self) -> Vec<Arc<dyn FeaturePlugin>> {
        lock(&self.plugins)
            .iter()
            .filter(|p| p.enabled && p.plugin.contributes_to_home())
            .map(|p| Arc::clone(&p.plugin))
            .collect()
    }

    /// 关键词完整匹配时进入对应插件的范围。
    pub fn take_scope(&self, input: &str) -> Option<(PluginManifest, Box<dyn PluginScope>)> {
        let normalized = input.trim().to_lowercase();
        if normalized.is_empty() {
            return None;
        }
        let candidates: Vec<(PluginManifest, Arc<dyn FeaturePlugin>)> = lock(&self.plugins)
            .iter()
            .filter(|p| p.enabled)
            .map(|p| (p.manifest.clone(), Arc::clone(&p.plugin)))
            .collect();
        for (manifest, plugin) in candidates {
            if manifest.matches_keyword(&normalized).is_none() {
                continue;
            }
            if let Some(scope) = plugin.take_scope(&Keyword::new(normalized.clone())) {
                return Some((manifest, scope));
            }
        }
        None
    }

    /// 运行首屏搜索。每个插件在独立线程中执行，带超时与 panic 隔离。
    pub fn search_home(&self, ctx: &SearchContext, timeout: Duration) -> PluginSearchOutcome {
        let contributors = self.home_contributors();
        let mut outcome = PluginSearchOutcome {
            results: Vec::new(),
            failures: Vec::new(),
        };
        for plugin in contributors {
            let manifest = plugin.manifest();
            match run_isolated(Arc::clone(&plugin), ctx.clone(), timeout) {
                Ok(items) => outcome.results.push((manifest.id, items)),
                Err(failure) => outcome.failures.push(failure),
            }
        }
        outcome
    }

    /// 运行一次插件范围搜索。与首屏搜索同样有超时、取消与 panic 隔离（ADR §6）：
    /// 插件报错、无响应或 panic 时本轮返回 [`PluginFailure`]，宿主自身结果与其他
    /// 插件不受影响，搜索线程也不会被卡住。
    pub fn search_scope(
        &self,
        plugin_id: &str,
        scope: Arc<dyn PluginScope>,
        ctx: &SearchContext,
        timeout: Duration,
    ) -> Result<Vec<SearchItem>, PluginFailure> {
        let (sender, receiver) = mpsc::channel::<PluginOutcome>();
        let scope_id = plugin_id.to_string();
        let thread_ctx = ctx.clone();
        let spawn_result = std::thread::Builder::new()
            .name(format!("flashcast-plugin-scope-{plugin_id}"))
            .spawn(move || {
                let payload = match catch_unwind(AssertUnwindSafe(|| scope.search(&thread_ctx))) {
                    Ok(Ok(items)) => PluginOutcome::Items(items),
                    Ok(Err(error)) => PluginOutcome::Failed(error),
                    Err(_) => PluginOutcome::Panicked,
                };
                // 接收端可能已因超时退出，发送失败无需处理。
                let _ = sender.send(payload);
            });
        if let Err(error) = spawn_result {
            return Err(PluginFailure {
                plugin_id: scope_id,
                reason: format!("无法启动插件搜索线程：{error}"),
                kind: PluginFailureKind::Error,
            });
        }
        collect(receiver, timeout, scope_id)
    }
}

/// 收集插件搜索线程的结果，把超时、错误与 panic 归一成 [`PluginFailure`]。
fn collect(
    receiver: mpsc::Receiver<PluginOutcome>,
    timeout: Duration,
    plugin_id: String,
) -> Result<Vec<SearchItem>, PluginFailure> {
    match receiver.recv_timeout(timeout) {
        Ok(PluginOutcome::Items(items)) => Ok(items),
        Ok(PluginOutcome::Failed(error)) => Err(PluginFailure {
            plugin_id,
            reason: error.to_string(),
            kind: PluginFailureKind::Error,
        }),
        Ok(PluginOutcome::Panicked) => Err(PluginFailure {
            plugin_id,
            reason: "插件在搜索时发生 panic，已隔离，本轮结果中不包含该插件".to_string(),
            kind: PluginFailureKind::Panic,
        }),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(PluginFailure {
            plugin_id,
            reason: format!("插件搜索超过 {} 毫秒未返回", timeout.as_millis()),
            kind: PluginFailureKind::Timeout,
        }),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(PluginFailure {
            plugin_id,
            reason: "插件搜索线程异常退出".to_string(),
            kind: PluginFailureKind::Panic,
        }),
    }
}

/// 在独立线程中运行插件搜索，隔离超时、错误与 panic。
fn run_isolated(
    plugin: Arc<dyn FeaturePlugin>,
    ctx: SearchContext,
    timeout: Duration,
) -> Result<Vec<SearchItem>, PluginFailure> {
    let plugin_id = plugin.manifest().id;
    let (sender, receiver) = mpsc::channel::<PluginOutcome>();
    let thread_plugin = Arc::clone(&plugin);
    let thread_ctx = ctx.clone();
    let spawn_result = std::thread::Builder::new()
        .name(format!("flashcast-plugin-{plugin_id}"))
        .spawn(move || {
            let payload = match catch_unwind(AssertUnwindSafe(|| thread_plugin.search(&thread_ctx)))
            {
                Ok(Ok(items)) => PluginOutcome::Items(items),
                Ok(Err(error)) => PluginOutcome::Failed(error),
                // panic 与普通错误必须区分，诊断报告才能说明真实原因。
                Err(_) => PluginOutcome::Panicked,
            };
            // 接收端可能已因超时退出，发送失败无需处理。
            let _ = sender.send(payload);
        });

    if let Err(error) = spawn_result {
        return Err(PluginFailure {
            plugin_id,
            reason: format!("无法启动插件搜索线程：{error}"),
            kind: PluginFailureKind::Error,
        });
    }

    collect(receiver, timeout, plugin_id)
}

/// 插件搜索线程的返回值。
enum PluginOutcome {
    Items(Vec<SearchItem>),
    Failed(PluginError),
    Panicked,
}
