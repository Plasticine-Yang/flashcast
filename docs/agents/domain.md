# Domain Docs

本仓库采用 **single-context** 领域文档布局。

## Before exploring, read these

- 根目录 `GLOSSARY.md`：领域术语及定义。
- `docs/adr/`：读取与当前工作相关的架构决策。

文件不存在时直接继续，无需报告缺失或提前创建占位文档。`domain-modeling` 在术语或决策实际确定后再创建这些文档。

## File structure

- `GLOSSARY.md`：全仓库共享的领域词汇。
- `docs/adr/<NNNN>-<slug>.md`：全仓库的架构决策记录。

## Use the glossary's vocabulary

Issue 标题、重构建议、假设、测试名称等涉及领域概念时，使用 `GLOSSARY.md` 定义的术语，避免它明确排除的同义词。

需要的概念尚未收录时，先确认是否符合项目用语；确有缺口时，为 `domain-modeling` 记录下来。

## Flag ADR conflicts

建议或实现与已有 ADR 冲突时，明确指出相关 ADR 和重新讨论的理由，避免静默覆盖既有决策。
