# 页面与控件验收

2026-10-06。保持现有电弧方向，用中性细边框、统一表面和更紧凑的区块关系修正。本轮不新增 UI 单元测试。

## 页面盘点

| 页面 | 控件及状态 | 画面证据目录 |
| --- | --- | --- |
| 主搜索 | 搜索框、设置按钮、正常列表，无逐行 Enter 装饰 | main-search |
| 备忘录插件 | 返回、新建、编辑、删除、确认／取消、空正文禁用保存 | after-confirm、after-editor、confirm-200 |
| 备忘录失败 | 保留正文／标签、错误只显示一次 | editor-error |
| 剪贴板插件 | 正常列表、动作菜单、清空确认 | clipboard-clear |
| Chrome 书签插件 | 正常列表及打开动作 | bookmark-page |
| 全局快捷键 | 文本输入、中性 focus、冲突确认、禁用状态 | settings-hotkey、hotkey-focus、hotkey-confirm |
| 外观主题 | 分段按钮、减少透明度复选框、禁用内置主题、安装输入，检查至页底 | settings-theme、theme-bottom |
| 功能插件 | 开／关 switch、说明与各行间距 | settings-plugins |
| 剪贴板设置 | 数字、隐藏 spinner、整数与范围错误、启用状态、快捷键 | settings-clipboard、after-number、invalid-number |
| 备忘录设置 | 打开动作、插件快捷键、保存／恢复／清除组 | settings-memos、shortcut-focus |
| Chrome 设置 | 正常用户列表、单用户无文件、索引、提示展开、来源与长路径 | settings-chrome、after-chrome-missing、chrome-200、chrome-open |
| 工作区设置 | 路径、URL、用户名、密码，检查至页底 | settings-workspace、workspace-bottom |
| 远端同步 | 信息换行、操作按钮、禁用取消、页底说明 | settings-sync、sync-bottom |
| 变更与提交 | 未选／选中 checkbox、差异、提交 textarea 和页底操作 | settings-changes、after-changes、commit-focus |
| 运行环境 | 能力信息、窄屏换行、页底说明 | settings-capabilities、capabilities-bottom |

页面源代码盘点未发现 select、range、file 等额外表单类型。共享控件样式覆盖 text、password、number、checkbox、textarea、button、summary；没有依靠某一个页面的补丁隐藏浏览器默认外观。

## 实际操作结果

使用 Codex 浏览器从搜索／设置入口操作浏览器内存模拟宿主：

- 数字输入 0、1.5、空容量时，页面内显示中文范围／整数错误，无浏览器验证气泡。修正后错误移除。保留期限 45 通过 ArrowUp 变为 46，容量 800，保存后离开设置小节再进入，重新读取到 46／800。
- 备忘录删除确认初始聚焦确认按钮；Tab 到取消，再 Tab 回 Esc，Shift+Tab 回取消；Escape 关闭。实际清空确认画面也有按钮间距，未执行清空用户数据。
- 复选框 Space 切换：减少透明度保存成功；变更列表首文件从未选变为已选，提交范围更新为 1/3。
- 备忘录保存失败：输入正文「保存失败后保留正文」与标签「工作」后保存，弹窗仍在，正文与标签原样保留；DOM 中仅 1 个 alert。
- 系统快捷键冲突：打开确认框，Tab 从暂不修改移至解除占用并绑定；Escape 关闭并返回解除冲突按钮。未调用真实系统修改。
- 凭据字段输入临时检查值后，用户名与访问令牌标签仍可见；password 保持遮蔽、两字段 appearance 为 none，聚焦无原生 outline。未提交克隆。

## 视觉检查

使用 skill 的 shoot.mjs 取证并逐张看图。10 个设置首屏均检查 640×420 与 480×420；正常列表、错误、弹窗、展开区与长页底部另有画面。深色检查编辑器、确认和数字 focus；删除与 Chrome missing 检查 @2x 细节。所有最终截图报告无控制台错误、横向溢出或未加载图片。

截图工具的 type 是逐字输入，验证数字错误时使用两次 Backspace 后输入 0；带空格的复合选择器必须改成无空格 child selector，早期生成的非目标状态已重截。检查通过的报告只证明截图页面的这些技术项，不能代替状态识别或实际看图。

类型检查 pnpm exec tsc --noEmit 与 git diff --check 通过。仓库未配置独立 lint 命令；没有运行生产构建或全量测试。

## 减法与范围

去掉 Chrome 重复的大标题和冗长首屏状态，常见处理保留一句，排查说明折叠；保留用户名称、目录、账号、管理状态、来源路径与关联操作。保留数字上下键、复选框语义和键盘焦点。确认组 8px gap；输入 32px、按钮 30px 的共同基线。

本次证据来自 Chromium 浏览器与模拟宿主，不代表 Linux Tauri / WebKit 原生窗口已实机验证。增加了 WebKit appearance 与 spinner 规则，但真实桌面焦点、剪贴板／全局快捷键行为不在此次浏览器证据覆盖范围。没有发布或修改实际用户配置。
