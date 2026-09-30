# 03: 在 macOS 搜索并打开软件

Status: ready-for-agent
Category: enhancement

**What to build:** Apple Silicon 和 Intel Mac 用户通过同一个主窗口搜索、启动本机应用，获得准确的快捷键及权限状态。

Blocked by: 01

- [ ] 发现系统与用户应用目录中的应用，名称、图标和标识可用，支持刷新与失效反馈。
- [ ] 宿主通过 macOS 适配启动目标应用，正确处理应用包与非 ASCII 名称。
- [ ] 快捷键与唤起支持 macOS，保存唤起前应用身份；需要系统权限时给出对应状态和设置入口。
- [ ] macOS runner 验证可执行的平台行为，Apple Silicon/Intel 的构建结果分别记录。
- [ ] 交互环境或权限限制明确列为未覆盖，不把命令调用成功写成用户操作成功。
- [ ] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

应用发布签名在候选安装包 ticket 中处理；本 ticket 验证软件发现与启动行为。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
