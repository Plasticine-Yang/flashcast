# 04: 从 CI 下载三平台候选安装包

Status: ready-for-agent
Category: enhancement

**What to build:** 用户和维护者能下载对应提交的候选安装包，在开发过程中安装 Flashcast 并反馈实际平台行为。

Blocked by: 01

- [ ] 同一提交生成 Linux x64 AppImage/deb、Windows x64 exe、macOS Apple Silicon 与 Intel dmg。
- [ ] 产物可从 Actions 下载，名称包含平台、架构和可追溯版本，构建提交与检查结果关联。
- [ ] 运行可用的打包、包内容检查及安装/启动检查，输出其实际覆盖范围；只有创建压缩文件不算安装通过。
- [ ] macOS 按适用方式配置 ad-hoc 签名；有凭证时可配置正式签名与公证，Windows 有凭证时可签名。
- [ ] 无证书时明确签名状态与安装说明；凭证不写入仓库或日志。
- [ ] 候选包作为构建产物交付，不提前发布正式 GitHub Release。
- [ ] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

只依赖可运行应用和 CI，不等待全部插件完成，使其他平台的安装体验能够尽早检查。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
