# 发布 v0.3.4 patch release

Status: done
Category: enhancement

通过仓库发布入口冻结已验证源码；等待同 SHA 完整 CI 成功后创建并推送 v0.3.4 标签；持续监听 Release，核验公开发布、五个安装包与下载 SHA256，提交发布记录。

## Comments

2026-10-06：用户授权开发后发布 patch，版本元数据与发布说明已准备。

2026-10-06：已通过同 SHA 七项完整 CI，并由仓库入口发布 v0.3.4，Release 流水线成功。公开页非草稿、非预发布，五个安装包下载后的 SHA256 全部 OK，deb 版本元数据确认 0.3.4。源码标签保持 09bb2bd5794330fb4ea0b2850afdb2b67213d86b；溯源、校验和和签名限制见 docs/release/v0.3.4.md。
