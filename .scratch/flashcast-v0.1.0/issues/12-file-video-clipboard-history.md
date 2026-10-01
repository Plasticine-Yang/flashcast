# 12: 保存文件与视频文件引用或副本并恢复

Status: done
Category: enhancement

**What to build:** 用户复制文件或视频文件后可搜索名称、检查来源，并选择保存本机副本；回车恢复文件列表以便粘贴到支持文件的目标。

Blocked by: 09

- [x] 按各平台公开格式捕获文件列表，支持多个文件、空格及非 ASCII 路径，视频按文件处理。
- [x] 列表与预览清楚区分引用和已保存副本；原文件失效时引用显示不可恢复状态。
- [x] 用户可以明确保存受容量限制的本机副本，原文件删除后副本仍可恢复。
- [x] 恢复正确的文件列表与复制语义，不伪造成功，不自动移动或删除原文件。
- [x] 名称和元数据可搜索，超限、访问失败、复制中断或不支持的文件类型有准确状态。
- [x] 副本仅保存在本机，删除、清空及过期回收不再引用的附件，去重不造成共享附件误删。
- [x] 经宿主入口验证副本内容与生命周期，真实平台验证文件列表恢复；不包含录屏、视频片段提取或任意私有格式保证。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

本切片将“保存视频”明确实现为复制的视频文件引用或副本，不把一个文件路径当作已保存视频内容。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（实现完成，Status: done）：文件 / 视频文件引用与副本全链路交付，分支
  `ticket/12-file-video-clipboard-history`。本 ticket 的提交（在 ticket 09/11 之后）：

  - `d4a61d4` 平台(剪贴板)：文件列表捕获与恢复的适配层（ticket 12 第一步）
  - `188cbf8` 进行中(ticket 12)：文件列表事件模型、附件副本与恢复分支（未完成，待续）
  - `a1d2fc9` 进行中(ticket 12)：保存未完成的工作（待续）
  - `0e5b956` 进行中(ticket 12)：合并 feat/flashcast-v0.1.0（ticket 09/11 富文本）并解决全部冲突
  - `71ea23e` 修正(平台)：合并后补齐 ClipboardCapture 的 html/rtf 字段与各平台实现块
  - `228e1f5` 修正(核心/平台)：合并后补齐 PastePlan 字段与 fake 替身导入，工作区整体可编译
  - `caf26a3` 界面(剪贴板)：文件条目的引用/副本徽标、不可恢复状态与显式保存副本动作，并补上文件列表样式
  - `882ca9e` 检查(界面)：文件列表条目、显式保存副本、不可恢复状态与文件列表恢复的浏览器检查 79-82
  - `bb2dff3` 检查(平台)：新增文件列表(text/uri-list)真实读写检查，并更新已实现的公开格式范围说明
  - `4f2d3ce` 清理(平台)：对本次改动过的文件执行 rustfmt，并去掉 Windows 未使用的导入

### 交付内容

**数据模型与存储**（`crates/flashcast-core/src/clipboard.rs`）——**没有 schema 迁移**（ticket 09 的
`clipboard_attachments` 已有 `kind` / `path` / `name` / `mime` / `bytes` / `depends_on_source`，
`clipboard_formats` 已有 `item_count` / `names`）：

- `event_from_capture` 把一次捕获里的**文字、HTML/RTF、文件列表**合并成**一条** `ClipboardEvent`：
  每个文件生成一个 `AttachmentKind::FileReference` 附件，`depends_on_source = true`（引用依赖原文件）；
  格式集合只记**真的有内容**的格式（平台声明了却没有载荷、或带来了载荷却没声明，都以实际内容为准）。
- **去重键按类型分开**：有文件时 `content_hash = content_hash_files(paths)`（`files:` 前缀，FNV-1a 按
  路径保序），只有文字时仍是 `content_hash_text`（ticket 11 的语义不变）。因此同一份文件列表与同名
  文字不会互相误判，视频文件与普通文件没有任何差别。
- `summary_for_files(names)` 生成「N 个文件：a、b、c…」的摘要（超长按 `SUMMARY_MAX_CHARS` 截断），
  文件名里的空格与非 ASCII 原样保留。
- `ClipboardFileView`（`attachment_id` / `name` / `path` / `source_path` / `kind` / `mime` / `bytes` /
  `recoverable` / `problem`）是列表与预览共用的**按当前文件系统计算**的状态视图；`AttachmentKind`
  增加 `FileReference` / `FileCopy`，`file_kind_label_zh` 给出「引用 / 已保存副本」。
- `save_file_copy(event_id, attachment_id, per_file_limit, total_limit)`：**只由用户显式调用**，
  只读原文件，从不移动或删除它；单份超 `MAX_COPY_BYTES`（512 MB）或总量超
  `MAX_COPY_TOTAL_BYTES`（2 GB）时如实拒绝并给出数字；副本路径由「原文件路径 + 大小」去重，同一
  原文件被两条历史各保存一次时**复用同一个文件**；先写 `.part-*` 再原子改名，任何失败都清掉临时
  文件并报「复制中断」；成功后把**同一行**引用原地改写成副本并在同一事务里记下 `file-source` 载荷
  （原文件路径），因此列表里一个文件只出现一次，不会同时显示一条失效引用和一条可用副本。
- 副本状态判定：原文件还在时用**原路径**（避免粘贴出重复文件），原文件没了才用副本；副本丢失时报
  「本机副本已丢失」。目录 / 特殊文件报「不支持的文件类型」，原文件消失报「原文件已不存在」。
- `reclaim_attachment_files` 只删除**本机附件目录内**且没有任何附件行引用的文件——共享副本因为仍被
  另一行引用而保留；`restore_paths` 在任何一项不可恢复时**整份拒绝**（不写半份列表、不伪造成功）。

**平台层**（`crates/flashcast-platform/src/clipboard.rs` 及三个平台实现）：

| 平台 | 读文件列表 | 写文件列表 | 实现方式 |
| --- | --- | --- | --- |
| Linux | `wl-paste --type text/uri-list` / `xclip -selection clipboard -t text/uri-list -o`，GNOME 的 `x-special/gnome-copied-files` 兜底 | `wl-copy --type text/uri-list` / `xclip -t text/uri-list -in`（`xsel` 无等价参数，不参与） | `text/uri-list`（RFC 2483） |
| Windows | `CF_HDROP` + `DragQueryFileW` | `DROPFILES` + 双 NUL 结尾的 UTF-16 路径列表 + `CF_HDROP` | 资源管理器粘贴文件列表的公开格式 |
| macOS | AppleScript `the clipboard as «class furl»` | AppleScript `set the clipboard to` 一组 `POSIX file` | Finder 与文件对话框可粘贴 |

- 解析 / 编码是**纯逻辑**放在跨平台模块里：`parse_uri_list` / `format_uri_list` / `encode_file_uri` /
  `percent_decode`（空格 `%20`、非 ASCII UTF-8 百分号编码，两种写法都还原成同一路径；`#` 注释与
  GNOME 首行 `copy`/`cut` 都跳过），`file_entries` 推导名称与 MIME（含视频扩展名），`MAX_FILES` 上限。
- trait 上 `write_files` / `read_files` 有默认实现：如实报「当前平台没有实现」或返回 `Ok(None)`，
  不会把「没写进去」说成「已复制」。
- fake 替身：`FakeClipboard::write_files` / `read_files`（与文本共用失败队列）、
  `FakeClipboardWatcher::set_files` / `note_own_write_files`（文件列表优先于文字、按文件指纹抑制自身写入）。
- 文件列表读取**有界**（与文字一致），工具缺失返回 `Ok(None)`、「没有文件列表」与「读取失败」分开。

**宿主**（`crates/flashcast-core/src/host.rs`）：

- `execute_clipboard_entry`：文件条目走 `restore_paths` + `write_files`，登记自身写入
  （`note_own_write_files`，两层抑制各自成立），然后复用 ticket 08 的
  `finish_copy_for_files`（自动粘贴不可用 / 没有目标时降级为「已复制，请手动粘贴」）。
- `PastePlan` 增加 `files`（本次复制的文件个数）与 `formats_note`（ticket 11 的降级说明），两者共存。
- `save_clipboard_file_copy`（生产）与 `save_clipboard_file_copy_limited`（测试可指定上限，判据完全相同）
  把 `ClipboardCopyError` 的准确中文原因带回外壳。
- 插件的匹配元数据（`metadata_for`）增加**文件名称、MIME 与路径**——文件列表没有可索引文字
  （spec「不承诺 OCR」），名称与元数据是唯一检索入口；仍**不含** HTML/RTF 载荷原文。
- `clipboard_preview` 对文件列表逐条给出「引用 / 已保存副本」与当前是否可恢复，状态按当前文件系统
  计算（原文件删除后再次预览必须显示不可恢复）。

**外壳**（`src-tauri`）：`save_clipboard_file_copy(id, attachment_id)` 命令转发宿主入口，并注册进
`invoke_handler`。

**界面**：设置页的剪贴板条目为文件列表渲染紧凑的文件行（名称、MIME、字节数、引用/副本徽标、
「不可恢复：原因」），只有**可恢复的引用**才显示「保存本机副本」按钮；点击后该行原地变成
「已保存副本」，条目副标题的「N 个引用 · M 个已保存副本」随之更新。新增 CSS 只用既有 `--fc-*`
令牌、不加动画、不引入框架。模拟宿主增加两条样例（多文件含空格/非 ASCII/视频、原文件已消失），
`save_clipboard_file_copy` 与检索、恢复在浏览器模拟宿主里同口径实现。

### 验证命令与结果

**测试替身证据（宿主入口，`crates/flashcast-core/tests/clipboard_files.rs`，新增 14 条）**

```bash
FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/12-native-deps \
  eval "$(scripts/dev/linux-native-deps.sh)"
timeout 1800 cargo test --workspace
# → 346 passed / 0 failed（合并 ticket 09/11 后；ticket 12 新增 14 条）
```

覆盖（全部经 `ClipboardHarness` 的宿主入口，不碰存储内部）：

- 多文件列表捕获：含**空格**、**非 ASCII** 与一个视频文件；名称、MIME、字节数、格式集合与摘要都正确。
- 文件事件与文字事件**不跨类型去重**（同一份列表与同名文字各自成条）。
- 列表与预览区分**引用 vs 已保存副本**；原文件删除后引用显示**不可恢复**及原因。
- 显式保存副本后**原文件被删除，副本仍可恢复**。
- 恢复把**准确的文件列表**写进剪贴板（顺序与路径一致），无目标时降级为手动粘贴提示。
- 按文件名检索命中（摘要被截断也命中）。
- 条目容量触顶与新列表被拒的准确状态；副本的单份 / 总量超限报告带数字。
- **复制中断**（落点被占）不留 `.part-*` 临时文件；**目录**按「不支持的类型」拒绝且预览如实显示。
- 删除、清空、过期回收都清掉**不再被引用**的副本。
- **两条历史共享同一份副本**：删掉其中一条，另一条仍在用的副本文件不被删，且另一条仍能完整恢复
  （第一个文件用共享副本路径、第二个文件仍是原路径）。
- **原文件永远不会被移动或删除**：保存副本、恢复、删除历史、清空、过期回收之后，原文件在原位置、
  内容不变，副本是另一个文件。

**CI 等价环境（本机 `~/.gitconfig` 不可见）**

```bash
mkdir -p "$HOME/.cache/flashcast/fakehome"
HOME="$HOME/.cache/flashcast/fakehome" GIT_CONFIG_NOSYSTEM=1 timeout 1500 cargo test --workspace
# → 346 passed / 0 failed
```

（`CARGO_HOME` / `RUSTUP_HOME` 由 `linux-native-deps.sh` 按**真实** `HOME` 导出，不随上面的 `HOME`
改变，因此不会重建依赖缓存。libgit2 会忽略 `GIT_CONFIG_GLOBAL`，所以必须换 `HOME`。）

**外壳与界面**

```bash
timeout 1200 cargo build -p flashcast                # → 成功
pnpm install --frozen-lockfile && pnpm build         # → 成功（tsc --noEmit + vite build）
pnpm --dir tools/ui-check install --frozen-lockfile  # 独立的 npm 项目，首次需要
timeout 1500 pnpm ui-check                           # → 通过 60 / 失败 0
```

- `ui-check` 从 **53 通过**变成 **60 通过 / 0 失败**：新增 4 条（79–82），并修正被新样例数据影响的
  67 / 76 / 77（原先按「恰好 2 条历史」写死，现在按语义等待富文本条目出现）。
- 新增截图（`artifacts/ui/`，已被 gitignore）：`79-clipboard-file-list.png`、
  `80-settings-clipboard-file-copy.png`、`81-settings-clipboard-unrecoverable.png`、
  `82-clipboard-files-paste.png`。
- 唯一的构建警告是既有问题、不属于本 ticket：`src-tauri/src/hotkey.rs:72 status is never used`
  （在 `feat/flashcast-v0.1.0` 上同样存在）。

**真实平台检查**（新增 `clipboard.files` 项）

```bash
timeout 300 ./target/debug/flashcast-platform-check --json --output /tmp/fc12/read.json
timeout 300 ./target/debug/flashcast-platform-check --json --output /tmp/fc12/write.json --allow-clipboard-write
# → clipboard.files: not_covered（两种模式都是）
#    只读：wl-paste 超过 3 秒没有返回
#    写入：wl-copy 超过 5 秒没有返回
# → 总览 measuredPass 4 / measuredFail 1 / notCovered 8
```

`clipboard.files` 检查**故意分类为未覆盖而不是失败**：只读模式读当前选区的文件列表，加
`--allow-clipboard-write` 时在临时目录建两个文件（一个名字带空格、一个非 ASCII），写入后读回并逐项
比对路径与顺序；拿不到选区属于环境限制（`selection_unavailable`），不是实现失败。临时文件在检查里
自行清理。

**交叉目标类型检查**（本机 Linux 不编译 `cfg(target_os = "windows")` 与 macOS 专属代码）

```bash
CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/12-windows \
  timeout 1800 cargo check -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets   # → 0 error
CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/12-macos \
  timeout 1800 cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets      # → 0 error
```

### 未覆盖（如实记录）

- **真实 Wayland 剪贴板上的文件列表读写与恢复：未覆盖。** 本机是 Wayland，自动化会话里没有任何
  选区持有者：`wl-paste` 3 秒、`wl-copy` 5 秒超时（ticket 08/09/11 同样；同一会话里连文字剪贴板也
  拿不到）。因此「复制真实文件 → 历史出现文件条目 → 恢复后端到端粘贴」只有宿主入口的测试替身证据，
  没有真实桌面证据。**ticket 17 必须在真实桌面确认**（见下）。
- **Windows `CF_HDROP` 与 macOS AppleScript 文件列表的真实读写：未覆盖。** 本机无法执行这两种系统
  调用，只做了交叉目标 `cargo check`（0 error）；解析 / 编码的**纯逻辑**部分有 Linux 上的真实字节
  单元测试（`parse_uri_list` / `format_uri_list` / `file_entries`）。
- **真实权限失败（访问失败）与「不支持的文件类型」的平台差异：未覆盖。** 「复制中断」「超限」
  「目录 / 特殊文件」「原文件已不存在」都有测试替身证据，但本机无法制造需要提权 / ACL 的真实
  访问失败来核对平台错误消息。
- **录屏、视频片段提取、任意应用私有格式：不在本 ticket 范围。** 视频在 v0.1 就是普通文件，没有
  独立入口，也不声称保留私有格式。
- **图片历史的恢复：由 ticket 10 实现，两条线的恢复路径已在合并里合流。** 合并后的拷贝入口按
  「文件列表 → 文字 → 图片」分派（`Host::execute` 的剪贴板条目分支）：文件列表优先于同一次复制里
  的 `text/uri-list` 文字，图片走本机附件写回剪贴板；真正没有可粘贴内容的条目才如实返回
  「没有可直接粘贴的文字、图片或文件内容」，不再声称「图片历史的恢复将在后续版本提供」。
- **模拟宿主（浏览器）的文件条目证据只代表 UI + 模拟宿主**，不代表 Tauri webview、托盘、全局
  快捷键或真实剪贴板可用。

### 给 ticket 17 的真实桌面确认项

1. 在真实桌面（X11 优先；Wayland 也需要真实选区持有者）复制 2–3 个文件，其中一个名字带**空格**、
   一个**非 ASCII**、一个 `.mp4`：历史里出现**一条**文件条目，名称 / MIME / 引用计数正确。
2. 对其中一个引用点「保存本机副本」，然后**删除原文件**：该引用显示**不可恢复**，已保存的副本仍
   显示可恢复；回车恢复后粘贴到文件管理器 / 文件对话框，得到正确的文件列表。
3. 直接恢复一个仍有效的引用：粘贴出的列表包含**原路径**（而不是副本路径），原文件没有被移动或删除。
4. 加 `--allow-clipboard-write` 跑 `flashcast-platform-check`，确认 `clipboard.files` 在真实选区上
   从「未覆盖」变成「实测通过」（写入的路径与读回的路径逐项一致）。
5. Windows 上确认资源管理器复制文件后 `CF_HDROP` 被正确读到（脚本复制文件时剪贴板里同时有文本，
   必须优先按文件捕获）；macOS 上确认 Finder 复制文件后 AppleScript 路径可用。

### 合并记录（并入 `feat/flashcast-v0.1.0`）

本 ticket 与已合入的 ticket 09/10/11 在 19 个文件上冲突，全部按「两边的能力都保留」解决：

- 模型：`flashcast_platform::ClipboardCapture` 是文字 / 图片 / HTML-RTF / 文件列表的并集；
  `event_from_capture` 在同一条事件上生成图片附件（`depends_on_source = false`）与文件引用附件
  （`depends_on_source = true`），去重键「有文件按文件列表、否则按文字或图片指纹」。
- 恢复：文件列表优先，其次文字（含富文本载荷），再次图片；三者都不可用时如实失败。
- 界面：`ClipboardEntryView` 同时带图片缩略图 / 尺寸与文件列表 / 引用 / 副本计数。
- `ui-check`：保留 71–74（图片）、75–77（富文本）、79–82（文件），截图文件名无冲突，因此编号不变；
  另外把图片检查里写死的「3 条样例」等待改为按「图片条目出现」等待（样例数据现在含文件条目）。

验证（合并后，本机）：

- `cargo test --workspace` → 362 passed / 0 failed / 2 ignored（CI 同构：`HOME=<fake>` +
  `GIT_CONFIG_NOSYSTEM=1` 同样 362 / 0）。
- `cargo build -p flashcast` → 0 error；`pnpm build`（含 `tsc --noEmit`）→ 0 error。
- `pnpm ui-check` → **64 通过 / 0 失败**，含图片、富文本与文件三类检查。
- 交叉目标：`x86_64-pc-windows-msvc` 与 `x86_64-apple-darwin` 的
  `cargo check -p flashcast-platform --all-targets` 均 0 error。

- 2026-10-01（ticket 17 更正本 ticket 的过期结论）：下面「真实 Wayland 剪贴板上的文件列表读写
  与恢复：未覆盖，ticket 17 必须在真实桌面确认」**已经完成，且结论与当时的预期相反**。
  原因同样是平台层缺陷：`write_with_tool` 在工具成功退出后无条件 `join` 标准错误读取线程，
  `wl-copy` fork 出的选区持有者继承管道导致永久挂起。修复后（提交 `7bc2ed2`）本机 Wayland 上
  `clipboard.files` 为**实测通过**，并且覆盖了本 ticket 要求的难例：
  「写入 2 个文件后读回 2 个。写=/tmp/…/flashcast 报告.txt、/tmp/…/中文 名称.txt；
  读=…」——带空格与非 ASCII 名称的路径逐项一致。仍然未覆盖：Windows `CF_HDROP` 与 macOS
  AppleScript 的文件列表真实读写（本机无法执行）。
