# 验证与实际边界

## 剪贴板定位

当前系统为 GNOME Wayland。安装的 Flashcast 进程仍在运行；观察到的是窗口失焦隐藏，不能称作已证实的进程崩溃。

GTK 焦点复现窗调用真实 wl-paste，出现 focus-out 和 unexpected_hide=True。baseline 不读取时为 False。直接 watch 探测报告 compositor 未提供 data-control。平台捕获路径加入能力预检后，同复现窗调用真实 Rust watcher 为 False。

预检使用 wl-paste --watch true，它在缺少协议时直接报错，避免普通读取的透明窗口回退。平台缓存探测结果；宿主不启动不可用的后台 pump。原生检查示例不打印剪贴板内容。

参考：上游 [wl-clipboard 发布说明](https://github.com/bugaevc/wl-clipboard/releases)、[焦点回退说明](https://github.com/bugaevc/wl-clipboard/issues/12)。

当前桌面不能新录制后台剪贴板。修复保证启用插件后停止不安全捕获并显示原因，已有历史仍可搜索和管理。原有 Wayland 自动按键注入限制保留；粘贴失败时显示手动粘贴提示。本轮未发版或更新 /usr/bin/flashcast，安装版本仍是旧代码。

## Chrome 空状态

检查当前 Chrome 用户目录及运行参数。Local State 只记录 Default；已关联 Default 正确。该 profile 没有 Bookmarks 或 Bookmarks.bak，所检查的 Chrome/Chromium 用户目录中也没有原生书签文件。不能据此断言浏览器扩展中保存的链接为空。

设置页解释缺失文件，提示新增原生书签后重新读取；如果浏览器里已有书签，核对 chrome://version 中的资料路径。没有生成虚假书签或更改用户 Chrome 文件。

## 后端检查

- flashcast-platform 的 background_ 定向测试：4 通过，覆盖协议缺失、连接失败、正常监听终止及失败缓存禁止所有读取。
- flashcast-core 的 clipboard、memos、paste 三组入口测试：40 + 18 + 19 = 77 通过。剪贴板不可捕获时保留插件和历史、不轮询；标签正文优先，粘贴目标沿用唤起前应用。
- 修改测试断言先观察标签排序失败；平台探测两个失败分支先观察失败，随后修复通过。
- TypeScript：pnpm exec tsc --noEmit 通过。Rust 变更用 rustfmt 格式化。git diff --check 通过。
- 没有添加 UI 单元测试，没有运行生产构建或全量测试。

## 浏览器交互

使用 HostApi 浏览器替身，不把替身反馈当作原生自动粘贴验证。

通过工作区关联后新建、编辑、保存后重新查询、标签与关键词冲突选择、删除确认、保存失败保留草稿。检查 Ctrl+Enter 保存、Escape 取消及编辑期间禁用搜索/设置。检查 640×420、480×420 与 200% 证据、深浅外观、长标题/正文及无工作区禁用态。

应用页面截图报告无控制台错误、横向溢出或加载失败图片。静态图标与说明页截图工具报告 favicon.ico 404，图标与图表正常加载；该无关请求不作为应用检查通过记录。

HTML renderer 生成八个面板、零写作警告。浏览器核实说明页主题切换及图标并排/单张切换。插件说明基于当前源码：统一 Rust 查询契约，仍编译进宿主；没有独立功能插件安装运行时或稳定 SDK。外部仓库通过 Cargo 依赖集成的工作流与未来所需能力分别说明。
