# Issue tracker: Local Markdown

本仓库的 issue 和 spec 使用 `.scratch/` 下的本地 Markdown 文件管理。

## Conventions

- 每个功能一个目录：`.scratch/<feature-slug>/`。
- Spec 路径：`.scratch/<feature-slug>/spec.md`。
- 每个实现 ticket 独立保存为 `.scratch/<feature-slug>/issues/<NN>-<slug>.md`，从 `01` 开始编号，不合并为单一 tickets 文件。
- 每个 issue 顶部附近使用一行 `Status: <state>` 记录唯一状态；状态词及完成流转规则见 `triage-labels.md`。
- 分诊后的 issue 使用 `Category: bug` 或 `Category: enhancement` 记录唯一分类，与状态分开保存。
- 评论和对话历史追加在文件底部的 `## Comments` 下。
- `.scratch/` 中的 spec 和 ticket 纳入 Git 版本管理。

## When a skill says "publish to the issue tracker"

按上述路径创建对应文件，必要时创建目录；实现 ticket 写入 `issues/`，spec 写入 `spec.md`。

## When a skill says "fetch the relevant ticket"

读取用户引用的文件路径。若仅提供编号，在 `.scratch/*/issues/` 查找；若不同功能下存在同号 ticket，先确定所属功能。

## Wayfinding operations

供 `/wayfinder` 使用：一份 map 加上每个问题或任务的独立 child ticket。

- **Map**：`.scratch/<effort>/map.md`，保存 Notes / Decisions-so-far / Fog。
- **Child ticket**：`.scratch/<effort>/issues/<NN>-<slug>.md`，从 `01` 编号；正文记录问题，`Type:` 记录 `research` / `prototype` / `grilling` / `task`。
- **Blocking**：顶部附近使用 `Blocked by: NN, NN`；列出的依赖全部为 `resolved` 或 `done` 时解除阻塞。
- **Frontier**：扫描 `issues/`，选取开放、无阻塞且未认领的 ticket，按编号从小到大处理；`resolved`、`done` 和 `wontfix` 为终态，不再选取。
- **Claim**：开始工作前，将 `Status:` 改为 `claimed` 并保存。这是 wayfinder 的工作状态，不属于 triage 标签。
- **Resolve**：在 `## Answer` 追加结论，并在 map 的 Decisions-so-far 追加摘要及文件链接。研究、原型和讨论 ticket 使用 `Status: resolved`；完成开发及验证的实现任务使用 `Status: done`，遵循 `triage-labels.md` 的完成规则。
