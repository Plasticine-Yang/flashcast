# 发布 v0.3.6 patch Release

Status: ready-for-agent
Category: bug

## 要求

- 将 ticket 01 的修复与三处版本元数据、Cargo.lock、中文发布说明一起冻结在同一源码 SHA。
- 同一 SHA 的完整 CI 成功后，使用仓库发布入口创建并推送 v0.3.6 标签。
- 跟踪 Release 完成，核验公开页面、五个安装包、字节数及下载 SHA256，并提交发布记录。

## Comments

- 2026-10-06：用户授权修复后直接发布 patch；准备 v0.3.6。独立工作区避免混入原工作区尚未提交的 .gitignore 修改。
