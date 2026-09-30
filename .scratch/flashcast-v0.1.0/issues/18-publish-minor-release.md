# 18: 发布带三平台安装包的 v0.1.0 GitHub Release

Status: ready-for-agent
Category: enhancement

**What to build:** 下载用户可以在正式 GitHub Release 获取 v0.1.0 的全部首版目标安装包、SHA256 校验值和中文说明，追溯到已完成验证的代码。

Blocked by: 17

- [ ] 应用各处版本元数据、v0.1.0 tag、目标提交和安装包内部版本一致，发布前再次核对仓库现有版本。
- [ ] 版本 tag 触发必需检查及目标构建，流程等待全部目标成功后汇总发布；任一必需任务失败不会正式发布。
- [ ] 上传 Linux x64 AppImage/deb、Windows x64 exe、macOS Apple Silicon/Intel dmg，文件名称清楚且没有缺失资产。
- [ ] 生成并验证每个安装包的 SHA256 校验值；资产来自本次对应 tag 的同一目标提交，不能混入旧构建。
- [ ] 中文发布说明包含功能、安装方式、签名状态、平台验证范围、已知限制和自动粘贴回退说明。
- [ ] 发布任务采用所需的最小 GitHub 权限；签名凭证不泄漏，失败重试不造成重复 Release 或产物版本错配。
- [ ] 实际推送实现与版本 tag、等待 GitHub Actions 完成、修复必需失败并确认公开 Release 及资产可访问，不以只写好流水线作为完成。
- [ ] 实现和发布前验证以中文 commit 保存；正式 Release 确认完成后将正式 ticket 更新为 done 并以中文 commit 记录实际发布地址与结果。

## Comments

用户已明确要求开发完成后发布 minor GitHub Release；本票完成条件包含实际公开发布，而非仅创建草稿。发布状态记录的后续文档提交不改变已发布 tag 的构建内容。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
