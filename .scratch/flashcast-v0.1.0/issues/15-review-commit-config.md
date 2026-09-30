# 15: 查看配置变更并创建 Git 提交

Status: done
Category: enhancement

**What to build:** 用户修改设置或备忘录后，在应用中检查工作区的变更，明确选择提交内容并创建 Git 提交。

Blocked by: 05

- [x] 设置展示工作区变更、文件差异与提交状态；能够输入提交说明并创建真实提交。
- [x] 提交范围明确且可选择，不默认纳入无关文件或用户已有暂存修改。
- [x] 提交后状态刷新，已有配置和备忘录继续正常使用，提交失败不丢失修改。
- [x] 无变更、Git 环境不可用、用户身份未配置和工作区异常有明确反馈。
- [x] 从宿主入口在临时真实仓库验证变更、选择范围、提交内容与错误保护，不仅检查 Git 命令字符串。
- [x] 手动或浏览器交互验证差异与提交流程；适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

远端网络同步独立交付，本切片完全离线可使用。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-01：实现完成并置为 done。实现提交 `276712b`、`338503a`、`b24c153`、`4204921`、`8895c7d`、`7e6995c`，
  合并集成分支 `feat/flashcast-v0.1.0`（f7623b5）的合并提交 `22b93b4`。

### 行为与设计（实测）

提交语义对齐 `git commit -- <path>`（本机用真实 git 实测确认：提交的是**工作区**内容，
不是已暂存的中间内容；提交后选中路径记入索引，未选中的暂存改动原样保留）：

1. **差异基准透明**：`WorkspaceChanges.diffBase` 形如 `HEAD (1a2b3c4) → 工作区（含已暂存改动）`；
   空仓库为 `空仓库（尚无提交）→ 工作区`。每个文件的补丁就是 `git diff HEAD` 的内容
   （已暂存与未暂存改动都算进去），因此「看到的差异 = 勾选后提交的内容」。
2. **提交范围显式**：`Host::commit_workspace(message, paths)` 只接受显式路径，且每个路径必须出现在
   **当前**变更列表中（否则 `GitError::UnknownPath`，越界路径同样拒绝），绝不默认纳入其它改动。
3. **不吞用户已有暂存状态**：提交树由一个**内存临时索引**写出（`HEAD` 树 + 选中路径的工作区内容），
   仓库真实索引里其它路径的暂存改动不会进入提交；随后只把选中路径按 git 的语义记入真实索引。
   因此提交后不会出现「新文件被记为已暂存删除」这类幽灵状态。重命名**不**打开重命名检测：
   重命名如实拆成「删除 + 新增」两个条目，必须都勾选，提交范围永远等于勾选集合。
4. **失败不丢改动**：动索引之前按字节备份 `.git/index`，提交失败时按字节还原（实测：锁住
   `refs/heads/*.lock` 让提交必然失败，索引字节、HEAD、工作区文件三者全部与提交前一致，
   选中路径的索引条目也回滚了）；工作区文件自始至终只读。`commit_workspace` 用
   `Host::set_git_busy(true)` 包住整个操作，提交期间的索引写入被文件监听过滤，不会触发自伤重载
   （有回归断言）。
5. **明确反馈**：没有可提交的变更、未配置 `user.name` / `user.email`（仓库本地写入空值同样视为
   未配置）、分离 HEAD、`MERGE_HEAD` / `rebase-merge` 等标记与 `Repository::state()` 识别出的
   进行中操作、`index.lock` 被占用、工作区不是 Git 仓库、尚未关联工作区、提交说明为空、
   未勾选路径，都返回可读中文原因，且此时不产生提交、不改动索引与工作区文件。
6. **只读探测不加锁**：状态与差异读取使用 `StatusOptions::no_refresh(true)`，不写索引；
   未跟踪文件的差异必须显式打开 `DiffOptions::show_untracked_content(true)` 才有补丁内容
   （默认只给 delta，不给内容）——这是本 ticket 踩到的坑。单文件差异超过 20000 字符时截断并标记。

### 文件地图

- `crates/flashcast-core/src/git.rs`：`ChangedFile` / `WorkspaceChanges` / `CommitOutcome` / `GitError`、
  `changes()`（状态分类、`git status --short` 风格代码、逐文件真实补丁、分支 / 分离 HEAD / 进行中操作 /
  截断）、`commit()`（临时索引写树、索引备份与还原、身份解析、提交后核对树内容与工作区逐字节一致）。
- `crates/flashcast-core/src/host.rs`：入口 `Host::workspace_changes()` 与
  `Host::commit_workspace(message, paths)`；后者用 `set_git_busy` 包住整个 Git 操作。
- `crates/flashcast-core/src/lib.rs`：导出新类型。
- `crates/flashcast-core/tests/workspace_git.rs`（17 项）：真实临时仓库上的集成测试。
- `crates/flashcast-core/tests/support/mod.rs`：真实仓库夹具（初始提交、暂存、索引字节、gitdir 标记、
  提交签名等）。
- `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`：`get_git_changes` / `commit_changes` 两个
  薄转发命令（业务判断全在宿主），已注册进 `invoke_handler`。
- `src/components/ChangesPanel.tsx`、`src/components/SettingsScreen.tsx`、`src/App.tsx`、
  `src/api.ts`、`src/types.ts`、`src/styles.css`：设置页「变更与提交」区段、浏览器模拟宿主的
  变更 / 提交语义与错误模拟、提交范围与差异基准的透明展示。
- `tools/ui-check/check.mjs`：新增 5 项变更与提交流程检查（并把 640×420 真实窗口检查扩展到新区段）。

### 实测命令与结果

```bash
eval "$(scripts/dev/linux-native-deps.sh)"
cargo test --workspace
# 合并 feat/flashcast-v0.1.0 之后：passed=156 failed=0（其中本 ticket 新增 17 项；
# 合并前 112 项，基线 95 项）
cargo build -p flashcast
# Finished dev profile；2 条告警为 ticket 01 遗留的 dead_code（commands::host_of、hotkey::status）
pnpm build
# tsc --noEmit 与 vite build 均通过
pnpm ui-check
# 汇总：通过 17，失败 0（共 17 项）；本 ticket 新增 5 项
CARGO_TARGET_DIR=/tmp/wcheck-15 cargo check -p flashcast-platform --target x86_64-pc-windows-msvc
CARGO_TARGET_DIR=/tmp/mcheck-15 cargo check -p flashcast-platform --target x86_64-apple-darwin
# 合并前后各跑一次，均 Finished（Windows 目标另外编译了 ticket 02 新增的 windows 适配层）
```

浏览器检查新增项与截图（`artifacts/ui/`，已被 gitignore）：

| 检查 | 截图 |
| --- | --- |
| 变更区展示分支、基准、状态与真实差异 | `14-settings-changes.png`、`15-settings-diff.png` |
| 只提交勾选的路径，未勾选的暂存改动保留 | `16-settings-commit-partial.png` |
| 提交失败时给出明确的中文原因且改动保留 | `17-settings-commit-error.png` |
| 不是 Git 仓库与无变更都有明确说明 | `18-settings-git-unavailable.png`、`19-settings-changes-empty.png` |

集成测试覆盖（`crates/flashcast-core/tests/workspace_git.rs`，全部经 `Host` 入口、在真实临时仓库上断言
真实仓库状态）：状态分类（未暂存 / 已暂存+未暂存 / 未跟踪）与差异基准、逐文件真实补丁内容、
空仓库（无提交）与普通目录、差异截断、只提交勾选路径并保留用户已有暂存条目（含提交树的精确路径集合、
父提交、作者身份、`find_commit` 复核）、空选择 / 未变更路径 / 越界路径被拒绝且索引字节不变、
空仓库的第一个提交、提交失败后索引按字节还原、干净工作区、空提交说明、身份未配置、分离 HEAD、
合并与变基进行中、`index.lock` 被占用、未关联工作区与不是 Git 仓库、提交后设置仍能写 / 备忘录仍在 /
重启后恢复并再次提交、重命名拆成删除+新增且只勾选一个不会静默吞掉另一半、提交不制造自伤重载。

### 未覆盖 / 限制

- **`flashcast-core` 的 Windows / macOS 目标编译未在本机实测**。本机没有对应 C 工具链：
  `CARGO_TARGET_DIR=/tmp/wcore-15 cargo check -p flashcast-core --target x86_64-pc-windows-msvc`
  在 `libz-sys` 的 C 编译阶段就失败（`cc` 无法为目标产出对象文件），**尚未**走到本 ticket 新增的
  Rust 代码。这与 ticket 05 记录的同一限制一致；本 ticket 未新增任何依赖（`Cargo.toml` 未改动），
  因此 CI 上能否编译无法由本机推断。任务要求的两条 `flashcast-platform` 交叉检查都已通过。
- **远端克隆 / 拉取 / 推送未接入**：本切片完全离线，`git2` 仍是 `default-features = false`
  （无 `https` / `ssh`），`Cargo.toml` 中保留了 ticket 14/16 打开这些 feature 的注释。
- **提交失败路径只构造了两类**：引用锁（`refs/heads/*.lock`）与 `index.lock`。磁盘写满、
  对象库损坏等真实 I/O 故障没有构造；这类失败同样走「按字节还原索引」的路径。
- **分离 HEAD 被明确拒绝而不是允许提交**：`git commit` 本身允许在分离 HEAD 上提交，本 ticket 选择
  拒绝并说明原因（配置工作区应当落在分支上）；ticket 16 若要支持需要显式决定。
- **真实桌面交互未覆盖**：浏览器检查只驱动 React UI + 浏览器模拟宿主，不代表 Tauri webview、
  托盘或全局快捷键；真实仓库上的提交流程由 Rust 集成测试覆盖。
- **大差异截断阈值（20000 字符）未做真实性能测量**：只是为 UI 设的上界。
- **clippy 未运行**：该 toolchain 未安装 `cargo-clippy`。

### 后继 ticket（16）可用的接缝

- 远端关系（`origin`、上游分支、远端 tracking ref）可用 `Repository::find_remote` /
  `branch.upstream()` 在 `crates/flashcast-core/src/git.rs` 里扩展；本 ticket 的 `WorkspaceChanges`
  （分支、分离 HEAD、`in_progress_state`）与 `GitError` 可直接复用，推送前的脏工作区判断建议复用
  `in_progress_state` 与「索引字节备份 + 还原」这套安全模式。
- 推送 / 拉取前后不要吞掉用户已有暂存状态：提交侧的临时索引方案（`build_scoped_tree`）、
  `Host::set_git_busy` 包住整个操作、操作结束后按 Git 状态显式重建视图，是 ticket 16 应当照抄的约定。
- `WorkspaceChanges` / `CommitOutcome` 已导出并被 Tauri 命令与 UI 消费；拉取 / 推送结果建议返回
  同类可序列化的结构，UI 侧在 `ChangesPanel` 同一区段扩展。
- 提交身份解析（`user.name` / `user.email`，空值视为未配置）与「不静默回退到硬编码身份」的策略
  已实现；若 ticket 16 需要凭证，注意不得把凭证写进工作区或日志（ADR §9）。
