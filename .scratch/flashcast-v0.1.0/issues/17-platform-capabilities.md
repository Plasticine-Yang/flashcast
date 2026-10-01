# 17: 展示平台能力状态并完成首版兼容性验收

Status: done
Category: enhancement

**What to build:** 用户能在设置查看运行环境、权限与能力状态；维护者获得候选版本的平台检查证据，明确哪些行为通过、失败或未覆盖。

Blocked by: 04, 10, 11, 12, 13, 16

- [x] 设置显示系统、架构、Linux 会话类型（适用时）、快捷键、剪贴板和自动粘贴的实际支持及权限状态。
- [x] 不以测试替身、编译成功或指令已发送作为真实桌面通过；诊断信息不包含历史内容或凭证。
- [x] 当前 Linux 桌面完成唤起、搜索软件、启动、备忘录标签粘贴、各格式历史、书签、配置与主题的实际验收。
- [x] GitHub runner 完成可运行的平台检查，记录环境、前置权限、实际结果、日志和可获得的截图；无法执行的项目记为未覆盖及原因。
- [x] UI 通过手动或浏览器交互检查键盘、中文输入、查询返回、紧凑/预览、浅深色、缩放及失败反馈，不添加 UI 单元测试。
- [x] 首版必需构建与核心行为检查全部通过；已发现的核心故障修复后重验，未覆盖与平台受限项列入发布说明。
- [x] 核对文件副本、富文本降级、复制回退和签名状态的实际行为与说明一致。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

此前每张功能票已完成自己的验证；本票交付可见的平台状态，并检查跨功能的最终候选版本，不代替前置票的验收。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（ticket 17 实施完成，Status → done）：交付**设置页的运行环境与能力报告**、
  **合并的平台能力报告**（机器可读 + 人读摘要）与**真实 Linux 桌面验收**，并修掉一个被
  前置票误判成「环境限制」的核心故障。以下只记录实际跑过的命令与结果。

  **本机环境**：Ubuntu 26.04.1 LTS x86_64，`XDG_SESSION_TYPE=wayland`、`WAYLAND_DISPLAY=wayland-0`，
  GNOME（`XDG_CURRENT_DESKTOP=ubuntu:GNOME`），另有 XWayland `DISPLAY=:0`；`/usr/bin/google-chrome` 存在；
  没有 xclip/xsel，也没有桌面截图/输入注入工具（grim/import/scrot/xdotool/wtype 均不存在）。
  所有 cargo 命令都在 `source /tmp/17-env.sh`（由 `FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/17-native-deps scripts/dev/linux-native-deps.sh` 生成）之后执行，
  并统一 `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/17-local`、加 `timeout`、输出重定向到日志文件。

  **1. 设置页的能力报告**

  `SettingsScreen` 新增「运行环境与能力」区段：系统与版本、架构、Linux 会话类型、
  全局快捷键 / 剪贴板 / 自动粘贴的实际状态与中文原因，另有「未覆盖 ≠ 支持」
  「X11 结果不能推断 Wayland」「诊断不含剪贴板内容、书签或凭证」的说明。
  数据来源是既有的 `CapabilityProbe`：`get_capabilities` → `host.capabilities()` → `probe()`，
  每次打开设置页重新探测一次，没有第二份真相。浏览器模拟宿主的 `get_status` 与
  `get_capabilities` 也统一到同一次探测。提交 `f4db6c2`。

  本机真实探测结果（`flashcast-platform-check` 的同一份 `CapabilityProbe`）：

  - 系统 `Ubuntu 26.04.1 LTS`，架构 `x86_64`，会话 `wayland`（有可交互桌面）；
  - 全局快捷键：**不支持**（Wayland 下 global-hotkey 0.8 只有 X11 的 XGrabKey）；
  - 剪贴板：**未覆盖**（文字/HTML-RTF/图片/文件列表都已实现，环境已具备 wl-copy；
    探测拒绝仅凭环境前提就写成「支持」）；
  - 自动粘贴：**不支持**（Wayland 不允许转移焦点后注入按键）。

  **2. 合并的平台能力报告（交付物）**

  - `docs/platform/capability-report.json`（机器可读，自包含）、
    `docs/platform/capability-report.md`（人读摘要）、
    `docs/platform/capability-report.meta.json`（编辑性输入）、
    `docs/platform/evidence/*`（原始证据）、
    `scripts/ci/capability-report.py`（可重复生成）。
  - 报告区分 **(a) 替身检查**、**(b) 真实平台 / 真实桌面检查**、**(c) 未覆盖**，
    每项都带环境与原因；开头写明「编译成功不是行为证据」「替身通过不证明平台适配」
    「X11（含 XWayland）不能推断 Wayland」。

  runner 证据取自 `feat/flashcast-v0.1.0` @ `6a2c946` 的**最新一次成功运行**
  `36861987134`（11 个任务全部 success），用下面命令实际取得（不是假设）：

  ```text
  gh run list --branch feat/flashcast-v0.1.0 --limit 12 --json databaseId,headSha,conclusion,createdAt
  gh run view 36861987134 --json headSha,jobs,conclusion,url
  gh run download 36861987134 -n diagnostics-{linux-x64,windows-x64,macos-arm64,macos-x64} -D …
  gh run download 36861987134 -n flashcast-candidate-{macos-arm64,windows-x64} -D …
  gh run download 36861987134 -n flashcast-sha256sums -D …
  ```

  汇总（`docs/platform/capability-report.json` 的 `summary`）：
  真实平台 / 真实桌面检查 **实测通过 49、实测失败 5、未覆盖 25**；替身检查 15 项全部通过；
  未覆盖条目 **12 条**（逐条原因见报告 (c) 节）。

  **3. 真实 Linux 桌面验收**

  新增 `crates/flashcast-core/tests/real_linux_desktop.rs`（默认 `#[ignore]`，用
  `flashcast_platform::current()` 的真实适配器组装宿主，走宿主对外的查询/命令入口）：

  ```text
  cargo test -p flashcast-core --test real_linux_desktop -- --ignored --nocapture --test-threads=1
  → 真实条目：id=org.gnome.Calculator.desktop name="Calculator" argv=["gnome-calculator"]
  → 查询 "Calculator" → 命中 Calculator（app:org.gnome.Calculator.desktop）
  → 真实进程出现：pid=926013（这证明回车真的启动了这个软件）
  → 已结束 pid=926013；真实启动验收完成
  → 标签查询 → 验收备忘录（memo:…）
  → 真实执行结果：status=CopiedNeedsManualPaste「已复制「验收备忘录」到剪贴板；
    没有记录到唤起前的应用，无法确定粘贴目标；请切换到目标应用后按 Ctrl+V 手动粘贴」
  → 3 passed / 0 failed（日志：artifacts/platform-check/17-real-desktop-acceptance.log）
  ```

  这一项补上了 ticket 01 第 2 个复选框（「回车启动软件」此前从未端到端跑过），
  现在按真实证据勾选；第 3 个复选框（全局快捷键）保持未勾选。

  真实 Chrome：`cargo test -p flashcast-platform --test chrome_fixture real_chrome_spawn -- --ignored --nocapture`
  → 通过（一次性 `--user-data-dir`，绝不使用用户真实 profile；argv 与 `Local State` 都核对）。

  平台检查（同一会话，两种模式）：

  ```text
  cargo run -p flashcast-platform --bin flashcast-platform-check -- --json
  → 实测通过 4 / 实测失败 1 / 未覆盖 9
  cargo run -p flashcast-platform --bin flashcast-platform-check -- --json --allow-clipboard-write
  → 实测通过 9 / 实测失败 1 / 未覆盖 4
     clipboard.write_text / clipboard.watch / clipboard.rich / clipboard.image / clipboard.files 全部实测通过；
     clipboard.rich 实测确认了 Linux 的降级结论（只提供纯文本，HTML/RTF 未提供）；
     clipboard.files 用带空格 + 非 ASCII 名的两个真实文件逐项核对；
     paste.auto 是唯一的实测失败（Wayland 不允许注入按键，行为是「已复制，请手动粘贴」）。
  只读模式通过数会随「剪贴板里此刻恰好有什么」在 4–5 之间变化（只读检查读的是当前选区）。
  ```

  强制 X11 后端（经 XWayland，对照记录，**不作为 Wayland 支持的证据**）：

  ```text
  FLASHCAST_FORCE_X11_BACKEND=1 flashcast-platform-check --json
  → 实测通过 8 / 实测失败 0 / 未覆盖 6；hotkey.register 与 focus.capture 在 X11 服务器上通过，
     paste.auto 仍为未覆盖（只有 XWayland 客户端收得到合成按键）。
  ```

  **4. 本次发现并修掉的核心故障（这是本票最有价值的结论）**

  `write_with_tool` / `write_bytes_with_tool` 在子进程**成功退出**后无条件 `join` 标准错误
  读取线程。`wl-copy` 会 fork 出持有选区的守护进程，守护进程继承了同一个管道，写端永不关闭，
  于是 `join` **永久挂起**——真实 Wayland 桌面上任何一次「复制备忘录 / 恢复剪贴板历史」都会
  把宿主卡死。实测：

  ```text
  printf x | wl-copy            # 立刻以 0 退出
  printf x | wl-copy 2>&1 | cat # 永不返回
  ```

  修复（提交 `7bc2ed2`）：读取线程把结果送进通道，调用方 `recv_timeout(500ms)` 有界取回，
  超时用空串（标准错误只是补充诊断）。新增两项回归用例，其中
  `forked_daemon_holding_the_pipe_does_not_hang_the_writer` 在修复前会永久挂起。

  这也推翻了前置票的一批结论：ticket 09/10/11/12 把 Wayland 剪贴板读写/监听记为
  「未覆盖（自动化会话拿不到选区）」，实际是上述缺陷造成的误判。已在那些 ticket 里
  各加一节「2026-10-01（ticket 17 更正）」，并更正了 Windows / macOS 能力文案里
  「文件列表尚未实现」的错误说法（提交 `5ef378e`；实现本来就在，未重跑 runner，
  因此这两个平台文件列表的真实读写仍记为未覆盖）。更正的文件：
  `01-linux-launcher.md`、`08-memo-tag-paste.md`、`09-text-clipboard-history.md`、
  `10-image-clipboard-history.md`、`11-richtext-clipboard-history.md`、
  `12-file-video-clipboard-history.md`；没有改动任何其他 ticket 或文档。

  **5. UI 检查（不添加 UI 单元测试）**

  ```text
  pnpm ui-check → 通过 66 / 失败 0（共 66 项；基线 64 + 设置页能力报告 2 项）
  ```

  覆盖键盘导航、中文输入法组合期回车、Escape 返回上一查询、紧凑/预览、浅色/深色/跟随系统、
  200% 系统缩放、失败反馈与设置页能力报告；新增截图 `83-settings-capabilities.png`、
  `84-settings-capabilities-supported.png`（artifacts/ 不入库）。范围说明照旧：
  浏览器 + 模拟宿主，不代表 Tauri webview、托盘、全局快捷键、自动粘贴或真实软件启动。

  **6. 关闭前的必需检查**

  ```text
  cargo test --workspace
  → passed 364 / failed 0 / ignored 5（基线 362 + 本次新增 2 项剪贴板回归用例；
     3 项 ignored 是真实桌面验收，另有 2 项既有的真实 Chrome / macOS 夹具用例）
  mkdir -p "$HOME/.cache/flashcast/fakehome" && HOME="$HOME/.cache/flashcast/fakehome" GIT_CONFIG_NOSYSTEM=1 cargo test --workspace
  → passed 364 / failed 0 / ignored 5
  cargo build -p flashcast                     → exit 0（1 条既有 dead_code 警告：src-tauri/src/hotkey.rs:72 status）
  pnpm install --frozen-lockfile && pnpm build → exit 0
  cargo check -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets  → exit 0（1 条既有 unused import 警告）
  cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets    → exit 0
  git merge feat/flashcast-v0.1.0              → 已经是最新的（目标分支仍在 6a2c946），无冲突
  ```

  新增回归用例后 `cargo test --workspace` 从 362 变成 364；除这 2 项外没有减少或跳过任何用例。

  **仍然 未覆盖（不要当成通过；完整 12 条在 `docs/platform/capability-report.md` 的 (c) 节）**

  1. **Wayland 全局快捷键唤起**：`hotkey.register` 未覆盖、能力判定为不支持；XWayland 下注册
     实测通过，但那只是 X11 服务器上的成功，不证明原生 Wayland 客户端收得到按键，
     也没有向真实桌面注入按键去验证唤起本身。托盘是本机唯一入口。
  2. **唤起前应用身份（focus.capture）**：Wayland 不暴露全局焦点窗口；XWayland 下能读到窗口
     句柄但拿不到应用身份（id=unknown）。
  3. **Wayland 自动粘贴端到端**：能力判定不支持（没有 XDG RemoteDesktop 门户授权），
     `paste.auto` 是唯一的实测失败；刻意不注入真实按键（会打到用户当前前台窗口）。
  4. **备忘录粘贴到「唤起前的应用」的最终内容**：真实剪贴板写入与「已复制，请手动粘贴」降级
     已实测通过，但没有目标应用可以核对「目标应用里真的出现了内容」。
  5. **剪贴板来源应用推导（X11 路径）**：需要 xclip，本机未安装；Wayland 没有公开接口。
  6. **真实 Tauri webview 的视觉/缩放核对**：本机没有截图与输入注入工具，只有浏览器截图；
     进程级启动由 ticket 04 的安装包检查覆盖。
  7. **托盘菜单点击**：同上，没有输入注入工具。
  8. **Chrome 书签在可见窗口里打开**：真实 Chrome 启动与页面加载只有 headless 证据
     （一次性 user-data-dir），没有做可见窗口核对。
  9. **Windows / macOS 真实剪贴板读写**：macOS runner 只支持纯文本（HTML/RTF 为实测失败，
     属已记录降级）；Windows runner 不执行剪贴板写入。
  10. **Windows `CF_HDROP` 与 macOS AppleScript 文件列表的真实读写**。
  11. **有证书时的签名分支**：Windows 导入 pfx → thumbprint、macOS 正式签名 + 公证从未执行
      （没有凭证）；当前状态是 macOS ad-hoc（未公证，Gatekeeper 会拦）、Windows 未签名
      （SmartScreen 会拦）。
  12. **X11 会话下的剪贴板**：本机是 Wayland 且没有 xclip/xsel，不能用 Wayland 结果推断 X11。

  **已知的非阻塞瑕疵**：`cargo build -p flashcast` 有 1 条既有 dead_code 警告
  （`src-tauri/src/hotkey.rs:72` 的 `status`）；Windows 目标交叉检查有 1 条既有
  unused import 警告（`crates/flashcast-platform/tests/macos_bundle_fixture.rs:16`）。
  两者都不影响行为，未在本票修复。

  **本次提交**：`f4db6c2`（设置页能力报告）、`7bc2ed2`（剪贴板挂起修复 + 回归用例）、
  `a01d284`（真实桌面验收）、`79c7c5c`（UI 检查 83/84）、
  `9f99112`（核对与更正过期结论）、`5ef378e`（更正 Windows/macOS 能力文案）、
  `2b2e49f`（合并的平台能力报告）。均为中文提交说明，未 push、未打 tag、未创建 Release。
  另需向上游说明：本分支开始时 `feat/flashcast-v0.1.0` 的 reflog 里存在一条被 reset 掉的
  前一任实施者提交 `d966bba`（设置页能力区段初稿），本票的 `f4db6c2` 在其基础上复核、
  补正并重新提交，该悬空提交不再需要。
