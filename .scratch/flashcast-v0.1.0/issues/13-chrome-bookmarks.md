# 13: 关联 Chrome profile、搜索书签并在 Chrome 打开

Status: ready-for-agent
Category: enhancement

**What to build:** 用户选择本机 Chrome profile，输入中文或英文关键词进入书签搜索，按标题、网址或目录找到书签，回车在选定 Chrome profile 打开。

Blocked by: 07

- [ ] 作为官方功能插件加载，支持 chrome bookmarks 和 chrome 书签两个入口。
- [ ] 各平台允许发现或选择 Chrome 及 profile，本机路径留在本机，关联状态可查看与修改。
- [ ] 读取本地书签并建立可重建索引，按标题、网址和目录搜索；变化后刷新，重启仍可重新关联与检索。
- [ ] 默认操作明确为在 Chrome 打开，正确指定关联 profile；网址和路径不经不安全的 shell 拼接。
- [ ] Chrome 缺失、profile 不可读、文件损坏或启动失败都有可操作反馈，不破坏原书签。
- [ ] 经宿主入口与样本 profile 验证搜索、刷新和启动参数；可运行环境补充真实 Chrome 打开检查，报告覆盖范围。
- [ ] 手动或浏览器交互检查插件界面；适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

不依赖配套 Chrome 扩展，不提供书签编辑。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
