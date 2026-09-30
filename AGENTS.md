# 仓库协作规则

## 开发与提交

- UI 变更通过手动检查或浏览器交互验证，不为 UI 添加任何单元测试。
- 完成开发及适用的验证后，创建 Git commit，提交说明使用中文。
- 开发 ticket 时，完成后按 `docs/agents/triage-labels.md` 流转为 `done`，将 ticket 更新与实现一起提交。

## Agent skills

### Issue tracker

本仓库使用 `.scratch/` 下的本地 Markdown 管理 spec 和 ticket；创建、读取或更新时，先读 `docs/agents/issue-tracker.md`。

### Triage labels

使用五个默认 triage 状态及完成状态 `done`；分诊或变更 ticket 状态时，先读 `docs/agents/triage-labels.md`。

### Domain docs

采用 single-context 布局：根目录 `GLOSSARY.md` 和 `docs/adr/`；探索代码或修改领域文档前，先读 `docs/agents/domain.md`。
