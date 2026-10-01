# 07: 通过备忘录插件管理带标签的常用内容

Status: done
Category: enhancement

**What to build:** 用户输入备忘录进入官方插件，创建、编辑、删除和搜索带多个标签的文字内容，预览或复制它；内容保存为工作区中的 Markdown。

Blocked by: 06

- [x] 备忘录通过统一插件清单与功能契约加载，声明关键词、版本、所需能力、结果和默认操作。
- [x] 支持标题、稳定标识、多个标签和文字正文；创建、编辑、删除后重启保留，外部有效修改可重载。
- [x] 关键词进入插件范围，搜索与完整预览可用；返回首屏恢复此前查询、选择和位置。
- [x] 支持复制内容，状态与反馈准确；自动粘贴留给下一切片。
- [x] 功能插件共用宿主搜索、列表、预览与操作栏；设置中可启用和停用，停用后不贡献搜索或后台任务。
- [x] 原生能力在宿主边界校验，可执行任务具有隔离、取消和超时；插件报错或无响应不阻断宿主与软件搜索。
- [x] 非 UI 验证穿透宿主入口检查工作区内容、插件停用、权限校验和失败隔离；界面用手动或浏览器交互验证。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

建立功能插件契约的同时交付可实际使用的备忘录管理；不引入插件市场或任意第三方代码运行承诺。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（实现完成）：备忘录插件落地。数据层、平台剪贴板与清单权威由上一提交
  `1f62e9e` 落盘；本次接着完成合并集成分支、集成测试、Tauri 外壳、UI 与验证。设计要点：

  - **数据格式是工作区里的可读 Markdown**：每条备忘录是 `memos/<id>.md`，front matter 记录
    `id` / `title` / `tags`，正文就是内容本身。没有 front matter 的普通 Markdown 也接受
    （标题取第一行 `#` 标题，或文件名），第一次经应用保存时补上 front matter。标识是稳定
    的（`memo-<nanos>-<counter>`，只允许字母数字与 `. _ -`，不能以点开头），因此
    `memos/<id>.md` 的文件名安全且可被 Git 追踪。
  - **宿主是唯一写入者**：`MemoBook` 只提供只读快照，插件拿不到工作区路径，也拿不到剪贴板
    句柄；创建 / 编辑 / 删除都经 `Host` 的入口，成功写入工作区后才更新生效内容。写入复用
    工作区既有的原子写入与自写抑制，因此不会与文件监听形成写入循环。
  - **清单是启停的唯一权威**：`manifest.json` 记录功能插件的标识、种类、版本、关键词与所需
    能力；`settings.toml` 的历史字段 `disabledPlugins` 只在清单还没有该条目时做一次性迁移
    （`seed_legacy_disabled`），不再覆盖清单里的选择。
  - **默认操作是粘贴，本切片先复制**：结果声明 `DefaultAction::Paste`（操作栏显示「粘贴」），
    执行时经原生边界校验后写入剪贴板，成功状态是 `CopiedNeedsManualPaste` 并给出
    「已复制…自动粘贴由后续版本提供，请手动粘贴」的中文反馈；自动粘贴（含唤起前应用恢复）
    留给 ticket 08。
  - **原生能力在宿主边界校验**：来源插件必须在清单里、已启用，并且声明了 `clipboard.write`；
    系统剪贴板不可用（能力探测为 Unsupported）时直接给出原因，不尝试写入。
  - **失败隔离**：首屏搜索与**范围搜索**都走「独立线程 + 超时 + panic 捕获」；插件的错误、
    超时与 panic 只记录为 `plugin_failures`，宿主结果与其它插件不受影响。

- 2026-10-01：本次一并修掉的真实缺陷（都不是测试写错，均由新测试或交互检查暴露）：

  1. **自锁死（上一提交带进来）**：`Host::unavailable_plugins()` 在 `lock(&self.inner)` 的临时
     守卫仍存活时调用 `self.settings()`（它也要锁 `inner`），`std::sync::Mutex` 不可重入，
     同一线程永久阻塞在 futex。该函数只在**克隆成功**后调用，因此 5 个成功克隆的用例全部挂死、
     4 个失败路径用例正常。改为先在锁外取出历史字段；并用脚本扫过 `host.rs` 全部 57 处
     `lock(&self.inner)` 语句与具名守卫块，确认没有第二处同类问题。
  2. **front matter 往返不稳定**：解析时只剥掉一个换行，把「空行分隔」留进了正文，于是每次
     「读—写」都给正文加一个前导换行；重启后正文与创建时不一致，且会随每次重载不断增长。
     改为连空行分隔一起剥掉。
  3. **同插件换别名不更新范围**：已在某插件范围里改用另一个关键词别名时不更新记下的关键词，
     导致范围标签显示旧别名、且「剥掉关键词前缀」失效（输入 `memo` 后搜不到任何东西）。
  4. **停用没有真正停止范围搜索**：`PluginRegistry::search_scope` 不检查插件是否仍然启用，
     宿主缓存的范围对象在插件停用后仍会产出结果。现在停用即返回空结果；同时
     `Host::set_plugin_enabled` 会丢弃该插件的范围对象、必要时退回首屏，并按当前输入重算结果。
  5. **显式重载看不到备忘录变化**：`reload_workspace` 在设置文件内容未变时提前返回，因此
     「外部只改了 `memos/*.md`」在「重新检测」入口不可见；现在补一次备忘录读取并如实报告
     `applied`。
  6. **返回后输入框不回显恢复的查询**：`back()` 之后 UI 只更新结果、不更新输入框，于是
     「从插件范围退回首屏」时输入框仍显示旧关键词、与列表不一致（此前因浏览器模拟宿主从不
     记录历史而不可达，本次随备忘录范围一起暴露）。现在会同步输入框并把焦点放回搜索框。

- 2026-10-01：实测命令与结果（分支 `ticket/07-memo-management`，已并入 `feat/flashcast-v0.1.0`
  `f4f5de2`）：

  - `timeout 1500 cargo test --workspace` → **EXIT=0**，23 个测试二进制合计 **218 passed / 0 failed**，
    其中本 ticket 新增 `crates/flashcast-core/tests/memos.rs` **18 个用例**（并入集成分支之后、
    加入本 ticket 的集成测试之前为 200 个）。
  - CI 对齐（**可靠形式**）：`mkdir -p "$HOME/.cache/flashcast/fakehome" &&
    HOME="$HOME/.cache/flashcast/fakehome" GIT_CONFIG_NOSYSTEM=1 timeout 1500 cargo test --workspace`
    → **EXIT=0**，218 passed / 0 failed。注意：不需要也不能在该命令里同时改写 `CARGO_HOME` /
    `RUSTUP_HOME`——它们在开发脚本里已被导出会被继承；若在同一条赋值语句里用 `$HOME` 拼这两个
    路径，bash 会用到**已被改写的** HOME，rustup 会报「no default toolchain」。第一次跑正是在
    这里踩了坑，按开发文档的原样命令重跑即通过。
  - `timeout 900 cargo build -p flashcast` → EXIT=0（仅剩并入集成分支带来的
    `hotkey::status` 未使用告警，不在本 ticket 的文件里）。
  - `timeout 600 pnpm install --frozen-lockfile` → 0；`timeout 600 pnpm build`
    （`tsc --noEmit && vite build`）→ 0，无类型错误。
  - `timeout 900 pnpm ui-check` → **通过 39，失败 0**（合并前 34 项，本 ticket 新增 5 项）；
    日志 `artifacts/ui/ui-check.log`。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/07-windows timeout 1800 cargo check
    -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets` → EXIT=0（Finished）。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/07-macos timeout 1800 cargo check
    -p flashcast-platform --target x86_64-apple-darwin --all-targets` → EXIT=0（Finished）。
  - `cargo fmt -p flashcast -- --check` 与 `cargo fmt -p flashcast-core -- --check`：本 ticket
    触碰的文件全部干净（仓库里 `device.rs` / `ranking.rs` / `theme.rs` / `state.rs` 等处有
    既存的格式漂移，不属于本 ticket，未改动）。

- 2026-10-01：本 ticket 的集成测试覆盖（`crates/flashcast-core/tests/memos.rs`，全部经宿主入口，
  工作区是真实临时目录、备忘录是磁盘上真实的 Markdown）：清单加载与关键词/能力契约、每个别名
  进入范围、首屏精确标签命中（非精确标签不命中）、范围内按标题/标签/正文检索、创建—重启保留—
  编辑（标识不变）—删除、外部有效修改经「重新检测」与文件监听两条路径重载、无效外部文件保留
  可用内容并报告原因、停用后不贡献结果且拒绝写入（重启仍按清单停用）、插件报错/范围超时/范围
  panic 的隔离、`back()` 恢复查询与选择、复制的状态与反馈（含剪贴板失败与系统不支持）、
  无 `clipboard.write` 能力的插件被拒绝、伪造来源被拒绝、未关联工作区拒绝创建。

- 2026-10-01：界面验证（浏览器交互检查，`tools/ui-check/check.mjs` 新增 5 项）与截图
  （均在 `artifacts/ui/`，已被 gitignore）：

  - `46-memo-scope.png`：三个别名（备忘录 / memo / memos）都进入范围、列表为备忘录条目、
    操作栏显示默认操作「粘贴」、预览区给出**完整正文**、范围内按标题继续检索、预览可收起/展开。
  - `47-memo-home-tag.png`：首屏输入完整标签「工作」命中两条备忘录，并留在首屏。
  - `48-memo-copied.png`：回车后反馈「已复制「常用回复」到剪贴板；自动粘贴由后续版本提供，
    请手动粘贴」，模拟宿主记录的剪贴板内容就是正文。
  - `49-settings-memo-created.png` / `50-settings-memo-edited.png`：设置页创建（多个标签、
    正文、生成的稳定标识）与编辑（标识不变、正文更新）。
  - `51-settings-plugin-disabled.png` / `52-memo-disabled-search.png`：停用插件后管理入口如实
    说明并禁用写入、首屏标签与关键词都不再命中；重新启用后恢复。

- 2026-10-01：未覆盖项与原因（严格如实）：

  - **真实桌面的复制与自动粘贴**：浏览器交互检查只覆盖 React UI 与浏览器模拟宿主，因此
    「真的写进系统剪贴板」只由平台层的真实实现承担，本次**没有**在真实 Wayland/X11 会话里
    执行端到端的复制检查（`cargo run -p flashcast-platform --bin flashcast-platform-check`
    可用；本 ticket 未把它纳入证据）。自动粘贴属 ticket 08，本切片不承诺。
  - **Windows / macOS 的剪贴板实现**：两个目标只做了 `cargo check -p flashcast-platform`
    （CI 覆盖类型正确性），没有在 Windows（Win32 剪贴板 API）或 macOS（`pbcopy`）上实测写入。
  - **备忘录的桌面端到端流程**：UI 检查走的是浏览器模拟宿主；真实 Tauri webview 下的
    创建 / 编辑 / 删除只由宿主的集成测试覆盖，没有在桌面会话里点一遍。
  - **并发写入与超大内容**：多个应用实例同时改同一条备忘录、超大正文、以及备忘录与其它工作区
    文件同时变化时的行为未单独验证（写入是原子写 + 自写抑制，单实例由单实例守卫保证）。
  - **标签匹配的规范化**：首屏标签命中按小写化后的**完整相等**比较，不做 Unicode 规范化、
    不做同义或前缀匹配（按 spec「标签精确匹配优先于较弱的内容匹配」）。
  - **主题/工作区切换时的预览**：切换工作区后预览按下一次选中项刷新，未专门验证「切换瞬间」
    的旧预览窗口期。
