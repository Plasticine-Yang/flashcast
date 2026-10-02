# 01: 发布带三平台安装包的 v0.2.0 GitHub Release

Status: ready-for-agent
Category: enhancement

**What to build:** 下载用户可以在正式 GitHub Release 获取 v0.2.0 的全部目标安装包、SHA256 校验值和中文说明，追溯到已完成验证的代码。

本版是**界面版本**：只改前端（结果列表排版、设置页信息架构、键盘与焦点交互）与版本元数据；平台适配层自 v0.1.0 起未改动。

- [ ] 版本元数据四处一致：`Cargo.toml`（`[workspace.package]`）、`src-tauri/tauri.conf.json`、`package.json`、`Cargo.lock`，并与 tag、安装包文件名一致。
- [ ] 版本 tag 触发必需检查与目标构建，全部成功后才汇总发布；任一必需任务失败不正式发布。
- [ ] 上传 Linux x64 AppImage/deb、Windows x64 exe、macOS Apple Silicon/Intel dmg，文件名清楚且无缺失资产。
- [ ] 逐个重新计算并验证 SHA256；资产来自本次 tag 的同一提交，不混入旧构建。
- [ ] 中文发布说明（`docs/release/v0.2.0.md`）含本版本内容、安装方式、签名状态、平台验证范围、已知限制与校验方法。
- [ ] 发布任务使用最小权限；失败重试不产生重复 Release 或产物版本错配。
- [ ] 实际推送分支与 tag、等待 GitHub Actions 完成、修复必需失败并确认公开 Release 与资产可访问。
- [ ] 把 `main` 快进到发布提交（不改写 `feat/flashcast-v0.1.0` 的历史）。
- [ ] 完成后把本 ticket 置 `done` 并记录公开地址、运行链接与资产核对结果，与实现一起以中文 commit 提交。

## Comments

维护者明确要求把当前分支的 UI 工作合并进 `main` 并发布一个 minor Release；完成条件包含**实际公开发布**，不是只写好流水线或只创建草稿。

### 范围与边界

- 只发布前端变更 + 版本元数据；`crates/` 与 `scripts/ci/` 的实现自 v0.1.0（`c1cf492`）起未改动。
- 因此 `docs/platform/capability-report.md` 及其证据**沿用 v0.1.0**（文件内仍标注 v0.1.0 的提交与运行），发布说明里写明这一点，不声称在 v0.2.0 上重新做过真实 Windows / macOS 桌面验证。
- 本次 tag 的 CI 仍会在三平台 runner 上重跑平台接口检查与安装包检查；每个安装包的 evidence 由本次构建现场产生，发布脚本强制核对其中的 `version` 与 `commit`。

### 本版本内容（`v0.1.0..本次发布提交`）

| 提交 | 内容 |
| --- | --- |
| `a2030c2` | 结果列表采用「舒展」排版；搜索框焦点改由窗口表达 |
| `8cfa6f2` | 结果列表行高由 52px 收到 48px（真实窗口 640×420 下首屏完整可见 6 条） |
| `bd40017` | 修复：鼠标点过结果行后键盘失效、唤起后不聚焦、模拟宿主把命令当启动成功而关窗 |
| `da09d93` | 设置页改成左侧分组目录 + 右侧内容区（10 个区块不再平铺） |
