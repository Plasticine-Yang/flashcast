# 书签与 GNOME 剪贴板验证

日期：2026-10-06。真实桌面：Ubuntu / GNOME Shell 50.1 / Wayland。

## 复现与修复

- 修复前 `cargo test -p flashcast-core --test chrome account_bookmarks -- --nocapture`：2 个新增测试失败，账号文件未发现、合并结果仍为 4 而不是 5。
- 修复前 `cargo run -p flashcast-platform --example clipboard-capture-check`：返回 data-control 不可用，后台捕获停止。
- 修复后 Chrome 宿主测试 26 项通过；平台发现夹具 7 项通过、2 项环境检查忽略。
- `cargo run -p flashcast-core --example chrome-bookmarks-check -- <本机关联 profile 目录>`：已索引 19 条，local=0、account=19。检查不输出标题、URL 或账号信息，也不修改 Chrome 文件。

## 剪贴板桥接

`bash scripts/gnome/check-clipboard-bridge.sh`：通过，见 [bridge.txt](evidence/bridge.txt)。

独立总线中的生产桥接代码使用真实 Mutter Selection 进行有界异步传输，覆盖文本、HTML、RTF、PNG、URI、来源信息、确认释放、停止/恢复、锁屏保护函数、超大载荷、旧 owner 取消与租约过期。
宿主入口穿透真实 D-Bus，覆盖 SQLite 入库与搜索、格式保持、自身文本写入抑制、暂停、停用/重新启用及桥接断开保留历史。

`/usr/bin/python3 scripts/gnome/check-shell-clipboard.py`：通过，见 [gnome-shell.txt](evidence/gnome-shell.txt)。
`FLASHCAST_SHELL_MODE=ubuntu /usr/bin/python3 scripts/gnome/check-shell-clipboard.py`：通过，见 [ubuntu-shell.txt](evidence/ubuntu-shell.txt)。

这两项使用实际 GNOME Shell 50.1、生产扩展、真实 Linux watcher 与 GTK 夹具窗口。
Headless 环境没有真实 seat 输入 serial，GTK 的 Ctrl+C 选区流程无法复现，且 GTK 活动状态不反映 compositor 焦点。
因此测试扩展给窗口一次初始焦点，再向 Shell 的真实 Selection 投递自建文字，并监听所有后续 compositor 焦点变化。
生产桥接成功捕获，焦点没有变化；这不是对真实登录会话 Ctrl+C 的声明。
其它 GNOME 版本、真实锁屏、其它应用的富文本/图片组合尚未逐一实测。

## 仓库检查与安装

- `cargo test --workspace`：全部可运行测试通过，环境要求的既有检查忽略。
- 最后受影响的宿主 Chrome 26 项和剪贴板 40 项再运行通过；桥接集成再运行通过。
- `pnpm build`、`cargo build -p flashcast`：通过。
- 修改的 Rust 文件通过 rustfmt；Shell/JavaScript 语法检查、`git diff --check` 通过。
- 未修改 UI 组件，没有新增 UI 单元测试。
- 真实用户运行 `bash scripts/gnome/install-clipboard-bridge.sh`：成功安装并设置启用，保留其它扩展设置。
- 当前 GNOME 未发现首次安装的本地扩展；用户需要退出登录后重新登录，再启动新版 Flashcast，启用剪切板、取消暂停并复制文字确认。未注销或重启用户桌面。
