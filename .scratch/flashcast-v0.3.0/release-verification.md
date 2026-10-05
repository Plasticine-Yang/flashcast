# v0.3.0 公开发布核验

- Release：https://github.com/Plasticine-Yang/flashcast/releases/tag/v0.3.0
- 发布时间：2026-10-05T03:15:26Z
- 标签提交：`304a495a3c0d2cb305babc6656d161e3e51e51bd`
- 标签 CI：https://github.com/Plasticine-Yang/flashcast/actions/runs/37258125713
- 全部 12 个任务成功；匿名页面 HTTP 200；非草稿、非预发布。
- 从公开 Release 重新下载五个安装包，逐一重算 SHA256，与下载的 SHA256SUMS 及 API digest（如提供）核对。

| 安装包 | 字节 | SHA256 |
| --- | ---: | --- |
| Flashcast_0.3.0_amd64.AppImage | 84539896 | `3791e9267c5e168a17f87f6168b434d91a43378ec0009341e4a05fd01e03770b` |
| Flashcast_0.3.0_amd64.deb | 6183090 | `11fb627c3212936a6909d3abe81a446f2ad0d3c41109b1e96ad393a69c5422a5` |
| Flashcast_0.3.0_x64-setup.exe | 3005502 | `b5d502a06744dbe9b1e0a4e38c2af2e5e8cd8e8707f30620d606fb8b6d354b77` |
| Flashcast_0.3.0_aarch64.dmg | 5150996 | `bb66789cca174ab55bd54509c982383dd36475be83dd47e6cf3623d36293050a` |
| Flashcast_0.3.0_x64.dmg | 5171626 | `75742ea8b420679edea356ae3c6ffa2345bc4248bb7f35c7b9eed995ae3998f3` |

签名证据：Windows 构建日志明确未配置证书，安装程序不带 Authenticode 签名；macOS 采用 ad-hoc，未公证。原生材质尚未作 Windows/macOS 人工桌面验收，液态是外观近似，Linux 使用实底。

发布前取消未公开候选构建并修正焦点／实时插件入口；上述提交是唯一正式 Release 的源码与安装包来源。下载文件和原始 CI 日志保存在忽略的 artifacts 目录，未提交二进制安装包。
