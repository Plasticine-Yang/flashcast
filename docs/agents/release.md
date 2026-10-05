# 发布流程

源码检查和发布分为两个工作流：`ci.yml` 对分支运行三平台、跨目标与浏览器检查；`release.yml` 复用同一 SHA 的成功 CI，只构建安装包、校验并发布。手动运行 Release 只生成候选安装包。

## 冻结源码

1. 完成实现及验收，再提交。界面检查使用 `pnpm ui-check`，宿主检查沿用现有 Cargo 检查。
2. 发布前完成连续操作走查：范围切换后键盘执行、功能插件启停后的即时入口、同名标签、保存失败后重试、深浅模式与风格独立切换。自动浏览器检查覆盖这些路径；原生系统行为按实际验证记录。
3. 审查跨文件状态：设置写入后，搜索入口和预览是否读取同一份实时状态；失败结果是否传回表单；焦点与键盘操作是否连贯。这些需要结合数据流判断。
4. 同步三处版本元数据并保存 `docs/release/vX.Y.Z.md`，提交并推送分支。
5. 执行 `pnpm release:prepare vX.Y.Z`。入口要求干净工作区、版本一致、标签不存在、同一提交的完整 CI 成功；等待期间源码变化会使冻结失效。记录在忽略的 `artifacts/release/frozen-source.json`。

纯文档提交跳过自动 CI；如该提交需要发版，先手动运行 CI。CI 必需任务明确包含浏览器与全部平台检查，PR 检查、缺项、跳过或失败都不能用于发布。

## 发布与核验

执行 `pnpm release:prepare vX.Y.Z --publish`：再次核验冻结条件，创建并推送绑定源码 SHA 的新标签。已存在标签要求使用新版本号。标签推送失败时保留本地标签；检查其指向后重试 `git push origin refs/tags/vX.Y.Z`。

使用 `gh run watch <run-id> --exit-status --interval 30` 持续监听一次 Release 运行。详细日志留在文件，进度只报告任务变化和失败。发布完成后检查公开页面、五个安装包和下载 SHA256，并提交发布记录；记录提交不再触发打包。

`pnpm release:check` 检查发布入口的拒绝条件，不发布也不访问远端。浏览器默认只留代表画面与失败截图；视觉精查用 `FLASHCAST_UI_SCREENSHOTS=all pnpm ui-check`，服务器日志保存在 `artifacts/ui/vite.log`。
