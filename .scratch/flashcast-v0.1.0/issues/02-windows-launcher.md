# 02: 在 Windows 搜索并打开软件

Status: ready-for-agent
Category: enhancement

**What to build:** Windows 用户通过同一个主窗口搜索和打开本机软件，并获得符合 Windows 环境的快捷键与错误反馈。

Blocked by: 01

- [ ] 在 Windows 上发现常用安装入口的软件并呈现名称、图标和稳定标识，支持重新扫描。
- [ ] 从宿主命令入口启动真实目标，路径中的空格及非 ASCII 字符处理正确，失效入口显示错误。
- [ ] 接入快捷键和窗口唤起，保存原应用身份；冲突或无法操作时提供备用入口和状态。
- [ ] Windows runner 验证可执行的平台集成行为，区分真实执行、测试替身及桌面环境未覆盖项。
- [ ] 共用核心查询行为与紧凑 UI，通过可用的浏览器交互检查，不添加 UI 单元测试。
- [ ] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

Windows runner 构建成功单独记录，不推断快捷键和焦点恢复已通过。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
