# GNOME Wayland 剪贴板后台记录
Status: done
Category: enhancement

## 原因

当前 GNOME 50.1 会话仅向普通客户端公布 `wl_data_device_manager`，现有 watcher 的只读捕获检查返回 data-control 不可用。继续调用 wl-paste 会夺走焦点。

## 验收

- 提供 GNOME Shell 配套扩展，经会话 D-Bus 将复制事件交给平台适配层。
- 捕获文本、富文本、PNG 与文件列表，遵循已有大小限制与自身写入抑制。
- 插件停用、记录暂停、锁屏及扩展停用时停止捕获；缺失时给出安装说明。
- 提供安装入口与文档，不更改无关系统配置。
- 检查桥接协议、错误路径及真实 GNOME 捕获和焦点；如需要重新登录，明确记录。

## 完成

新增按需捕获的 Shell 扩展、GNOME D-Bus 后端、安装脚本与文档。记录有界载荷、来源及格式，暂停/停用显式停止，锁屏停止，进程退出后租约自动停止。停止与轮询序列化，防止暂停后的迟到请求重新续租。保留原生 data-control / X11 路径与失败时的焦点保护。

真实 Mutter / D-Bus / 宿主集成检查通过；生产扩展在独立 GNOME 50.1 的 user 与 ubuntu 模式中加载，实际 watcher 捕获 Shell 选区事件且 compositor 焦点保持。真实 Ctrl+C 登录会话未覆盖，见 [验证记录](../verification.md)。

配套扩展已在用户目录安装并设置启用；首次加载需要用户退出登录再登录。已构建新版宿主，未注销用户会话。
