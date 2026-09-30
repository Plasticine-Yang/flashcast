# 04: 从 CI 下载三平台候选安装包

Status: done
Category: enhancement

**What to build:** 用户和维护者能下载对应提交的候选安装包，在开发过程中安装 Flashcast 并反馈实际平台行为。

Blocked by: 01

- [ ] 同一提交生成 Linux x64 AppImage/deb、Windows x64 exe、macOS Apple Silicon 与 Intel dmg。
- [ ] 产物可从 Actions 下载，名称包含平台、架构和可追溯版本，构建提交与检查结果关联。
- [x] 运行可用的打包、包内容检查及安装/启动检查，输出其实际覆盖范围；只有创建压缩文件不算安装通过。
- [x] macOS 按适用方式配置 ad-hoc 签名；有凭证时可配置正式签名与公证，Windows 有凭证时可签名。
- [x] 无证书时明确签名状态与安装说明；凭证不写入仓库或日志。
- [x] 候选包作为构建产物交付，不提前发布正式 GitHub Release。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

只依赖可运行应用和 CI，不等待全部插件完成，使其他平台的安装体验能够尽早检查。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-01（续）：实现并完成**本机可做**的全部验证。第 1、2 项**故意未勾选**，原因见下。

  **本机环境**：Linux 26.04 x86_64（`uname -r` = 7.0.0-30-generic），会话为 Wayland（`WAYLAND_DISPLAY=wayland-0`，另有 XWayland `DISPLAY=:0`）。所有命令前先 `eval "$(scripts/dev/linux-native-deps.sh)"`。

  ### 做了什么

  **1. bundle 配置（`src-tauri/tauri.conf.json`）**——配置在 scaffold 阶段（ticket 01 之前）就已存在，本 ticket 逐项核对，未做修改：
  `productName: "Flashcast"`、`identifier: "dev.flashcast.launcher"`、`version: "0.1.0"`（与 `Cargo.toml` 的 `[workspace.package] version` 和 `package.json` 一致）、`bundle.targets: ["appimage","deb","nsis","dmg"]`、`bundle.windows.nsis.installMode: "currentUser"`（避免 UAC）、`bundle.macOS.signingIdentity: "-"`（ad-hoc）、`build.frontendDist: "../dist"`。Tauri 会忽略与宿主 OS 无关的 target，因此一份配置即可覆盖三平台。

  **2. CI 打包 job（扩展 `.github/workflows/ci.yml`，未改动 ticket 01 的 verify / cross-check 矩阵）**——新增 `bundle` job（4 个矩阵分支）与 `checksums` job：

  | 分支 | runner | 参数 | 产物 |
  | --- | --- | --- | --- |
  | Linux x64 | `ubuntu-22.04` | `--bundles appimage,deb` | `Flashcast_0.1.0_amd64.AppImage`、`Flashcast_0.1.0_amd64.deb` |
  | Windows x64 | `windows-latest` | `--bundles nsis` | `Flashcast_0.1.0_x64-setup.exe` |
  | macOS arm64 | `macos-latest` | `--target aarch64-apple-darwin --bundles dmg` | `Flashcast_0.1.0_aarch64.dmg` |
  | macOS x64 | `macos-latest` | `--target x86_64-apple-darwin --bundles dmg` | `Flashcast_0.1.0_x64.dmg` |

  - 用 `tauri-apps/tauri-action@v1`（**不是**官方 macOS 签名页仍在写的 `@v0`）。两个 macOS 分支都在 arm64 runner 上交叉编译，**没有用 `macos-13`**（该标签已不存在，会让 job 永远排队）。
  - 打包前 `pnpm install --frozen-lockfile && pnpm build` 并断言 `dist/index.html` 非空——`generate_context!` 在非 dev 构建里会嵌入 `frontendDist`，缺了就没有界面。
  - **不创建 Release**：job 级 `permissions: contents: read`，且不传 `tagName` / `releaseId`。前者比「不传 tagName」更强——即使 action 想建 Release 也没有权限。
  - 凭证处理：macOS 的 `APPLE_CERTIFICATE`/`APPLE_SIGNING_IDENTITY`/`APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID`、Windows 的 `WINDOWS_CERTIFICATE`/`WINDOWS_CERTIFICATE_PASSWORD` 全部只经 step `env` → `$GITHUB_ENV` 传给后续步骤，不写入仓库、不进日志。`APPLE_SIGNING_IDENTITY` 在无证书时写成 `-`（**绝不写空值**：空值会让 Tauri 用空标识去 `codesign`），并且当证书与标识只提供其一时提前报错，避免 tauri-bundler 那句晦涩的 `does not match provided identity`。
  - 没有 `needs: verify`：候选包要尽早可用，是否允许发布由 ticket 18 汇总所有必需检查后决定。

  **3. 检查脚本（新增 `scripts/ci/`）**：

  - `check-installers.sh`：逐平台产出 `artifacts/installer-check-<slug>.json`（schemaVersion 1，与 ticket 01 的 `platform_check` 同一套三态「实测通过 / 实测失败 / 未覆盖」）、`.log`、`SHA256SUMS-<slug>.txt` 与每个包的 `<name>.sha256`；产物复制进 `artifacts/packages/`。必需产物缺失或体积异常时以非零退出（打包失败必须让 job 变红）；安装/启动失败只记入报告并打 `::warning::`，不与「打包成功」混为一谈。
  - `installer-report.sh`：报告数据结构的公共函数库。
  - `verify-sha256sums.sh`：checksums job 下载全部产物后重算 SHA256，并与各 job 自己算出的 `SHA256SUMS-<slug>.txt` 逐条比对（校验上传/下载链路），产出 `SHA256SUMS.txt` 供 ticket 18 附加到 Release。

  **4. 顺带修掉 `scripts/dev/linux-native-deps.sh`**（本机非 root 前缀无法完成 AppImage 打包）：
  - linuxdeploy 的 gtk 插件会把 pkg-config 报出的 `libdir` 直接当成 AppDir 内的相对路径做字符串替换，而前缀路径本身含 `/usr/lib/<triplet>`，会让替换错位，依次在 `Failed to copy custom files` 与 `Failed to run plugin: gtk` 处中断；改为在改写 `.pc` 后把 `libdir` 还原成系统路径，并导出 `LIBRARY_PATH` 指向前缀库目录补回链接器需要的 `libfoo.so`。
  - 补出运行库原名（`libayatana-appindicator3.so.1`）与 `gio/modules`、`gtk-3.0`、`gdk-pixbuf-2.0` 等数据目录（指向系统同名目录，并在下次解压前删除这些链接，避免 `dpkg-deb -x` 写穿链接）。
  - `eval "$(scripts/dev/linux-native-deps.sh)" && pnpm tauri build` 现在能在本机产出 AppImage 与 deb。

  ### 实测命令与结果

  以下结果都在合并 `feat/flashcast-v0.1.0` tip（`4b6f76b`）**之后**重跑得到。

  ```bash
  export FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/04-native-deps   # 见「未覆盖/注意事项」第 4 条
  eval "$(scripts/dev/linux-native-deps.sh)"
  rm -rf dist && pnpm install --frozen-lockfile && pnpm build   # → 通过（tsc --noEmit + vite build，dist/index.html 非空）
  cargo test --workspace                                        # → 95 通过 / 0 失败，退出码 0
  pnpm tauri build                                              # → 退出码 0，2 个 bundle
  scripts/ci/check-installers.sh --platform linux --arch amd64 --slug linux-x64   # → 退出码 0
  ```

  **真实产物**（`target/release/bundle/`，本仓库是 Cargo workspace，bundle 落在仓库根的 `target/`，不是 `src-tauri/target/`）：

  | 文件 | 字节 | SHA256 |
  | --- | --- | --- |
  | `appimage/Flashcast_0.1.0_amd64.AppImage` | 88 197 624 | `f3a6648e9b5fdb9e76c8cff8841cb0c602e4a1d03226f85127ede716288114f4` |
  | `deb/Flashcast_0.1.0_amd64.deb` | 2 652 820 | `8ed6b3c43456930de5b1dce483de93e5423e16f267a2dec1dc16c138cd3f7e6c` |

  **Linux x64 检查 8/8 实测通过**（`artifacts/installer-check-linux-x64.json`，`summary = {实测通过 8, 实测失败 0, 未覆盖 0}`）：

  - 版本号一致性：`tauri.conf.json` / `Cargo.toml(workspace.package)` / `package.json` 均为 `0.1.0`。
  - deb 包内容（`dpkg-deb -I` / `-c`）：`Package: flashcast`、`Version: 0.1.0`、`Architecture: amd64`、`Depends: libayatana-appindicator3-1, libwebkit2gtk-4.1-0, libgtk-3-0`；负载含 `usr/bin/flashcast`（5 053 704 字节）、`usr/share/applications/Flashcast.desktop`、`usr/share/icons/hicolor/{32x32,128x128,256x256@2}/apps/flashcast.png`。
  - AppImage 可执行位存在；`--appimage-extract` 成功，`squashfs-root/` 含 `AppRun`、可执行的 `usr/bin/flashcast`、`.desktop` 与图标。
  - AppImage 直接运行（FUSE 挂载）成功；进程存活 10 秒未崩溃，随后被终止。另外单独验证过 `--appimage-extract-and-run` 回退路径同样能起来（无 FUSE 的容器里走这一条）。

  **签名状态（真实）**：Linux 上 deb/AppImage 不使用代码签名，报告里记为「无需签名」，校验依赖 SHA256。macOS 的 ad-hoc 签名（`bundle.macOS.signingIdentity: "-"` + 无证书时 `APPLE_SIGNING_IDENTITY=-`）已配置，但**没有在 macOS 上实测**：`codesign --verify`、`stapler validate`（公证）、`spctl`（Gatekeeper）三条检查都由检查脚本实现，从未在真 runner 上执行过。ad-hoc 签名**不绕过 Gatekeeper**，用户仍需 `xattr -dr com.apple.quarantine /Applications/Flashcast.app` 或在「隐私与安全性」里选择仍要打开；仓库里没有也不需要写死任何凭证。Windows Authenticode 同理：无证书时脚本会读 `Get-AuthenticodeSignature` 并记为「未签名」+ SmartScreen 提示，但未实测。

  **工作流静态验证**（本机能做的部分）：YAML 用 PyYAML 解析通过；`jobs = [verify, cross-check, bundle, checksums]`；全部 **23 个 `run` 块逐个 `bash -n` 通过**；断言过没有 `macos-13`、没有 `upload-artifact@v8`（该 tag 不存在）、没有 `tauri-action@v0`。

  ### 未覆盖（不要当成通过）

  1. **workflow 从未在 GitHub 上运行过。** 我无权 push，因此 `bundle`（4 个分支）与 `checksums` 都只在本地做过静态校验。因此第 1 项（三平台产物）与第 2 项（从 Actions 下载）**未勾选**——Windows NSIS 与两个 dmg 只能在真 runner 上产出，我无法在 Linux 上交叉打包。只要跑通一次就可以勾选并补上真实文件名与大小。
  2. **Windows x64 NSIS**：`nsis` target、`installMode: currentUser`、静默安装（`//S`，Git Bash 的 MSYS 会把 `/S` 改写成 `S:\`）、安装目录探测、启动与静默卸载检查都写好了，但从未执行。Windows runner 无需预装 NSIS（tauri-bundler 自行下载并 SHA-1 校验），这条依赖 runner 的网络出口。
  3. **macOS arm64/x64 dmg**：hdiutil 挂载、`.app` 内容、`CFBundleIdentifier`/`CFBundleShortVersionString`、`codesign --verify`、公证、`spctl`、以及运行 `.app` 内二进制都在脚本里，但从未执行。两个 dmg 是在 arm64 runner 上交叉编译的，没有用已下线的 `macos-13`。
  4. **本机非 root 依赖前缀的并发竞态（已验证过，不是猜测）**：`scripts/dev/linux-native-deps.sh` 的前缀 `~/.local/share/flashcast/linux-native-deps` 被**所有 worktree 共享**。本机同时有 8 个 worktree，其中 7 个还是改动前的旧脚本；两版脚本会反复覆盖同一批 `.pc` 文件，使我的 AppImage 打包在几分钟内时好时坏（先 `Failed to copy custom files`、后 `Failed to run plugin: gtk`）。本地验证时我改用隔离前缀 `FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/04-native-deps` 绕开。合并进 `feat/flashcast-v0.1.0` 后各 worktree 重新基于新脚本即消失；CI runner 上不存在这个问题（apt 安装，不用这个前缀）。
  5. **`bundle.targets` 里的 `nsis`/`dmg` 在 Linux 上被忽略**：这是 Tauri 的设计（跨 OS 打包不可能），已在 CI 里用 `--bundles` 显式覆盖。
  6. **Windows 有证书时的签名分支**（导入 pfx → 取指纹 → `--config` 传 `certificateThumbprint`）与 **macOS 正式签名 + 公证分支**没有凭证可试，属于未实测代码；无凭证时这两段都不会执行。另外 `--config` 在本仓库的 workspace 布局下未经验证（tauri-action 会按 `projectPath` 解析相对路径，我传的是 `$RUNNER_TEMP` 下的绝对路径）。
  7. **AppImage 启动检查依赖图形环境**：本机有 XWayland 直接跑；CI 的 `ubuntu-22.04` runner 无 `$DISPLAY`，脚本会退回 `xvfb-run`（job 里显式装了 `xvfb` 与 `dbus-x11`），这条分支未实测。同理，若 runner 上 FUSE 不可用，脚本会用 `--appimage-extract-and-run` 并把「FUSE 挂载」单独记为未覆盖。
  8. **本机反复跑检查时注意**：`tauri-plugin-single-instance` 会让新实例在已有 Flashcast 运行时立刻以 0 退出，看起来像启动失败。本机复验前先 `pkill -x flashcast`；脚本的失败说明里也加了这条提示。CI 是干净环境，不受影响。

  ### 后续 ticket（17/18）需要在真 runner 上确认

  - 四个 `bundle` 分支全部 success，且 artifact 里确实有五个安装包（Linux 2 + Windows 1 + macOS 2）。
  - `artifacts/packages/` 里的文件名与 ARCH/版本一致；`installer-check-<slug>.json` 的 `summary` 与 job summary 一致，`measuredFail` 为 0 或给出可解释的原因。
  - `checksums` job 重算的 SHA256 与各 job 的 `SHA256SUMS-<slug>.txt` 一致，`SHA256SUMS.txt` 覆盖全部五个包。
  - 若届时提供了签名凭证：macOS 应为 `Developer ID Application` + `stapler validate` 通过 + `spctl` 接受；Windows 应为 `Authenticode Valid`。
  - ticket 18 发布时：产物必须来自同一个构建提交，不得混入旧产物；发布说明里要写清签名状态与 macOS 首次启动的 `xattr` 指令。

  **本次提交**：`c97cd40`（前缀内补运行库原名链接）、`e9d0a66`（让本地前缀可用于 AppImage 打包）、`e4a7328`（新增 bundle/checksums job 与检查脚本）、`48b009f`（修正检查脚本的路径与报告细节）、`ae801f8`（Windows/macOS 检查更可靠）、`46aed9b`（合并 `feat/flashcast-v0.1.0` tip `4b6f76b`）。均为中文提交说明，未 push、未打 tag、未创建 Release。
