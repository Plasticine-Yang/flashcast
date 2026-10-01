# 08: 在首屏按标签找到备忘录并粘贴到原应用

Status: done
Category: enhancement

**What to build:** 用户在其他应用中唤起 Flashcast，输入备忘录标签、选择内容并按回车，将内容粘贴回原应用。

Blocked by: 02, 03, 07

- [x] 备忘录标签参与首屏搜索，精确标签匹配优先，结果显示来源；多个条目共享标签时可选择。
- [x] 关键词与标签冲突时保留插件入口及备忘录候选，不静默丢失任何一方。
- [x] 操作栏显示粘贴，完整内容可预览；回车准备剪贴板、关闭浮窗、恢复原应用并执行粘贴。
- [x] Linux、Windows、macOS 平台采用各自适配；原应用失效、权限缺失或环境不支持时复制内容并提示手动粘贴，不向错误应用注入。
- [x] 中文组合输入不误粘贴；快速关闭、切换结果和执行时不会粘贴过期选择。
- [ ] 经宿主入口验证结果与操作；真实粘贴检查验证目标应用内容，替身、CI 未覆盖与本地实测分别记录。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

02 与 03 提供对应平台的应用身份、窗口唤起与返回目标能力；本切片完成共享的粘贴流程，后续剪贴板插件复用。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（实现完成）：自动粘贴落地。分支 `ticket/08-memo-tag-paste`（基于
  `feat/flashcast-v0.1.0` `0dd7c3d`）。**第 6 个复选框只勾了一半**：宿主入口的验证做完了，
  但「真实粘贴检查验证目标应用内容」在本机（Wayland 会话）做不到，见下面的未覆盖清单；
  按「只勾验证过的」原则，该框保持未勾。

  ### 实现要点（接缝）

  - **`Paster` 是新增的平台能力**（`crates/flashcast-platform/src/paste.rs`），只回答
    「能不能注入」与「失败了为什么」：恢复焦点是 `FocusTracker` 的职责，「恢复后的前台
    是不是那个应用」的核对是宿主的职责。三个平台各自适配，OS 调用没有进 `flashcast-core`：
    Linux X11 走 XTEST（`enigo` 的 `x11rb` 后端，**不启用 `wayland` feature**：那是 wlroots
    专有协议，GNOME 未实现）；Windows 走 `SendInput` + `dwExtraInfo` 打标；macOS 走 `CGEvent`
    + `EVENT_SOURCE_USER_DATA` 打标，并在注入前自己检查 `AXIsProcessTrusted()`。
  - **顺序固定在宿主 + 外壳之间**（spec「粘贴是宿主级操作」）：`Host::execute` 写剪贴板并
    返回 `PastePending` + `PastePlan`（含唤起前捕获的目标应用与单调递增的 `epoch`）→ 外壳
    `commands::execute` 关闭浮窗 → 等 `PASTE_SETTLE`(150ms) → `Host::complete_paste()`
    恢复焦点、**回读前台核对**、注入粘贴。关窗必须先于恢复，否则注入会落到 Flashcast 自己
    身上；这一步只能在关窗之后做，所以降级提示由外壳重新显示窗口来保证用户看得见。
  - **绝不向错误的窗口注入**：`same_app()`（窗口句柄 > 进程号 > 稳定标识，两边都拿不准就
    返回 false）在注入前核对「恢复后的前台 == 唤起时捕获的应用」，不相等就退回手动粘贴。
  - **过期选择有三层防线**：每次 `execute` 覆盖唯一一份待完成计划并递增 `epoch`；
    `set_paste_target`（每次唤起调用）作废旧计划；用户主动关窗（Escape / 托盘 / 关闭按钮）
    调用 `cancel_paste()`。内容永远按**工作区当前内容**重新读取，不用列表快照。
    外壳复刻了「计划 → 关窗 → 完成」的顺序，因此 `std::sync::Mutex` 的自锁风险被避开：
    粘贴状态用**独立**的 `Mutex<PasteState>`，锁内不调用任何 `self.<method>()`。
  - **能力探测如实**：Linux X11 真的去问服务器是否有 XTEST 扩展（没有就报不支持）；Wayland
    直接报不支持（原因写清 GNOME 没有公开接口、只有 XDG RemoteDesktop 门户授权后才可行）；
    macOS 只有拿到辅助功能权限才报支持；Windows 报支持并把 UIPI 限制写进 notes。
    决策只看 `Capabilities` + 唤起时捕获到的目标：`Supported` 才尝试；`Unknown` 视为
    「无法确认」→ 手动粘贴（未覆盖不等于可用）。

  ### 关键词与标签冲突：修掉一个与 ADR 相反的真实缺陷

  ticket 07 的注释声称「宿主在首屏同时给出软件 / 插件入口」，但代码并没有：备忘录插件在
  查询**完整等于自己的关键词**时提前返回空结果（`plugins/memo.rs` 的旧逻辑），宿主又立刻
  切进插件范围。于是「有一条备忘录的标签正好叫 `memo`」时，标签命中在首屏被关键词**静默
  吞掉**，与 ADR §4「标签精确匹配不因同名插件关键词而静默消失」直接冲突。现在：

  - 插件**始终**按标签贡献候选（删掉了那段提前返回），判定权归宿主；
  - `Host::search` 先看首屏有没有「标签精确命中」的备忘录：有冲突就留在首屏，并补一个
    **插件入口条目**（`flashcast.plugin.<id>`，`ItemKind::Command`，相关度最高、排在最前，
    因此「输入关键词后直接回车」仍然进入插件范围，往下选才是粘贴那条备忘录）；
  - 执行入口条目走 `SearchMode::ExplicitPluginEntry`，跳过冲突保留真正进入范围，且**已经在
    范围内时不再算冲突**（否则 UI 执行后按当前输入重新查询会被弹回首屏）；
  - UI 侧：执行 `command` 类条目后按当前输入重新查询（列表必须跟宿主状态一致，顺带修掉
    「重新扫描后列表不刷新」）。

  ### 本次修掉的其它真实缺陷

  1. **剪贴板写入会无限阻塞**（ticket 07 的实现，由本 ticket 的真实检查暴露）：
     `clipboard::write_with_tool` 用 `wait_with_output()` 无上限等待。在拿不到选区的会话里
     `wl-copy` **永久不返回**（本机实测挂满 5 分钟），宿主的执行入口会一起挂死。
     现在改为有界等待（5 秒）+ 标准错误由独立线程读取（工具 fork 出的守护进程会继承该管道，
     在主线程里读到 EOF 会再次挂住），超时杀掉进程并如实返回「剪贴板工具没有返回」。
     新增单元测试用真实的 `sh -c 'sleep 30'` 验证有界返回（不依赖任何替身）。
  2. **UI 的降级提示在关窗之后会丢失**：`complete_paste` 降级时窗口已经隐藏，用户既看不到
     「已复制」也看不到「请手动粘贴」。外壳在降级时重新显示窗口（`summon::reshow`，**不**
     重新采集唤起前应用，避免把当前前台记成新目标）。
  3. 平台检查二进制过去会跟着剪贴板工具一起挂死；现在所有外部命令调用都有超时与「不读管道
     EOF」的保护。

  ### 实测命令与结果（分支最终提交见下）

  - `timeout 1500 cargo test --workspace` → **EXIT=0**，24 个测试二进制合计 **241 passed /
    0 failed**；其中本 ticket 新增 `crates/flashcast-core/tests/paste.rs` **19 个用例**、
    `crates/flashcast-platform/src/focus.rs` 的 `same_app` 单元用例 **3 个**、
    `clipboard.rs` 的「工具阻塞必须有界返回」**1 个**（ticket 07 结束时为 218）。
  - CI 对齐（**可靠形式**）：`mkdir -p "$HOME/.cache/flashcast/fakehome" &&
    HOME="$HOME/.cache/flashcast/fakehome" GIT_CONFIG_NOSYSTEM=1 timeout 1500 cargo test
    --workspace` → **EXIT=0**，241 passed / 0 failed。
  - `timeout 900 cargo build -p flashcast` → **EXIT=0**（只剩 ticket 07 已记录的
    `hotkey::status` 未使用告警，不在本 ticket 的文件里）。
  - `timeout 600 pnpm install --frozen-lockfile` → 0；`timeout 600 pnpm build`
    （`tsc --noEmit && vite build`）→ 0，无类型错误。
  - `timeout 900 pnpm ui-check` → **通过 42，失败 0**（合并前 39 项，本 ticket 新增 3 项）；
    日志 `artifacts/ui/ui-check.log`。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/08-windows timeout 1800 cargo check
    -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets` → **EXIT=0**
    （Finished）。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/08-macos timeout 1800 cargo check
    -p flashcast-platform --target x86_64-apple-darwin --all-targets` → **EXIT=0**（Finished）。
  - `cargo fmt --check`（只对本 ticket 触碰的文件）→ 干净；仓库里 `device.rs` /
    `ranking.rs` / `theme.rs` 等既有格式漂移不属于本 ticket，未改动（rustfmt 会顺着
    `mod` 级联，级联出来的无关改动已全部回退）。

  ### 宿主入口用例（`crates/flashcast-core/tests/paste.rs`，19 个）

  全部经 `set_paste_target` → `query` → `execute` → `complete_paste`，工作区是真实临时目录、
  备忘录是磁盘上真实的 Markdown；焦点 / 剪贴板 / 粘贴是替身：

  - 支持自动粘贴：`execute` 只写剪贴板并给出计划（此时 `paste_count == 0`），
    `complete_paste` 才恢复目标并注入一次，且**注入时剪贴板内容 == 刚执行的那条正文**；
  - 重复完成不会二次注入；新唤起作废旧计划（且不去恢复旧目标）；`cancel_paste` 之后迟到的
    完成请求不会注入；
  - 降级为手动粘贴的每种条件：Wayland 会话、没有捕获到目标、能力是「无法确认」、
    目标应用已退出（恢复失败）、恢复后前台**不是**捕获的应用（`paste_count == 0`）、
    注入本身失败（内容仍在剪贴板）；每种都断言中文反馈同时含「已复制」+ 原因 + 「手动粘贴」；
  - 过期选择：连续执行两条备忘录，剪贴板与注入内容都是**最后执行的那条**，成功反馈里不含
    上一条的标题；列表快照过期时按当前正文复制；已删除的备忘录不写剪贴板也不产生计划；
  - 首屏标签：同一标签的两条都列出（来源 = 备忘录插件、默认操作 = 粘贴、层级 =
    关键词或标签精确），非完整标签不命中；关键词与标签冲突时同时保留入口与候选，
    执行入口进入范围、之后同一输入的查询仍留在范围内。

  ### 界面验证（浏览器交互检查，`tools/ui-check/check.mjs` 新增 3 项）

  - `53-memo-pasted.png` / `54-memo-pasted-switched.png` / `55-memo-paste-fallback.png`：
    打开「支持自动粘贴」后执行备忘录，模拟宿主记录的外壳顺序为
    `copied → windowHidden → restored → pasted`，粘贴内容 = 刚执行的那条正文；切换选择后
    粘贴内容随之变化（不是上一次的选择）；把「唤起前应用」清空后，界面显示
    「已复制…没有记录到唤起前的应用…请切换到目标应用后按 Ctrl+V 手动粘贴」，且没有注入。
  - `56-memo-keyword-tag-collision.png` / `57-memo-collision-entered-scope.png`：
    新建一条标签为 `memo` 的备忘录后输入 `memo`，首屏**同时**给出插件入口（第一条）与
    该备忘录；回车进入范围，列表全部是备忘录。
  - `58-memo-ime-no-paste.png`：真实浏览器级中文组合（CDP `Input.imeSetComposition`）期间
    回车既不复制也不粘贴；组合结束后回车才复制。
  - 既有 39 项全部保持通过（含 ticket 07 的复制反馈、范围进入、返回恢复、插件停用）。

  ### 真实桌面证据（本地实测，与替身分开记录）

  `cargo run -p flashcast-platform --bin flashcast-platform-check`（真实实现，无替身）：

  ```
  [实测通过] flashcast-platform 编译 — linux x86_64 上编译并运行成功
  [实测通过] 桌面会话类型判定 — XDG_SESSION_TYPE="wayland"，判定为 Wayland；DISPLAY=true，WAYLAND_DISPLAY=true
  [实测通过] freedesktop 软件发现 — 发现 35 个可启动软件，其中 35 个解析到图标文件
  [未覆盖] 读取唤起前前台应用 — Wayland 会话不向普通应用暴露全局焦点窗口…
  [未覆盖] 全局快捷键注册 — Wayland 会话不支持全局快捷键抓取…
  [未覆盖] 剪贴板文字读写 — 文本复制已由 ticket 07 实现；图片、富文本与文件列表尚未实现…
  [未覆盖] 真实写入系统剪贴板并回读 — 写入没有完成：超过 5 秒没有返回，已放弃等待。…（wl-copy 无法取得选区）
  [实测失败] 自动粘贴到唤起前应用 — Wayland 不允许应用把焦点交给其他应用后注入按键：…（见下方说明）
  [未覆盖] 自动粘贴前置条件（XTEST / 会话类型） — 会话=Wayland，XTEST=true，注入后端=不可用；未注入真实按键
  [实测通过] X11/EWMH 诊断 — 可用性=Available，窗口管理器=Some("GNOME Shell")，_NET_CLIENT_LIST 窗口数=Some(1)，前台窗口=None
  汇总：实测通过 4，实测失败 1，未覆盖 5
  ```

  - `[实测失败] 自动粘贴` 这一行**不是缺陷**：自动粘贴在 Wayland 上的能力结论就是「不支持」，
    这个二进制的既有约定把 `Support::Unsupported` 映射成「实测失败」（ticket 01 起如此，
    Linux/Windows/macOS 三处一致），原因文本里写得很清楚。它**没有**被当作通过。
  - 新增的 `真实写入系统剪贴板并回读` 需要显式 `--allow-clipboard-write`（会覆盖用户剪贴板）；
    本机加该参数运行 → **未覆盖**：`wl-copy` 在这个自动化会话里拿不到剪贴板选区，5 秒后按
    超时中止（真实桌面有输入序列时应当正常，这一点**未验证**）。这次尝试同时暴露并修掉了
    上面那个「无限阻塞」缺陷。
  - `paste.prepare` 报告 XTEST 扩展在 XWayland 的 X 服务器上**确实存在**（真实查询），但注入
    后端仍按会话判定为不可用——Wayland 下不去注入，与能力结论一致。
  - 检查二进制**不注入真实按键**：它运行时前台窗口是用户的终端 / 编辑器，盲发一次 Ctrl+V
    会往那里粘贴用户剪贴板里的任意内容。研究笔记与 spec 都明确禁止「把命令发出去了」当作
    粘贴成功。

  ### 未覆盖项与原因（严格如实）

  - **真实桌面上的自动粘贴端到端**：本机会话是 Wayland，平台结论就是「不支持自动粘贴」，
    因此**没有**任何真实粘贴发生过，也**没有**检查过任何目标应用里的内容。真实验证需要在
    X11 会话里对准一个真实目标应用：唤起 → 输入标签 → 回车 → 在目标应用里确认正文出现。
  - **真实剪贴板写入**：`wl-copy` 在自动化会话里拿不到选区并阻塞（已按 5 秒超时中止），
    因此「写进系统剪贴板并回读」本机未取得通过证据。
  - **X11 上的 XTEST 注入与 EWMH 焦点恢复联调**：XTEST 扩展存在（实测），但本机没有可用的
    X11 桌面会话来真的注入按键；`FLASHCAST_FORCE_X11_BACKEND=1` 只用于诊断，不算支持证据。
  - **Windows 的 `SendInput` 真实注入**：本机不是 Windows，只做了
    `cargo check --target x86_64-pc-windows-msvc --all-targets`（编译正确性），没有在
    Windows 上实测粘贴，也没有实测 UIPI（管理员窗口收不到）这一限制。
  - **macOS 的 `CGEvent` 真实注入与辅助功能权限**：只做了
    `cargo check --target x86_64-apple-darwin --all-targets`；`AXIsProcessTrusted()` 的
    权限分支没有在 macOS 上跑过，「未授权 → 手动粘贴」这条路径只由替身覆盖。
  - **界面检查是浏览器模拟宿主**：不是 Tauri webview，也没有真实剪贴板；它证明的是 UI 的
    状态与反馈，不证明任何平台真的粘贴成功。
  - **`pastePending` 的界面分支**：真实外壳会在同一次命令里把 `pastePending` 解析成最终状态，
    因此 UI 通常看不到该状态；`StatusBanner` 里为它保留的信息条没有交互检查覆盖（防御性渲染）。
  - **`same_app` 的 id 回退分支**：X11/Windows 有窗口句柄、macOS 有进程号，稳定标识回退只在
    数据不完整时可达，仅由单元测试覆盖，没有真实桌面样本。

  ### 给 ticket 09（剪贴板历史）的接缝

  - **复用 `Host::finish_copy_for_paste(label, text)`**：先把内容（及其受支持格式）写进剪贴板，
    再调用它，就会得到两种结果之一：`PastePending` + `PastePlan`（外壳接着走关窗 → 恢复 →
    注入），或 `copied_needs_manual_paste`（中文反馈已经写清原因与下一步）。粘贴的恢复、
    核对、注入与降级逻辑全部在 `complete_paste()` 里，不需要复制一份。
  - **外壳侧不需要新代码**：`commands::execute` 已经处理任何带 `paste` 计划的 `ActionOutcome`，
    剪贴板条目只要 `ItemKind::ClipboardEntry` 走同一条命令入口即可（`Host::execute` 目前只对
    `Memo` 分支调用了 `finish_copy_for_paste`，09 增加 `ClipboardEntry` 分支时照抄那 5 行）。
  - **UI 侧**：`ActionOutcome.paste` 与 `pastePending` 已在 `src/types.ts` 里；执行后按当前输入
    重新查询的逻辑只针对 `command` 条目，剪贴板条目不受影响。
  - 注意每个条目的 `default_action` 必须是 `Paste`，且 `PastePlan.label` 要是人能看懂的摘要
    （粘贴成功/降级反馈都会用它）。
