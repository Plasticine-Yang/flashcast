# 04: 从 CI 下载三平台候选安装包

Status: done
Category: enhancement

**What to build:** 用户和维护者能下载对应提交的候选安装包，在开发过程中安装 Flashcast 并反馈实际平台行为。

Blocked by: 01

- [x] 同一提交生成 Linux x64 AppImage/deb、Windows x64 exe、macOS Apple Silicon 与 Intel dmg。
- [x] 产物可从 Actions 下载，名称包含平台、架构和可追溯版本，构建提交与检查结果关联。
- [x] 运行可用的打包、包内容检查及安装/启动检查，输出其实际覆盖范围；只有创建压缩文件不算安装通过。
- [x] macOS 按适用方式配置 ad-hoc 签名；有凭证时可配置正式签名与公证，Windows 有凭证时可签名。
- [x] 无证书时明确签名状态与安装说明；凭证不写入仓库或日志。
- [x] 候选包作为构建产物交付，不提前发布正式 GitHub Release。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

只依赖可运行应用和 CI，不等待全部插件完成，使其他平台的安装体验能够尽早检查。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-01（续）：实现完成，并在**真实 runner 上跑通了三平台候选包**（CI run `36774016355`）。下面只记录实际跑过的内容。

  本机：Linux x86_64（`uname -r` = 7.0.0-30-generic），会话 Wayland + XWayland。CI：`ubuntu-22.04` / `windows-latest`（Windows Server 2025，内核 3.6.10）/ `macos-latest`（macOS 25.6）。

  ### 做了什么

  **1. bundle 配置（`src-tauri/tauri.conf.json`）**——配置在 scaffold 阶段就已存在，本 ticket 逐项核对，未做修改：
  `productName: "Flashcast"`、`identifier: "dev.flashcast.launcher"`、`version: "0.1.0"`（与 `Cargo.toml` 的 `[workspace.package] version`、`package.json` 一致，检查脚本每次都会断言这三处相等）、`bundle.targets: ["appimage","deb","nsis","dmg"]`、`bundle.windows.nsis.installMode: "currentUser"`（避免 UAC）、`bundle.macOS.signingIdentity: "-"`（ad-hoc）、`build.frontendDist: "../dist"`。Tauri 会忽略与宿主 OS 无关的 target，因此一份配置覆盖三平台；CI 里再用 `--bundles` 显式指定，避免依赖该行为。

  **2. CI 打包 job（扩展 `.github/workflows/ci.yml`）**——新增 `bundle`（4 个矩阵分支）与 `checksums`，**未改动** ticket 01 的 verify / cross-check 矩阵：

  | 分支 | runner | 参数 | 产物 |
  | --- | --- | --- | --- |
  | Linux x64 | `ubuntu-22.04` | `--bundles appimage,deb` | `Flashcast_0.1.0_amd64.AppImage`、`Flashcast_0.1.0_amd64.deb` |
  | Windows x64 | `windows-latest` | `--bundles nsis` | `Flashcast_0.1.0_x64-setup.exe` |
  | macOS arm64 | `macos-latest` | `--target aarch64-apple-darwin --bundles dmg` | `Flashcast_0.1.0_aarch64.dmg` |
  | macOS x64 | `macos-latest` | `--target x86_64-apple-darwin --bundles dmg` | `Flashcast_0.1.0_x64.dmg` |

  - 用 `tauri-apps/tauri-action@v1`（**不是**官方 macOS 签名页仍在写的 `@v0`）；`actions/upload-artifact@v7`（v8 这个 tag 不存在）、`actions/download-artifact@v8`。
  - 两个 macOS 分支都在 arm64 runner 上交叉编译，**没用 `macos-13`**（该标签已不存在）。
  - 打包前 `pnpm install --frozen-lockfile && pnpm build` 并断言 `dist/index.html` 非空——`generate_context!` 在非 dev 构建里嵌入 `frontendDist`，缺了就没有界面。
  - **不创建 Release**：job 级 `permissions: contents: read`，且不传 `tagName` / `releaseId`。前者比「不传 tagName」更强：即使 action 想建 Release 也没有权限。
  - 凭证：macOS 的 `APPLE_CERTIFICATE`/`APPLE_CERTIFICATE_PASSWORD`/`APPLE_SIGNING_IDENTITY`/`APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID`、Windows 的 `WINDOWS_CERTIFICATE`/`WINDOWS_CERTIFICATE_PASSWORD` 只经 step `env` → `$GITHUB_ENV` 传给后续步骤（runner 临时文件），不写仓库、不进日志，并在最后一步删除 Windows 的临时 pfx。`APPLE_SIGNING_IDENTITY` 无证书时写成 `-`，**绝不写空值**（空值会让 Tauri 拿空标识去 `codesign`）；证书与标识只提供其一时提前报错，避免 tauri-bundler 那句晦涩的 `does not match provided identity`。
  - 没有 `needs: verify`：候选包要尽早可用；是否允许发布由 ticket 18 汇总全部必需检查后决定。

  **3. 检查脚本（新增 `scripts/ci/`）**：

  - `check-installers.sh`：逐平台产出 `artifacts/installer-check-<slug>.json`（schemaVersion 1，与 ticket 01 的 `platform_check` 同一套三态「实测通过 / 实测失败 / 未覆盖」）、`.log`、`SHA256SUMS-<slug>.txt`，产物复制进 `artifacts/packages/`。必需产物缺失或体积异常时非零退出（打包失败必须让 job 变红）；安装/启动失败只记入报告并打 `::warning::`，不与「打包成功」混为一谈。
  - `installer-report.sh`：报告数据结构。
  - `verify-sha256sums.sh`：checksums job 下载全部产物后重算 SHA256，与各 job 的 `SHA256SUMS-<slug>.txt` 逐条比对（校验上传/下载链路），产出 `SHA256SUMS.txt` 供 ticket 18 附加到 Release。

  **4. 顺带修掉 `scripts/dev/linux-native-deps.sh`**，否则本机非 root 前缀无法完成 AppImage 打包：linuxdeploy 的 gtk 插件会把 pkg-config 报出的 `libdir` 当成 AppDir 内相对路径做字符串替换，而前缀路径本身含 `/usr/lib/<triplet>`，会让替换错位（`Failed to copy custom files` → `Failed to run plugin: gtk`）；改为改写 `.pc` 后把 `libdir` 还原成系统路径，并导出 `LIBRARY_PATH` 补回链接器需要的 `libfoo.so`，同时补出运行库原名与 `gio/modules`、`gtk-3.0`、`gdk-pixbuf-2.0` 数据目录。

  ### 真实 CI 运行（run `36774016355`，commit `46aed9b`）

  ```
  ✓ 候选安装包 Linux x64 AppImage + deb    7m42s
  ✓ 候选安装包 Windows x64 NSIS exe        8m51s
  ✓ 候选安装包 macOS arm64 dmg             6m23s
  ✓ 候选安装包 macOS x64 dmg (cross)       6m8s
  ✓ SHA256 校验和                          14s
  X Windows x64 / macOS arm64 / macOS x86_64（verify 矩阵）—— ticket 05 的
    crates/flashcast-core/tests/workspace_watch.rs 跨平台自写抑制缺陷，与本 ticket 无关；
    Linux x64 verify 与两个跨目标 cargo check 均 ✓
  ```

  用 `gh run download 36774016355` 取回全部 artifact 后逐项核对（五个包都在 `artifacts/packages/`，文件名含平台、架构与版本）：

  | 产物 | 字节 | SHA256（该次运行） |
  | --- | --- | --- |
  | `Flashcast_0.1.0_amd64.AppImage` | 81 349 112 | `2c10f7f3e777edf111c88426c49dd21c5007efb2564a34b318c60b54b767c641` |
  | `Flashcast_0.1.0_amd64.deb` | 2 628 386 | `48beb6130ae157b23ad771af2137ed6e74bfd9e7cfc83ad4a626270dc74a01ef` |
  | `Flashcast_0.1.0_x64-setup.exe` | 1 909 003 | `ecca29a0dfb3f5a4c77190f0eaa2cbd32c4d27eaf00283a0263739d7cde9f64c` |
  | `Flashcast_0.1.0_aarch64.dmg` | 2 191 458 | `edbad15e31f1c5fca6d79487854fa637d8b773c049606db3a0464c061cdde099` |
  | `Flashcast_0.1.0_x64.dmg` | 2 371 732 | `0b83c840c9e640a08fa8d436334436c6805ab566458a897a2c7619f4e8a8d892` |

  **逐平台检查结果**（来自各 job 上传的 `installer-check-<slug>.json`）：

  - **Linux x64：8 实测通过 / 0 失败 / 0 未覆盖。** 版本三处一致；deb 的 `dpkg-deb -I/-c` 有 `Package: flashcast` / `Version: 0.1.0` / `Architecture: amd64` 与 `usr/bin/flashcast`、`.desktop`、图标；AppImage 可执行位、`--appimage-extract` 结构、**直接运行（FUSE 挂载）**成功、进程存活 10 秒未崩溃。本机复验时走的是 `direct` 图形后端，CI 上走的是 **`xvfb` 回退分支**——两条路径现在都有真实证据。
  - **Windows x64：5 实测通过 / 0 失败 / 0 未覆盖。** `currentUser` 静默安装成功，主程序在 `C:\Users\runneradmin\AppData\Local\Flashcast\flashcast.exe`；启动后存活 10 秒；`uninstall.exe //S` 之后程序被移除。签名一项当时返回「未覆盖」（见下）。
  - **macOS arm64 与 x64：各 5 实测通过 / 1 失败 / 1 未覆盖。**
    通过：dmg 挂载出 `Flashcast.app`，`Info.plist`、可执行文件、`.icns` 齐备；`codesign --verify --deep --strict` 通过且签名标识为 **ad-hoc（`-`）**；解包后的 `.app` 内可执行文件启动后存活 10 秒。
    实测失败：`spctl -a -t exec` **拒绝**——ad-hoc 签名不绕过 Gatekeeper，这是**预期结论而不是构建失败**，报告里已写明用户需要 `xattr -dr com.apple.quarantine /Applications/Flashcast.app` 或在「隐私与安全性」里选择仍要打开。
    未覆盖：公证需要 Apple 开发者账号，本次没有凭证，`stapler validate` 无法进行。
  - `checksums` job 重算 5 个包的 SHA256 并与 4 个平台 job 的 `SHA256SUMS-<slug>.txt` 逐条比对，`sha256-report.txt` 报「全部校验和一致」；job summary 里列出了完整 `SHA256SUMS.txt`。

  ### 真实运行暴露并已修掉的两个问题

  1. `verify-sha256sums.sh` 用「相对输出目录 + `cd "$DIR"`」，把 `SHA256SUMS.txt` 写到了 `artifacts/artifacts/`，等于**没有产出**（比对本身是对的，也没有报错）。现在先把输入/输出目录转成绝对路径，并在写不进去、或 `SHA256SUMS.txt` 为空时直接失败；checksums job 的上传也改成 `if-no-files-found: error`。已用该次运行下载下来的真实 artifact 树本地复现并验证：`SHA256SUMS.txt` 现在落在 `artifacts/SHA256SUMS.txt`，5 个包齐全、与 4 个平台校验和全部一致。
  2. `check_windows_signature` 把 Git Bash 形式（`/d/a/...`）的路径交给 PowerShell，PowerShell 读不到文件，于是签名状态成了「未覆盖」。改用 `cygpath -w` 的 `D:\a\...` 形式，并把 PowerShell 的 stderr 也写进报告，避免下次只看到「返回空」。

  这两处修复**只在本机验证过语法与逻辑，还没有在真 runner 上复跑**。

  ### 本机实测（commit `39177f3`，合并 `feat/flashcast-v0.1.0` tip `4b6f76b` 之后）

  ```bash
  export FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/04-native-deps   # 见「未覆盖」第 4 条
  eval "$(scripts/dev/linux-native-deps.sh)"
  rm -rf dist && pnpm install --frozen-lockfile && pnpm build   # 通过
  cargo test --workspace                                        # 95 通过 / 0 失败
  pnpm tauri build                                              # 退出码 0，2 个 bundle
  scripts/ci/check-installers.sh --platform linux --arch amd64 --slug linux-x64   # 8 通过 / 0 失败 / 0 未覆盖
  ```

  本机产物（`target/release/bundle/`；本仓库是 Cargo workspace，bundle 在仓库根的 `target/`，不是 `src-tauri/target/`）：

  | 文件 | 字节 | SHA256 |
  | --- | --- | --- |
  | `appimage/Flashcast_0.1.0_amd64.AppImage` | 88 197 624 | `f3a6648e9b5fdb9e76c8cff8841cb0c602e4a1d03226f85127ede716288114f4` |
  | `deb/Flashcast_0.1.0_amd64.deb` | 2 652 820 | `8ed6b3c43456930de5b1dce483de93e5423e16f267a2dec1dc16c138cd3f7e6c` |

  注意**同一次提交在不同机器上构建出的 SHA256 不同**（deb 里带文件时间戳，AppImage 里带构建信息，且 CI 用的 ubuntu-22.04 与本机 Ubuntu 26.04 打包进 AppImage 的 WebKit 库也不一样，AppImage 体积 81 MB vs 88 MB）。所以 ticket 18 必须用**同一次运行**的产物生成 `SHA256SUMS.txt`，不能混用其它提交或本地产物。

  **工作流静态验证**：YAML 用 PyYAML 解析通过（`jobs = [verify, cross-check, bundle, checksums]`）；**23 个 `run` 块逐个 `bash -n` 通过**；断言没有 `macos-13`、没有 `upload-artifact@v8`、没有 `tauri-action@v0`。

  ### 仍未覆盖（不要当成通过）

  1. **上面两处修复还没在真 runner 上复跑**（需要再推一次分支触发一次 CI）。已知待确认：checksums job 的 `flashcast-sha256sums` artifact 里应当出现 `SHA256SUMS.txt`；Windows 的签名项应从「未覆盖」变成「未签名 + SmartScreen 说明」。
  2. **有凭证时的签名分支从未执行**：Windows 的导入 pfx → 取指纹 → `--config` 传 `certificateThumbprint`，以及 macOS 的正式签名 + 公证。没有证书可试。另外 `--config` 传绝对路径在 tauri-action 下未经验证（它按 `projectPath` 解析相对路径，我传的是 `$RUNNER_TEMP` 下的绝对路径）。
  3. **Gatekeeper 放行流程未实测**：`spctl` 拒绝是实测的，但「用户按说明放行后能正常打开」没有在真机上验证；本机也没有 macOS 可以试 `xattr`。
  4. **本机非 root 依赖前缀的并发竞态（已实测确认）**：`~/.local/share/flashcast/linux-native-deps` 被所有 worktree 共享，改动前后两个脚本版本会反复覆盖同一批 `.pc` 文件，使本机 AppImage 打包时好时坏。本地验证改用隔离前缀 `FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/04-native-deps` 绕开；合并进 `feat/flashcast-v0.1.0` 后各 worktree 基于新脚本即消失。CI runner 用 apt，不涉及这个前缀。
  5. **本机反复跑检查前要先 `pkill -x flashcast`**：`tauri-plugin-single-instance` 会让新实例在已有实例时立刻以 0 退出，看起来像启动失败；脚本的失败说明里加了这条提示。CI 是干净环境，不受影响。

  ### 后续 ticket（17/18）需要在真 runner 上确认

  - 再跑一次后：四个 `bundle` 分支 + `checksums` 全绿，`flashcast-sha256sums` 里确实有 `SHA256SUMS.txt`（5 行，覆盖全部五个包），Windows 签名项为「未签名 + 安装说明」。
  - 发布时：产物必须来自同一个构建提交，不得混入旧产物；发布说明要写清签名状态、macOS 首次启动的 `xattr` 指令，以及 Windows SmartScreen 的「更多信息 → 仍要运行」。
  - 若届时提供凭证：macOS 应为 `Developer ID Application` + `stapler validate` 通过 + `spctl` 接受；Windows 应为 `Authenticode Valid`。

  **本次提交**：`c97cd40`、`e9d0a66`（本地依赖前缀可打包）、`e4a7328`（新增 bundle/checksums job 与检查脚本）、`48b009f`、`ae801f8`（检查脚本修正）、`46aed9b`（合并 `feat/flashcast-v0.1.0` tip `4b6f76b`）、`39177f3`（ticket 置 done），以及一条「修复(CI)：修掉真实运行暴露的校验和与 Windows 签名检查问题」（记录本次 CI 结果并修 `verify-sha256sums.sh` 的相对输出路径、Windows Authenticode 的路径形式）。均为中文提交说明，未 push、未打 tag、未创建 Release。

- 2026-10-01（集成后复核）：把 ticket 04 合并进 `feat/flashcast-v0.1.0` 后，CI 运行 `36785786369`（提交 `d4e75a9`）**11 个任务全部 success**，两个遗留问题都在真 runner 上得到确认：
  - `flashcast-sha256sums` artifact 中确实出现了 `SHA256SUMS.txt`（5 行，覆盖 AppImage / deb / exe / 两份 dmg），并且 `sha256-report.txt` 显示四个平台各自的 `SHA256SUMS-<slug>.txt` 与汇总文件**逐个一致**。原先「写到 artifacts/artifacts/」的缺陷已确认修复。
  - Windows 的签名项已从「未覆盖」变为 **未签名 + SmartScreen 说明**：`installer-check-windows-x64.json` 的 `signing` 写明「没有提供 Windows 代码签名证书……用户需选择「更多信息 → 仍要运行」」，同时 `installer.version_consistent` / `installer.nsis.present` / `installer.nsis.install` 均为实测通过。
  - 四个打包任务（Linux AppImage+deb、Windows NSIS exe、macOS arm64 dmg、macOS x64 dmg）与 `SHA256 校验和` 在同一运行中全部通过。
  - 一并修复的构建问题：macOS 的 x86_64 是在 arm64 runner 上交叉编译的，`git2` 的 `https` 会拉入 `openssl-sys`，那里只有 arm64 的 Homebrew OpenSSL；现改为 `vendored-openssl` 并为两条 macOS 交叉腿显式导出 `CC/CFLAGS` 的 `-arch x86_64`（verify 矩阵与 bundle 任务都加了）。Windows 侧同时断言 Perl 与 NASM 存在。
  - 注意：`gh run download -n flashcast-sha256sums` 会报 `would result in path traversal`，需用 API 下载该 artifact 的 zip；ticket 18 直接上传产物而不经此路径即可。
