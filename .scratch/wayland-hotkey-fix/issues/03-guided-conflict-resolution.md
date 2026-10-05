# GNOME Alt+Space 冲突的产品引导与修复

Status: done
Category: enhancement

## 需求与选定方案

当目标快捷键为 Alt+Space 时，检测 GNOME 窗口菜单占用，提供明确确认后的解除按钮；不能自动处理时显示可执行的手动步骤。2026-10-05 用户选定设置页内处理，授权开发完成后发布 patch。

原型来源：分支 `codex/prototype-hotkey-conflict`，提交 `f6cd01f`；设计说明位于该分支 `.scratch/hotkey-conflict-prototype/design-notes.md`。原型保留在独立 worktree，不进入产品或发布包。

## 完成内容

- GNOME Wayland 下只读检测 Alt+Space 占用、实际系统绑定、无法读取和系统能力；不在启动时修改系统设置。
- 设置页显示目标按键与当前绑定，确认弹窗说明两项系统修改。只移除窗口菜单的 Alt+Space/Mod1+Space，其他组合及 Flashcast 其他操作保留。
- 修改前在本机数据目录保存撤销记录；备份失败不改系统。通过 GNOME RebindShortcuts 应用绑定，不依赖门户 preferred_trigger。
- 系统拒绝修改时尝试恢复旧值，并将恢复结果显示给用户。撤销记录跨重启保留；系统之后被再次修改时拒绝覆盖。
- 无授权、缺少 schema、缺少重新绑定接口、设置只读或检测未知时提供手动步骤；可改用 Ctrl+Alt+Space。
- 操作在工作线程执行，界面显示处理中；确认弹窗支持 Escape、焦点返回和取消；减少动效偏好停用动画。
- 系统更新后重新注册以获取门户绑定说明。设置保存或注册成功不等于物理按键唤起通过，界面请用户实际确认。

## 验证

- `cargo test --workspace --quiet`：375 项通过，0 失败，8 项按既有桌面/子进程/显式原生诊断规则忽略。
- 新宿主集成检查使用独立 D-Bus 和内存 GSettings，包含成功、服务缺失、首次重新绑定失败后恢复、备份失败、目标与实际绑定不一致、未授权空绑定、非 GNOME、schema 缺失，以及菜单/其他应用操作被再次修改后的撤销保护。保留 Shift+Alt+Space 与其他按键；父测试显式执行子进程检查。
- `CHOKIDAR_USEPOLLING=1 pnpm ui-check`：80 项浏览器交互检查通过；新增六项连续操作检查，不添加 UI 单元测试。
- `pnpm build`、`cargo build -p flashcast`、15 项 `pnpm release:check` 通过，`git diff --check` 通过。
- 真实 GNOME Wayland 中显式执行 `gnome_live_cycle`：检测占用 → 自动处理 → 撤销全部通过，诊断前的系统设置已恢复。此项验证真实 GSettings 和 GNOME 服务，不等于物理按键唤起、输入焦点实测。
- 视觉证据见本目录上一级 `evidence/`：640×420 设置页与确认、深色、手动指引；480×420 窄窗口和处理中截图在本机 `artifacts/hotkey-conflict/`。沿用现有主题变量，没有新增插画或外部素材。局部 UI 自查，无独立评分。

## 实现依据与限制

GNOME 系统设置使用的原生接口：
- https://raw.githubusercontent.com/GNOME/gnome-control-center/main/panels/applications/org.gnome.GlobalShortcutsRebind.xml
- https://raw.githubusercontent.com/GNOME/gnome-control-center/main/panels/applications/cc-application-shortcut-dialog.c

自动处理限于具备上述 schema、可写设置和 GNOME 接口的 Wayland 会话；其他环境保持原有注册流程。仅处理已保存的 Alt+Space，编辑草稿需先保存。手动系统菜单的名称因 GNOME 版本而异。
