# 11: 保留并恢复富文本的多种剪贴板格式

Status: done
Category: enhancement

**What to build:** 用户复制富文本后，历史将同一次复制的纯文本与受支持的 HTML/RTF 格式关联保存，恢复时让目标应用选择合适格式。

Blocked by: 09

- [x] 同一复制事件的文本与受支持富文本格式保存为同一条历史，不能拆成相互重复的记录。
- [x] 按平台公开格式恢复 HTML/RTF 与文本，目标支持富文本时尽量保留样式，文本目标可接收纯文本。
- [x] 搜索使用可索引文本，预览安全呈现内容，不执行剪贴板提供的脚本或远端活动内容。
- [x] 平台或格式不支持时标明实际保存的格式及降级方式，不声称已完整保留任意应用私有格式。
- [x] 从宿主入口验证格式集合、持久化和恢复操作；真实系统检查与测试替身证据分别记录。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

这是文字历史的独立扩展，不需要先完成图片或文件历史。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（实现完成，Status: done）：富文本剪贴板格式全链路交付，分支 `ticket/11-richtext-clipboard-history`。

### 交付内容

**数据模型与存储**（`crates/flashcast-core/src/clipboard.rs`）——**没有 schema 迁移**（ticket 09 已备好
`clipboard_payloads`）：

- `event_from_capture` 由一次 `ClipboardCapture` 生成**一条** `ClipboardEvent`：文本进 `text_content`，
  HTML/RTF 原文进 `clipboard_payloads`（role `html` / `rtf`，mime `text/html` / `text/rtf`），
  格式集合进 `clipboard_formats`，字节数取自真实载荷。只记**真的有内容**的格式：空载荷不会
  凭空产生一种格式。
- `content_hash` **仍是纯文本 FNV-1a 指纹**（`content_hash_text`），纯文本条目的去重语义完全不变。
- 去重命中且已有条目**还没有任何载荷**时，把后一次复制带来的 HTML/RTF 补进这条已有条目
  （`enrich_payloads`）：先复制纯文本、后复制同一段文字的富文本版本时不会丢格式，也不会
  产生重复记录；先到的富文本版本不会被覆盖。
- 检索只看 `text_content` / `summary` / 来源列；插件的匹配元数据只有可索引文字、来源与格式名，
  **不含载荷原文**。

**平台层**（`crates/flashcast-platform/src/clipboard.rs` 及三个平台实现）：

| 平台 | 读（一次读全） | 写（一次写全） | 结论 |
| --- | --- | --- | --- |
| Windows | `CF_UNICODETEXT` + `HTML Format`(CF_HTML) + `Rich Text Format`，**一次** `OpenClipboard` 内读完 | 同一剪贴板打开里 `EmptyClipboard` 后依次写入三种格式 | 唯一三种格式齐全 |
| Linux | `wl-paste --type text/html` / `xclip -selection clipboard -t text/rtf -o`（`xsel` 无按类型读取） | 只写纯文本 | `wl-copy` 一次只能提供一种 MIME 类型，写 HTML 会顶掉文本 → 如实降级 |
| macOS | 只读纯文本 | 只写纯文本 | `pbcopy` 没有富文本写入口；`pbpaste -Prefer rtf` 无法区分「真 RTF」与退回内容 |

- 新增 `ClipboardContent`（文本 + 可选 HTML/RTF）与 `ClipboardWriteReport`（**实际提供了哪些
  格式** + 未提供的格式及原因）+ `ClipboardSkippedFormat`；trait 上的 `write_content` 默认实现
  退化为「只写文本 + 如实报告富文本未写入」，因此没有重写的平台不会假装支持。
- `ClipboardCapture` 增加 `html` / `rtf` 字段与 `ClipboardCapture::rich()`；`rich_formats()`
  用于如实推导格式集合。
- CF_HTML 头部编解码（`cf_html_bytes` / `cf_html_fragment`）是**纯逻辑**，放在跨平台模块里，
  因此 Windows 专属代码也有 Linux 上的真实字节单元测试（3 条）。
- fake 替身：`FakeClipboardWatcher::set_rich`（同一次事件）、`FakeClipboard` 记录每次写入的
  **完整内容**与返回的报告，`FakeClipboard::text_only(reason)` 模拟只能提供纯文本的平台。

**宿主**（`crates/flashcast-core/src/host.rs`）：

- `execute_clipboard_entry` 用 `clipboard_content_for(event, text)` 把这条历史的文本 + HTML/RTF
  一起交给平台（`write_content`），并登记自身写入（按纯文本指纹，两层抑制各自成立）。
- `ClipboardWriteReport` 降级时，结果反馈（`PastePlan.formats_note`）写明**实际提供了哪些格式、
  哪些没有以及原因**；不降级时不打扰用户。`complete_paste` 的成功 / 手动粘贴反馈同样带上它。
- 新增 `Host::finish_copy_for_paste_with_note`；`finish_copy_for_paste` 委托给它（备忘录路径不变）。

**预览安全**：选择**渲染成惰性纯文本**，而不是「先消毒再以 HTML 插入」。预览只取
`ClipboardEvent::text`，用 `Preview::Text` 交给 UI 渲染成 React 文本节点；HTML/RTF 载荷既不进
`SearchItem`，也不下发给界面（`src-tauri` 的 `ClipboardEntryView` 只给 `text` 与格式名），
模拟宿主在 `clipboardStateView` 里同样剥掉载荷。理由：消毒器的绕过面就是攻击面，而纯文本渲染在
结构上不可能执行剪贴板提供的内容，也更容易被断言。

**界面**：`PastePlan` 增加可选 `formatsNote`（`src/types.ts`）；模拟宿主的历史里有一条带
HTML/RTF 的条目（载荷含 `<script>` / 远端 `<img onerror>`），但不下发给界面。

### 验证命令与结果

**测试替身证据（宿主入口，`crates/flashcast-core/tests/clipboard.rs`，30 条，新增 8 条）**

- 一次携带 HTML 的复制 → **恰好一条**历史，格式集合（文字 + HTML，字节数来自真实载荷）、
  载荷、可索引文字都在这一条上；重复轮询不再产生条目；**重启后**（同设备目录 + 同工作区）
  格式集合与载荷原样还在；结果副标题显示「文字 · HTML」。
- HTML + RTF 一起到达同样只有一条历史，两种载荷都保存；空载荷不产生格式。
- 检索只用可索引文字：「可见的正文」命中 1 条，`HTML_ONLY_MARKER` / `alert(1)` / `b>` 均不命中
  （插件范围与 `clipboard_entries(Some(...))` 两侧都验证）。
- 恢复：断言**真的写进剪贴板的载荷集合**（`text` + `html` + `rtf` 三者都对）+ 报告格式为
  `[Text, Html, Rtf]`、无跳过项、无降级说明；随后 `complete_paste` 注入成功，自身写入被登记，
  反复轮询不形成自身写入循环。
- 文本目标仍拿到纯文本：`text_only` 平台下 `last_write() == 纯文本`，报告只含 Text、
  未提供项为 `html`/`rtf`，`formats_note` 与完成反馈都写明「未提供」与原因。
- 纯文本条目恢复不带降级说明，只写文本。
- 预览惰性：预览正文是可索引纯文本（不含 `<`），宿主 `preview` 入口同样如此；把整条
  `SearchItem` 序列化后不含 `script` / `alert` / `onerror` / `example.invalid` / `<img`；
  而载荷本身仍完整保存在历史里（安全 ≠ 丢弃）。
- 去重语义不变：同一纯文本复制两次仍是一条；先纯文本后富文本合并到同一条并补入载荷；
  之后的纯文本复制不会抹掉已保存的载荷；`content_hash == content_hash_text(text)`。

命令与结果：

- `cargo test --workspace` → **328 passed / 0 failed**（基线 317，新增 11：8 条宿主集成 +
  3 条 CF_HTML 单元测试）。
- **CI 平价（强制）**：`mkdir -p "$HOME/.cache/flashcast/fakehome" && HOME="$HOME/.cache/flashcast/fakehome" GIT_CONFIG_NOSYSTEM=1 timeout 1500 cargo test --workspace`
  → **328 passed / 0 failed**（不依赖开发机的 `~/.gitconfig`）。
- `cargo build -p flashcast` → 成功，**没有新增警告**（只剩既有的 `hotkey::status` dead_code）。
- `pnpm install --frozen-lockfile && pnpm build` → 成功（`tsc --noEmit` + `vite build`）。
- `pnpm ui-check` → **56 passed / 0 failed**（基线 53，新增 3 项：75–77）。
  截图：`artifacts/ui/75-clipboard-richtext-scope.png`、`76-clipboard-richtext-preview.png`、
  `77-clipboard-richtext-paste.png`（`artifacts/` 已被 gitignore）。
  75 校验格式集合可见且富文本条目只有一条；76 校验预览是惰性纯文本、预览与结果列表 DOM 里
  没有 `<script>` / `<img>` / `onerror` / 远端地址；77 校验富文本条目粘贴时提供的是纯文本。
- 交叉检查（本地唯一的 Windows / macOS 编译守卫，`cfg(target_os = "windows")` 与 macOS 代码
  在 Linux 上不参与 `cargo test` 编译）：
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/11-windows timeout 1800 cargo check -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets` → 成功。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/11-macos timeout 1800 cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets` → 成功。
- 真实平台检查：`flashcast-platform-check --json` 与同一命令加 `--allow-clipboard-write`。
  两次总览都是 **实测通过 4 / 实测失败 1 / 未覆盖 7**（ticket 09 的未覆盖 6，新增
  `clipboard.rich` 一项）。实测失败仍然只有既有的 `paste.auto`（Wayland 不允许注入按键）。
  `clipboard.rich` 两次都是**未覆盖**，原因见下。
- `git merge feat/flashcast-v0.1.0` → **「已经是最新的」**（集成分支仍在本 ticket 的基线
  5dcc3a9 上），**没有冲突**可解。截图从 75 起编号（ticket 10 用 71+，未冲突）。

### 未覆盖（如实记录）

- **真实 Wayland 剪贴板上的富文本读取与监听：未覆盖。** 本机是 Wayland，自动化会话没有可用的
  选区持有者：`clipboard.rich`（只读）报「读取剪贴板失败：剪贴板工具 `/usr/bin/wl-paste`
  超过 3 秒没有返回」，加 `--allow-clipboard-write` 后报「`wl-copy` 超过 5 秒没有返回」。
  与 ticket 08/09 的结论一致：这既不能推断真实桌面上的富文本复制会失败，也不能推断可用，
  需要在有交互桌面的会话里手动复核。
- **Linux 的写回降级只有代码与文档证据，没有实测通过。** `wl-copy 2.2.1` 的 man page 写明
  `-t` 决定「wl-copy 提供内容的类型」（单数），再调用一次会接管选区；本机没有 `xclip`
  可对照。因此「恢复时只提供纯文本」这一降级结论**未在真实选区上实测**，需要在真实桌面上
  用 `--allow-clipboard-write` 复核 `ClipboardWriteReport` 是否与预期一致。
- **Windows 的 CF_UNICODETEXT + HTML Format + Rich Text Format 读写：未覆盖（真实执行）。**
  本机无法执行，只做了交叉编译检查。CF_HTML 头部编解码本身有 Linux 上的真实字节单元测试
  （偏移自洽、空片段、换行片段、非 CF_HTML 载荷），但 `RegisterClipboardFormatW`、
  `GlobalSize`、`SetClipboardData` 的实机行为未验证；`flashcast-platform-check` 在 Windows 上
  如实报「未覆盖」。
- **macOS 的富文本能力未在真机确认。** 代码只提供纯文本，并在平台检查里如实报「平台不支持」；
  「`pbpaste` 是否可能在别的 macOS 版本上可靠地给出 RTF」未调查。
- **真实桌面的端到端富文本粘贴：未覆盖。** 目标应用（Word / 浏览器富文本域 / 终端）到底拿到
  哪一份格式，需要在真机上手动确认；UI 自动化只驱动浏览器里的模拟宿主。
- **私有格式（应用自定义的 clipboard format）从未尝试保存或恢复**，这是设计决定而非缺口：
  只保存并恢复平台公开的文本 / HTML / RTF 三种格式。

### ticket 17 需要在真实桌面上确认的事

1. 交互式 Wayland / X11 桌面上复制一段富文本 → 历史里**只有一条**记录、副标题同时标出文字与
   HTML（若来源提供 RTF 则还有 RTF）。
2. 把该条粘贴到富文本目标（Word / 富文本编辑器）是否保留样式，以及 Linux 上是否如预期只拿到
   纯文本（`ClipboardWriteReport` 的降级说明是否与实际一致）。
3. 把该条粘贴到纯文本目标（终端 / 纯文本编辑器）是否至少拿到纯文本。
4. Windows 真机上 `CF_HTML` / `Rich Text Format` 是否被 Word / 写字板正确识别（这是本 ticket
   唯一声称「一次提供多种格式」的平台）。
5. 复制一段含 `<script>` / 远端图片的富文本后，预览区不执行脚本、不请求远端资源。
