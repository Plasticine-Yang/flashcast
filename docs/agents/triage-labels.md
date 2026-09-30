# Triage Labels

保留五个默认 triage 角色，并增加本仓库的完成状态 `done`。

| 角色 | 本地 tracker 状态词 | 含义 |
| --- | --- | --- |
| `needs-triage` | `needs-triage` | 待维护者评估 |
| `needs-info` | `needs-info` | 等待补充信息 |
| `ready-for-agent` | `ready-for-agent` | 需求明确，可由 agent 开发 |
| `ready-for-human` | `ready-for-human` | 需要人工实现 |
| `wontfix` | `wontfix` | 不予实施 |
| `done`（仓库扩展） | `done` | 开发及适用的验证已完成 |

技能要求应用角色或标签时，将对应状态词写入 ticket 顶部附近的 `Status:` 行。每个 ticket 只保留一个状态；更新时替换旧值。

## 完成流转

- Ticket 的要求已实现且适用的验证已完成后，将状态改为 `Status: done`。UI 验证方式遵循根目录 `AGENTS.md`。
- 在 ticket 中记录完成内容及验证结果，便于后续读取。
- 存在未完成要求或阻塞时，保留当前状态并记录原因。
- `done` 是终态；分诊待办和可领取工作列表排除它。完成开发是正常流转，无需为设置 `done` 再次确认。
- Ticket 更新与实现一起提交；提交规则遵循根目录 `AGENTS.md`。
