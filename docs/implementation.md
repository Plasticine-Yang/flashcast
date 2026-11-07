# Flashcast 实现说明

本文记录 [prd.md](./prd.md) 与 [ui-ux.md](./ui-ux.md) 如何落到代码中，以及实现时的取舍和已验证范围。

## 1. 技术栈决策

UI/UX 规范第 12 节把 Electron / Tauri 列为待确认项，同时以 Electron 能力上限描述毛玻璃、剪贴板媒体、全局快捷键和窗口置顶。本项目选择 **Electron 45 + React 19 + TypeScript + Vite/electron-vite**：

- 三端都可用的原生 `globalShortcut`、`clipboard`、`Tray`、透明置顶窗口、`shell`；
- Electron 45 的异步 W3C 风格 Clipboard API 可直接读写 MIME 数据；
- Tauri 在 Linux 的窗口模糊、媒体剪贴板和全局快捷键成本更高；
- 渲染层仍是普通 Web，UI 可脱离 Electron 在浏览器中用 mock bridge 预览与截图。

## 2. 进程与模块

```text
src/main/
  index.ts                 应用生命周期、单实例、userData/协议注册
  application.ts           FlashcastApplication：服务编排、业务 API、托盘、主题
  window-manager.ts        全屏透明启动器窗口 / 设置窗口 / 动画协调
  shortcut-manager.ts      全局快捷键注册、事务式更新、冲突/占用错误
  update-assets.ts         版本比较、按平台挑选 Release 资产、SHA256SUMS 解析
  update-service.ts        检查 / 下载 / 校验 / 各平台安装，状态广播
  ipc.ts                   IPC 白名单 handler
  config-store.ts          工作区 config.json + 插件配置 + 原子写入
  workspace-service.ts     工作区切换、Git 状态、自动提交
  git-service.ts           git 命令封装（rev-parse/status/commit/push/diff）
  paths.ts                 工作区布局、userData、资源与版本
  apps.ts                  Linux .desktop / macOS .app / Windows 快捷方式发现与启动
  mac-alias.ts             Finder 替身（别名）文件的字节解析
  clipboard-service.ts     剪贴板历史轮询、媒体落盘、预览数据、激活/清理
  chrome-bookmarks.ts      Chrome Profile 探测、书签解析、在 Chrome 打开
  favicons.ts              favicon 拉取与磁盘缓存
  native-paste.ts          平台自动粘贴（AppleScript / PowerShell / wtype/xdotool/ydotool）
  external-plugins.ts      插件安装/更新/卸载、模块加载、上下文、设置 schema
  plugin-service.ts        内置插件页面与设置、命令列表
  builtins/manifests.ts    内置插件 manifest 与默认配置
src/preload/index.ts       contextBridge 白名单 API
src/renderer/src/
  launcher/                Launcher、搜索、键盘路由、剪切板、书签、外部插件、设置页
  settings/                独立设置窗口
  styles/                  tokens/base/launcher/plugin/settings
  mock/                    浏览器预览用 mock bridge 与示例数据
src/shared/
  types.ts                 IPC / 数据契约
  fuzzy.ts                 子序列 + 词首/连续加分、多字段匹配、高亮切片
  shortcuts.ts             accelerator 解析、平台符号、清除快捷键
  update.ts                更新状态标签、字节/百分比格式化、Release 说明降级为纯文本
  time.ts                  相对时间与类型标签
```

## 3. 关键流程

### 3.1 启动与窗口

1. `app.requestSingleInstanceLock()` 保证只有一条常驻进程；第二次启动唤起已有实例。
2. 启动器是**全屏透明、无边框、置顶、跳过任务栏**的窗口，面板本身固定 720×480，由 `.fc-panel-anchor` 居中并通过 `top: min(18vh, 160px)` 定位。窗口 bounds 使用当前鼠标所在显示器的 `workArea`，避免超出工作区。
3. 显示时重新定位到鼠标所在屏，`show()` + `focus()`。macOS 会在 `show()` 时把铺满工作区的窗口整体平移（实测 x 从 0 变成 218），面板是相对窗口视口居中的，因此 `show()` 之后必须再 `setBounds()` 一次，否则每次呼出都不居中。
4. 窗口失焦 100ms 后请求关闭动画；关闭动画由渲染进程用 WAAPI 播放（180ms），完成后调用 `launcher:hide`，主进程 `hide()`。主进程还有 420ms 兜底定时器。
5. 打开设置窗口时启动器失焦自动收起；设置窗口独立 920×640。

启动器入场不再缩放/模糊整块面板：`.fc-panel` 初始 62px 高、只显示输入区，`.fc-panel-inner` 固定按最终内容高度（478px）排版；`playEntrance()` 用 WAAPI 先停住 85ms 让输入框出现，再以 S 曲线把外壳高度长到 480px（+5px 过冲回落），内容由裁剪逐层露出。关闭时外壳先收回 62px 再淡出，180ms 后隐藏。输入焦点在动画前就绪，不阻塞键入。启动器不再绘制全屏压暗遮罩，`.fc-dismiss-layer` 只是透明点击层，点击外部仍然关闭。

### 3.2 搜索与模糊匹配

`src/shared/fuzzy.ts` 将 query 按空白分词，每个 token 必须在目标字段中命中：

- 先找大小写不敏感的子串，命中给予前缀/词首/长度加分；
- 否则做贪心子序列匹配，连续、词首、开头命中加分；
- 多字段（名称、关键字、URL、文件夹）逐 token 选择得分最高字段，并返回每个字段的高亮 index；
- 中文走子串路径，因此 `书签` 会高亮 `浏览器书签` 中的连续片段；
- 结果统一返回 `{score, indices}`，渲染层用 `highlightSegments` 生成 `<mark class="fc-match">`。

插件、应用、命令都在 renderer 内基于同一算法排序；书签和剪切板在各自页面内过滤。书签超过 60 行时切到固定行高虚拟列表。

macOS 应用图标不走 `app.getFileIcon`（实测只返回 32×32 的纯白占位图）：先解析 bundle 的 `Contents/Resources/*.icns`，交给系统自带的 `sips` 转成 64px PNG，失败或全透明时再用 `nativeImage.createThumbnailFromPath` 兜底（图标只在 `Assets.car` 里的新版系统应用）。Finder 替身形式的 `.app` 先按别名文件里的 `file://` 路径找到真正的 bundle 再取图。结果按 bundle 修改时间缓存在 `userData/app-icons`，取不到图标时渲染层回退到首字母徽标。

### 3.3 插件进入 / 退出动画

- 宿主始终挂载全局视图；插件视图绝对定位叠在上方，不做高度动画。
- 进入：全局视图 `opacity 0 / scale .99 / blur 4px`（140ms），插件视图 `opacity/y/blur`（380ms，延迟 60ms），插件图标 spring 出现，placeholder 120ms 交叉淡入。
- 退出：插件视图 140ms 淡出 + 下移；全局视图 220ms 弹性回归；240ms 后卸载插件视图。
- 设置页与插件主页面通过 `key` 变化重播轻微入场动画。
- `prefers-reduced-motion` 下取消 scale/blur/stagger、外壳高度生长与水珠展开；预览改为 120ms 纯 opacity。

### 3.4 剪切板

- 主进程每 700ms 读取一次异步 `clipboard.read()`；按“图片 → 文件 URI → 文本”的优先级归一化。文件 URI 会区分图片/视频/音乐扩展名。
- 内容以 SHA-256 fingerprint 去重并置顶；容量超过插件配置（默认 10）时删除最旧项及其媒体文件。
- 图片保存原图与缩略图；视频优先用 `ffmpeg` 截首帧，缺失时复制内置设计封面；音乐保留文件名与大小（预览用音符图标）。
- 媒体文件位于 `<workspace>/state/clipboard/media`，渲染层通过注册的自定义协议 `flashcast-media://media/<file>` 加载；协议校验文件名与目录，仅暴露媒体目录。
- 预览：预览是剪切板页面的常驻组成部分，进入剪切板且存在选中项后展开右侧 300px 面板，不再有 1s 停留计时，也不提供收起动作；鼠标或 `j/k` 切换只原位更新内容，不收起、不重放动画。只有搜索无结果（没有选中项）或离开插件时预览才消失。面板展开时列表让出 320px 内边距；鼠标悬停只改选中，不触发滚动，键盘 `j/k` 才把选中项滚入视野。
- 预览出场由 `ClipboardView` 在挂载时测量选中行与预览的 rect，生成贴着预览左缘、对齐选中行的 12×8px 水珠，再用 WAAPI 展开 `clip-path`（圆角 6 → 14）、`translateX(16 → 0)` 与 opacity，总长 420ms；预览不再播放收起动画。旧版的 2px 停留进度线已移除。
- 列表 FLIP 只在条目集合/顺序变化时测量 `offsetTop`；悬停、预览内容更新等普通渲染不会重启动画，避免行在鼠标移动时抖动。
- 快捷键路径的 `esc`：通过全局剪切板快捷键直接进入时，`esc` 直接关闭启动器；从全局搜索页主动进入剪切板时，`esc` 仍退出插件回到全局页。设置页内 `esc` 先返回插件主页。
- 回车：写回系统剪贴板 → 置顶 → 关闭动画 → 隐藏窗口，不粘贴。
- `Alt/Option+Enter`：同上，并在窗口隐藏后由主进程调用平台粘贴助手。
- `Ctrl+Backspace`：行先 120ms 淡出，随后删除；剩余行通过 `useFlipAnimation` 做 220ms FLIP。
- 清空：组合键第一次进入危险确认条，3s 内再按执行；Esc 或其他键取消。

### 3.5 Chrome 书签

- 解析平台默认 Chrome 用户数据目录（可用 `FLASHCAST_CHROME_USER_DATA` 覆盖），读取 `Local State` 展示 Profile 名称，优先 `Default`。Profile 发现以 `Local State#profile.info_cache` 与 `profiles_order` 为准，目录存在即可选中；缺少 Local State 时回退到 `Default` / `Profile N` / 含 `Preferences`、`Bookmarks` 或 `AccountBookmarks` 的目录。
- Chrome 有两种书签存储：本地 `Bookmarks`，以及登录 Google 账户并启用账户书签后的 `AccountBookmarks`（新版 Chrome 的默认行为，此时 `Bookmarks` 根本不会生成）。两者格式一致，读取时按 `mtime` 选择最近写入的一份，时间相同取 `AccountBookmarks`；最新文件解析失败时回退到另一份。设置页显示当前实际使用的书签文件与来源（本地 / 账户）。
- 递归扁平化 `roots.bookmark_bar / other / synced`，只保留 http/https/file，记录 Chrome 顺序与文件夹路径；设置页选择 Profile 后立即写插件配置。两个文件都不存在（从未添加书签）时返回空列表而不是“未找到 Profile”，`bookmarkSource` 为 `null`。
- favicon 按域请求 Google s2 并缓存为 PNG data URL，渲染为 32px 图标；失败回退 Globe。
- 打开时通过检测到的 Chrome 可执行文件 `--new-tab` 打开；macOS 使用 `open -a "Google Chrome"`。找不到 Chrome / 解析失败均提供就地错误与重试。

### 3.6 工作区与 Git

- 默认 `~/.flashcast`，首次运行 `git init -b main`、创建 README/.gitignore，并在所有内置插件默认配置生成后做一次初始提交。
- 选择目录时 `rev-parse --show-toplevel` 解析到仓库根；动态切换会重载 config、插件、剪切板历史、favicon 缓存与快捷键，不需要重启。
- 自动提交只 `git add` Flashcast 管理路径，提交信息 `config: update Flashcast workspace`；有 upstream 才推送。
- 工作区状态每 20s 轮询并在配置变更后刷新；设置页展示分支、最近提交、dirty/conflict/ahead/behind，差异弹窗读取 `git status --short` 与 `git diff --stat`。

### 3.7 外部插件

- manifest 支持 `plugin.json` 或 `package.json#flashcast`；校验 id、入口路径（禁止逃逸目录）、图标、关键字、设置 schema。
- 加载时机为启用状态；CJS 走 `createRequire`，ESM 走原生动态 import（带 cache-buster）。`setup/dispose` 生命周期、`context.effect/onDispose` 清理在禁用、卸载、更新和重载时执行。
- 安装到 `<workspace>/extensions/<id>`，用 staging → rename → backup 的两阶段替换；`.flashcast-source.json` 记录 Git/本地来源以支持更新。
- 安装前 `inspect` 会 clone/检查清单并返回一次性 token，UI 展示名称、版本、权限、入口、文件数量/大小与信任警告；安装过程禁止重复提交。
- 插件列表、启用状态、更新错误由主进程广播到两个窗口。

### 3.8 快捷键

- 全局动作存储为规范 accelerator（`Control+Alt+C`、`Meta+...`），运行时按平台映射为 Electron accelerator（macOS 的 `Meta→Command`，Linux/Windows 的 `Meta→Super`）。
- 设置页录制使用真实 `keydown`，要求至少一个主修饰键；与另一个全局动作冲突时禁止保存。主进程注册失败会事务式恢复旧组合并返回错误。
- Wayland 会话不经过 X11 抓键，而是由 `xdg-desktop-portal` 的 `GlobalShortcuts` 接口绑定：Electron 把 `package.json#desktopName`（`com.flashcast.app.desktop`）作为 portal 身份，因此该名字必须与已安装的 `.desktop` 文件一致（生产版由 `linux.syncDesktopName` 保证，开发版由主进程 `app.setDesktopName()` 改成 `com.flashcast.app.dev.desktop`，并由 `scripts/ensure-dev-desktop-entry.mjs` 写入用户目录）。身份不能共用：用户级同名条目会遮住 `/usr/share` 里的生产版。身份无法解析时 portal 会拒绝绑定而 `globalShortcut.register()` 仍返回 `true`，所以 `src/main/linux-portal.ts` 在启动时检查身份解析结果，把问题写入设置页的 `shortcutErrors`（`id: 'system'`）并发送系统通知。
- 设置页把 `system` 诊断与“本次保存失败”区分开：录制/恢复默认的错误判断会过滤该 id，避免环境问题被当成冲突而拒绝保存。恢复默认走 `ShortcutManager.getDefaults()`（默认值不区分开发版与安装版，见 `defaultHotkeys`），注册失败时事务式回滚，因此页面提示“恢复默认失败”而不是“已恢复”，并在行内展示默认组合、当前值即默认时显示“默认”标识。
- `Ctrl+J/K/H/L`、`Esc`、`Enter`、`Ctrl+Backspace`、清空组合键、`?` 都经过渲染器统一的 `keyHandler`；输入法组合输入期间全部忽略。

### 3.9 发布流水线

- 发布以 tag 为唯一事实来源：`scripts/release.mjs`（`pnpm release:minor` / `release:patch` / `release:major`，或直接传 `x.y.z`）升 `package.json` 版本、提交「chore(release): 发布 vX.Y.Z」、打附注 tag 并推送。
- `.github/workflows/release.yml` 在 `v*` tag 上运行：先校验 tag 与 `package.json` 版本一致，再并行构建 Linux（AppImage/deb）、macOS（dmg/zip，x64 + arm64）、Windows（nsis）。各 job 先把产物上传为 workflow artifact，由独立的 `publish` job 汇总、生成 `SHA256SUMS.txt` 后用 `gh release create` 创建 Release（已存在时 `--clobber` 覆盖资产），避免多 job 并发写同一个 Release。
- 产物未签名（仓库没有证书）：macOS 需右键打开，Windows 会触发 SmartScreen；macOS 通过 `CSC_IDENTITY_AUTO_DISCOVERY=false` 跳过签名发现。
- `.github/workflows/ci.yml` 在 PR 与 `main` 推送时执行 `typecheck`、`test`、`build`。

### 3.10 自动更新

1. **检查**：`UpdateService.check()` 读取 GitHub Releases API（默认 `repos/Plasticine-Yang/flashcast/releases/latest`；`FLASHCAST_UPDATE_FEED` 可整体覆盖地址，`FLASHCAST_UPDATE_REPO` 可换仓库）。仅比较 `tag_name` 与当前版本：`isNewerVersion()` 处理 `v` 前缀、缺省段位与预发布排序，`0.3.0` 高于 `0.3.0-beta.2`。发布构建启动 6s 后静默检查一次，其后每 6 小时一次；手动检查有 60s 节流（GitHub 匿名 API 限 60 次/小时）。开发构建不自动检查、不允许就地安装，但可以手动检查。
2. **选包**：`pickUpdateAsset()` 按 electron-builder 的命名规则挑选：macOS 优先 `-arm64-mac.zip` / `-mac.zip`（x64 无架构后缀），缺失时退回同架构 `.dmg`；Windows 取 `Setup*.exe`；Linux 优先 `.AppImage`，其次 `.deb`。`installKindFor()` 决定是"原地替换并重启"还是"交给系统安装器"。
3. **下载**：主进程用 `net.request`（跟随 GitHub 302 到资产存储）流式写入 `<userData>/updates/<资产名>.part`，每 120ms 合并一次进度并广播 `updates:changed`；完成后按需移除 `.part` 再改名，保证失败不会留下半个安装包。取消会 `abort()` 请求并回到 `available`。
4. **校验**：发布流水线用 `sha256sum *` 生成 `SHA256SUMS.txt`。GitHub 会把上传资产名里的空格改成点（`Flashcast Setup 0.2.3.exe` → `Flashcast.Setup.0.2.3.exe`），所以校验按"分隔符归一化"匹配；摘要不一致时不落盘、直接报错。
5. **安装**：macOS 用 `ditto` 解包 zip，然后由一个脱离父进程的 `sh` 脚本等应用退出、把旧 `.app` 备份、替换、去隔离属性并 `open`；`open` 失败会自动回滚备份。Windows 以 `/S --updated` 启动 NSIS 安装器后退出。Linux AppImage 先写 `.new` 再替换并重启；`.dmg`/`.deb`/无 `APPIMAGE` 环境交给系统安装器。安装前会再次确认下载文件存在、目标可写与包内可执行文件完整。
6. **UI 同步**：`UpdateState` 是唯一事实来源，主进程 `broadcast(IPC.updateChanged)` 给启动器与设置窗口；`useUpdateState()` 订阅事件并读取初值，因此两个窗口的进度、就绪与错误状态始终一致。启动器只在 `available / downloading / ready / installing` 显示顶栏更新按钮，其余状态保持原布局。

## 4. UI 设计令牌与组件

- `styles/tokens.css` 完整实现 UI 规范第 3 节的蓝色毛玻璃深色/浅色令牌（窗口、玻璃描边、内部光源、磨砂颗粒、壁纸层、选中行、极光背景）、字体、间距、圆角、阴影和动效；三端统一使用半透明面板，`prefers-reduced-transparency` / `data-contrast="more"` 下退回接近不透明。
- 面板不透明度已从 `0.92` / `blur(44px)` 提升到 `0.964` / `blur(83px)`（浅色 `0.956`），壁纸层强度从 `0.5` 降到 `0.324`。原因是桌面不在透明窗口 `backdrop-filter` 的采样范围内，“透出多少”完全由 alpha 决定，旧值下桌面文字仍可辨认。曾经做过 0–100 的「毛玻璃强度」滑杆，但可调区间在真实桌面上肉眼几乎无差别（macOS 还没有壁纸层），已移除，避免留下一个看不出效果的开关。
- `main/wallpaper.ts` 通过 `gsettings`（GNOME）或注册表（Windows）定位壁纸，经 `nativeImage` 下采样后由 `flashcast-wallpaper://` 协议提供给启动器；macOS 回退纯玻璃。
- `styles/base.css` 提供 Keycap、按钮、开关、Chip、Toast、Skeleton、焦点环与 `prefers-reduced-motion` / `prefers-contrast` 适配。
- `styles/launcher.css` 负责窗口/面板、搜索栏、分组行、stagger 入场、Footer、Help 浮层、Toast。
- `styles/plugin.css` 负责插件外壳、剪切板预览、设置表单、书签、错误卡、虚拟列表。
- `styles/settings.css` 负责 200px 侧栏、内容区、主题选择器、插件卡片、模态框、工作区、快捷键表、关于页。
- 图标统一 Lucide 1.5px；无 emoji 图标；插件图标 fallback 为 puzzle。
- 快捷键符号由 `keycapParts(accelerator, platform)` 生成，不硬编码 Ctrl/⌘。

## 5. 安全与边界

- 渲染器开启 `contextIsolation`、关闭 `nodeIntegration`、禁用 `webviewTag`；preload 仅暴露白名单方法。
- CSP 限制脚本/样式/图片/字体来源；媒体自定义协议只读剪切板媒体目录。
- 外部插件在主进程有完整 Node 权限，这是有意的可扩展性取舍；UI 在安装确认卡中明确提示，不做虚假的权限隔离。
- 工作区相关的路径都由主进程解析，renderer 传 id/域/URL，不传任意文件系统路径（插件安装输入除外）。
- 外部网络请求只有三处：Favicon、插件 Git 安装、更新检查/下载。更新只访问 Releases API 与 Release 资产地址，安装包先做 SHA256 校验；不会执行 Release 说明里的任何内容。没有遥测。

## 6. 已验证范围

- `pnpm typecheck` 通过；`pnpm test` 70 个用例通过（模糊匹配、快捷键、Chrome 书签扁平化与账户书签选择、`.desktop`/Exec 解析、更新版本比较/选包/校验、假网络栈驱动的检查→下载→校验状态机，以及配置读写与旧键清理）。
- 用本机真实 Chrome（153，登录 Google 账户，Profile 内只有 `AccountBookmarks`）验证：能读出 18 条书签及其文件夹路径，`bookmarkSource` 为 `account`。
- `pnpm build` 通过；`electron-builder --dir --linux` 生成可运行的 unpacked 应用，asar 与 `resources/build` 图标/封面资源路径验证通过。
- 在 Ubuntu 26.04 / GNOME 50 Wayland 会话启动真实 Electron 应用：渲染正常、无崩溃。此前记录的“全局快捷键注册成功”只反映 `globalShortcut.register()` 的返回值；Wayland 下实际绑定由 portal 完成，需要应用身份能解析到已安装的 `.desktop` 文件（见 3.8）。
- 通过 Chrome DevTools Protocol 驱动真实应用：
  - `ArrowDown` / `Ctrl+J` 改变选中项；
  - `Enter` 激活剪切板项后窗口 `IsUnMapped`，系统剪贴板变为该文件路径，历史顺序置顶；
  - `Ctrl+Backspace` 删除选中项；`Super+Alt+Backspace` 两次触发清空确认与清空；
  - 打开外部插件、外部插件设置、安装本地插件、主题切换、快捷键更新与恢复默认均成功。
- 真实 Electron 截图验证深色/浅色主题、剪切板四类内容与默认预览、书签 favicon/文件夹 chip、设置窗口。
- 在 Chrome 预览环境用 CDP 验证入场水珠/高度曲线、无全屏压暗遮罩，以及默认预览、鼠标跨行与 `j/k` 切换时预览保持展开且内容原位更新（不再闪烁）。
- `pnpm preview:ui` 支持在无 Electron 环境下检查启动器、各插件页、设置页和模态框；`?update=available|downloading|ready|latest|error` 可预览更新的全部状态。
- 更新检查用真实 v0.2.3 Release 数据核对：资产命名、`SHA256SUMS.txt` 的空格→点差异、`isNewerVersion('0.2.3','0.2.3') === false` 都已在测试中固定。

## 7. 需求覆盖对照

| PRD / UI 条目 | 实现位置 |
|---|---|
| 三端与 Raycast 风格 | Electron 主进程、`styles/*`、`electron-builder.yml` |
| 三端启动快捷键 | `ShortcutManager` 默认值、设置页录制 |
| Vim `ctrl+j/k/h/l`，插件页同样生效 | `Launcher.keyHandler`、`HelpOverlay`、插件设置字段导航 |
| 工作区关联 Git、配置入仓 | `WorkspaceService`、`ConfigStore`、`GitService` |
| 插件安装 / 关键字 / 模糊匹配 | `ExternalPluginManager`、`fuzzy.ts`、插件分组搜索 |
| 输入框插件图标、点击进插件设置 | `SearchBar`、`PluginSettingsView` |
| 插件设置由插件维护 | 内置插件自定义 schema；外部 manifest `settings` |
| 进入插件复用输入框、Esc 退出 | `Launcher` 视图状态与 `SearchBar` |
| 打开 / 关闭 / 进出插件流体动画 | WAAPI + CSS keyframes，`launcher.css` / `plugin.css` |
| 剪切板快捷键、容量、类型展示 | `ClipboardService`、`plugins/clipboard/config.json` |
| 剪切板默认预览、回车/Alt+回车 | `ClipboardView`、`native-paste.ts` |
| Chrome 书签设置与列表 | `chrome-bookmarks.ts`、`BookmarksView` |
| 设置窗口五页、快捷键冲突、差异、许可 | `src/renderer/src/settings/*` |
| 空/加载/错误/Toast 状态 | 各视图与 `components/ui.tsx` |
| 无障碍、减少动效、高对比 | `aria-*`、焦点环、`prefers-reduced-motion` / `prefers-contrast` |
| 启动器内的设置入口 | `SearchBar` 齿轮按钮（仅全局页）、`⌘,` / `Ctrl+,`、帮助浮层 |
| 插件设置唯一入口 | 顶栏插件图标；插件头部不再有设置按钮 |
| 检查更新与一键安装 | `update-service.ts`、`UpdateChip`、`settings/UpdateCard.tsx` |

## 8. 已知取舍

- 搜索结果分组采用可滚动列表；UI 规范中“每组最多 4 行即分页”没有硬性实现，以避免隐藏结果、保证键盘可达性。
- 视频封面在没有 `ffmpeg` 时使用设计封面，而不是尝试解析视频容器；视频始终不播放。
- 外部插件 UI 为宿主渲染的列表 + 设置 schema，不支持插件自带 React bundle；这是为了跨进程安全和安装可审查性。
- 更新源固定为 GitHub Releases，没有增量差分（每次整包下载）与灰度/回滚开关；`.dmg`/`.deb` 只能下载后交给系统安装器，做不到完全无交互。产物未签名，更新后的首次启动仍受 Gatekeeper / SmartScreen 约束（macOS 安装脚本会清掉下载文件的 quarantine 属性）。
- 更新状态保存在内存里，应用重启后需要重新检查；这是有意的——避免把"已下载"的路径写进工作区配置并在清理后指向不存在的文件。
- 毛玻璃只能靠面板 alpha 把桌面“盖住”，不能真正模糊窗口后面的其他应用：Chromium 透明窗口的 `backdrop-filter` 不采样桌面。要做到系统级背景模糊需要 `NSVisualEffectView` / Mica / KWin 模糊，本期未接入；可调强度也因此没有保留。
- 书签超过 60 行使用固定行高虚拟列表；模糊匹配仍在渲染线程完成，超大书签库（1000+）后续可迁移到 Worker 并做 60ms 防抖。
- 外部插件通过宿主渲染的声明式列表与设置 schema 扩展，暂不支持插件自带页面 bundle，也暂不让插件消费 `esc`（`esc` 始终由宿主路由；剪切板预览不再消费它）。
