## Project workflow

- Commit 描述与正文使用中文；可保留 Conventional Commit 类型与 scope。开发完成后 commit。

- 发布版本用 `pnpm release:patch`（加 `-- --watch` 可跟到 Release 创建完成）；失败处置见 `docs/release.md`。

- 使用 worktree 时，放在当前仓库根目录的 `.worktrees/`。

- 产生的临时文件放在 `.agent-tmp/`。