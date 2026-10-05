# 修复失效桌面入口导致的 Wayland 应用身份注册失败

Status: done
Category: bug

## 问题

开发启动时已有 `dev.flashcast.launcher.desktop`，但其中 `Exec=flashcast` 在门户服务的 PATH 中无法找到。原实现只检查文件存在并直接跳过，导致门户返回 `Could not register app ID: App info not found for 'dev.flashcast.launcher'`，尚未进入快捷键授权。

## 验收

- 使用 GIO 检查按 XDG 优先级选中的桌面入口是否能加载。
- 不存在或失效的身份在用户目录重建，使用当前可执行程序或 AppImage 路径。
- 有效的用户入口保留内容；有效系统入口不创建用户覆盖。
- 回归检查通过真实 `LinuxHotkeyManager` 注册入口，验证缺失、失效命令、已删除的绝对路径、AppImage、有效用户和系统入口，以及取消授权、空绑定和门户缺失。
- 在当前真实 Wayland 会话验证原始应用身份错误消失。

## Comments

2026-10-05：用真实门户 Registry.Register 调用复现原始错误；仅把 Exec 改为开发程序的绝对路径后连续两次注册成功，恢复原值后错误再次出现。新增隔离门户检查使用真实 GIO 加载规则，修复前在失效命令场景返回同类错误。

## 完成记录

2026-10-05：以 GIO 的 DesktopAppInfo 加载结果替代文件存在检查。有效用户和系统入口继续使用；缺失或不可加载的身份在用户目录重建，Exec 使用当前进程或 AppImage 路径，不再回退到无法解析的裸命令。写入后再次验证文件能被 GIO 加载，避免把无效身份交给门户。

验证：

- `cargo test -p flashcast-platform --test wayland_portal -- --nocapture` 通过。父测试覆盖九个独立总线场景，穿透真实 LinuxHotkeyManager 与 GIO；子进程测试在常规测试列表中标为 ignored，由父测试显式执行。
- `cargo test --workspace --quiet`：374 项通过、0 项失败、6 项按既有环境/子进程规则忽略。
- 修改的两个 Rust 文件通过 `rustfmt --edition 2021 --check`；`git diff --check` 通过。本机 cargo 未提供 fmt 子命令，使用已安装的 rustfmt 直接检查。
- 当前 Tauri 开发进程自动重启后，实际用户桌面入口已自动修复为 `/home/plasticine/code/projects/flashcast/target/debug/flashcast` 的绝对路径。真实 Registry.Register 返回 `()`，GIO 按应用 ID 加载成功，desktop-file-validate 通过。

本次未改动 UI。原生 Alt+Space 按键唤起与窗口输入焦点未做人工走查，不计入通过项。
