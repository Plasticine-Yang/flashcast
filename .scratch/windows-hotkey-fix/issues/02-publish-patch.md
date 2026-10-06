# 发布 v0.3.6 patch Release

Status: done
Category: bug

## 要求

- 将 ticket 01 的修复与三处版本元数据、Cargo.lock、中文发布说明一起冻结在同一源码 SHA。
- 同一 SHA 的完整 CI 成功后，使用仓库发布入口创建并推送 v0.3.6 标签。
- 跟踪 Release 完成，核验公开页面、五个安装包、字节数及下载 SHA256，并提交发布记录。

## Comments

- 2026-10-06：用户授权修复后直接发布 patch；准备 v0.3.6。独立工作区避免混入原工作区尚未提交的 .gitignore 修改。

- 2026-10-06：已冻结源码 `678321679c8fb3f5482d32033addcd8f1245f4bb`，完整 CI 七项成功，Windows 原生热键 API 回归通过。使用仓库入口创建并推送 annotated tag v0.3.6；Release 七项成功。公开页面、六个资产、五个安装包下载字节数、SHA256 和 GitHub digest 全部核对，deb 版本／架构一致。发布记录见 docs/release/v0.3.6.md；完成流转为 done。
