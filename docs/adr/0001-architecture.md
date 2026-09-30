# ADR 0001：v0.1.0 代码级架构基线

Status: accepted
Date: 2026-10-01

## Context

Spec `flashcast-v0.1.0` 已确定产品与技术基线（Tauri 2 + Rust + React/TypeScript），并确定了主测试边界：**宿主对外的查询与命令入口**。本文记录该边界落地到仓库目录、crate 划分与接口契约的具体决策，供 18 张实现 ticket 共同遵循，避免每个切片各自发明结构。

集成测试必须能在没有桌面会话的 CI runner 上运行，因此桌面相关能力必须与可测的宿主逻辑分离。

## Decision

### 1. 仓库布局

```
Cargo.toml                     # Rust workspace
crates/flashcast-core/         # 宿主逻辑（无 Tauri 依赖，可在 CI 无头运行）
crates/flashcast-platform/     # 平台适配层：trait + 各平台实现 + 测试替身
src-tauri/                     # Tauri 宿主二进制：窗口、托盘、快捷键、命令转发
src/                           # React + TypeScript UI（Vite）
.github/workflows/             # 三平台 CI
```

Rust workspace members 为 `crates/*` 与 `src-tauri`。语言版本统一使用 edition 2021。

### 2. 宿主与外壳的职责边界

- `flashcast-core` 提供**无头宿主** `Host`：查询范围、结果排序与选择状态、预览、操作执行、功能插件注册表、主题注册表、设置、配置工作区、备忘录、剪贴板历史索引、书签索引。
- `src-tauri` 只负责：窗口与唤起、全局快捷键、托盘、唤起前应用身份的采集时机、Tauri command 转发、事件推送给 UI。它不含业务判断，不得出现只在 `src-tauri` 内的业务分支。
- UI 只通过 Tauri command 与宿主的**查询入口**和**命令入口**交互。

### 3. 宿主入口契约（唯一测试边界）

```rust
// 查询入口
pub fn query(&self, input: &str) -> QueryResponse;
pub struct QueryResponse {
    pub seq: u64,                          // 单调递增；UI 丢弃 seq 小于已应用值的响应
    pub scope: QueryScope,                 // 首屏 / 某插件范围
    pub items: Vec<SearchItem>,
    pub selection: usize,                  // 宿主维护的键盘选择
    pub notice: Option<Notice>,            // 错误或提示
}

// 命令入口
pub fn execute(&self, item: &SearchItem) -> ActionOutcome;
pub struct ActionOutcome {
    pub status: ActionStatus,              // Done / CopiedNeedsManualPaste / Failed
    pub message: Option<String>,           // 面向用户的中文反馈
}
```

补充入口：`select` / `move_selection` / `set_selection`（键盘）、`back()`（返回上一查询范围并恢复查询与选择）、`preview(item)`、设置读写、插件启停、工作区操作、同步操作。

规则：

- `query` 与 `execute` 之外的方法都不得绕过这两个入口表达业务结果。
- 选择状态由宿主持有。鼠标移动**不**调用 `set_selection`，因此不会抢走键盘选择。
- 记录入查询历史的 `input`、`scope` 与 `selection`，`back()` 恢复它们。
- 组合输入（IME）期间 UI 不触发 `execute`。

### 4. 结果模型与排序

```rust
pub struct SearchItem {
    pub id: String,               // 稳定标识，跨查询与重启一致
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<IconRef>,
    pub source: SourceId,         // 宿主自身 / 插件 id
    pub kind: ItemKind,           // Application / Memo / ClipboardEntry / Bookmark / Command
    pub default_action: DefaultAction,  // Open / OpenInChrome / Paste
    pub preview: Preview,
    pub score: Score,             // 贡献者计算，宿主排序使用
}
```

稳定排序键：(匹配层级降序, 贡献者内分数降序, 来源优先级, 来源内原始顺序, id)。匹配层级由宿主定义，至少区分：关键词完整匹配 = 标签精确匹配 > 标题前缀匹配 > 标题子串匹配 > 正文/路径/元数据匹配。标签精确匹配不因同名插件关键词而静默消失：关键词命中给出插件入口条目，标签命中给出带来源的备忘录条目。

首屏范围只检索软件与备忘录标签，以及有限数量的快速访问项；不检索全部剪贴板历史或书签。

### 5. 平台适配层

trait 定义在 `flashcast-platform`，按能力拆分，宿主按能力注入：

| trait | 职责 |
| --- | --- |
| `AppCatalog` | 发现已安装软件，返回标识、名称、图标、稳定 id |
| `AppLauncher` | 启动目标软件并报告失败 |
| `FocusTracker` | 读取唤起前应用身份、恢复其焦点 |
| `ClipboardAccess` | 读取/写入文本、HTML/RTF、图片、文件列表 |
| `ClipboardWatcher` | 剪贴板变化监听与自身写入抑制 |
| `Paster` | 在恢复的目标应用上发起系统粘贴 |
| `HotkeyManager` | 注册/注销/更新全局快捷键 |
| `ChromeProvider` | 发现 Chrome 与 profile、定位书签文件、按 profile 打开链接 |
| `CapabilityProbe` | 报告系统、架构、Linux 会话类型、权限与支持状态 |

实现按 `#[cfg(target_os)]` 分模块，测试替身实现在 `flashcast-platform::fake`，由 feature `fake` 提供（`flashcast-core` 的 dev-dependencies 启用它）。替身只能出现在这一层。

### 6. 功能插件契约

```rust
pub trait FeaturePlugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;                 // id / 种类 / 版本 / 关键词别名 / 所需能力
    fn contributes_to_home(&self) -> bool;                // 是否参与首屏搜索
    fn search(&self, ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError>;
    fn take_scope(&self, keyword: &Keyword) -> Option<Box<dyn PluginScope>>;
}
```

- 宿主对每个插件的 `search` 施加超时与 panic 隔离。插件超时、报错或 panic 时本轮返回空结果并记录插件错误，**不**影响宿主自身结果与其他插件。
- 停用插件后不参与搜索、不产生后台活动。
- 原生能力只能经宿主授权的接口调用，权限校验在原生边界。

### 7. 主题

主题为声明式数据（JSON），不支持可执行代码。宿主把主题解析为一组语义 token（颜色、字体、间距、圆角、阴影、状态），UI 以 CSS 自定义属性消费。默认提供浅色、深色、跟随系统。无效主题保留上次可用外观并给出原因。

### 8. 持久化

- 本机数据（剪贴板历史、附件、索引、缓存）使用 SQLite，`rusqlite` 配 `bundled` feature，避免各平台系统 SQLite 版本差异。索引可重建，不作为配置的唯一来源。
- 配置工作区使用人类可读文件：设置用 TOML，插件清单与主题配置用 JSON，备忘录用带 front matter 的 Markdown。
- 工作区中的文件写入必须原子（临时文件 + rename），并让文件监听忽略应用自身写入。

### 9. Git

Git 操作在 `flashcast-core` 内实现，使用一个 Rust Git 库（见实现 ticket 记录的选择）。凭证复用用户已有配置（ssh-agent、`~/.ssh`、credential helper），不写入工作区或日志。网络操作只在鉴权失败与网络失败边界可控。

### 10. 测试策略

- 非 UI 集成测试位于 `crates/flashcast-core/tests/`，全部经由 `Host::query` / `Host::execute` 及其余宿主入口，穿透真实 SQLite 文件、真实临时目录与真实临时 Git 仓库。
- 平台适配层用 `flashcast-platform::fake` 的替身；替身通过的检查不证明平台适配通过。
- 真实平台检查（软件发现、启动、剪贴板读写、快捷键、粘贴）在对应平台 runner 上以独立二进制或忽略环境缺失的测试执行，输出「通过 / 失败 / 未覆盖」及原因。
- **UI 不添加单元测试**：通过手动或浏览器交互检查。允许为 UI 逻辑保留纯函数并在浏览器中检查，不为 UI 写组件测试。
- 不引入与首版无关的测试框架或快照体系。

### 11. CI

GitHub Actions 在每次提交运行三平台矩阵：Linux x64、Windows x64、macOS Apple Silicon、macOS Intel。每个矩阵项：

1. 安装平台依赖；
2. `cargo test`（无头可跑的测试必须全部通过）；
3. `cargo build -p flashcast`（Tauri 宿主编译）；
4. 执行该平台可运行的核心检查，输出结构化诊断（系统版本、架构、Linux 会话类型、检查项与结果）；
5. 上传诊断日志与可获得的截图作为 artifact。

编译成功单独记录，不得推断桌面交互能力通过。

### 12. 版本与发布

版本号单一来源为 `Cargo.toml` workspace 的 `version`，由 `src-tauri/tauri.conf.json` 与 `package.json` 引用一致的值。v0.1.0 的 tag、构建提交与资产命名必须一致，发布流程等待全部必需矩阵项成功。

## Consequences

- 宿主逻辑可在无桌面会话的 CI 上完整测试，桌面相关风险集中在平台适配层与 Tauri 外壳。
- 所有 ticket 的集成测试成本低，但必须在 `flashcast-core` 中暴露真实入口，不允许「只测辅助函数」。
- 平台适配层的 trait 数量较多，新增能力需要同时更新 trait、各平台实现与替身。
- UI 无法通过自动化测试保证，依赖浏览器交互与真实桌面检查；覆盖率差异必须在平台能力报告中如实呈现。
