# GNOME Wayland 剪贴板记录

没有 data-control 的 GNOME 会话使用配套 Shell 扩展，避免 wl-paste 的临时窗口夺走焦点。
普通 Ctrl+C / Ctrl+V 不需要这个扩展。

## 安装

在仓库根目录运行：

```bash
bash scripts/gnome/install-clipboard-bridge.sh
```

首次安装时 GNOME 可能尚未发现新扩展，需要退出登录再登录。随后启动新编译的 Flashcast，
启用剪切板插件并取消暂停，在其它应用复制内容。新增记录应出现在插件页面。
安装脚本保留其它扩展设置，不会注销或重启桌面。

移除支持时运行 `gnome-extensions disable flashcast-clipboard@flashcast.app`。
已有 Flashcast 历史仍可使用。重新启用后重启 Flashcast。

## 行为

- 扩展通过 `Meta.Selection::owner-changed` 和 `transfer_async` 读取文本、HTML、RTF、PNG、文件 URI。
- 扩展不保存历史到磁盘。Flashcast 沿用本机 SQLite 历史、容量、保留期限和自身写入抑制。
- Flashcast 开始轮询后才订阅复制事件，不补录开始、恢复记录之前的剪贴板内容。
- 暂停、停用插件或锁屏时停止读取并清空桥接队列。Flashcast 退出后，两秒租约到期也会停止。
- 单格式文本最多 4 MiB、PNG 最多 16 MiB，队列最多 32 条 / 32 MiB。读取有超时和取消。
- 扩展暂时断开时报告原因，不回退到可能夺焦点的读取方式。

扩展声明兼容 GNOME 45–50 的 ES module 入口；本次实测 GNOME 50.1，其它版本未实测。
其它版本需先验证再加入 metadata。
接口依据 [Mutter Selection](https://gnome.pages.gitlab.gnome.org/mutter/meta/class.Selection.html)。

## 维护检查

```bash
bash scripts/gnome/check-clipboard-bridge.sh
```

该检查在独立 D-Bus 会话中使用真实 Mutter Selection 和真实桥接代码验证复制、暂停、
来源、格式、取消与租约，随后经宿主入口检查捕获和历史。不读取真实剪贴板。
实际桌面焦点与 Shell 扩展加载需要额外检查，不能从该检查推断通过。

```bash
/usr/bin/python3 scripts/gnome/check-shell-clipboard.py
# Ubuntu 的派生会话模式：
FLASHCAST_SHELL_MODE=ubuntu /usr/bin/python3 scripts/gnome/check-shell-clipboard.py
```

后者启动独立总线与临时目录中的 headless GNOME Shell，安装生产扩展，给 GTK 夹具窗口
一次初始焦点，再向 Shell 的真实 Selection 投递自建文字。真实 Linux watcher 读取后
检查 compositor 焦点。Headless 会话没有输入 serial，不能证明普通应用的 Ctrl+C 流程；
该流程仍需在安装扩展后的真实登录会话检查。不会修改当前桌面的设置。
