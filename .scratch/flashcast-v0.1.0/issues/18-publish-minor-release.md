# 18: 发布带三平台安装包的 v0.1.0 GitHub Release

Status: done
Category: enhancement

**What to build:** 下载用户可以在正式 GitHub Release 获取 v0.1.0 的全部首版目标安装包、SHA256 校验值和中文说明，追溯到已完成验证的代码。

Blocked by: 17

- [x] 应用各处版本元数据、v0.1.0 tag、目标提交和安装包内部版本一致，发布前再次核对仓库现有版本。
- [x] 版本 tag 触发必需检查及目标构建，流程等待全部目标成功后汇总发布；任一必需任务失败不会正式发布。
- [x] 上传 Linux x64 AppImage/deb、Windows x64 exe、macOS Apple Silicon/Intel dmg，文件名称清楚且没有缺失资产。
- [x] 生成并验证每个安装包的 SHA256 校验值；资产来自本次对应 tag 的同一目标提交，不能混入旧构建。
- [x] 中文发布说明包含功能、安装方式、签名状态、平台验证范围、已知限制和自动粘贴回退说明。
- [x] 发布任务采用所需的最小 GitHub 权限；签名凭证不泄漏，失败重试不造成重复 Release 或产物版本错配。
- [x] 实际推送实现与版本 tag、等待 GitHub Actions 完成、修复必需失败并确认公开 Release 及资产可访问，不以只写好流水线作为完成。
- [x] 实现和发布前验证以中文 commit 保存；正式 Release 确认完成后将正式 ticket 更新为 done 并以中文 commit 记录实际发布地址与结果。

## Comments

用户已明确要求开发完成后发布 minor GitHub Release；本票完成条件包含实际公开发布，而非仅创建草稿。发布状态记录的后续文档提交不改变已发布 tag 的构建内容。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。

- 2026-10-01（ticket 18 实施完成，Status → done）：**v0.1.0 已正式公开发布**。
  https://github.com/Plasticine-Yang/flashcast/releases/tag/v0.1.0

  **1. 发布结果（全部实测，不是「流水线已写好」）**

  - tag：`v0.1.0`（注解 tag 对象 `9d31d611d6fa59b82a3255f4baf2cb60821aaa3b`，
    指向提交 `c1cf492f5d57d5abccd90e19da39cb27e0d71261`）
  - 触发运行：[36868862685](https://github.com/Plasticine-Yang/flashcast/actions/runs/36868862685)
    → **completed / success**，12 个任务全部 success（含 `发布 v0.1.0 Release`）
  - Release：公开、非草稿、非预发布（`isDraft=false`、`isPrerelease=false`），标题 `Flashcast v0.1.0`，
    6 个资产（5 个安装包 + `SHA256SUMS.txt`）
  - `main` 已快进到发布提交（`d0dbd63..c1cf492`，未改写 feat 的历史）

  | 资产 | 平台 / 架构 | 字节 | SHA256 |
  | --- | --- | --- | --- |
  | `Flashcast_0.1.0_amd64.AppImage` | Linux x64 | 84507128 | `a0752f23ecc2bb8a582463520ac3b53dcdde373713969a8079cfffc52a84bef2` |
  | `Flashcast_0.1.0_amd64.deb` | Linux x64 | 6141834 | `a13807f03a3a62fa3a3ec791cc1f38155f62024a8780c87742a6aa40d6cea202` |
  | `Flashcast_0.1.0_x64-setup.exe` | Windows x64 | 2981957 | `97326b42221acf4a1009b30ccc13308d22d70c1a8b19964b334044069894b059` |
  | `Flashcast_0.1.0_aarch64.dmg` | macOS arm64 (Apple Silicon) | 5119986 | `6a7c4b4c744e7f3a55b1d8b65009e9f57baa12a6a9717da7ad60b8625e0da7bd` |
  | `Flashcast_0.1.0_x64.dmg` | macOS x64 (Intel) | 5140299 | `ea994b5d4f0c010294813a352e70332aed8f42ff188410aefe179b35d3fbb9a2` |
  | `SHA256SUMS.txt` | 校验和 | 469 | 内容就是上表五行 |

  **发布后独立复核**（`/tmp/fc18-verify-release.sh`，全程匿名、不带 token，证明真的公开）：

  ```text
  curl -o /dev/null -w '%{http_code}' https://github.com/Plasticine-Yang/flashcast/releases/tag/v0.1.0
  → 200
  curl -fsSL .../releases/download/v0.1.0/SHA256SUMS.txt → 5 行
  逐个匿名下载 5 个安装包并重算 SHA256 → 5/5 与 SHA256SUMS.txt 完全一致
  gh release view v0.1.0 --json isDraft,isPrerelease,assets → draft=false, prerelease=false, 资产数 6
  ```

  **2. 实现（中文 commit）**

  - `bea8caa` 发布(ticket 18)：新增 `scripts/ci/publish-release.sh`、`docs/release/v0.1.0.md`，
    扩展 `.github/workflows/ci.yml`。
  - `19ecb74` 文档(ticket 18)：发布说明改用 tag 固定链接（相对链接在 Release 页面会 404），
    并把「自动粘贴端到端」的未覆盖范围从只写 Wayland 补成三平台一致。
  - `c1cf492` 修复(剪贴板)：修掉阻塞发布的竞态（见第 3 节）。
  - 本提交：ticket 状态与发布记录。

  **发布路径的四道闸门**（`scripts/ci/publish-release.sh`，任一不满足都不发布）：

  1. 一致性：tag ↔ `Cargo.toml` 的 `[workspace.package].version` ↔ `src-tauri/tauri.conf.json` ↔
     `package.json` ↔ 安装包文件名，并逐个核对四个平台 `installer-check-<slug>.json` 里的
     `version` 与 `environment.commit` 等于本次 tag 提交——**旧提交或旧版本的产物会被拒绝**。
  2. 完整性：五个目标安装包一个不少；产物目录里出现预期外的安装包（别的版本/别的提交）也拒绝发布。
  3. 校验和：逐个**重新计算** SHA256 写出 `SHA256SUMS.txt`（发布时不复用汇总 job 的文件自比，
     而是与它逐条比对；这一点在重试时尤其重要，已专门处理 `--out` 目录的自比陷阱）。
  4. 发布与复核：Release 已存在时走 `gh release edit` + `gh release upload --clobber`
     （不产生重复 Release），发布后用 `gh release view` 复核资产名称、字节数与 digest。

  权限：workflow 顶层 `permissions: contents: read`，只有 `publish` 任务临时提到
  `contents: write`；`publish` 的 `needs: [verify, bundle, checksums]` 保证任一必需任务失败时
  该任务被跳过（未用 `if: always()`）。签名凭证只经 `GITHUB_ENV` 传给打包步骤，未写入仓库或日志。

  **3. 修复的必需失败（这是一次真实的失败重试，过程可追溯）**

  第一次 tag 推送前，`feat/flashcast-v0.1.0` @ `7a2fb13` 的运行
  [36866517578](https://github.com/Plasticine-Yang/flashcast/actions/runs/36866517578)
  在 **Linux x64 的 `cargo test --workspace`** 失败（Windows / macOS / 四个候选包 / 校验和全部 success）：

  ```text
  clipboard::tests::failing_tool_still_reports_its_stderr
  panicked at crates/flashcast-platform/src/clipboard.rs:1650
  必须带上工具的标准错误：写入剪贴板失败：写入剪贴板失败：Broken pipe (os error 32)
  ```

  根因是 ticket 17 新加的用例形状本身有竞态：`sh -c 'echo boom >&2; exit 3'` 不读标准输入，
  「父进程写完输入」与「工具已经退出」谁先谁后不确定；工具先退出时写端拿到 EPIPE，
  而 `write_with_tool` / `write_bytes_with_tool` 立刻返回低层写入错误，把工具自己的标准错误
  （真正可执行的原因）盖住了。本机连跑 30 次都赢了这场竞态，所以只在 runner 上暴露。
  修复（`c1cf492`）：两个写入函数都先记下标准输入的写入错误，等拿到退出状态后再决定原因——
  工具失败时以工具的标准错误优先；只有工具成功却没写完输入时才报写入错误。
  `forked_daemon_holding_the_pipe_does_not_hang_the_writer` 的脚本先 `cat >/dev/null` 读完输入，
  另加 `stdin_broken_pipe_does_not_mask_the_tool_stderr`（`exec 0<&-` 确定性复现 EPIPE）。
  本地验证：`cargo test -p flashcast-platform --lib clipboard` 20 passed / 0 failed，该组连跑 30 次 0 失败，
  `cargo test --workspace` exit 0（29 个套件全 ok）。

  **tag 移动历史（可追溯，且从未发布过中间状态的 Release）**：tag 先落在 `bea8caa`
  （运行 36867517824，发现 36866517578 的阻塞失败后**在发布前**取消）、再落到 `19ecb74`
  （运行 36867738526，同样在发布前取消）、最终落到 `c1cf492`（运行 36868862685 成功并发布）。
  前两次运行都**没有**创建任何 Release（动手前 `gh release list` 为空，最终 Release 是全新创建的），
  因此公开资产只来自 `c1cf492` 这一次构建；三次 tag 运行同属并发组
  `ci-CI-refs/tags/v0.1.0`，旧运行在发布前被取消（`gh run cancel`，结果均为 `cancelled`），
  不存在并发发布，也没有任何一步把旧提交的产物混进来。

  **4. 版本一致性**

  `Cargo.toml`（`[workspace.package].version`）、`src-tauri/tauri.conf.json`、`package.json`
  发布前后都是 `0.1.0`，与 tag `v0.1.0` 和五个安装包文件名里的 `0.1.0` 一致；
  各平台 `installer-check-*.json` 里的 `version` 也是 `0.1.0`、`commit` 是 tag 提交（发布脚本强制）。
  `Cargo.toml` 里的 `repository = "https://github.com/flashcast-app/flashcast"` 与实际远端
  `Plasticine-Yang/flashcast` 不一致（deb 元数据会带上它），不影响本版本身份一致性，
  未在本票改动——留给后续 ticket 决定是否更正。

  **5. 发布说明遵守的约束**（`docs/release/v0.1.0.md`，已逐条对照 ticket 17 与能力报告）

  不声称：真实 Windows / macOS 桌面完成全流程（只写 runner 系统接口检查与安装包检查，
  「不等于人工在真实 Windows / macOS 桌面上走完整流程」）；Wayland 全局快捷键可用
  （写「没有可用的全局快捷键，托盘是唯一入口」）；Wayland 自动粘贴可用（写**实测失败**，
  行为是复制 + 手动粘贴提示）；macOS / Windows 已签名或已公证（写 ad-hoc 未公证 / 未签名，
  并写明首次启动放行方式与 SmartScreen「仍要运行」）。
  实测通过的项照实写：Linux Wayland 上文字 / 图片 / 富文本降级 / 文件列表的剪贴板读写与监听、
  真实软件启动（回车确有其进程）、备忘录标签复制与手动粘贴降级。
  发布说明与 ticket 均要求逐条列出未覆盖项，见下一节。

  **6. 未覆盖项（完整，不得当作通过；逐条原因见 `docs/platform/capability-report.md` 的 (c) 节）**

  1. Wayland 全局快捷键唤起：未覆盖 / 判定不支持；XWayland 下注册成功只算 X11 服务器上的成功，托盘是唯一入口。
  2. 唤起前应用身份（focus.capture）：Wayland 未覆盖；XWayland 下 `id=unknown`。
  3. 自动粘贴端到端：Wayland **实测失败**（复制 + 手动粘贴提示）；Windows / macOS runner 只验证了
     `paste.prepare` 前置条件、刻意未注入真实按键。
  4. 备忘录粘贴到「唤起前应用」的最终内容：没有目标应用可核对，未覆盖。
  5. 剪贴板来源应用推导：X11 需要 xclip（本机未装），Wayland 无公开接口，未覆盖。
  6. 真实 Tauri webview 的视觉 / 缩放核对：只有浏览器截图，未覆盖。
  7. 托盘菜单点击：没有输入注入工具，未覆盖（只验证托盘进程能初始化）。
  8. Chrome 书签在可见窗口里打开：只有 headless 证据（一次性 user-data-dir），未覆盖。
  9. Windows / macOS 真实剪贴板读写：Windows runner 不执行写入；macOS runner 的 HTML/RTF 写入为实测失败（已记录降级），未覆盖。
  10. Windows `CF_HDROP` / macOS AppleScript 文件列表的真实读写：未覆盖（Linux 的 `text/uri-list` 实测通过）。
  11. 有证书时的签名 / 公证分支：从未执行（无凭证）；当前状态就是 ad-hoc / 未签名。
  12. X11 会话下的剪贴板：本机是 Wayland 且无 xclip/xsel，未覆盖，不能用 Wayland 结果推断。

  另外两项平台内降级（按设计，不是隐瞒）：Linux `wl-copy` 一次只能提供一种 MIME 类型，
  恢复富文本时只提供纯文本（载荷仍保存在本机历史）；macOS `pbcopy`/`pbpaste` 只支持纯文本。

  **7. 发布说明的生成方式**

  Release 正文 = 仓库里的 `docs/release/v0.1.0.md`（随 tag 提交冻结，含功能、安装、签名状态、
  平台验证范围、已知限制、校验方法）+ 发布流水线生成的「发布溯源」（tag / 提交 / 版本元数据 /
  运行链接 / 生成时间）与「资产与 SHA256」（逐个资产的平台、字节数、SHA256 与 `SHA256SUMS.txt` 全文）。
