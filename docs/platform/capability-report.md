# Flashcast v0.1.0 平台能力报告

本文件由 `scripts/ci/capability-report.py` 从 `docs/platform/capability-report.meta.json` 与 `docs/platform/evidence/*` 生成；机器可读版本是同目录的 `capability-report.json`（含每一项的完整原因与复现命令）。

- 生成时间：2026-10-01T21:05:00Z
- CI 证据：`feat/flashcast-v0.1.0` @ `6a2c9466`，[运行 36861987134](https://github.com/Plasticine-Yang/flashcast/actions/runs/36861987134)，11 个任务全部 success

- 编译成功**不是**行为证据：`cargo build` / `cargo test` / 交叉 `cargo check` 通过只说明代码能编译，不说明任何桌面行为可用。
- 测试替身通过**不**证明平台适配通过：替身检查与真实平台检查在本报告里分开列出，只有「真实平台 / 真实桌面」一栏才算行为证据。
- 「未覆盖」不等于「支持」也不等于「不支持」：它表示当前环境无法判定该项；本报告对每一项都写明原因。
- X11（含 XWayland）下的检查结果**不能**推断 Wayland 支持：本机是 Wayland，XWayland 的注册成功只记录为 X11 服务器上的实测通过。
- 安装包构建成功单独列示（见 `runner-SHA256SUMS.txt` 与 installer 证据），不与桌面交互结论混在一起。

## 证据采集说明

- 证据采集于提交 `6a2c946` 的 CI 运行；本机证据采集于 ticket 17 工作期间，其中剪贴板相关的本机证据是**修复剪贴板挂起缺陷之后**重新采集的（同一台机器、同一会话）。
- runner 的 Windows / macOS 证据里，`clipboard.text` 的原因文本当时仍写着「文件列表尚未实现」；这是文案错误，已在 ticket 17 更正（提交 `5ef378e`，`windows/cap.rs` / `macos/cap.rs`），实现（CF_HDROP / osascript）本来就在。没有重跑 runner，因此这两个平台文件列表的**真实读写**仍然是未覆盖。
- 本机的剪贴板检查会覆盖用户当前剪贴板内容（`--allow-clipboard-write`）；ticket 17 运行时没有能力先备份原内容（当时读取也受同一个缺陷影响）。

## 结论速览

| 类别 | 实测通过 | 实测失败 | 未覆盖 |
| --- | --- | --- | --- |
| 真实平台 / 真实桌面检查 | 49 | 5 | 25 |
| 替身检查（`cargo test` 经宿主入口，共 362 项） | 15 | 0 | 0 |
| 未覆盖条目（逐条写原因） | — | — | 12 |

## 环境

| 环境 | 系统 | 版本 | 架构 | 会话 | 桌面 | 签名状态 | 必要权限 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| GitHub runner Linux x64 | linux | Ubuntu 22.04.5 LTS | x86_64 | headless | 无 | 不适用（该 runner 只做检查，不打包、不签名） | 无（无桌面会话，无剪贴板选区） |
| GitHub runner Windows x64 | windows | Windows Server 2025 Datacenter 24H2（build 26100.33438） | x86_64 | not-applicable | 有 | 安装程序未签名（没有提供代码签名证书）；见 evidence/runner-windows-x64-installer.json | 交互式桌面（runner 有前台窗口与 shell 窗口）；无需提权 |
| GitHub runner macOS arm64（原生） | macos | Version 26.6.2 (Build 25G83) | aarch64 | not-applicable | 有 | ad-hoc 签名（未公证）；Gatekeeper 拒绝属预期，用户需手动放行。见 evidence/runner-macos-arm64-installer.json | 辅助功能权限：runner 上 AXIsProcessTrusted() = true |
| GitHub runner macOS x86_64（在 arm64 runner 上交叉编译） | macos | Version 26.6.2 (Build 25G83) | x86_64 | not-applicable | 有 | ad-hoc 签名（未公证），与 arm64 同一次构建流程 | 辅助功能权限：AXIsProcessTrusted() = true |
| 本机 Ubuntu 26.04 x86_64 / GNOME / Wayland（真实桌面） | linux | Ubuntu 26.04.1 LTS | x86_64 | wayland | 有 | 不适用（本机不打包、不签名） | 无（自动化会话没有输入注入/截图能力；剪贴板写入会覆盖当前剪贴板内容） |
| 本机同一会话、强制 X11 后端（FLASHCAST_FORCE_X11_BACKEND=1，经 XWayland） | linux | Ubuntu 26.04.1 LTS | x86_64 | wayland | 有 | 不适用 | XWayland 的 DISPLAY=:0 可用 |

## (a) 替身检查：不构成平台行为证据

以下检查全部经过宿主对外的查询 / 命令入口，但平台适配层使用 `flashcast-platform` 的 `fake` 替身（`--features fake`）。它们证明宿主逻辑正确，**不**证明任何真实平台行为。工作区、Git 与文件系统部分是真实的临时目录/真实仓库。

| 平台 | 套件数 | 通过 | 失败 |
| --- | --- | --- | --- |
| linux-x64（runner） | 28 | 362 | 0 |
| windows-x64（runner） | 28 | 351 | 0 |
| macos-arm64（runner） | 28 | 360 | 0 |
| macos-x64（runner） | 28 | 360 | 0 |

- `pnpm ui-check` → **通过 66 / 失败 0（共 66 项）**；真实 Chrome 驱动的浏览器 UI + 浏览器模拟宿主；不是 Tauri webview，不代表托盘、全局快捷键、自动粘贴或真实软件启动可用

| 检查 | 覆盖内容 | 用例 | 结论 |
| --- | --- | --- | --- |
| `host.query.scope` | 查询范围、插件关键词与标签冲突（替身） | `tests/host_search.rs, tests/memos.rs` | 实测通过 |
| `host.execute.actions` | 命令入口：启动、粘贴、书签、备忘录、剪贴板（替身 launcher/paster/chrome/clipboard） | `tests/host_execute.rs, tests/paste.rs, tests/chrome.rs` | 实测通过 |
| `host.selection` | 键盘选择、异步过期结果、返回上一查询 | `tests/host_selection.rs` | 实测通过 |
| `host.plugin_isolation` | 插件停用与失败隔离、超时 | `tests/plugin_isolation.rs` | 实测通过 |
| `host.settings` | 设置读写与校验失败保留上次可用状态 | `tests/settings.rs` | 实测通过 |
| `host.clipboard_text` | 剪贴板历史文字：去重、抑制自身写入、暂停、期限与容量（替身剪贴板） | `tests/clipboard.rs` | 实测通过 |
| `host.clipboard_files` | 文件引用与已保存副本语义（真实文件系统 + 替身剪贴板） | `tests/clipboard_files.rs` | 实测通过 |
| `host.themes` | 主题安装/切换/无效主题保留上次外观（真实文件系统） | `tests/themes.rs` | 实测通过 |
| `host.workspace` | 配置工作区选择/初始化/外部重载（真实临时仓库） | `tests/workspace.rs, tests/workspace_watch.rs` | 实测通过 |
| `host.workspace_git` | 变更查看、部分提交（真实 Git 仓库） | `tests/workspace_git.rs` | 实测通过 |
| `host.workspace_clone` | 克隆（本地 bare 远端；鉴权在外部边界控制） | `tests/workspace_clone.rs` | 实测通过 |
| `host.workspace_sync` | 拉取/推送、分叉、脏工作区保护、离线与鉴权失败（本地 bare 远端 + 桩 HTTP） | `tests/workspace_sync.rs` | 实测通过 |
| `platform.freedesktop_fixture` | freedesktop .desktop 解析与字段码展开（真实解析器 + 临时夹具） | `crates/flashcast-platform/tests/freedesktop_fixture.rs` | 实测通过 |
| `platform.chrome_fixture` | Chrome profile 发现与书签解析（临时 profile 夹具） | `crates/flashcast-platform/tests/chrome_fixture.rs` | 实测通过 |
| `ui.browser` | UI 主流程、键盘、中文输入、返回、紧凑/预览、浅深色、缩放、失败反馈（浏览器 + 模拟宿主） | `tools/ui-check/check.mjs` | 实测通过 |

## (b) 真实平台 / 真实桌面检查：行为证据

### GitHub runner Linux x64

- 证据：`docs/platform/evidence/runner-linux-x64.json`（diagnostics-linux-x64 / platform-check-linux-x64.json）
- 说明：无头 runner：编译与部分真实行为可测，桌面交互一律未覆盖。

| 检查 | 结论 | 说明 |
| --- | --- | --- |
| `build.compile` flashcast-platform 编译 | 实测通过 | linux x86_64 上编译并运行成功 |
| `session.detect` 桌面会话类型判定 | 实测通过 | XDG_SESSION_TYPE="<未设置>"，判定为 无桌面会话；有 DISPLAY=false，WAYLAND_DISPLAY=false |
| `apps.discovery` freedesktop 软件发现 | 实测通过 | 发现 10 个可启动软件，其中 9 个解析到图标文件；扫描 18 个 .desktop 文件，跳过 8 个；应用目录 3 个，图标索引主题 9 个 |
| `focus.capture` 读取唤起前前台应用 | 未覆盖 | 当前没有桌面会话，无法读取焦点窗口 |
| `hotkey.register` 全局快捷键注册 | 未覆盖 | 当前没有桌面会话，无法注册全局快捷键 |
| `clipboard.text` 剪贴板文字读写 | 实测失败 | 未找到 xclip 或 xsel，X11 下无法读写剪贴板 |
| `clipboard.write_text` 真实写入系统剪贴板并回读 | 未覆盖 | 未执行：该检查会覆盖当前剪贴板内容，需要显式加 --allow-clipboard-write |
| `clipboard.watch` 剪贴板变化监听（只读） | 未覆盖 | 读取没有完成：未找到可用的剪贴板工具：无桌面会话 会话需要 wl-copy / wl-paste（Wayland）或 xclip / xsel（X11），当前都没有找到；这不代表真实桌面上的复制无法被捕获 |
| `clipboard.rich` 富文本格式（HTML/RTF）的真实读写 | 未覆盖 | 读取没有完成：未找到可用的剪贴板工具：无桌面会话 会话需要 wl-copy / wl-paste（Wayland）或 xclip / xsel（X11），当前都没有找到；这不代表真实桌面上的富文本复制失败 |
| `clipboard.image` 图片剪贴板读写（PNG） | 未覆盖 | 当前剪贴板的选区里没有 PNG 图片（此刻可能没有复制图片；自动化会话里也可能根本没有选区持有者）。用 --allow-clipboard-write 可以实测本平台的图片写入与读回 |
| `clipboard.files` 文件列表（text/uri-list）的真实读写 | 未覆盖 | 当前剪贴板里没有文件列表（此刻没有复制文件；自动化会话里也可能根本没有选区持有者）。用 --allow-clipboard-write 可以实测本平台的写入与读回 |
| `paste.auto` 自动粘贴到唤起前应用 | 实测失败 | 当前没有桌面会话，无法注入粘贴 |
| `paste.prepare` 自动粘贴前置条件（XTEST / 会话类型） | 未覆盖 | 会话=无桌面会话，XTEST=false，注入后端=不可用；未注入真实按键（会打到当前前台窗口）；Wayland 不允许应用在把焦点交给其他应用后注入按键：GNOME 没有可用的公开接口，只有经 XDG RemoteDesktop 门户授权后才能做到，本版本不申请该权限。内容已复制到剪贴板，可手动粘贴。 |
| `x11.diagnostics` X11/EWMH 诊断 | 未覆盖 | 可用性=ConnectFailed("$DISPLAY variable not set and no value was provided explicitly")，窗口管理器=None，_NET_CLIENT_LIST 窗口数=None，前台窗口=None |

### GitHub runner Windows x64

- 证据：`docs/platform/evidence/runner-windows-x64.json`（diagnostics-windows-x64 / platform-check-windows-x64.json）
- 说明：真实 Windows API：软件发现、CreateProcess/ShellExecute、剪贴板打开、焦点读取与恢复、Win32 热键注册都在 runner 上真实执行。

| 检查 | 结论 | 说明 |
| --- | --- | --- |
| `build.compile` flashcast-platform 编译 | 实测通过 | windows x86_64 上编译并运行成功 |
| `paste.prepare` 自动粘贴前置条件（SendInput） | 未覆盖 | 注入后端=SendInput（Ctrl+V，带 dwExtraInfo 标记），能力=支持； 本检查不注入真实按键，端到端粘贴未覆盖（受 UIPI 限制，管理员窗口收不到） |
| `session.detect` 桌面会话判定 | 实测通过 | Windows 会话（不适用 X11/Wayland 分类）：交互式桌面（有前台窗口与 shell 窗口）；GetForegroundWindow=true，GetShellWindow=true |
| `apps.discovery` Windows 软件发现（开始菜单 / 注册表 / 打包应用） | 实测通过 | 发现 346 个可启动条目：开始菜单快捷方式 161 个（54 次走 IShellLinkW 纠正），注册表子键 456 个，打包应用 175 个；其中 346 个解析到图标文件、抽取失败 0 次；跳过 444 条 |
| `apps.launch` 启动真实目标与失效条目报错 | 实测通过 | 用 C:\Windows\system32\cmd.exe /c exit 启动成功（pid=Some(8168)）；失效路径按预期报错：找不到可执行文件 C:\Flashcast\definitely-missing\nope.exe |
| `focus.capture` 读取唤起前前台应用 | 实测通过 | 读取到 id=c:\program files\windowsapps\microsoft.windowsterminal_1.23.20211.0_x64__8wekyb3d8bbwe\windowsterminal.exe name=C:\ProgramData\GitHub\HostedComputeAgent\hosted-compute-agent pid=Some(8888) window=Some(131542) |
| `focus.restore` 把焦点还给唤起前应用 | 实测通过 | ShowWindow(SW_RESTORE)+SetForegroundWindow 把焦点还给 C:\ProgramData\GitHub\HostedComputeAgent\hosted-compute-agent 并在回读校验中一致 |
| `hotkey.register` 全局快捷键注册 | 实测通过 | 注册并注销 Ctrl+Alt+F12 成功 |
| `clipboard.text` 剪贴板文字读写 | 未覆盖 | 文字、HTML/RTF 与图片已实现（ticket 09/10/11：一次剪贴板打开里读全部格式，图片同时提供 CF_DIB 与注册格式 PNG）；文件列表尚未实现（ticket 12 覆盖）；Windows 提供 OpenClipboard 与 WinRT Clipboard API，环境本身具备条件 |
| `paste.auto` 自动粘贴到唤起前应用 | 实测通过 | 支持 |
| `clipboard.rich` 富文本格式（HTML/RTF）的真实读写 | 未覆盖 | 实现：一次剪贴板打开里写 CF_UNICODETEXT + HTML Format + Rich Text Format，读取时同样一次读出三种格式（HF-11）。本 runner 不执行写入（可能没有交互剪贴板），真机复核见 ticket 17 |
| `win.shell` Windows shell 环境（explorer / PowerShell） | 实测通过 | WINDIR=Some("C:\\Windows")，ComSpec=Some("C:\\Windows\\system32\\cmd.exe")，C:\Windows\explorer.exe 存在=true，PowerShell=5.1.26100.33438（实测通过） |
| `x11.diagnostics` X11/EWMH 诊断 | 未覆盖 | X11 / EWMH 只存在于 Linux 会话，Windows 上不适用 |

### GitHub runner macOS arm64（原生）

- 证据：`docs/platform/evidence/runner-macos-arm64.json`（diagnostics-macos-arm64 / platform-check-macos-arm64.json）
- 说明：NSWorkspace 前台应用、图标渲染、Carbon 热键注册与 CGEvent 注入前置条件真实执行；不注入真实按键。

| 检查 | 结论 | 说明 |
| --- | --- | --- |
| `build.compile` flashcast-platform 编译 | 实测通过 | macos aarch64 上编译并运行成功 |
| `session.detect` 桌面会话可用性 | 实测通过 | macOS 没有 X11/Wayland 会话类型；NSWorkspace 能报出前台应用，桌面会话可用 |
| `apps.discovery` macOS 应用发现 | 实测通过 | 发现 86 个 .app 包，其中 85 个可启动；跳过 1 个；应用目录存在 4 个、缺失 1 个（["/Users/runner/Applications"]）；图标渲染成功 85 个、失败 0 个 |
| `apps.discovery.skipped` 被跳过的 .app 包明细 | 实测通过 | /System/Applications/Passwords.app（不是应用包（CFBundlePackageType 不是 APPL）） |
| `apps.icon_render` 应用图标渲染（NSWorkspace → PNG） | 实测通过 | NSWorkspace 渲染 /System/Applications/Utilities/Activity Monitor.app 得到 1814217 字节 PNG（宽度 128 点） |
| `focus.capture` 读取唤起前前台应用 | 实测通过 | 读取到 id=com.apple.finder name=Finder bundle=Some("com.apple.finder") pid=Some(357) |
| `accessibility.permission` 辅助功能（Accessibility）权限 | 实测通过 | AXIsProcessTrusted() = true；系统设置 → 隐私与安全性 → 辅助功能 |
| `paste.prepare` 自动粘贴前置条件（辅助功能权限 / CGEvent） | 实测通过 | 注入后端=CGEvent（Cmd+V），权限=已授权；未注入真实按键，端到端粘贴需要对准目标应用手动验证 |
| `hotkey.register` 全局快捷键注册 | 实测通过 | 注册并注销 Ctrl+Alt+F12 成功 |
| `clipboard.text` 剪贴板文字读写 | 未覆盖 | 文字与图片已实现（ticket 09/10：pbpaste 读文本、图片经 osascript 读写）；HTML/RTF 不声称支持（ticket 11）；文件列表尚未实现（ticket 12 覆盖）；macOS 由 NSPasteboard 提供，无需外部工具 |
| `paste.auto` 自动粘贴到唤起前应用 | 实测通过 | 支持 |
| `clipboard.rich` 富文本格式（HTML/RTF）的真实读写 | 实测失败 | macOS 的 pbcopy / pbpaste 只支持纯文本：HTML/RTF 既不被保存也不被恢复，恢复时只提供文本（历史里的富文本载荷若来自其它平台仍原样保存在本机） |

### GitHub runner macOS x86_64（在 arm64 runner 上交叉编译）

- 证据：`docs/platform/evidence/runner-macos-x64.json`（diagnostics-macos-x64 / platform-check-macos-x64.json）
- 说明：与 arm64 结果一致；两个架构分别记录，不互相推断。

| 检查 | 结论 | 说明 |
| --- | --- | --- |
| `build.compile` flashcast-platform 编译 | 实测通过 | macos x86_64 上编译并运行成功 |
| `session.detect` 桌面会话可用性 | 实测通过 | macOS 没有 X11/Wayland 会话类型；NSWorkspace 能报出前台应用，桌面会话可用 |
| `apps.discovery` macOS 应用发现 | 实测通过 | 发现 86 个 .app 包，其中 85 个可启动；跳过 1 个；应用目录存在 4 个、缺失 1 个（["/Users/runner/Applications"]）；图标渲染成功 85 个、失败 0 个 |
| `apps.discovery.skipped` 被跳过的 .app 包明细 | 实测通过 | /System/Applications/Passwords.app（不是应用包（CFBundlePackageType 不是 APPL）） |
| `apps.icon_render` 应用图标渲染（NSWorkspace → PNG） | 实测通过 | NSWorkspace 渲染 /System/Applications/Utilities/Activity Monitor.app 得到 1814185 字节 PNG（宽度 128 点） |
| `focus.capture` 读取唤起前前台应用 | 实测通过 | 读取到 id=com.apple.finder name=Finder bundle=Some("com.apple.finder") pid=Some(361) |
| `accessibility.permission` 辅助功能（Accessibility）权限 | 实测通过 | AXIsProcessTrusted() = true；系统设置 → 隐私与安全性 → 辅助功能 |
| `paste.prepare` 自动粘贴前置条件（辅助功能权限 / CGEvent） | 实测通过 | 注入后端=CGEvent（Cmd+V），权限=已授权；未注入真实按键，端到端粘贴需要对准目标应用手动验证 |
| `hotkey.register` 全局快捷键注册 | 实测通过 | 注册并注销 Ctrl+Alt+F12 成功 |
| `clipboard.text` 剪贴板文字读写 | 未覆盖 | 文字与图片已实现（ticket 09/10：pbpaste 读文本、图片经 osascript 读写）；HTML/RTF 不声称支持（ticket 11）；文件列表尚未实现（ticket 12 覆盖）；macOS 由 NSPasteboard 提供，无需外部工具 |
| `paste.auto` 自动粘贴到唤起前应用 | 实测通过 | 支持 |
| `clipboard.rich` 富文本格式（HTML/RTF）的真实读写 | 实测失败 | macOS 的 pbcopy / pbpaste 只支持纯文本：HTML/RTF 既不被保存也不被恢复，恢复时只提供文本（历史里的富文本载荷若来自其它平台仍原样保存在本机） |

### 本机 Ubuntu 26.04 x86_64 / GNOME / Wayland（真实桌面）

- 证据：`docs/platform/evidence/local-wayland-write.json`（artifacts/platform-check/17-local-wayland-{readonly,write}.{json,log}）
- 说明：自动化 shell 会话：能真正读写 Wayland 剪贴板与文件系统，但没有可注入按键的桌面会话；真实启动/备忘录粘贴由 evidence/local-real-desktop-acceptance.log 记录。

| 检查 | 结论 | 说明 |
| --- | --- | --- |
| `build.compile` flashcast-platform 编译 | 实测通过 | linux x86_64 上编译并运行成功 |
| `session.detect` 桌面会话类型判定 | 实测通过 | XDG_SESSION_TYPE="wayland"，判定为 Wayland；有 DISPLAY=true，WAYLAND_DISPLAY=true |
| `apps.discovery` freedesktop 软件发现 | 实测通过 | 发现 35 个可启动软件，其中 35 个解析到图标文件；扫描 108 个 .desktop 文件，跳过 73 个；应用目录 3 个，图标索引主题 24 个 |
| `focus.capture` 读取唤起前前台应用 | 未覆盖 | Wayland 会话不向普通应用暴露全局焦点窗口；GNOME 等合成器未提供可用的公开接口。如需记录唤起前应用，请在 X11 会话下运行，或使用后续 ticket 提供的替代方案。 |
| `hotkey.register` 全局快捷键注册 | 未覆盖 | Wayland 会话不支持全局快捷键抓取；请使用托盘入口，或在 X11 会话下运行 |
| `clipboard.text` 剪贴板文字读写 | 未覆盖 | 文字、HTML/RTF、图片与文件列表已实现（ticket 09/10/11/12：读按 MIME 类型与 text/uri-list，写纯文本、图片按 image/png、文件列表用 text/uri-list）；环境已具备 wl-copy |
| `clipboard.write_text` 真实写入系统剪贴板并回读 | 实测通过 | 用 LinuxClipboard 写入并由 LinuxClipboardWatcher 读回同一段文本（38 字节） |
| `clipboard.watch` 剪贴板变化监听（只读） | 实测通过 | 监听读到当前剪贴板文本（Wayland 会话，38 字节） |
| `clipboard.rich` 富文本格式（HTML/RTF）的真实读写 | 实测通过 | 写入成功。实际提供：文字；未提供：HTML（wl-copy 一次只能提供一种 MIME 类型，同时提供会让纯文本目标拿不到内容；富文本载荷仍保存在本机历史里）、RTF（wl-copy 一次只能提供一种 MIME 类型，同时提供会让纯文本目标拿不到内容；富文本载荷仍保存在本机历史里）。Linux 上 wl-copy 一次只能提供一种 MIME 类型，因此恢复时只提供纯文本是**已知降级**：富文本载荷仍完整保存在 本机历史（clipboard_payloads）里；读回：HTML=不可读，RTF=不可读 |
| `clipboard.image` 图片剪贴板读写（PNG） | 实测通过 | 写入 3×2 PNG（94 字节）并读回 image/png 3×2（94 字节）：像素逐点一致 |
| `clipboard.files` 文件列表（text/uri-list）的真实读写 | 实测通过 | 写入 2 个文件后读回 2 个。写=/tmp/flashcast-check-files-1790859013060/flashcast 报告.txt、/tmp/flashcast-check-files-1790859013060/中文 名称.txt；读=/tmp/flashcast-check-files-1790859013060/flashcast 报告.txt、/tmp/flashcast-check-files-1790859013060/中文 名称.txt（含空格与非 ASCII 名，路径与顺序完全一致） |
| `paste.auto` 自动粘贴到唤起前应用 | 实测失败 | Wayland 不允许应用在把焦点交给其他应用后注入按键：GNOME 没有可用的公开接口，只有经 XDG RemoteDesktop 门户授权后才能做到，本版本不申请该权限。内容已复制到剪贴板，可手动粘贴。 |
| `paste.prepare` 自动粘贴前置条件（XTEST / 会话类型） | 未覆盖 | 会话=Wayland，XTEST=true，注入后端=不可用；未注入真实按键（会打到当前前台窗口）；Wayland 不允许应用在把焦点交给其他应用后注入按键：GNOME 没有可用的公开接口，只有经 XDG RemoteDesktop 门户授权后才能做到，本版本不申请该权限。内容已复制到剪贴板，可手动粘贴。 |
| `x11.diagnostics` X11/EWMH 诊断 | 实测通过 | 可用性=Available，窗口管理器=Some("GNOME Shell")，_NET_CLIENT_LIST 窗口数=Some(1)，前台窗口=None |

### 本机同一会话、强制 X11 后端（FLASHCAST_FORCE_X11_BACKEND=1，经 XWayland）

- 证据：`docs/platform/evidence/local-xwayland-forced.json`（artifacts/platform-check/17-local-xwayland-forced.{json,log}）
- 说明：刻意保留的对照：X11 服务器上的成功**不**作为 Wayland 支持的证据。本机没有安装 xclip，X11 剪贴板后端因此不可用。

| 检查 | 结论 | 说明 |
| --- | --- | --- |
| `build.compile` flashcast-platform 编译 | 实测通过 | linux x86_64 上编译并运行成功 |
| `session.detect` 桌面会话类型判定 | 实测通过 | XDG_SESSION_TYPE="wayland"，判定为 Wayland；有 DISPLAY=true，WAYLAND_DISPLAY=true |
| `apps.discovery` freedesktop 软件发现 | 实测通过 | 发现 35 个可启动软件，其中 35 个解析到图标文件；扫描 108 个 .desktop 文件，跳过 73 个；应用目录 3 个，图标索引主题 24 个 |
| `focus.capture` 读取唤起前前台应用 | 实测通过 | 读取到 id=unknown name=unknown wm_class=None pid=None window=Some(2097155) |
| `hotkey.register` 全局快捷键注册 | 实测通过 | 注册并注销 Ctrl+Alt+F12 成功 |
| `clipboard.text` 剪贴板文字读写 | 未覆盖 | 文字、HTML/RTF、图片与文件列表已实现（ticket 09/10/11/12：读按 MIME 类型与 text/uri-list，写纯文本、图片按 image/png、文件列表用 text/uri-list）；环境已具备 wl-copy |
| `clipboard.write_text` 真实写入系统剪贴板并回读 | 未覆盖 | 未执行：该检查会覆盖当前剪贴板内容，需要显式加 --allow-clipboard-write |
| `clipboard.watch` 剪贴板变化监听（只读） | 实测通过 | 监听读到当前剪贴板文本（Wayland 会话，49 字节） |
| `clipboard.rich` 富文本格式（HTML/RTF）的真实读写 | 未覆盖 | 当前剪贴板没有提供 text/html 或 text/rtf（可能此刻没有复制富文本；自动化会话里也可能根本没有选区持有者）。用 --allow-clipboard-write 可以实测本平台的写回行为与降级结论 |
| `clipboard.image` 图片剪贴板读写（PNG） | 未覆盖 | 当前剪贴板的选区里没有 PNG 图片（此刻可能没有复制图片；自动化会话里也可能根本没有选区持有者）。用 --allow-clipboard-write 可以实测本平台的图片写入与读回 |
| `clipboard.files` 文件列表（text/uri-list）的真实读写 | 未覆盖 | 当前剪贴板里没有文件列表（此刻没有复制文件；自动化会话里也可能根本没有选区持有者）。用 --allow-clipboard-write 可以实测本平台的写入与读回 |
| `paste.auto` 自动粘贴到唤起前应用 | 未覆盖 | 已通过 FLASHCAST_FORCE_X11_BACKEND 强制使用 X11 后端：只有 XWayland 里的 X11 客户端能收到合成按键，原生 Wayland 客户端收不到，因此不能算作 Wayland 支持 |
| `paste.prepare` 自动粘贴前置条件（XTEST / 会话类型） | 实测通过 | 会话=Wayland，XTEST=true，注入后端=X11/XTEST；未注入真实按键（会打到当前前台窗口） |
| `x11.diagnostics` X11/EWMH 诊断 | 实测通过 | 可用性=Available，窗口管理器=Some("GNOME Shell")，_NET_CLIENT_LIST 窗口数=Some(1)，前台窗口=None |

## (c) 未覆盖：逐条原因

| 条目 | 原因 | 证据 |
| --- | --- | --- |
| `desktop.global_hotkey_summon` | Wayland 下 global-hotkey 0.8 只有 X11 的 XGrabKey 实现，注册直接判定不可用；本报告不把 XWayland 下「注册成功」当成唤起可用，也没有向真实桌面注入按键来验证唤起。 | evidence/local-wayland-write.json 的 hotkey.register=未覆盖；evidence/local-xwayland-forced.json 的 hotkey.register=实测通过（仅 X11 服务器） |
| `desktop.focus_capture` | Wayland 不向普通应用暴露全局焦点窗口（GNOME 无公开接口）；XWayland 下能读到窗口句柄但拿不到应用身份（id=unknown）。 | evidence/local-wayland-write.json 的 focus.capture=未覆盖 |
| `desktop.auto_paste_end_to_end` | Wayland 不允许应用把焦点交给其他应用后注入按键；没有 XDG RemoteDesktop 门户授权，也没有真实目标应用接收粘贴内容。跳过注入真实按键是刻意的（会打到用户当前前台窗口）。 | evidence/local-wayland-write.json 的 paste.auto=实测失败、paste.prepare=未覆盖；evidence/local-xwayland-forced.json 的 paste.auto=未覆盖 |
| `desktop.memo_paste_into_previous_app` | 真实剪贴板写入与「已复制，请手动粘贴」降级已实测通过，但没有唤起前的目标应用可用于核对「目标应用里真的出现了内容」。 | evidence/local-real-desktop-acceptance.log |
| `desktop.clipboard_origin_app` | X11 下的来源应用推导需要 xclip，本机未安装；Wayland 下没有公开的来源应用接口。 | evidence/local-wayland-write.json 的 clipboard.text 说明 |
| `desktop.tauri_webview_visual` | 本机没有桌面截图/输入注入工具（grim/import/scrot/xdotool/wtype 均不存在），无法目视核对真实 webview 的缩放、浅深色与渲染；只能记录进程级启动（ticket 04 的安装包检查）。 | artifacts/ui/*.png 是浏览器截图，不是 Tauri webview |
| `desktop.tray_menu` | 同上：没有输入注入工具，无法点击托盘菜单；只验证过托盘进程能初始化（ticket 01 的冒烟）。 | ticket 01 Comments 的未覆盖项 |
| `desktop.chrome_visible_window_open` | 真实 Chrome 的启动与页面加载已由 headless 检查覆盖（一次性的 --user-data-dir，绝不使用用户的真实 profile）；在真实桌面里弹出可见窗口并核对书签页没有做，因为没有可用的窗口观察手段，也不允许触碰用户真实 Chrome profile。 | crates/flashcast-platform/tests/chrome_fixture.rs 的 real_chrome_spawn_starts_a_process；artifacts/platform-check/17-real-chrome-spawn.log |
| `runner.macos_windows_clipboard_roundtrip` | macOS runner 的 pbcopy/pbpaste 只能提供纯文本（HTML/RTF 写入为实测失败，属已记录的降级）；Windows runner 不执行剪贴板写入（可能没有交互剪贴板），真实读写未覆盖。 | evidence/runner-macos-arm64.json、evidence/runner-windows-x64.json |
| `runner.windows_cf_hdrop_macos_applescript_files` | Windows 的 CF_HDROP 与 macOS AppleScript 文件列表的真实读写没有执行过；Linux 上的 text/uri-list 已实测通过。 | evidence/runner-windows-x64.json、evidence/runner-macos-arm64.json |
| `runner.signing_with_credentials` | 有证书时的签名分支（Windows 导入 pfx → certificateThumbprint；macOS 正式签名 + 公证）从未执行：没有凭证。当前状态是 macOS ad-hoc（未公证）、Windows 未签名。 | evidence/runner-macos-arm64-installer.json、evidence/runner-windows-x64-installer.json |
| `local.linux_x11_clipboard` | 本机是 Wayland 且未安装 xclip/xsel，X11 剪贴板后端不可用；不能用 Wayland 的结果推断 X11 可用。 | evidence/local-wayland-write.json 的 clipboard.text=未覆盖（原因写明 wl-copy 可用） |

## 安装包与签名

安装包构建成功单独列示，不构成桌面行为证据。

- 证据：evidence/runner-SHA256SUMS.txt（5 个包：Linux AppImage/deb、Windows exe、macOS arm64/x64 dmg）

| 平台 | 签名状态 | 说明 |
| --- | --- | --- |
| macOS arm64 / x64 | ad-hoc 签名（未公证） | codesign --verify --deep --strict 通过，签名标识 -；spctl 拒绝，用户需 xattr -dr com.apple.quarantine 或「仍要打开」。 |
| Windows x64 | 未签名 | 没有提供代码签名证书；SmartScreen 提示「更多信息 → 仍要运行」。 |
| Linux x64 | 不签名 | AppImage 与 deb 不做代码签名；以 SHA256 校验值核对（见 installers 检查的 8 实测通过 / 0 失败 / 0 未覆盖，由 ticket 04 记录）。 |

## 发布说明不得声称

- 不得声称「已在 Windows / macOS 真实桌面完成全流程」：这两个平台只有 runner 上的真实系统接口检查 + 安装包检查，没有人工桌面交互。
- 不得声称全局快捷键在 Linux Wayland 下可用：本机为未覆盖 / 不支持，托盘是唯一入口。
- 不得声称自动粘贴在 Linux Wayland 下可用：实测失败，行为是「已复制，请手动粘贴」。
- 不得声称 macOS / Windows 代码已签名或已公证：macOS 是 ad-hoc（Gatekeeper 会拦），Windows 未签名（SmartScreen 会拦）。
- 必须写明 macOS 首次启动需要手动放行、Windows 需要「仍要运行」。
- 已经实测通过的项可以写：Linux Wayland 上的文字 / 图片 / 富文本降级 / 文件列表剪贴板读写与监听、真实软件启动（回车）、备忘录标签复制与手动粘贴降级。
- 「未覆盖」项必须逐条列出并给出原因，不得因为构建或替身测试全绿就省略。
