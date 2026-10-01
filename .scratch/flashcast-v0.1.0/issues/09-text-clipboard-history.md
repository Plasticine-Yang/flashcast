# 09: 保存、搜索并恢复文字剪贴板历史

Status: done
Category: enhancement

**What to build:** 用户启用剪贴板插件后，复制的文字被后台保存；输入剪贴板或剪切板搜索历史，回车粘贴，并能控制保留的数据。

Blocked by: 08

- [x] 两个关键词进入同一插件，后台捕获文字并本机持久化，重启后可搜索、预览和粘贴。
- [x] 数据模型按一次复制事件保存格式集合、时间、摘要和附件引用，可扩展到图片、富文本及文件。
- [x] 重复内容去重，Flashcast 恢复历史不会导致自身监听循环和重复条目。
- [x] 支持置顶、删除、清空、暂停/恢复记录以及保留期限和容量配置；停用插件停止后台活动。
- [x] 历史与索引保留在本机，配置工作区只保存可迁移偏好。
- [x] 捕获失败、存储失败和容量限制有准确状态，粘贴复用原应用恢复与复制回退。
- [x] 非 UI 验证覆盖真实持久化、去重、暂停、过期与删除行为；平台监听与读写在可执行环境真实检查。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

先交付完整文字历史链路，同时定义格式集合与附件引用，媒体扩展各自独立验收。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（实现完成，Status: done）：文字剪贴板历史全链路交付，分支 `ticket/09-text-clipboard-history`。

### 交付内容

数据模型与存储（`crates/flashcast-core/src/clipboard.rs`，一次复制事件）：

| 字段 | 存储位置 |
| --- | --- |
| 稳定事件 id、捕获时间、去重键、摘要、可索引文字、来源应用、置顶、重复次数 | `clipboard_events` |
| 格式集合（text / html / rtf / image / files，kind 是字符串标签） | `clipboard_formats`（含 mime / width / height / item_count / names） |
| 需要原样保留的载荷（html / rtf） | `clipboard_payloads`（role 是字符串标签） |
| 附件引用（图片、文件副本、原文件引用） | `clipboard_attachments`（含 depends_on_source） |

schema 从本 ticket 起就是完整的：tickets 10（图片）、11（HTML/RTF）、12（文件列表）只增加行，
不需要迁移；未知标签有 `ClipboardFormat::Other` / `PayloadRole::Other` / `AttachmentKind::Other`
降级表示，不丢数据。本机路径 `<应用数据目录>/clipboard/history.sqlite3` + `attachments/`，
数据库带 schema 版本，高于本应用的版本会被如实拒绝。

- 插件：`crates/flashcast-core/src/plugins/clipboard.rs`。剪贴板 / 剪切板 / clipboard 三个别名
  进入**同一个**插件（spec 明确剪切板只是输入别名）；`contributes_to_home()` 为 false（首屏
  不检索历史）；`default_enabled()` 为 false（隐私敏感，必须由用户显式启用）。
- 平台层：`ClipboardWatcher`（ADR §5）轮询 + 自身写入抑制；Linux / Windows / macOS 三套实现 +
  `fake` 替身。Linux 读 `wl-paste` / `xclip -o` / `xsel -b -o`，X11 下从 CLIPBOARD 选区持有者
  推导来源应用；Windows 用 `GetClipboardSequenceNumber` + `GetClipboardOwner`；
  macOS 用 `pbpaste`（来源应用如实留空）。读取与写入一样有界（3s / 5s），拿不到选区时如实报错。
- 宿主：捕获运行时（轮询 → 去重 → 自身写入抑制 → 落库 → 回收），停用插件即停止线程；
  管理入口（置顶 / 删除 / 清空 / 暂停 / 保留期限 / 容量）；`clipboard_state()` 如实报告
  存储失败、容量触顶、最近一次捕获失败。设置新增 `[clipboard]`（paused / retentionDays /
  capacity），随其它可迁移偏好写进工作区。
- 外壳：`get_clipboard_state` / `set_clipboard_paused` / `set_clipboard_limits` /
  `pin_clipboard_entry` / `delete_clipboard_entry` / `clear_clipboard_history`；启动时按清单
  恢复后台捕获。execute 增加 `ItemKind::ClipboardEntry` 分支，默认操作粘贴，复用 ticket 08 的
  恢复 + 复制回退。UI 新增设置页的剪贴板历史面板（状态、暂停、范围、置顶、删除、清空）。

### 验证命令与结果

- `cargo test --workspace` → **317 passed / 0 failed**（基线 286，新增 31）。
  命令：`eval "$(scripts/dev/linux-native-deps.sh)" && cargo test --workspace`
- CI 平价：`mkdir -p "$HOME/.cache/flashcast/fakehome" && HOME="$HOME/.cache/flashcast/fakehome" GIT_CONFIG_NOSYSTEM=1 timeout 1500 cargo test --workspace`
  → **317 passed / 0 failed**（不依赖开发机的 `~/.gitconfig`）。
- `cargo build -p flashcast` → 成功（仅有一处既有的 dead_code 警告：`hotkey::status` 未被使用）。
- `pnpm install --frozen-lockfile && pnpm build` → 成功（`tsc --noEmit` + `vite build`）。
- `pnpm ui-check` → **53 passed / 0 failed**（基线 49，新增 4 项：67–70）。
  截图：`artifacts/ui/67-clipboard-scope.png`、`68-clipboard-preview.png`、
  `69-settings-clipboard.png`、`70-settings-clipboard-disabled.png`（`artifacts/` 已被 gitignore）。
- 交叉检查（本地唯一的 Windows / macOS 编译守卫）：
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/09-windows timeout 1800 cargo check -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets` → 成功。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/09-macos timeout 1800 cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets` → 成功。
- 真实平台检查：`cargo run -p flashcast-platform --bin flashcast-platform-check -- --json --output …`
  与同一命令加 `--allow-clipboard-write`。总览 实测通过 4 / 实测失败 1 / 未覆盖 6；其中
  实测失败只有既有的 `paste.auto`（Wayland 不允许注入按键），与本 ticket 无关。
- `git merge feat/flashcast-v0.1.0` → 「已经是最新的」（集成分支仍在本 ticket 的基线提交
  32b9296 上），**没有冲突**可解。

### 覆盖到的行为（`crates/flashcast-core/tests/clipboard.rs`，22 条，全部经宿主入口）

两个中文别名进入同一插件且看到同一份历史、首屏不检索历史、捕获 → 重启 → 检索 / 预览 / 粘贴、
无目标时降级为手动粘贴、去重（copies 累加）、自身写入抑制**两层各一条**（适配层按序号/指纹、
宿主按内容指纹兜底，断言不得形成循环）、粘贴备忘录不进入历史、置顶 / 删除 / 清空、
暂停与恢复不补记、容量淘汰最旧未置顶、改小容量立即回收、容量触顶如实拒绝、
保留期限与置顶豁免、清空历史同步回收孤儿附件、停用不捕获不返回（且**不读取**剪贴板）、
停用停止后台线程、读取失败与存储失败的状态（含查询结果里的插件失败列表）、
一次文字事件的格式集合 / 时间 / 摘要 / 来源应用字段、历史与索引不进配置工作区。

模块内另有单元测试：存储层的字段往返（含 tickets 10–12 的图片 / 文件 / HTML 字段与未知标签）、
去重、容量、过期与附件回收、存储失败与自动恢复、检索与置顶排序；平台层读取有界与空输出语义。

### 未覆盖（如实记录）

- **真实 Wayland 剪贴板读写与监听：未覆盖。** 本机是 Wayland
  （`XDG_SESSION_TYPE=wayland`，`WAYLAND_DISPLAY=wayland-0`）。`flashcast-platform-check` 的
  `clipboard.write_text` 报告「写入没有完成：超过 5 秒没有返回」，`clipboard.watch` 报告
  「读取剪贴板失败：剪贴板工具 /usr/bin/wl-paste 超过 3 秒没有返回」。原因是 Wayland 的选区
  由持有者进程提供，自动化会话没有可用的持有者；这与 ticket 08 的 `wl-copy` 结论一致。
  **这既不能推断真实桌面上的复制会失败，也不能推断 Wayland 下可用**：需要在有交互桌面的
  Wayland 会话里手动复核。
- **Windows / macOS 真实剪贴板读写与来源应用：未覆盖。** 本机无法执行；只做了交叉编译检查，
  由对应平台的 CI runner 覆盖构建，真实读写仍需在真机上验证。Windows 的
  `GetClipboardSequenceNumber` 与 `GetClipboardOwner`、macOS 的 `pbpaste` 都未在真机执行过。
- **X11 下的来源应用推导：未覆盖。** 代码路径是 `x11::clipboard_owner()`（选区持有者 →
  `WM_CLASS` / `_NET_WM_PID`），本机 Wayland 会话下走不到。
- **去重在真实适配层上的表现：未覆盖。** Linux / macOS 用内容指纹判断变化，同一段文字被复制
  两次只算一次变化，因此「去重累加 copies」只能由宿主的 content_hash 保证，并用替身覆盖；
  真实适配层上观察到的是「第二次不产生变化」，与去重结果一致但不是同一条路径。
- **UI 未做真机粘贴核对。** `pnpm ui-check` 只驱动浏览器里的模拟宿主，不代表 Tauri 托盘、
  全局快捷键、自动粘贴或真实软件启动可用；真正的目标应用粘贴仍需手动检查。

- 2026-10-01（ticket 17 更正本 ticket 的过期结论）：下面「真实 Wayland 剪贴板读写与监听：
  未覆盖（wl-copy 拿不到选区 / 自动化会话没有选区持有者）」是**误判**，真实原因是一个平台层
  缺陷：`write_with_tool` 在子进程成功退出后无条件 `join` 标准错误读取线程，而 `wl-copy`
  fork 出的选区持有者继承了同一个管道，`join` 永久挂起。修复后（提交 `7bc2ed2`）本机 Wayland 上
  `clipboard.write_text`（写入并由 `LinuxClipboardWatcher` 读回同一段文本）与 `clipboard.watch`
  都是**实测通过**，平台检查总览从 4 通过 / 9 未覆盖变为 **9 通过 / 1 失败 / 4 未覆盖**。
  仍然成立的部分：Windows / macOS 的真实剪贴板读写、X11 下的来源应用推导仍未覆盖。
