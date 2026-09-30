# 06: 安装主题插件并切换外观

Status: done
Category: enhancement

**What to build:** 用户在设置中管理主题插件，切换浅色、深色或跟随系统，并可安装本地主题包；所选主题随配置工作区保存。

Blocked by: 05

- [x] 建立可识别插件标识、种类、版本和启用状态的插件清单，默认主题通过该清单加载。
- [x] 默认提供浅色、深色和跟随系统，本地主题包可校验、安装、选择和移除。
- [x] 主题为声明式数据，使用统一颜色、字体、间距、圆角、阴影和状态语义，不运行主题代码。
- [ ] 切换后搜索、列表、预览和设置保持可读，焦点与选中状态可辨识，布局和操作位置稳定。
- [x] 无效主题显示原因并保留可用外观；重启与工作区切换后正确恢复。
- [x] 不让主题改变高频键盘操作的无动画规则，尊重减少动态效果设置。
- [x] 手动或浏览器交互检查全部主题和系统缩放，不添加 UI 单元测试；通过宿主入口检查主题配置保存。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

第 4 项未勾选：内容预览区（preview）在 v0.1.0 的 UI 里**尚未实现**，因此「预览保持可读」没有可验证对象。已覆盖的是搜索列表与设置页的可读性、焦点/选中/错误的可辨识性与跨主题布局一致性；预览落地后会自动继承同一组文字与状态语义（见 Comments 的未覆盖清单）。

## Comments

此切片用真实主题管理建立共享插件标识、版本及清单；不把插件机制单独拆成无用户行为的基础层 ticket。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（实现完成）：主题插件落地。设计取舍与实现要点：

  - **清单是唯一权威**：`crates/flashcast-core/src/manifest.rs` 定义插件清单（标识、种类、版本、启用状态、来源），写入工作区 `manifest.json`（原子写入 + 自写抑制）。浅色 / 深色 / 跟随系统三个默认主题就是清单条目，主题解析路径是「清单条目 → ThemeLibrary 按 id 取文档 → 解析 token」，没有按 id 硬编码的分支。
  - **只读工作区**：关联已有 Git 仓库**不写任何文件**（`select_workspace` 与启动恢复都只读）；只有 `init_workspace`（应用自己刚建的空目录）会写出默认 `manifest.json` 与 `theme.json`。缺少文件时用内存默认值（三个内置主题 + 浅色）。用户真正选择主题、启停插件或安装/移除主题包时才落盘。这样 `workspace_git` 的「干净工作区」用例不会被应用自己弄脏。
  - **声明式数据、几何钉住**：`theme.rs` 定义统一语义 token（颜色 14 项、字体、间距、圆角、阴影、选中/焦点/错误/禁用四类状态）。`deny_unknown_fields` 让 `transition` / `animation` 之类的字段直接报错，主题无法引入动画。间距与字号被校验钉死在宿主基准几何上（`space.*`、`font.body/input/aux`），因此「切换主题不移动控件」是结构保证而不是靠主题自觉。颜色必须能被解析（`#rgb/#rrggbb/#rrggbbaa/rgb()/rgba()`），并逐项校验对比度与状态可辨识度。
  - **CSS 自定义属性是唯一消费方式**：宿主把 token 映射为 `--fc-*` 属性（35 个），UI 只把它们写到根元素（`src/App.tsx` 的 `applyThemeVars`），`src/styles.css` 全面改用这些属性（颜色、字体族、字号、间距、圆角、阴影、状态、禁用透明度），不再有硬编码颜色与内边距；`:root` 只保留与内置浅色一致的兜底值。
  - **跟随系统在运行时生效**：UI 监听 `prefers-color-scheme` 的 change，调用宿主 `set_system_appearance`，同一个「跟随系统」主题立刻换成另一套 palettes，不需要重启，也不改变主题选择。
  - **无效主题**：主题状态分两类原因——选中主题解析失败（`theme_error`）与工作区配置层问题（`theme_notice`，例如 `theme.json` 指向不存在的主题）。两者都保留上一次可用 token 与已下发的 CSS 属性，并回传可读中文原因；主题列表把坏掉的主题标成「不可用」并带上原因。切换工作区时以目标工作区为准（缺少清单/主题配置回到内置默认，不残留上一个工作区安装的主题）。
  - **本地主题包**：`install_theme_package` 接受「包含 `theme.json` 的目录」或直接的主题 JSON 文件，校验（`schemaVersion`、标识、外观与 tokens/palettes 的一致、全部颜色可解析、几何钉住、对比度）失败时原样返回中文原因且不改动任何已安装内容与当前外观；成功后写入工作区 `themes/<id>/theme.json` 并登记清单条目。`remove_theme` 只允许移除已安装主题，移除当前选中的主题会回退到浅色并说明原因。

- 2026-10-01：实测命令与结果（全部在当前分支 `ticket/06-theme-plugins`，已并入 `feat/flashcast-v0.1.0` 后重跑）：

  - `cargo test --workspace` → 全部通过：共 172 个测试用例（0 failed），其中本 ticket 新增 `crates/flashcast-core/tests/themes.rs` 14 个用例；合并前基线为 95 个，其余增量来自同时并入的其它 ticket。覆盖：清单默认条目与重启往返、启停持久化、停用不可选、清单驱动可选性、token 完整性与 CSS 变量映射一一对应、token 不含动画语义、跟随系统随系统外观切换、布局几何钉住（改间距/字号/圆角被拒绝）、可读性与状态可辨识（对比度 ≥ 4.5:1，选中/焦点/错误可区分）、无效主题保留可用外观并可修复、主题包校验/安装/选择/移除、未关联工作区拒绝安装、主题随工作区切换与重启恢复、关联已有工作区不写任何文件。
  - `pnpm build` → 通过（`tsc --noEmit && vite build`，无类型错误）。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/06-windows cargo check -p flashcast-platform --target x86_64-pc-windows-msvc` → `Finished`（合并后重跑，通过；`/tmp` 是 16GB tmpfs，构建目录改用 `$HOME/.cache`）。
  - `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/06-macos cargo check -p flashcast-platform --target x86_64-apple-darwin` → `Finished`（合并后重跑，通过）。
  - `pnpm ui-check` → 22/22 通过（0 失败），日志 `artifacts/ui/ui-check.log`。本 ticket 新增 5 项：三个默认主题的切换 + 跨主题布局包围盒一致 + 每个主题的对比度/状态可辨识 + 高频元素 `transition/animation` 全为 0s；跟随系统随 `prefers-color-scheme` 运行时切换；`reduced-motion` 下同样无动画；本地主题包无效（保留外观）/安装/选择/移除；`deviceScaleFactor=2`、视口 640×420 的系统缩放。
  - 本 ticket 的主题截图（均在 `artifacts/ui/`，已被 gitignore）：`14-theme-light.png`、`15-theme-light-list.png`、`16-theme-dark.png`、`17-theme-dark-list.png`、`18-theme-follow-system-light.png`、`19-theme-follow-system-light-list.png`、`20-theme-follow-system-dark.png`、`21-theme-follow-system-dark-list.png`、`22-theme-reduced-motion.png`、`23-theme-invalid-keeps-appearance.png`、`24-theme-installed.png`、`25-theme-installed-list.png`、`26-theme-removed.png`、`27-scaling-200.png`。
  - 顺带修掉一个真实缺陷：设置页的 Escape 之前挂在容器上，点击主题按钮后按钮变成禁用并失去焦点，Escape 就不再生效；改为在 `window` 上监听。

- 2026-10-01：未覆盖项与原因（严格如实）：

  - **内容预览**：v0.1.0 UI 尚无预览区，第 4 项里「预览保持可读」没有验证对象，故未勾选。列表与设置页已实测；预览落地后自动继承同一组 `--fc-text/--fc-text-muted/--fc-surface` 语义。
  - **真实桌面 webview**：`pnpm ui-check` 只覆盖浏览器里的 React UI + 浏览器模拟宿主。主题渲染、系统外观信号（webview 的 `prefers-color-scheme`）、真实 Tauri 窗口下的缩放未在桌面会话中验证；未关联工作区、安装主题包的真实文件写入由 `cargo test` 的宿主入口用例覆盖。
  - **Windows / macOS**：只做了平台层的 `cargo check`（CI 覆盖），没有在 Windows/macOS 上跑浏览器交互检查或桌面检查；本 ticket 的主题逻辑不依赖平台分支，但跨平台渲染未经实测。
  - **主题包来源**：只支持本地目录 / JSON 文件，不支持远端市场或压缩包；安装时不做签名或来源校验（属 Out of Scope）。
  - **`Settings::disabled_plugins` 与清单的关系**：功能插件的启停现在同时存在于设置文件（ticket 01 的字段，外壳启动时应用）与清单（本 ticket 的权威记录）。本 ticket 未收敛这两处；ticket 07 引入官方插件时应在同一处收口，避免两套启用状态。
