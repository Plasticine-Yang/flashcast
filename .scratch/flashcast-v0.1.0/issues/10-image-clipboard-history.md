# 10: 保存、预览并恢复图片剪贴板历史

Status: done
Category: enhancement

**What to build:** 用户复制图片后能从历史中看到缩略图和尺寸，查看完整预览，并在重启或来源消失后重新粘贴。

Blocked by: 09

- [x] 三个平台分别实现受支持图片格式的捕获与恢复，格式支持及环境限制准确记录。
- [x] 图片数据保存在本机附件存储，不仅保留源引用；重启和来源消失后仍可恢复。
- [x] 列表有缩略图、类型与尺寸，预览按需展开，不阻塞文字结果操作。
- [x] 图片去重与容量限制生效；删除、清空与过期释放不再引用的附件。
- [x] 大图片、解码失败或无法恢复的格式显示准确状态，不生成看似成功的空历史。
- [x] 经宿主入口检查附件持久化和清理；真实平台检查验证恢复的图片内容，UI 用手动或浏览器交互验证。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

不包含 OCR 或图片语义搜索。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（实现完成，Status: done）：图片剪贴板历史全链路交付，分支
  `ticket/10-image-clipboard-history`。前一位 agent 在编辑界面时中断，本次先合并集成分支、
  再补齐外壳 / 界面 / ui-check / 真实平台检查并做完整验证。

### 交付内容

数据模型沿用 ticket 09 的完整 schema（本 ticket 只增加行，**没有迁移**）：

| 内容 | 存储位置 |
| --- | --- |
| 稳定事件 id、时间、去重键、摘要、格式集合、来源、置顶、重复次数 | `clipboard_events` / `clipboard_formats` |
| 图片元数据（mime / width / height / bytes） | `clipboard_formats`（kind = `image`） |
| 图片字节（**本机副本**，`depends_on_source = false`） | `<设备目录>/clipboard/attachments/<att-id>.png` + `clipboard_attachments` |

- 平台层（`crates/flashcast-platform/src/clipboard.rs` 与三平台实现）：
  - Linux：`wl-paste` / `xclip -t image/png -o` 按 MIME 读，`wl-copy -t image/png` /
    `xclip -t image/png -in` 写（`xsel` 不支持按目标选择格式，图片路径上不回退到它，
    如实报工具缺失）；读取与写入都有界（3s / 5s）。
  - Windows：一次打开剪贴板里读注册格式 `PNG`，失败回退 `CF_DIB`；写入时同时提供
    `CF_DIB`（由 PNG 转换）与注册格式 `PNG`，转换失败在写之前就失败，不留半个剪贴板。
  - macOS：`osascript` + 临时文件以 `«class PNGf»` 读 / 写剪贴板图片，临时文件成功或
    失败后都删除。
  - 尺寸从文件头解析（PNG / JPEG / GIF / BMP），解析不出如实为「尺寸未知」。
- 去重与容量（`crates/flashcast-core/src/clipboard.rs`）：`content_hash` 在文字与图片之间
  不共通（文字用文本指纹、图片用字节指纹），跨类型不会误判成同一条；容量淘汰、保留期限
  与置顶豁免在入库后回收不再引用的附件。
- 宿主恢复（`crates/flashcast-core/src/host.rs`）：`execute_clipboard_entry` 对
  `text = None` 的图片条目改为读本机附件 → `write_image` → 走 ticket 08 的
  「复制 → 关窗 → 恢复目标 → 核对前台 → 注入粘贴」；自动粘贴不可用时降级为
  「已复制，请手动粘贴」。附件被删掉时如实失败（不再报「后续版本提供」）。
- 外壳（`src-tauri/src/commands.rs`、`src-tauri/src/icon.rs`）：`ClipboardEntryView`
  增加 `image_data_url`（96px 缩略图 data URL）与 `image_size`（`PNG 1920×1080`）；
  `ItemView.thumbnail_data_url` 供结果列表用；`preview` 命令把图片附件按需读成
  data URL（超过 16 MB 或读不出来时退回文字说明）。命令仍在同一个 `invoke_handler` 里，
  没有新增需要注册的命令。
- 界面（`src/components/ResultList.tsx` / `MemoPreview.tsx` / `ClipboardPanel.tsx` +
  `src/styles.css`）：结果列表显示 32px 缩略图，副标题给出格式、类型与尺寸；预览区在
  **按需展开**时显示完整图片（`data-kind="image"`），文字条目预览不受影响；设置页的
  历史列表同样显示缩略图与尺寸。无新 CSS 框架，只用既有 `--fc-*` 变量，无动画。

### 验证命令与结果

- `cargo test --workspace` → **344 passed / 0 failed**（合并 ticket 09/11 后的基线 328）。
  命令：`eval "$(scripts/dev/linux-native-deps.sh)" && cargo test --workspace`
- CI 平价：`mkdir -p "$HOME/.cache/flashcast/fakehome" && HOME="$HOME/.cache/flashcast/fakehome" GIT_CONFIG_NOSYSTEM=1 timeout 1500 cargo test --workspace`
  → **344 passed / 0 failed**（不依赖开发机的 `~/.gitconfig`）。
- `cargo build -p flashcast` → 成功（只有既有的 `hotkey::status` 未使用警告）。
- `pnpm install --frozen-lockfile && pnpm build` → 成功（`tsc --noEmit` + `vite build`）。
- `pnpm ui-check` → **60 passed / 0 failed**（基线 53；ticket 11 的 75–77 已在集成分支上，
  本 ticket 新增 4 项 71–74）。新增截图：
  `artifacts/ui/71-clipboard-image-thumbnail.png`、`72-clipboard-image-preview.png`、
  `73-clipboard-image-paste.png`、`74-settings-clipboard-image.png`。
  67–70 因模拟历史多出一条图片条目而同步更新（3 条）。
- 交叉检查（本地唯一的 Windows / macOS 编译守卫）：
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/10-windows timeout 1800 cargo check -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets` → **成功**（0 条本 ticket 相关警告；`RegisterClipboardFormatW` 的导入位置与两个平台限定辅助函数的死代码警告就是这一步发现的）。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/10-macos timeout 1800 cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets` → **成功**（0 警告）。
- 真实平台检查：新增 `clipboard.image` 这一项（只读探测；`--allow-clipboard-write` 时写一张
  3×2 已知像素 PNG 再逐像素读回）。
  - `cargo run -p flashcast-platform --bin flashcast-platform-check -- --json --output …`
    → 实测通过 4 / 实测失败 1 / 未覆盖 8
  - 同上加 `--allow-clipboard-write` → 实测通过 4 / 实测失败 1 / 未覆盖 8
  - 其中 `clipboard.image` **未覆盖**：只读时报「wl-paste 超过 3 秒没有返回」，写入时报
    「wl-copy 超过 5 秒没有返回」；唯一的实测失败是既有的 `paste.auto`（Wayland 不允许
    注入按键），与本 ticket 无关。
- `git merge feat/flashcast-v0.1.0` → 38 处冲突（11 个文件）全部按「两边都保留」解决：
  图片的附件 / 缩略图 / 恢复与 ticket 11 的 HTML/RTF 载荷、格式集合、`write_content`
  降级报告共存；`ClipboardCapture` 同时带 text / image / image_problem / html / rtf。

### 覆盖到的行为（宿主入口，`crates/flashcast-core/tests/clipboard.rs`）

捕获 → 重启 → 列表 / 预览 / 恢复（含完整图片回写与自动粘贴）；附件文件真的存在且数据库
行指向它；**来源消失后仍能恢复**（删掉源文件后附件仍可读回）；去重（同一张图 copies 累加，
且与文字条目用不同指纹、绝不互相误判）；容量淘汰最旧未置顶的图片并回收其附件；容量触顶
如实拒绝；删除 / 清空 / 过期都回收不再引用的附件文件；超大与无法解码的图片如实失败、
不留下空条目；仅有图片的一次事件格式集合 / 时间 / 摘要被填好；首屏仍然不检索历史。
平台层另有纯逻辑单元测试：尺寸解析、缩略图（**不放大**）、PNG→DIB→PNG 逐像素往返、
CF_HTML 偏移。UI 侧另有浏览器交互检查 71–74（真实 Chrome + 模拟宿主）。

### 未覆盖（如实记录）

- **真实 Wayland 剪贴板图片读写与监听：未覆盖。** 本机是 Wayland
  （`XDG_SESSION_TYPE=wayland`）。`clipboard.image` 只读时 `wl-paste` 超时、写入时
  `wl-copy` 超时，与 tickets 08/09/11 的结论一致：Wayland 的选区由持有者进程提供，
  自动化会话没有可用的持有者。**这既不能推断真实桌面上图片复制会失败，也不能推断可用**。
- **Windows / macOS 真实图片读写：未覆盖。** 本机无法执行；只做了交叉编译检查。
  Windows 的注册格式 `PNG` + `CF_DIB` 回退、macOS 的 `osascript` 读写都未在真机跑过。
- **X11 下的图片读写：未覆盖。** 代码路径是 `xclip -t image/png`，本机 Wayland 会话下走不到。
- **Linux 只探测 `image/png`：已知格式边界。** 剪贴板里只有 JPEG / GIF 等非 PNG 图片、
  且没有文字时，这一轮会被当成「没有可保存内容」——不生成条目，也没有失败状态。选择这
  一行为而不是逐个猜测 MIME：每多探一种类型就多一次有界子进程调用，而浏览器与截图工具
  的图片复制通常同时提供 PNG。此项需要真实桌面复核是否需要扩展（见下）。
- **去重在真实适配层上的表现：未覆盖。** Linux / macOS 用内容指纹判断变化，同一张图片被
  复制两次只算一次变化；「去重累加 copies」由宿主的 content_hash 保证并用替身覆盖。
- **UI 未做真机核对。** `pnpm ui-check` 只驱动浏览器里的模拟宿主，不代表 Tauri 托盘、
  全局快捷键、自动粘贴或真实应用粘贴图片可用。

### 需要 ticket 17 在真实桌面确认

1. Wayland 交互会话下：复制一张图片（浏览器 / 截图工具）→ 历史里出现缩略图与尺寸 →
   重启后仍能预览 → 回车把图片粘回目标应用。
2. X11 会话下同一流程，并确认 `xclip -t image/png` 这条路径（含只有 JPEG 的复制）。
3. Windows：注册格式 `PNG` 与 `CF_DIB` 两条读取路径各都能恢复；旧应用（只认 `CF_DIB`）
   粘贴得到正确像素。
4. macOS：`osascript` 读写图片成功，临时文件在成功与失败后都不残留。
5. 超大图片（> 16 MB）与损坏图片在真实平台上给出中文字段级原因，且不留空条目。
