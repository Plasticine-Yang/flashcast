# 01: 发布带三平台安装包的 v0.2.0 GitHub Release

Status: done
Category: enhancement

**What to build:** 下载用户可以在正式 GitHub Release 获取 v0.2.0 的全部目标安装包、SHA256 校验值和中文说明，追溯到已完成验证的代码。

本版是**界面版本**：只改前端（结果列表排版、设置页信息架构、键盘与焦点交互）与版本元数据；平台适配层自 v0.1.0 起未改动。

- [x] 版本元数据四处一致：`Cargo.toml`（`[workspace.package]`）、`src-tauri/tauri.conf.json`、`package.json`、`Cargo.lock`，并与 tag、安装包文件名一致。
- [x] 版本 tag 触发必需检查与目标构建，全部成功后才汇总发布；任一必需任务失败不正式发布。
- [x] 上传 Linux x64 AppImage/deb、Windows x64 exe、macOS Apple Silicon/Intel dmg，文件名清楚且无缺失资产。
- [x] 逐个重新计算并验证 SHA256；资产来自本次 tag 的同一提交，不混入旧构建。
- [x] 中文发布说明（`docs/release/v0.2.0.md`）含本版本内容、安装方式、签名状态、平台验证范围、已知限制与校验方法。
- [x] 发布任务使用最小权限；失败重试不产生重复 Release 或产物版本错配。
- [x] 实际推送分支与 tag、等待 GitHub Actions 完成、修复必需失败并确认公开 Release 与资产可访问。
- [x] 把 `main` 快进到发布提交（不改写 `feat/flashcast-v0.1.0` 的历史）。
- [x] 完成后把本 ticket 置 `done` 并记录公开地址、运行链接与资产核对结果，与实现一起以中文 commit 提交。

## Comments

维护者明确要求把当前分支的 UI 工作合并进 `main` 并发布一个 minor Release；完成条件包含**实际公开发布**，不是只写好流水线或只创建草稿。

### 范围与边界（发布前记录）

- 只发布前端变更 + 版本元数据；`crates/` 与 `scripts/ci/` 的实现自 v0.1.0（`c1cf492`）起未改动（`git diff --stat v0.1.0..HEAD -- crates/ scripts/` 为空）。
- 因此 `docs/platform/capability-report.md` 及其证据**沿用 v0.1.0**（文件内仍标注 v0.1.0 的提交与运行），发布说明里写明这一点，不声称在 v0.2.0 上重新做过真实 Windows / macOS 桌面验证。
- 本次 tag 的 CI 仍会在三平台 runner 上重跑平台接口检查与安装包检查；每个安装包的 evidence 由本次构建现场产生，发布脚本强制核对其中的 `version` 与 `commit`。

### 本版本内容（`v0.1.0..本次发布提交`，发布前记录）

| 提交 | 内容 |
| --- | --- |
| `a2030c2` | 结果列表采用「舒展」排版；搜索框焦点改由窗口表达 |
| `8cfa6f2` | 结果列表行高由 52px 收到 48px（真实窗口 640×420 下首屏完整可见 6 条） |
| `bd40017` | 修复：鼠标点过结果行后键盘失效、唤起后不聚焦、模拟宿主把命令当启动成功而关窗 |
| `da09d93` | 设置页改成左侧分组目录 + 右侧内容区（10 个区块不再平铺） |
| `8ca57e1` | 版本元数据升到 0.2.0，新增发布说明与本 ticket |

### 发布结果（2026-10-02，Status → done）

**v0.2.0 已正式公开发布**：https://github.com/Plasticine-Yang/flashcast/releases/tag/v0.2.0

- tag：`v0.2.0`（注解 tag 对象 `6e54bd91ca8be55542e5961275b7f3740474fe43`，指向提交 `8ca57e1e4d30abfaff497384d2cb75aa345fe08b`）
- 触发运行：[37009545152](https://github.com/Plasticine-Yang/flashcast/actions/runs/37009545152)
  → **completed / success**，12 个任务全部 success（含 `发布 v0.2.0 Release`），12:54:28Z → 13:07:35Z
- Release：公开、非草稿、非预发布（`isDraft=false`、`isPrerelease=false`），6 个资产（5 个安装包 + `SHA256SUMS.txt`）

| 资产 | 平台 / 架构 | 字节 | SHA256 |
| --- | --- | --- | --- |
| `Flashcast_0.2.0_amd64.AppImage` | Linux x64 | 84511224 | `717af0a7a29d89c404fb340610569cc2ce018db6e0b7b00b7f33c4474b393ef7` |
| `Flashcast_0.2.0_amd64.deb` | Linux x64 | 6143626 | `726a963cf265b7ced87f2aca16cc851aa84c2c72bfa48cde45e7d53366e222a9` |
| `Flashcast_0.2.0_x64-setup.exe` | Windows x64 | 2981024 | `c9c1dba6465811bbe80a4017bfb58dbf1400a652875183caa446f74bcafdf61e` |
| `Flashcast_0.2.0_aarch64.dmg` | macOS arm64 (Apple Silicon) | 5120963 | `dcbb9301dd48f6a02919ce068506be837602b280fbfb8e578fb07e721a66db4c` |
| `Flashcast_0.2.0_x64.dmg` | macOS x64 (Intel) | 5141761 | `91aa5e2cab60cf980e21d8c40371756ffa9a556035651d988d217cf01621830e` |
| `SHA256SUMS.txt` | 校验和 | 469 | 内容就是上表五行 |

**发布后独立复核**（`artifacts/verify-release-v0.2.0.sh`，全程匿名、不带 token，证明真的公开）：

```text
curl -o /dev/null -w '%{http_code}' https://github.com/Plasticine-Yang/flashcast/releases/tag/v0.2.0
→ 200
匿名下载 .../releases/download/v0.2.0/SHA256SUMS.txt → 5 行
逐个匿名下载 5 个安装包并重算 SHA256 → 5/5 与 SHA256SUMS.txt 完全一致
gh release view v0.2.0 --json isDraft,isPrerelease,assets → draft=false, prerelease=false, 资产数 6
```

**本地验证（对齐 CI 必需闸门，在发布提交 `8ca57e1` 上执行）**

- `pnpm install --frozen-lockfile` ✓、`pnpm build` ✓
- `cargo test --workspace` ✓（CI 式假 HOME + 本机依赖前缀）
- `cargo build -p flashcast` ✓、交叉 `cargo check`（`x86_64-pc-windows-msvc` / `x86_64-apple-darwin`）✓
- `pnpm ui-check` **69 项全通过**（含真实窗口 640×420 下的侧栏「10 个区块一屏内全部可见」与区块切换断言）

**平台证据**：沿用 v0.1.0（`crates/` 与 `scripts/` 未改动）。发布说明已明确写出这一点，并在「已知限制」保留原 12 条、注明本版没有重新验证。本次运行在三条 runner 上重跑了 `flashcast-platform-check` 与安装包检查（前者 `continue-on-error`，结论见各 job 摘要与 `installer-check-*.json`）。

**版本一致性**：`Cargo.toml`（`[workspace.package]`）、`src-tauri/tauri.conf.json`、`package.json`、`Cargo.lock` 与 tag、五个安装包文件名均为 `0.2.0`；发布脚本会强制核对并拒绝旧提交或旧版本的产物。

**`main`**：已包含发布提交 `8ca57e1`（从 `12f2f8a` 快进，未改写 `feat/flashcast-v0.1.0` 的历史）；本记录提交也一并进入 `main`。

**已知的非阻塞项**：`scripts/ci/check-installers.sh:121` 的 `[ "$VERSION" != "0.1.0" ]` 现在恒真，每次发布都会多打一条「产物文件名按 ticket 04 要求包含产品名、版本与架构」的提示。它只是提示、不参与失败判定（版本一致性是读 `tauri.conf.json` 比对），但硬编码的旧版本号值得后续清理。
