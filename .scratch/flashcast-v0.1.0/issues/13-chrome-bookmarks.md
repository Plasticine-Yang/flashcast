# 13: 关联 Chrome profile、搜索书签并在 Chrome 打开

Status: done
Category: enhancement

**What to build:** 用户选择本机 Chrome profile，输入中文或英文关键词进入书签搜索，按标题、网址或目录找到书签，回车在选定 Chrome profile 打开。

Blocked by: 07

- [x] 作为官方功能插件加载，支持 chrome bookmarks 和 chrome 书签两个入口。
- [x] 各平台允许发现或选择 Chrome 及 profile，本机路径留在本机，关联状态可查看与修改。
- [x] 读取本地书签并建立可重建索引，按标题、网址和目录搜索；变化后刷新，重启仍可重新关联与检索。
- [x] 默认操作明确为在 Chrome 打开，正确指定关联 profile；网址和路径不经不安全的 shell 拼接。
- [x] Chrome 缺失、profile 不可读、文件损坏或启动失败都有可操作反馈，不破坏原书签。
- [x] 经宿主入口与样本 profile 验证搜索、刷新和启动参数；可运行环境补充真实 Chrome 打开检查，报告覆盖范围。
- [x] 手动或浏览器交互检查插件界面；适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

不依赖配套 Chrome 扩展，不提供书签编辑。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（实现完成）：分支 `ticket/13-chrome-bookmarks`，基于 `feat/flashcast-v0.1.0`
  `0dd7c3d`，7 个提交：`254d37e`（平台层 Chrome 发现与 argv 构造）、`6a639e3`（书签索引、
  官方插件与宿主入口）、`f4aed61`（宿主入口集成测试）、`dd9c869`（Tauri 命令与界面）、
  `b7fed16`（浏览器交互检查）、`72592f5`（Windows 目标缺再导出的修复）、`d7339a2`
  （真实 `Local State` 字段类型修复 + 两个默认忽略的真实机器检查）。设计要点：

  - **平台层只做发现与启动**（ADR §5 的 `ChromeProvider`）：跨平台 `chrome.rs` 提供
    `ChromeEnvironment`/`ChromeProfile`/`ChromeLaunchRequest`、argv 构造、URL 校验、
    `--user-data-dir` 判定与 `spawn_chrome`；Linux（deb / snap / flatpak / `$XDG_CONFIG_HOME`）、
    Windows（Program Files、Program Files (x86)、每用户安装）、macOS（`/Applications` 与
    `~/Applications`）的候选路径是**纯函数**，因此三个平台的布局都能在 Linux 上被测试。
  - **不经 shell**：`build_open_args` 产出一个 argv 数组，`--profile-directory=<目录名>`、
    `--no-first-run`、`--no-default-browser-check`、可选的 `--user-data-dir=<路径>`，URL 永远是
    最后一个独立元素。`--user-data-dir` 只在**默认位置之外**时传：平台默认目录 Chrome 自己
    会用，snap / flatpak 的目录由各自启动包装器映射，再传一遍可能起第二个浏览器进程。
  - **启动只 `spawn`**，立即返回；用一个短命线程回收进程表项并**丢弃退出码**（Chrome 已在
    运行时新进程会交接给现有进程并几乎立刻退出，退出码 0 不能证明页面打开）。反馈文案
    明确写「已请求 Chrome…无法据此确认页面是否已加载」。
  - **索引可重建且内存化**：`BookmarkIndex` 保存文件指纹（mtime + size）与解析结果，文件是
    唯一事实来源，进程重启后按设备本地记录的关联重新读取即可重建。**没有**落盘/没有用
    SQLite：书签文件本身很小、就在本机，多一份副本只会多一处敏感 URL 的泄露面（研究笔记
    §5）；ticket 09 的剪贴板索引仍按 ADR §8 用 SQLite。
  - **解析宽松到底**：`checksum`/`checksum_sha256` 永不校验，三个根都可选，未知根与未知字段
    都容忍，`version != 1` 照常解析，节点缺 `id`/`name` 也能收进来；`Bookmarks` 缺失是**正常
    空状态**。解析失败按「可能读到写入中途的半个文件」处理：等 500ms 重读一次，仍然失败才
    报损坏，并**保留上一次可用的索引**；`daily`/搜索路径用不重试的版本，避免把 500ms 带进
    有超时的插件搜索线程。
  - **权限与校验在原生边界**：来源插件必须在清单里、已启用、声明了 `chrome.open`，否则拒绝；
    打开前先校验 profile 目录**确实存在**（Chrome 对不存在的 `--profile-directory` 会静默新建
    空 profile，本机已实测），再重读书签文件、找到条目、校验 URL 只允许 http/https 且无控制
    字符，最后才交 argv 给 Chrome。关联只接受**发现结果里的目录名**，调用方给不了任意路径。
  - **本机路径留在本机**：关联（目录名、显示名、用户数据目录、可执行文件、是否要传
    `--user-data-dir`）写在设备本地 `device-local.json` 的 `chrome.association` 键下，重启后恢复；
    集成测试断言配置工作区里不出现这些本机路径。
  - **`Local State` 只读 `profile.info_cache`**，绝不读取 `os_crypt.encrypted_key`；文件损坏时
    只用目录枚举（含 `Preferences` 或 `Bookmarks` 的子目录）并给出警告，不报致命错误。

- 2026-10-01：实测命令与结果。

  - `timeout 1500 cargo test --workspace` → **EXIT=0，263 passed / 0 failed**（ticket 起点为
    218 passed；本 ticket 新增：`flashcast-core/tests/chrome.rs` 22 个用例、
    `flashcast-platform/tests/chrome_fixture.rs` 6 个用例 + 2 个 `#[ignore]`、
    `flashcast-core/src/chrome.rs` 4 个单元测试、`flashcast-platform` 各 6 个单元测试）。
    逐二进制关键项：core lib 4、chrome.rs 22、platform lib **54**、chrome_fixture 6（2 ignored）。
  - CI 对齐：`mkdir -p "$HOME/.cache/flashcast/fakehome" && HOME="$HOME/.cache/flashcast/fakehome"
    GIT_CONFIG_NOSYSTEM=1 timeout 1500 cargo test --workspace` → **EXIT=0，263 passed / 0 failed**。
    （第一次跑时把三条 cargo 任务并行启动、各自 `eval` 了同一个原生依赖前缀脚本，脚本互相
    踩到 `ln: Already exists`，其中一条以 `timeout: failed to execute process` 失败；改为串行后
    两次都通过。原生依赖脚本不要并发执行。）
  - `timeout 900 cargo build -p flashcast` → EXIT=0（仅剩并入集成分支带来的
    `hotkey::status` 未使用告警，不在本 ticket 的文件里）。
  - `timeout 600 pnpm install --frozen-lockfile` → 0；`timeout 600 pnpm build`
    （`tsc --noEmit && vite build`）→ 0，无类型错误。
  - `timeout 900 pnpm ui-check` → **通过 46，失败 0**（合并前 39 项，本 ticket 新增 7 项）；
    日志 `artifacts/ui/ui-check.log`，截图（`artifacts/ui/` 已被 gitignore）：
    `53-chrome-scope.png`、`54-chrome-open.png`、`55-settings-chrome-profiles.png`、
    `56-settings-chrome-associated.png`、`57-settings-chrome-corrupt.png`、
    `58-settings-chrome-unavailable.png`、`59-chrome-refreshed.png`、
    `60-settings-chrome-disabled.png`。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/13-windows timeout 1800 cargo check
    -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets` → **EXIT=0**。
    ⚠ 第一次是**失败**的：`windows/mod.rs` 里漏了
    `#[cfg(target_os = "windows")] pub use chrome::WindowsChromeProvider;`，而 `cargo test`
    在 Linux 上根本不编译 `cfg(windows)` 的代码。修复见 `72592f5`。
  - 同命令（`x86_64-apple-darwin`，目录 `13-macos`）→ **EXIT=0**。

- 2026-10-01：夹具/替身证据（与真实 Chrome 证据分开记录）。

  - 全部经宿主入口（`Host::query` / `Host::execute` / `Host::chrome_state` /
    `Host::associate_chrome_profile`），夹具是测试自己写的临时 Chrome profile（假的
    `google-chrome` 可执行文件、`Local State` 的 `profile.info_cache`、两份 `Bookmarks`
    样本）。**发现走真实实现**（存在性检查 + 真实 JSON 解析），只有进程启动被替换成记录
    argv 的替身 `FakeChrome`。
  - 覆盖：清单加载与两个关键词别名；首屏不检索书签；按标题 / 网址 / 目录检索（含非 ASCII
    查询串）；排序层级（标题匹配优先于只有网址匹配）；预览给完整链接与目录；文件变化后
    刷新；解析失败重读一次即恢复（夹具在 150ms 后补写完整文件）；损坏报告（断言耗时
    ≥450ms 证明重读、保留上一次索引、范围搜索把原因带回）；文件不可读（用同名目录顶替，
    与 uid 无关地触发 EISDIR）；`Bookmarks` 缺失是空状态不是错误；profile 目录不存在、
    关联不存在的目录、Chrome 未安装、Chrome 启动失败、`javascript:` 链接被拒；
    停用插件后不检索也不打开；缺少 `chrome.open` 的能力缺失与伪造来源被拒；
    **完整 argv 断言**（目录名带空格的 `Profile 1`、URL 带 `&`/查询串/非 ASCII 的
    `https://intranet.example.com/login?token=abc&next=首页`、默认目录**不传**
    `--user-data-dir`、默认位置之外**要传** `--user-data-dir=<路径>`）；
    重启后重新关联并检索；换关联后书签集合跟着换；本机路径不进入工作区。

- 2026-10-01：真实 Chrome / 真实机器的证据（区分「参数向量已断言」「进程已启动」「页面真的
  打开」三件事）。

  - **真实发现（只读，`--ignored` 用例 `real_machine_discovery`）**：
    `cargo test -p flashcast-platform --test chrome_fixture real_machine_discovery -- --ignored
    --nocapture` → 品牌 Chrome、可执行文件 `/usr/bin/google-chrome`、用户数据目录
    `~/.config/google-chrome`（Default 来源，不需要 `--user-data-dir`）、1 个 profile
    「Default」显示名「文锋」、账号 `ywf975036719@gmail.com`、非企业托管、**没有 Bookmarks
    文件（正常空状态）**。只读 `profile.info_cache`，未读取 `os_crypt`。
  - **参数向量已断言**：替身断言（上一条）+ 浏览器交互检查断言
    `["--profile-directory=Profile 1","--no-first-run","--no-default-browser-check",
    "https://intranet.example.com/login?token=abc&next=首页"]`。
  - **真实 Chrome 进程 + 页面真的加载（headless，一次性 /tmp user-data-dir）**：
    本地 `python3 -m http.server 8791` 提供带标记的页面，然后用应用**同一组开关**运行真实
    Chrome（额外加 `--headless=new --dump-dom` 以避免在桌面弹窗）：
    ① `--profile-directory=Default` → EXIT=0，`--dump-dom` 输出含 `FLASHCAST_CHROME_CHECK_OK`；
    ② 目录名带空格的 `--profile-directory="Profile 1"` → EXIT=0，同样含标记。
    即：真实 Chrome 接受了这组参数、用了指定的用户数据目录、并真的把本地页面加载渲染出来。
  - **产品代码路径的真实启动（`--ignored` 用例 `real_chrome_spawn_starts_a_process`）**：
    调用 `chrome::spawn_chrome`（spawn 后立即返回、不等待、不看退出码），argv 为
    `["/usr/bin/google-chrome","--profile-directory=Default","--no-first-run",
    "--no-default-browser-check","--user-data-dir=/tmp/…/udd","--headless=new","--disable-gpu",
    "about:blank"]`；一次性 user-data-dir 里随后出现了真实 Chrome 写出的 `Local State`，
    证明进程确实带着这些参数跑起来了。**这只证明「进程已启动」**。
  - **实证「Chrome 会为不存在的 profile 目录静默新建空 profile」**：在全新的
    `--user-data-dir` 下用 `--profile-directory="Profile 42"` 运行 → Chrome 创建了
    `Profile 42` 目录。这就是宿主必须在启动前校验 profile 目录存在的原因。
  - **实证「Chrome 自己会重写 Bookmarks」**：真实 Chrome 加载夹具 profile 后把 `Bookmarks`
    重写（丢掉手写的 `checksum`，补上 `guid`/`date_added`/真实校验和），并新建了
    `Bookmarks.bak` 与 `AccountBookmarks`。研究笔记称 Chrome 153 没有 `Bookmarks.bak`，
    实测**有**（Chrome 自己重写时生成）。因此：`checksum` 永不校验是对的；Flashcast 只读、
    绝不写入是对的（写会与 Chrome 抢同一个文件）；宿主对「文件被 Chrome 重写」是安全的
    （指纹变化 → 重读 → 新内容照常解析）。
  - 真实机器检查暴露并修掉的两个真实缺陷（`d7339a2`）：真实 Chrome 153 把
    `profile.info_cache[*].is_managed` 写成**整数** `0`（不是布尔），原来的 `Option<bool>`
    让整份 `Local State` 解析失败、显示名全部退化成目录名；未托管时 `hosted_domain` 的字面量
    是 `"NO_HOSTED_DOMAIN"`，原来的「非空即受管理」会把所有 profile 都标成企业管理。现在
    标记按布尔/整数/字符串宽松解析，`NO_HOSTED_DOMAIN` 视为没有企业域，并补了单元测试与
    夹具形状。修复后真实发现如上读到「文锋」与真实账号。

- 2026-10-01：一次**必须记录在案的失误**。`real_chrome_spawn_starts_a_process` 第一次写的时候
  给 `build_open_args` 传了 `None`（「默认目录不传 `--user-data-dir`」在产品里是对的），于是
  被启动的真实 Chrome 用了**用户真实的 `~/.config/google-chrome`**。该用例当时因为一次性目录里
  没有出现 `Local State` 而失败，随即改成显式传 `Some(&udd)` 并加注释说明「测试绝不能用用户
  真实 profile」。事后只读复核：真实 profile 的显示名「文锋」、账号、`is_managed` 都还在
  （该 profile 本来就没有 `Bookmarks` 文件，因此没有书签可丢）。教训：凡是会真的启动 Chrome 的
  检查，必须显式指定一次性 `--user-data-dir`，不能依赖「默认位置」的语义。

- 2026-10-01：未覆盖项与原因（严格如实）。

  - **Windows / macOS 的 Chrome 发现与启动**：只做了
    `cargo check -p flashcast-platform --target …--all-targets`（类型正确性）与纯路径推导的
    单元测试（在 Linux 上跑），**没有**在 Windows / macOS 主机上执行过真实发现或启动；注册表
    `App Paths\chrome.exe` 查询（研究笔记称其为 canonical）**未实现**，只覆盖研究笔记列出的
    文件安装位置。
  - **桌面可见窗口**：真实 Chrome 检查全部是 `--headless=new`。**没有**在真实桌面会话里用
    产品路径弹出一个可见窗口并肉眼确认页面打开——按 ticket 要求刻意避免；「页面真的打开」
    的证据仅限 headless 的 `--dump-dom` 输出。
  - **Chromium / Edge**：候选路径与单元测试都有，但没有在装有 Chromium / Edge 的机器上跑过
    真实发现或启动。
  - **索引刷新机制**：按 mtime + size 在**宿主入口被调用时**检查（`chrome_state`、范围搜索、
    打开前），没有后台定时轮询线程；UI 只在设置页打开时读取一次状态，搜索范围里靠每次查询
    触发。研究笔记建议的 2–5s / 15–30s 轮询与失焦暂停未实现（v0.1.0 用入口触发已足够，代价
    是「停留在列表里不动」时新书签不会自动出现）。
  - **索引不是 SQLite**：见上面的设计说明（内存索引 + 文件指纹）。因此「索引可以重建」成立，
    但没有任何本机索引文件；若后续要求跨进程复用大索引，需要重新评估。
  - **`spawn_chrome` 的进程回收**：只断言了「启动成功 + 进程真的用了该 user-data-dir」，
    没有验证僵尸进程回收线程在各平台的实际行为，也没有验证「Chrome 已在运行时新进程立刻
    退出」这一路径（本机检查时 Chrome 未在运行）。
  - **多 profile 并发 / Chrome 正在运行时的行为**：没有验证「Chrome 正在运行时打开链接会
    交接给现有进程」以及「同一 user-data-dir 下两个 profile 的状态」；相关行为只按研究笔记
    的结论处理（不等待、不看退出码）。
  - **企业策略 / 托管 profile 的打开被拦截**：只在 `Local State` 解析与 UI 标记上覆盖
    （`is_managed` / `hosted_domain`），没有在受策略管理的真实 profile 上验证打开行为。
  - **未来加密书签库**（`EncryptedBookmarks`）：未实现任何检测；路径解析与解析是分开的、
    可报告的步骤，出现时只会表现为「解析失败」而不是明确的「不支持的 Chrome 版本」。
  - **浏览器交互检查的边界**：`tools/ui-check` 只覆盖浏览器里的 React UI 与模拟宿主，不是
    Tauri webview；真实 Tauri 窗口下的设置页、结果图标与操作栏反馈没有在桌面会话里点过。
