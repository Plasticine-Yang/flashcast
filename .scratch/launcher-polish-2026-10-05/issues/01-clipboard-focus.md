# 剪贴板后台读取导致启动器隐藏
Status: done
Category: bug

## 验收

启用后台捕获后，唤起窗口不会因读取工具夺走焦点而隐藏。无法无焦点读取时保留插件与历史并准确展示原因，不用临时窗口持续夺焦点。

## 复现

`/usr/bin/python3 .scratch/launcher-polish-2026-10-05/focus-repro.py poll`：FAIL，unexpected_hide=True。
同脚本 baseline：unexpected_hide=False。安装进程仍在运行。

## 完成

完成协议预检、宿主启动降级与四个平台/一个宿主回归场景。真实 watcher 焦点复现不再隐藏。当前 GNOME 不支持后台新录制，已有历史仍可用；详见 ../verification.md。
