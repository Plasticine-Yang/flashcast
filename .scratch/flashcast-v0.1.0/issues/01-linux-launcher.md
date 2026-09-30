# 01: 在 Linux 唤起、搜索并打开软件，接入三平台 CI

Status: ready-for-agent
Category: enhancement

**What to build:** 用户在当前 Linux 桌面通过快捷键或备用入口打开 Flashcast，输入软件名、选择结果并启动真实软件；每次提交获得三平台构建结果。

Blocked by: None (can start immediately)

- [ ] 提供可运行的桌面应用，默认首屏为紧凑搜索列表，软件名称与图标可辨识，空查询有快速访问项。
- [ ] 发现当前 Linux 环境的软件，支持搜索、方向键选择、回车启动、重新扫描和失败反馈。
- [ ] 快捷键可配置，唤起即进入输入状态；冲突或环境不支持时提供明确反馈及托盘或应用菜单入口。
- [ ] 唤起、关闭、键盘选择与结果更新无动画；中文输入法确认不误执行，旧查询结果不会覆盖新查询，鼠标移动不抢走键盘选择。
- [ ] 保存唤起前应用的身份，明确本地实际 X11/Wayland 会话类型；记录通过与无法覆盖的系统能力。
- [ ] 建立宿主查询/命令入口的非 UI 验证，手动或浏览器交互检查主流程，不添加 UI 单元测试。
- [ ] GitHub Actions 在 Linux x64、Windows x64、macOS Apple Silicon/Intel 上编译，执行可适用的核心检查，并保存诊断信息。
- [ ] 完成适用验证后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

首次切片包含真实 Linux 软件启动，不以静态窗口或空工程作为验收。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
