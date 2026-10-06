# Flashcast 插件规范 v1 / 主题文档 v2

本规范适用于新插件。共同契约、功能贡献和主题贡献各有版本，插件包版本遵循 SemVer；包版本不能代替 API 兼容声明。宿主当前为 0.3.4。

## 共同契约

所有插件具有 `id`、`name`、`kind`、`version` 和 `contract`。`id` 为稳定标识，允许小写字母、数字、点、下划线、连字符，最长 64 字符；升级不能更换 id。`kind` 当前为 `feature` 或 `theme`。主题文档的贡献类型由安装入口确定，工作区清单统一记录 kind。

```json
{"apiVersion": 1, "hostVersion": ">=0.3.0, <0.4.0", "configVersion": 1}
```

- `apiVersion`：宿主插件契约版本，目前只支持 1。
- `hostVersion`：SemVer 兼容范围，注册／安装时核对实际宿主版本。不兼容的插件不能激活。
- `configVersion`：插件配置版本，目前只支持 1。升级到其他版本需要先实现迁移并升级宿主契约，不能静默读错旧数据。
- 功能插件声明 `keywords`（关键词别名）与 `capabilities`（所需宿主能力）。主题没有原生能力，不运行代码。
- 工作区 `manifest.json` 是启停状态的唯一权威；官方功能插件运行时清单由代码提供。旧清单缺少共同契约时，读取器补入当前基线，不自动改写文件。
- 发布者应在安装／注册前校验兼容范围、贡献结构和配置版本；升级失败保留原包和可用状态。不同 API 主版本需要显式迁移。

## 主题文档 v2

新安装和更新必须使用 `schemaVersion: 2`。必须提供 `palettes.light` 和 `palettes.dark`，两套都独立校验；`appearance` 固定为 `system`，表示具备双模式，用户的深浅偏好写在外观配置中。不能提供单模式 `tokens`，不能由宿主自动反色补另一套。

每套 palette 包含完整的 `color`、`font`、`space`、`radius`、`shadow`、`state`。完整字段见 [可安装模板](examples/paper/theme.json)，Rust `ThemeTokens` 的严格解析与可读性校验是权威。正文／辅助文字要求足够对比，选中／焦点／错误／禁用必须可辨识。字号和间距由宿主固定，不能改变关键控件位置；字体族、颜色、圆角、阴影可变化。按钮前景由宿主选择对比更高的黑／白色。

`pageBackground`、`surface` 和 `accent` 必须是不透明颜色，作为减少透明度、效果不支持时的回退。透明度由表面风格表达，不写入这两个颜色。

`styles` 提供 1–16 种风格，每项包含唯一 `id`、`name`、`renderer`、`light`、`dark`；`defaultStyle` 指向其中一项。风格名称和 id 自由，例如 `paper`／纸面。一种风格时宿主隐藏风格切换器，不显示不适用的玻璃按钮。

当前渲染器为 `solid`、`frosted`、`liquid`。每种风格的两种模式都提供以下参数：

| 参数 | 范围 | 含义 |
| --- | --- | --- |
| fillOpacity | 0.65–1 | 窗口底色遮蔽 |
| blur | 0–40 | 浏览器模糊半径，px；原生模糊由平台提供 |
| saturation | 0–2 | 背景饱和倍率 |
| rim | 0–4 | 液态亮边宽度，px |

正文和辅助文字还必须通过透明阅读区域叠在黑／白桌面上的对比校验；强调色在页面上需达到 4.5:1。风格 id 在升级时应保持稳定，删除已保存的风格会拒绝更新，要求先切换到仍受支持的风格。

`solid` 参数固定为 `1 / 0 / 1 / 0`。液态渲染器在阅读区域增加稳定衬底，不让正文折射。未知渲染器直接拒绝；添加新效果必须实现版本化宿主渲染器和校验，不能靠任意 CSS／JavaScript 绕过契约。主题可以采用实底渲染器实现纸感、金属配色等非玻璃表达，未来扩展不需要把风格名字归为玻璃。

## 外观配置与迁移

```json
{
  "selected": "flashcast.theme.arc",
  "appearance": "system",
  "styles": {"flashcast.theme.arc": "liquid", "example.paper": "paper"},
  "reduceTransparency": false
}
```

优先级：用户减少透明度／平台实底降级 → 当前主题风格 → 当前主题默认风格。降低透明度不覆盖记住的风格。系统外观决定 `system` 的实际模式，不改变主题和风格。

配置工作区关联后，此文件保存于工作区 `theme.json` 并跟随同步；未关联时保存于设备本地 `appearance.json`，重启可恢复。切换工作区以目标工作区为权威，旧的只有 `selected` 的配置可读，缺失新字段使用跟随系统、默认风格、不减少透明度。旧单模式主题保持原外观；外观页提示旧主题需更新才可独立切换深浅。

已有工作区的 v1 主题只作兼容读取；新安装／更新拒绝 v1 单模式包，要求作者交付 v2。无效配置、无效包、无法写入均给出原因，保留上一次可用外观。停用或移除当前主题回到内置电弧，保留深浅偏好和其他主题的风格记忆。内置电弧必须保持启用，作为恢复基线。

## 功能插件

目前功能插件随应用编译注册，尚未提供第三方代码下载／执行 SDK。注册必须经过 `PluginRegistry::try_register` 校验共同契约；受信任的内置注册可使用 `register`，契约不合法会明确失败。新增运行载体时仍需遵守本规范，再定义隔离、安装和更新流程。

- 使用统一 `SearchContext`、`SearchItem`、`Preview`、默认操作和宿主命令；稳定结果 id、正确来源及关键词别名。
- 搜索超时／错误／panic 不影响其他来源；停用后不贡献结果、不产生后台活动。
- 原生能力只能调用宿主授权边界。新增能力须同步实现平台适配、能力报告和拒绝路径，不能直接访问系统绕过宿主。
- 设置、结果和预览使用宿主组件及语义颜色，自动跟随当前深浅与表面风格；不用固定白／黑背景。自带图标、图片、富文本需在深浅环境可辨认，提供必要的替代资源。
- 用户输入、选择和滚动位置应在可恢复失败后保留；成功信息必须来自宿主真实操作。
- 可迁移配置保存工作区；缓存、凭证、设备路径、剪贴板附件保留设备本地。配置升级必须声明版本、验证后原子写入。

## 开发与验收

```sh
# 生成完整的双模式、非玻璃主题模板
cargo run -p flashcast-core --example theme-template > theme.json
# 校验主题文件或包目录
cargo run -p flashcast-core --example theme-template -- check theme.json
```

验证浅色／深色／跟随系统、所有声明风格、减少透明度、系统不支持效果、选中／焦点／错误／禁用、中文长文本和 480 宽窗口。主题与配置逻辑通过宿主入口集成测试；UI 用浏览器或真实桌面检查，不添加 UI 单元测试。玻璃在 Windows/macOS 使用系统模糊加宿主外观；Linux 使用实底回退。液态效果不是 Apple 原生 Liquid Glass，不包含真实几何折射。

## 插件页面、命令与快捷键

详见 [ADR 0003](../adr/0003-plugin-pages-and-commands.md)。关键词只匹配首屏的插件入口，不自动改变页面。`FeaturePlugin::commands()` 默认返回 `PluginCommand::open_page(&manifest)`；需要额外入口时覆盖此方法，命令 ID 必须属于 `flashcast.plugin.<自己的 id>` 或其点分子命名空间。宿主按命令目标调用 `take_scope`，插件内 `SearchContext.query` 是原始搜索词，不再剥去入口关键词。初始和清空查询必须列出全部可用内容。

```rust
fn commands(&self) -> Vec<flashcast_core::PluginCommand> {
    let mut command = flashcast_core::PluginCommand::open_page(&self.manifest);
    command.defaults.linux = Some("Ctrl+Alt+C".into());
    command.defaults.windows = Some("Ctrl+Alt+C".into());
    command.defaults.macos = Some("Control+Command+C".into());
    vec![command]
}
```

按键只通过贡献声明，不直接注册系统监听。用户覆盖保存在 `settings.toml` 的 `commandShortcuts`：每个命令可分别设置 linux、windows、macos；缺失值恢复默认，空字符串关闭该平台绑定。配置页展示实际注册状态，系统拒绝和配置写入失败都必须可见。

在当前仓库增加内置插件：新增 `crates/flashcast-core/src/plugins/<name>.rs`，实现 `FeaturePlugin` 和 `PluginScope`，在 `plugins::register_official` 注册；业务数据和能力执行经宿主边界，界面接入统一插件页面与动作托盘。使用宿主入口测试能力、搜索和失败路径，UI 经浏览器连续操作验收。

在独立仓库写插件：建立受信任的 Rust crate，引入兼容的 core 契约，在 Flashcast workspace 添加依赖和注册调用，再随宿主构建发布。当前没有将任意外部仓库下载安装到运行时的 SDK；扩展独立载体前须先制定隔离和更新契约。
