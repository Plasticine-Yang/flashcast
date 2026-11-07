# Flashcast 主题包开发指南

Flashcast 的主题包（theme pack）是一份 **纯数据** 的 `theme.json`：它只描述颜色、玻璃、阴影与壁纸强度，不包含 JavaScript、远程资源或字体文件。主题按工作区生效，安装后写入 `<workspace>/themes/<id>/`，并纳入工作区 Git 仓库。

- 面向用户的主题列表与内置主题见 [README 的主题章节](../README.md#主题)。
- 可运行的最小示例见 [examples/hello-theme](../examples/hello-theme)。
- 主题契约、令牌构建与校验逻辑在 [`src/shared/theme.ts`](../src/shared/theme.ts)。

## 1. 文件布局

一个可安装的主题源目录只需要两个文件：

```text
hello-harbor/
├── theme.json   # 必需，schema v1
└── LICENSE      # 可选，推荐随包提供
```

源目录里可以有 README、截图或其他文件，但安装器**只复制** `theme.json` 与可选的 `LICENSE`。安装后的工作区布局：

```text
<workspace>/
└── themes/
    └── hello-harbor/
        ├── theme.json              # 主题定义
        ├── LICENSE                 # 可选
        └── .flashcast-source.json  # 安装器写入的来源元数据，用于检查更新
```

`themes/` 会纳入工作区 Git；主题文件会随工作区提交、推送，并在另一台电脑拉取后恢复。机器本地的 `state/`（安装临时目录、剪贴板历史等）不会入仓。

> 主题 ID 必须与安装目录名一致：`themes/<id>/theme.json` 中的 `id` 要等于 `<id>`。

## 2. 完整示例

下面的 `theme.json` 就是 [examples/hello-theme/theme.json](../examples/hello-theme/theme.json)，实现了深色与浅色两套变体，可以直接安装：

```json
{
  "schemaVersion": 1,
  "id": "hello-harbor",
  "name": "Hello Harbor",
  "description": "海港青与深海蓝：一盏安静的青色航标，深色与浅色成对。",
  "version": "1.0.0",
  "author": "Flashcast",
  "license": "MIT",
  "homepage": "https://github.com/Plasticine-Yang/flashcast/tree/main/examples/hello-theme",
  "application": "generated-tokens",
  "variants": {
    "dark": {
      "label": "Harbor Night",
      "palette": {
        "bg": "#071a1e",
        "bgElevated": "#0e262c",
        "surface": "#3d8f94",
        "border": "#63a9ae",
        "text": "#e8f7f5",
        "textSecondary": "#bfe0dd",
        "textTertiary": "#9ec4c2",
        "accent": "#5fd7cf",
        "onAccent": "#ffffff",
        "success": "#6ee7a8",
        "warning": "#f2c46d",
        "danger": "#ff9b8a",
        "focus": "#7ee8dd",
        "accentFill": "#0f766e",
        "accentFillHover": "#0d6b63"
      },
      "overrides": {
        "glassSheen": "radial-gradient(120% 130% at 6% -24%, rgba(95, 215, 207, 0.20), transparent 58%), radial-gradient(90% 110% at 104% 112%, rgba(56, 189, 248, 0.12), transparent 62%)",
        "aurora": "radial-gradient(1100px 720px at 4% -14%, rgba(95, 215, 207, 0.28), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(56, 189, 248, 0.16), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(15, 118, 110, 0.24), transparent 64%), linear-gradient(158deg, #071a1e 0%, #041214 52%, #020c0e 100%)"
      },
      "wallpaperOpacity": 0.2,
      "wallpaperSaturate": 1
    },
    "light": {
      "label": "Harbor Morning",
      "palette": {
        "bg": "#f3faf8",
        "bgElevated": "#e9f4f1",
        "surface": "#26595c",
        "border": "#456f70",
        "text": "#0f2a2c",
        "textSecondary": "#31595b",
        "textTertiary": "#44696b",
        "accent": "#0f6b66",
        "onAccent": "#ffffff",
        "success": "#1f6b45",
        "warning": "#7a4b12",
        "danger": "#9e2b24",
        "focus": "#0b5f5b",
        "accentFill": "#0f766e",
        "accentFillHover": "#0d6b63"
      },
      "overrides": {
        "glassSheen": "radial-gradient(120% 130% at 6% -24%, rgba(15, 107, 102, 0.14), transparent 58%), radial-gradient(90% 110% at 104% 112%, rgba(14, 116, 144, 0.08), transparent 62%)",
        "aurora": "radial-gradient(1100px 720px at 4% -14%, rgba(15, 107, 102, 0.12), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(14, 116, 144, 0.08), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(15, 118, 110, 0.10), transparent 64%), linear-gradient(158deg, #f3faf8 0%, #e9f4f1 52%, #e2efec 100%)"
      },
      "wallpaperOpacity": 0.14,
      "wallpaperSaturate": 0.8
    }
  }
}
```

## 3. 字段参考

### 3.1 顶层字段

| 字段 | 必填 | 说明 |
|---|---|---|
| `schemaVersion` | 是 | 必须为 `1`；当前只有 v1。 |
| `id` | 是 | 2–64 字符，匹配 `/^[a-z0-9][a-z0-9._-]{1,63}$/`；必须等于安装目录名。 |
| `name` | 是 | 展示名，非空字符串。 |
| `description` | 是 | 主题描述；允许空字符串。 |
| `version` | 是 | 主题版本，建议 SemVer；检查更新时按字符串比较是否同一版本。 |
| `author` | 是 | 作者或组织名。 |
| `license` | 是 | 许可标识，如 `MIT`；建议同时在源目录提供 `LICENSE`。 |
| `homepage` | 否 | 主题主页或仓库地址，仅用于展示。 |
| `application` | 否 | 外部主题只能为 `generated-tokens`（默认值）；`bundled-css` 仅保留给内置 Blue Glass。 |
| `variants` | 是 | 至少包含 `dark` 或 `light` 之一；只接受这两个键。 |

### 3.2 `palette`

`palette` 是主题的语义色板，必填 13 个字段，另有 2 个可选字段：

| 字段 | 必填 | 用途 |
|---|---|---|
| `bg` | 是 | 窗口最底层背景色；所有对比度都以它为基准核算。 |
| `bgElevated` | 是 | 菜单、弹窗、Toast、预览等抬升表面。 |
| `surface` | 是 | 生成半透明表面、悬停、选中底色的基色。 |
| `border` | 是 | 生成分隔线、描边与玻璃边缘的基色。 |
| `text` | 是 | 主文字。 |
| `textSecondary` | 是 | 次级文字、说明。 |
| `textTertiary` | 是 | 三级文字、分区标题、时间。 |
| `accent` | 是 | 强调色：匹配高亮、光标、选中图标、焦点扩散的基色。 |
| `onAccent` | 是 | 强调填充（主按钮、开关）上的文字色。 |
| `success` | 是 | 成功、已同步。 |
| `warning` | 是 | 警告、可更新、冲突提醒。 |
| `danger` | 是 | 删除、错误、危险操作；只用于功能语义，不建议做装饰色。 |
| `focus` | 是 | 焦点环；对 `bg` 至少 3:1。 |
| `accentFill` | 否 | 主按钮/开关的填充色；缺省等于 `accent`。建议提供更深的填充色，让白字稳定达到 4.5:1。 |
| `accentFillHover` | 否 | 填充悬停色；缺省为 `accentFill` 混入 10% 黑。 |

所有 `palette` 值必须是**不透明**的 `#rgb` / `#rrggbb` / `#rrggbbaa`（aa 必须为 `ff`）或 `rgb()` / `rgba()` 颜色。带透明度的颜色不要写进 palette，改用 `overrides` 中的半透明令牌，否则对比度校验没有意义。

`surface` 与 `border` 通常不是直接绘制的底色：`buildThemeVariant()` 会把它们与透明度混合，生成 `surface1/2`、`borderSubtle/Strong`、描边与玻璃边缘。选色时优先保证它们在深/浅底上有足够的可见度。

### 3.3 `variants.dark` / `variants.light`

| 字段 | 必填 | 说明 |
|---|---|---|
| `label` | 是 | 该变体的展示名，如 `Harbor Night`；主题画廊与安装确认卡会显示。 |
| `palette` | 是 | 见 3.2。 |
| `overrides` | 否 | 覆盖生成后的 `--fc-*` 令牌，见 3.4。 |
| `wallpaperOpacity` | 否 | 壁纸磨砂层强度，`0`–`1`；深色默认 `0.24`，浅色默认 `0.2`。 |
| `wallpaperSaturate` | 否 | 壁纸层饱和度，`0.4`–`2.5`；深色默认 `1.2`，浅色默认 `1.4`。 |

只实现一个变体也可以：单变体主题会把生效模式锁定到该变体，同时保留用户的系统/浅色/深色偏好（见 3.5）。

### 3.4 `overrides` 与令牌白名单

不写 `overrides` 时，主题完全由 palette 生成。需要更细的材质控制时，可以覆盖任意一个已知令牌；**未知键会直接报错**，不会静默忽略。

令牌名使用 camelCase，由 [`THEME_TOKEN_CSS`](../src/shared/theme.ts) 映射到渲染器消费的 `--fc-*`。当前共 55 个：

```text
窗口与玻璃：window windowSolid glassOpaque frostOpacity wallpaperOpacity
            wallpaperBlur wallpaperSaturate glassSheen glassBorder glassRimTop
            glassRimBottom glassFilter glassFilterSoft scrim previewBg previewBorder
表面与列表：surface1 surface2 surfaceHover surfaceSelected surfaceSelectedStrong
            borderSubtle borderStrong rowSelectedBg rowSelectedRim rowSelectedGlow
文字与强调：textPrimary textSecondary textTertiary accent accentStrong accentSoft
            accentLine accentFill accentFillHover accentFillDeep accentFillHoverDeep
            onAccent
反馈：      success successSoft warning warningSoft danger dangerSoft focusRing
深度与细节：shadowWindow shadowPopover shadowAccent highlightLine aurora
            scrollbarThumb scrollbarThumbHover keycapHighlight checkerA checkerB
```

覆盖值必须是非空字符串，长度不超过 512 字符，并满足以下限制：

- 禁止出现 `{`、`}`、`;`、`@`、`<`、`>`、`url(`、`var(`、`expression(`、`javascript:`、`data:`、`/*`、`*/`。这可以阻止声明逃逸、远程资源加载与表达式注入。
- 只允许这些 CSS 函数：`rgb`、`rgba`、`hsl`、`hsla`、`color-mix`、`linear-gradient`、`radial-gradient`、`blur`、`saturate`、`brightness`、`inset`、`drop-shadow`。
- 主题包不是 CSS 文件：不能写选择器、媒体查询、`@import`，也不能引用图片或字体 URL。

示例：只改选中行的渐变与设置窗口背景：

```json
"overrides": {
  "rowSelectedBg": "linear-gradient(180deg, rgba(15, 118, 110, 0.35), rgba(7, 26, 30, 0.65))",
  "rowSelectedRim": "rgba(95, 215, 207, 0.42)",
  "aurora": "linear-gradient(158deg, #071a1e 0%, #041214 52%, #020c0e 100%)"
}
```

### 3.5 模式支持

- 一个主题可以实现 `dark`、`light`，或两者都实现；没有“自动生成另一套模式”的机制。
- 只实现一个变体时，生效模式锁定到该变体：Dracula / Tokyo Night 这类仅深色主题即使系统是浅色也保持深色。用户的 `system` / `light` / `dark` 偏好**不会被改写**；切回双变体主题后继续生效。
- 仅单变体时，设置 → 外观页会隐藏模式选择器，改为显示一条说明当前锁定模式与偏好已保留的信息条，而不是展示一个禁用的控件。
- 双变体主题在 `system` 模式下跟随操作系统；显式选择 `light` / `dark` 则固定使用对应变体。

### 3.6 壁纸与材质

- `wallpaperOpacity` / `wallpaperSaturate` 只影响启动器读取到的桌面壁纸磨砂层；macOS 无免授权壁纸接口时会退回纯玻璃，这两个值不生效。
- 减少透明度（`prefers-reduced-transparency: reduce`）下，渲染器会强制接近不透明的实色窗口、隐藏壁纸层并关闭玻璃滤镜与内部光源；这些降级由宿主完成，主题不需要（也不应该）自己判断。
- 高对比（`prefers-contrast: more`）下使用 `buildContrastTokens()` 生成的对比度令牌：提高表面/描边 alpha，次级与三级文字提升为主文字色，壁纸强度归零。主题的 `contrastTokens` 由 palette 自动生成，不需要手写。

## 4. 校验规则

主题在安装前、加载时都会走同一套校验（`parseThemeDefinition()` + `validateThemeDefinition()`）。任一错误都会阻止安装或加载，不会出现“部分应用”的主题。

| 检查 | 规则 |
|---|---|
| JSON | 必须是对象；`theme.json` 不超过 256 KB，且不能是符号链接。 |
| `schemaVersion` | 必须为 `1`。 |
| `id` | 匹配 `/^[a-z0-9][a-z0-9._-]{1,63}$/`；不得与内置主题 ID 冲突；加载时还必须与目录名一致。 |
| `variants` | 只允许 `dark` / `light`，且至少定义一个。 |
| palette 必填项 | 13 个必填字段都存在、可解析且不透明。 |
| 对比度 | `text`、`textSecondary`、`textTertiary`、`accent`、`success`、`warning`、`danger` 对 `bg` ≥ 4.5:1；`focus` 对 `bg` ≥ 3:1；`onAccent` 对 `accentFill`（缺省为 `accent`）≥ 4.5:1。允许 0.005 的浮点容差。 |
| `overrides` | 键必须在 55 个令牌白名单内；值满足 3.4 的长度、字符与函数限制。 |
| `LICENSE` | 可选；加载时超过 64 KB 会被忽略，安装时超限文件会被拒绝。 |
| 安装包文件 | 安装时只复制 `theme.json` 与 `LICENSE`；符号链接、目录、超限文件一律拒绝。 |

硬性对比度告警会带上实际比值，例如：

```text
variants.dark.palette.textTertiary: textTertiary must reach 4.5:1 contrast against bg (got 4.21:1)
```

可用以下命令在本地快速核算（与实现使用同一套 WCAG 相对亮度公式）：

```bash
node --experimental-strip-types -e "
import('./src/shared/theme.ts').then(({ contrastRatio }) => {
  console.log(contrastRatio('#bfe0dd', '#071a1e').toFixed(2))
})"
```

### 4.1 Hello Harbor 的对比度

| 检查项 | 深色（Harbor Night） | 浅色（Harbor Morning） |
|---|---|---|
| `text` / `bg` | 16.21:1 | 14.30:1 |
| `textSecondary` / `bg` | 12.69:1 | 7.32:1 |
| `textTertiary` / `bg` | 9.47:1 | 5.70:1 |
| `accent` / `bg` | 10.30:1 | 5.98:1 |
| `success` / `bg` | 11.62:1 | 6.11:1 |
| `warning` / `bg` | 10.97:1 | 6.98:1 |
| `danger` / `bg` | 8.77:1 | 7.02:1 |
| `focus` / `bg` | 12.31:1 | 7.08:1 |
| `onAccent` / `accentFill` | 5.47:1 | 5.47:1 |

## 5. 安装、更新与卸载

### 5.1 安装

入口是 **设置 → 外观 → 安装主题**：

1. 输入 Git 仓库地址（`https://`、`git@`、`ssh://`，或以 `.git` 结尾）或本地目录路径；本地路径支持 `~`。
2. 点击 **检查主题**：Git 源会以 `--depth 1 --single-branch` 克隆到 `<workspace>/state/install-temp/`，本地源直接读取；随后校验 `theme.json`，返回主题 ID、版本、作者、许可、模式与预览。检查结果 10 分钟内有效。
3. 确认后点击 **确认安装**：安装器只把 `theme.json` 与可选的 `LICENSE` 复制到暂存目录，重新校验，写入 `.flashcast-source.json`，再用 rename 原子替换 `themes/<id>/`。已有同名主题时会先备份旧目录，替换失败会自动回滚。
4. 安装完成后主题立即出现在画廊中，但不会自动选中；选中后写入 `config.json` 的 `themeId`。

主题不需要“启用/禁用”：**安装即可用**，要移除就卸载。

### 5.2 更新

在主题画廊中打开工作区主题的 `...` 菜单 → **检查更新**。更新器读取 `themes/<id>/.flashcast-source.json` 中记录的来源，重新拉取并校验：

- 主题 ID 必须与已安装主题一致；
- 版本字符串相同则提示“已是最新版本”；
- 更新同样走暂存 → 校验 → rename 流程，失败时旧主题保持可用；
- 内置主题不能更新或卸载，它们随 Flashcast 版本发布。

手工复制进来的主题没有 `.flashcast-source.json`，更新会返回“没有可用的更新源”；仍可重新复制文件后点击 **重新加载**。

### 5.3 卸载

`...` → **卸载** → 二次确认。卸载会删除整个 `themes/<id>/` 目录：

- 如果卸载的是当前激活主题，`config.json` 的 `themeId` 会先切回 `blue-glass`，界面回退到默认主题；
- 如果卸载操作返回失败，且原主题仍存在，会恢复原来的主题选择；
- 内置主题不能卸载。

### 5.4 错误与重新加载

某个主题目录损坏（JSON 语法错误、ID 与目录名不一致、对比度不达标）时，应用仍可启动：该主题不会进入画廊，设置 → 外观页顶部会显示“部分工作区主题未能加载”并列出目录名与具体原因。修复文件后点击 **重新加载**，或直接删除该目录。

## 6. 工作区 Git 行为

- 主题是**工作区级**的：`config.json` 的 `themeId` 决定使用哪个主题，`themes/<id>/` 提供主题文件。切换工作区会加载新工作区的 `config.json` 与 `themes/`，因此主题也随之切换。
- `themes/` 与 `extensions/` 都纳入工作区 Git；`state/` 保持机器本地并被忽略。
- 自动提交默认关闭。开启后，Flashcast 只 stage 自己管理的路径：`config.json`、`plugins/`、`themes/`、`extensions/`、`README.md`、`.gitignore`；提交信息默认是 `config: update Flashcast workspace`，主题操作会使用 `theme: install …` / `theme: update …` / `theme: remove …`。
- 安装、更新、卸载主题成功后，在自动提交开启时会请求一次立即提交；关闭时只写入工作区，并提示“提交并推送后可在其他电脑恢复”。
- 只有提交并推送后，另一台电脑拉取同一仓库才能恢复相同的主题与外部插件代码；仅在本机提交不会自动出现在另一台机器。
- 旧版本写入工作区 `.gitignore` 的 `extensions/` 行会在启动时被自动清理，插件代码从此纳入版本管理。

## 7. 主题许可与署名

- `license` 字段必填；建议使用 MIT，并随包提供 `LICENSE`。安装器会复制它，但不会替你生成。
- 改编第三方主题的配色时，请保留其 MIT 许可与署名，并在 `homepage` / 文档中链接上游项目。Flashcast 内置主题的出处：

| 主题 | 许可 | 上游 / 说明 |
|---|---|---|
| Blue Glass | MIT | Flashcast 原创，默认主题。 |
| Dracula | MIT | [draculatheme.com](https://draculatheme.com) |
| Tokyo Night | MIT | [enkia/tokyo-night-vscode-theme](https://github.com/enkia/tokyo-night-vscode-theme) |
| Kanagawa | MIT | [rebelot/kanagawa.nvim](https://github.com/rebelot/kanagawa.nvim) |
| Atelier | MIT | Flashcast 原创；受暖色编辑排版风格启发，与 Claude / Anthropic 无关联。 |
| Everforest | MIT | [sainnhe/everforest](https://github.com/sainnhe/everforest) |

- 不要使用上游主题项目的 logo 或商标作为自己的主题标识；不要使用 Claude / Anthropic 的名称、商标或官方素材暗示关联。主题名称应描述自己的配色，而不是冒充官方主题。
- 主题只改变外观令牌（颜色、玻璃、阴影、壁纸），v1 **不支持**字体、圆角、布局或动效覆盖。不要试图通过 `overrides` 绕过这一点：未知令牌与不受支持的 CSS 值都会被拒绝。

## 8. 排查清单

| 现象 | 可能原因 |
|---|---|
| 安装时提示“主题校验失败” | `theme.json` 语法、必填字段、ID 格式或对比度不达标；错误信息会带字段路径。 |
| 安装后画廊里没有 | 目录名与 `id` 不一致，或 `id` 与内置主题冲突；查看外观页顶部错误条。 |
| 更新提示“没有可用的更新源” | 主题是手工复制进来的，没有 `.flashcast-source.json`。 |
| 更新提示“已是最新版本” | 源目录中的 `version` 与已安装版本相同。 |
| 浅色/深色切换无效 | 主题只有一个变体；切回双变体主题后偏好继续生效。 |
| 设置窗口背景没有变化 | `aurora` 只影响设置窗口；启动器面板还受壁纸是否可读影响。 |
| 高对比下颜色和预期不同 | 高对比使用自动生成的 `contrastTokens`，它会提升表面 alpha 并把次级/三级文字提升为主文字色。 |
