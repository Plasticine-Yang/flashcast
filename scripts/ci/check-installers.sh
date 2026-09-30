#!/usr/bin/env bash
# ticket 04：候选安装包的内容、安装与启动检查。
#
# 每个平台只检查**本平台 runner 上真实产生**的产物；无法在当前平台执行的检查一律记为
# 「未覆盖」并写明原因。生成压缩文件本身不算安装通过，所以这里逐项区分：
#
#   * 必需产物是否存在、体积是否非平凡（缺失即让 job 变红，这是打包失败）；
#   * 包内容（deb 用 dpkg-deb 拆开看；dmg 用 hdiutil 挂载看 .app；AppImage 解包看 AppDir）；
#   * 真实启动/安装（AppImage 直接运行；Windows 静默安装并启动；macOS 直接运行 .app 内二进制）；
#   * 签名状态（macOS 读 codesign，Windows 读 Authenticode；读不到就写「未覆盖」并说明）。
#
# 用法：
#   scripts/ci/check-installers.sh --platform linux|windows|macos --arch amd64|x64|aarch64|x86_64 \
#       --slug linux-x64 [--target x86_64-apple-darwin] [--bundles-dir DIR] \
#       [--artifacts-dir artifacts] [--work-dir DIR] [--display-mode auto|direct|xvfb|none] \
#       [--launch-seconds 10]
#
# 退出码：必需产物缺失或体积异常时为 1（打包问题必须暴露）；安装/启动检查失败只记入报告，
# 不影响退出码——那属于「该平台的安装体验」结论，由 job summary 与 JSON 报告呈现。

set -uo pipefail

here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/ci/installer-report.sh
. "$here/installer-report.sh"

REPO_ROOT=$(cd -- "$here/../.." && pwd)

PLATFORM=""
ARCH=""
SLUG=""
TARGET=""
BUNDLES_DIR=""
ARTIFACTS_DIR=""
WORK_DIR=""
DISPLAY_MODE="auto"
LAUNCH_SECONDS=10
EXPECTED_OS=""

usage() {
  sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --platform) PLATFORM=${2-}; shift 2 ;;
      --arch) ARCH=${2-}; shift 2 ;;
      --slug) SLUG=${2-}; shift 2 ;;
      --target) TARGET=${2-}; shift 2 ;;
      --bundles-dir) BUNDLES_DIR=${2-}; shift 2 ;;
      --artifacts-dir) ARTIFACTS_DIR=${2-}; shift 2 ;;
      --work-dir) WORK_DIR=${2-}; shift 2 ;;
      --display-mode) DISPLAY_MODE=${2-}; shift 2 ;;
      --launch-seconds) LAUNCH_SECONDS=${2-}; shift 2 ;;
      -h | --help) usage; exit 0 ;;
      *)
        echo "未知参数：$1" >&2
        usage >&2
        exit 2
        ;;
    esac
  done
}

die() {
  echo "错误：$*" >&2
  exit 2
}

# ── 配置一致性 ──────────────────────────────────────────────────────────
PRODUCT=""
VERSION=""
PACKAGES_DIR=""

read_config() {
  local conf="$REPO_ROOT/src-tauri/tauri.conf.json"
  [ -f "$conf" ] || die "找不到 $conf"
  PRODUCT=$(sed -n 's/^[[:space:]]*"productName"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$conf" | head -1)
  VERSION=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$conf" | head -1)
  [ -n "$PRODUCT" ] || die "无法从 $conf 读出 productName"
  [ -n "$VERSION" ] || die "无法从 $conf 读出 version"
}

version_of_cargo_workspace() {
  awk '
    /^\[workspace\.package\]/ { inside = 1; next }
    /^\[/ { inside = 0 }
    inside && /^[[:space:]]*version[[:space:]]*=/ {
      sub(/^[^=]*=[[:space:]]*/, "")
      gsub(/["[:space:]]/, "")
      print
      exit
    }
  ' "$REPO_ROOT/Cargo.toml" 2>/dev/null
}

version_of_package_json() {
  sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    "$REPO_ROOT/package.json" 2>/dev/null | head -1
}

# 版本三处必须一致：tauri.conf.json 决定安装包版本，Cargo 决定二进制版本，
# package.json 决定前端版本。三者漂移会让产物无法追溯。
check_version_consistency() {
  local cargo_version package_version problems=""
  cargo_version=$(version_of_cargo_workspace)
  package_version=$(version_of_package_json)
  [ "$cargo_version" = "$VERSION" ] || problems="${problems}Cargo.toml workspace.package.version=${cargo_version:-（缺失）} 与 tauri.conf.json=${VERSION} 不一致；"
  [ "$package_version" = "$VERSION" ] || problems="${problems}package.json version=${package_version:-（缺失）} 与 tauri.conf.json=${VERSION} 不一致；"
  if [ -n "$problems" ]; then
    add_check "installer.version_consistent" "版本号一致性" "$STATUS_FAIL" "$problems" "sed -n 's/.*\"version\".*/\&/p' src-tauri/tauri.conf.json Cargo.toml package.json"
  else
    add_check "installer.version_consistent" "版本号一致性" "$STATUS_PASS" \
      "tauri.conf.json / Cargo.toml(workspace.package) / package.json 均为 ${VERSION}" \
      "grep -n 'version' src-tauri/tauri.conf.json Cargo.toml package.json"
  fi

  # 二进制名（= mainBinaryName 缺省值 = Cargo 包名）决定 macOS .app 内的可执行文件名与
  # deb 安装到 /usr/bin 下的名字，也决定 Windows 安装目录里的 exe 名。
  if [ "$PRODUCT" != "Flashcast" ] || [ "$VERSION" != "0.1.0" ]; then
    add_note "productName=${PRODUCT}，version=${VERSION}：产物文件名按 ticket 04 要求包含产品名、版本与架构。"
  fi
}

# ── 产物清单 ────────────────────────────────────────────────────────────
# expect_artifact <kind> <相对 bundle 目录的路径> <最小字节数> <说明>
expect_artifact() {
  local kind=$1 rel=$2 min_size=$3 title=$4
  local path="$BUNDLES_DIR/$rel"
  local cmd="ls -l $path && sha256sum $path"
  if [ ! -f "$path" ]; then
    report_mark_missing_artifact
    add_check "installer.${kind}.present" "$title" "$STATUS_FAIL" \
      "未找到 ${rel}（预期路径 ${path}）" "$cmd"
    return 1
  fi
  local size sha
  size=$(file_size "$path")
  sha=$(sha256_of "$path")
  # 把安装包复制到产物目录：上传/下载 artifacts 时目录层级更简单，校验和文件也能和包放在一起。
  local staged="$PACKAGES_DIR/$(basename "$path")"
  if ! cp -f "$path" "$staged"; then
    add_check "installer.${kind}.present" "$title" "$STATUS_FAIL" \
      "找到 ${rel}，但复制到产物目录失败" "$cmd"
    return 1
  fi
  add_artifact "$kind" "$(basename "$path")" "$staged" "$size" "$sha"
  if [ "$size" -lt "$min_size" ]; then
    report_mark_missing_artifact
    add_check "installer.${kind}.present" "$title" "$STATUS_FAIL" \
      "${rel} 只有 ${size} 字节（< ${min_size}），不像完整安装包" "$cmd"
    return 1
  fi
  if ! is_sha256 "$sha"; then
    add_check "installer.${kind}.present" "$title" "$STATUS_FAIL" \
      "${rel} 存在（${size} 字节）但无法计算 SHA256（缺少 sha256sum/shasum/certutil）" "$cmd"
    return 1
  fi
  add_check "installer.${kind}.present" "$title" "$STATUS_PASS" \
    "${rel}，${size} 字节，sha256=${sha}" "$cmd"
  return 0
}

# ── 运行环境 ────────────────────────────────────────────────────────────
LAUNCHER=()
LAUNCH_MODE=""

# 组装「带可用图形环境」的启动前缀。CI 的 Linux runner 没有显示器，用 xvfb-run；
# 本机开发时 $DISPLAY 已可用则直接运行。
build_launcher() {
  local display=""
  case "$DISPLAY_MODE" in
    direct) display=direct ;;
    xvfb) display=xvfb ;;
    none) return 1 ;;
    auto)
      if [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; then
        display=direct
      elif command -v xvfb-run >/dev/null 2>&1; then
        display=xvfb
      else
        return 1
      fi
      ;;
    *) return 1 ;;
  esac
  LAUNCHER=()
  if [ "$display" = "xvfb" ]; then
    command -v xvfb-run >/dev/null 2>&1 || return 1
    LAUNCHER+=(xvfb-run -a --server-args="-screen 0 1280x1024x24")
  fi
  # 无会话总线时（CI）自建一个，GTK / libappindicator 需要它；本机开发沿用用户会话。
  if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && command -v dbus-run-session >/dev/null 2>&1; then
    LAUNCHER=(dbus-run-session -- ${LAUNCHER[@]+"${LAUNCHER[@]}"})
  fi
  LAUNCH_MODE=$display
  return 0
}

# smoke_launch <工作目录> <命令...>：启动后等 N 秒，看进程是否还活着，然后收尾。
# 结果写入 SMOKE_ALIVE / SMOKE_EXIT / SMOKE_LOG。
SMOKE_ALIVE=""
SMOKE_EXIT=""
SMOKE_LOG=""
smoke_launch() {
  local workdir=$1
  shift
  local log="$workdir/launch.log"
  SMOKE_LOG=$log
  : >"$log"
  (
    cd "$workdir" || exit 1
    exec ${LAUNCHER[@]+"${LAUNCHER[@]}"} "$@"
  ) >"$log" 2>&1 &
  local pid=$!
  local waited=0
  while [ "$waited" -lt "$LAUNCH_SECONDS" ]; do
    if ! kill -0 "$pid" 2>/dev/null; then break; fi
    sleep 1
    waited=$((waited + 1))
  done
  if kill -0 "$pid" 2>/dev/null; then
    SMOKE_ALIVE=1
    SMOKE_EXIT=""
    kill -TERM "$pid" 2>/dev/null || true
    sleep 1
    kill -KILL "$pid" 2>/dev/null || true
  else
    SMOKE_ALIVE=0
    wait "$pid" 2>/dev/null
    SMOKE_EXIT=$?
  fi
  return 0
}

tail_log() {
  local log=${1-} lines=${2:-15}
  [ -f "$log" ] || {
    printf '（无输出）'
    return
  }
  tail -n "$lines" "$log" | tr '\n' '|' | sed 's/|$//'
}

# ── Linux ───────────────────────────────────────────────────────────────
check_linux_deb() {
  local deb=$1
  local cmd="dpkg-deb -I $deb; dpkg-deb -c $deb"
  if ! command -v dpkg-deb >/dev/null 2>&1; then
    add_check "installer.deb.payload" "deb 包内容" "$STATUS_SKIP" \
      "本机没有 dpkg-deb，无法解包检查（deb 只能在 Debian 系上检查）" "$cmd"
    return
  fi
  local listing control
  listing=$(dpkg-deb -c "$deb" 2>&1)
  control=$(dpkg-deb -I "$deb" 2>&1)
  printf '\n--- dpkg-deb -I %s ---\n%s\n--- dpkg-deb -c %s ---\n%s\n' \
    "$deb" "$control" "$deb" "$listing"

  local problems=""
  # dpkg-deb 的输出随版本变化：新版本打印 `usr/bin/flashcast`，老版本打印 `./usr/bin/flashcast`，
  # 因此只匹配不含 `./` 的核心片段。
  case "$listing" in
    *"usr/bin/flashcast"*) : ;;
    *) problems="${problems}缺少 /usr/bin/flashcast；" ;;
  esac
  case "$listing" in
    *"usr/share/applications/"*".desktop"*) : ;;
    *) problems="${problems}缺少桌面入口 .desktop；" ;;
  esac
  case "$listing" in
    *"usr/share/icons/"*) : ;;
    *) problems="${problems}缺少图标；" ;;
  esac

  local field
  for field in "Package" "Version" "Architecture"; do
    if ! printf '%s' "$control" | grep -qE "^[[:space:]]*${field}:"; then
      problems="${problems}control 缺少 ${field} 字段；"
    fi
  done
  local pkg_version pkg_arch
  pkg_version=$(printf '%s' "$control" | sed -n 's/^[[:space:]]*Version:[[:space:]]*//p' | head -1)
  pkg_arch=$(printf '%s' "$control" | sed -n 's/^[[:space:]]*Architecture:[[:space:]]*//p' | head -1)
  [ "$pkg_version" = "$VERSION" ] || problems="${problems}control Version=${pkg_version}（预期 ${VERSION}）；"
  [ "$pkg_arch" = "$ARCH" ] || problems="${problems}control Architecture=${pkg_arch}（预期 ${ARCH}）；"

  if [ -n "$problems" ]; then
    add_check "installer.deb.payload" "deb 包内容" "$STATUS_FAIL" "$problems" "$cmd"
  else
    add_check "installer.deb.payload" "deb 包内容" "$STATUS_PASS" \
      "包含 /usr/bin/flashcast、.desktop 与图标；control 为 Package/Version=${pkg_version}/Architecture=${pkg_arch}" \
      "$cmd"
  fi
}

check_linux_appimage() {
  local img=$1 work=$2
  local cmd="$img --appimage-extract"
  if [ ! -x "$img" ]; then
    add_check "installer.appimage.executable" "AppImage 可执行位" "$STATUS_FAIL" \
      "$(basename "$img") 没有可执行位，用户下载后无法直接运行" "chmod +x $img" 
    return
  fi
  add_check "installer.appimage.executable" "AppImage 可执行位" "$STATUS_PASS" \
    "$(basename "$img") 带可执行位" "test -x $img"

  # 解包不需要 FUSE，能验证 squashfs 负载与 AppDir 结构。
  local extract_dir="$work/appimage-extract"
  rm -rf "$extract_dir"
  mkdir -p "$extract_dir"
  local extract_log="$extract_dir.log"
  if (cd "$extract_dir" && "$img" --appimage-extract >"$extract_log" 2>&1); then
    local root="$extract_dir/squashfs-root"
    local problems=""
    [ -f "$root/AppRun" ] || problems="${problems}缺少 AppRun；"
    [ -x "$root/usr/bin/flashcast" ] || problems="${problems}缺少可执行的 usr/bin/flashcast；"
    case "$(ls "$root"/*.desktop 2>/dev/null)" in
      *".desktop") : ;;
      *) problems="${problems}缺少 .desktop；" ;;
    esac
    case "$(ls "$root"/*.png "$root"/*.svg "$root"/.DirIcon 2>/dev/null)" in
      *".png"* | *".svg"* | *"DirIcon"*) : ;;
      *) problems="${problems}缺少图标；" ;;
    esac
    if [ -n "$problems" ]; then
      add_check "installer.appimage.payload" "AppImage 负载结构" "$STATUS_FAIL" "$problems" "$cmd"
    else
      add_check "installer.appimage.payload" "AppImage 负载结构" "$STATUS_PASS" \
        "解包成功：AppRun、usr/bin/flashcast、桌面入口与图标齐备" "$cmd"
    fi
  else
    add_check "installer.appimage.payload" "AppImage 负载结构" "$STATUS_FAIL" \
      "--appimage-extract 失败：$(tail_log "$extract_log" 5)" "$cmd"
  fi

  # 真实启动：优先直接运行（走 FUSE 挂载），失败再退回 --appimage-extract-and-run。
  local launch_work="$work/appimage-run"
  mkdir -p "$launch_work"
  local used_flag=""

  # tauri-plugin-single-instance：若本机已经有一个 Flashcast 在跑，新实例会立刻正常退出（退出码 0），
  # 看起来像「启动失败」。CI 上是干净环境不会遇到，本机反复检查时要先关掉旧实例。
  local single_instance_hint=""
  if command -v pgrep >/dev/null 2>&1 && pgrep -x flashcast >/dev/null 2>&1; then
    single_instance_hint="（注意：本机已有 flashcast 进程在运行，single-instance 插件会让新实例立刻退出；请先结束旧实例）"
  fi

  if ! build_launcher; then
    add_check "installer.appimage.launch" "AppImage 启动" "$STATUS_SKIP" \
      "当前环境既没有 \$DISPLAY/\$WAYLAND_DISPLAY，也没有 xvfb-run，无法启动图形应用" \
      "xvfb-run -a $img"
    return
  fi

  smoke_launch "$launch_work" "$img"
  local direct_alive=$SMOKE_ALIVE direct_exit=$SMOKE_EXIT direct_log=$SMOKE_LOG
  if [ "$direct_alive" != "1" ]; then
    smoke_launch "$launch_work" "$img" "--appimage-extract-and-run"
    if [ "$SMOKE_ALIVE" = "1" ]; then
      used_flag="--appimage-extract-and-run"
      add_check "installer.appimage.fuse" "AppImage FUSE 挂载" "$STATUS_SKIP" \
        "直接运行失败（退出码 ${direct_exit}；容器内通常没有可用的 FUSE），改用 --appimage-extract-and-run 后正常启动" \
        "$img"
    else
      add_check "installer.appimage.fuse" "AppImage FUSE 挂载" "$STATUS_FAIL" \
        "直接运行失败（退出码 ${direct_exit}）：$(tail_log "$direct_log" 5)" "$img"
      add_check "installer.appimage.launch" "AppImage 启动" "$STATUS_FAIL" \
        "直接运行与 --appimage-extract-and-run 都未存活${single_instance_hint}：$(tail_log "$SMOKE_LOG" 12)" \
        "xvfb-run -a $img --appimage-extract-and-run"
      return
    fi
  else
    add_check "installer.appimage.fuse" "AppImage FUSE 挂载" "$STATUS_PASS" \
      "直接运行（FUSE 挂载）成功" "xvfb-run -a $img"
  fi

  local how="直接运行"
  [ -n "$used_flag" ] && how="--appimage-extract-and-run"
  add_check "installer.appimage.launch" "AppImage 启动" "$STATUS_PASS" \
    "${how}（${LAUNCH_MODE} 图形后端）后进程存活 ${LAUNCH_SECONDS} 秒，未崩溃" \
    "xvfb-run -a ${img} ${used_flag}"
}

check_linux() {
  local work=$1
  expect_artifact appimage "appimage/${PRODUCT}_${VERSION}_${ARCH}.AppImage" 8388608 "AppImage 安装包"
  expect_artifact deb "deb/${PRODUCT}_${VERSION}_${ARCH}.deb" 1048576 "deb 安装包"

  local appimage="$BUNDLES_DIR/appimage/${PRODUCT}_${VERSION}_${ARCH}.AppImage"
  local deb="$BUNDLES_DIR/deb/${PRODUCT}_${VERSION}_${ARCH}.deb"
  [ -f "$deb" ] && check_linux_deb "$deb"
  [ -f "$appimage" ] && check_linux_appimage "$appimage" "$work"

  add_signing "linux" "无需签名" \
    "AppImage 与 deb 不使用代码签名；校验依赖 SHA256（本报告 artifacts[].sha256）"
  add_note "Linux 上 deb/AppImage 不做代码签名，用户应以 SHA256SUMS.txt 校验下载内容。"
}

# ── macOS ───────────────────────────────────────────────────────────────
check_macos() {
  local work=$1
  expect_artifact dmg "dmg/${PRODUCT}_${VERSION}_${ARCH}.dmg" 1048576 "dmg 安装包"
  local dmg="$BUNDLES_DIR/dmg/${PRODUCT}_${VERSION}_${ARCH}.dmg"
  if [ ! -f "$dmg" ]; then
    add_signing "macos" "未覆盖" "没有 dmg，无法检查签名状态"
    return
  fi

  local cmd="hdiutil attach $dmg; codesign -dv --verbose=4 <app>"
  if ! command -v hdiutil >/dev/null 2>&1; then
    add_check "installer.dmg.payload" "dmg 内容" "$STATUS_SKIP" \
      "本机没有 hdiutil（只能在 macOS 上挂载 dmg）" "$cmd"
    add_signing "macos" "未覆盖" "本机没有 codesign/hdiutil"
    return
  fi

  local mnt="$work/dmg-mnt"
  mkdir -p "$mnt"
  if ! hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$mnt" >"$work/dmg-attach.log" 2>&1; then
    add_check "installer.dmg.payload" "dmg 内容" "$STATUS_FAIL" \
      "hdiutil attach 失败：$(tail_log "$work/dmg-attach.log" 8)" "$cmd"
    add_signing "macos" "未覆盖" "dmg 未能挂载"
    return
  fi

  local app candidate
  # 不用 `find -maxdepth`：macOS 自带的是 BSD find，这里改用 glob，行为在各平台一致。
  for candidate in "$mnt"/*.app; do
    if [ -d "$candidate" ]; then
      app=$candidate
      break
    fi
  done
  local problems=""
  if [ -z "$app" ]; then
    problems="dmg 内没有 .app"
  else
    [ -f "$app/Contents/Info.plist" ] || problems="${problems}缺少 Contents/Info.plist；"
    [ -x "$app/Contents/MacOS/flashcast" ] || problems="${problems}缺少可执行的 Contents/MacOS/flashcast；"
    case "$(ls "$app/Contents/Resources/"*.icns 2>/dev/null)" in
      *".icns") : ;;
      *) problems="${problems}缺少 .icns 图标；" ;;
    esac
    if [ -z "$problems" ] && command -v defaults >/dev/null 2>&1; then
      local bundle_id bundle_version
      bundle_id=$(defaults read "$app/Contents/Info" CFBundleIdentifier 2>/dev/null)
      bundle_version=$(defaults read "$app/Contents/Info" CFBundleShortVersionString 2>/dev/null)
      [ "$bundle_id" = "dev.flashcast.launcher" ] || problems="${problems}CFBundleIdentifier=${bundle_id}（预期 dev.flashcast.launcher）；"
      [ "$bundle_version" = "$VERSION" ] || problems="${problems}CFBundleShortVersionString=${bundle_version}（预期 ${VERSION}）；"
    fi
  fi
  if [ -n "$problems" ]; then
    add_check "installer.dmg.payload" "dmg 内容" "$STATUS_FAIL" "$problems" "$cmd"
  else
    add_check "installer.dmg.payload" "dmg 内容" "$STATUS_PASS" \
      "挂载成功并含 $(basename "$app")，Info.plist、可执行文件与图标齐备" "$cmd"
  fi

  # 把 .app 拷贝出 dmg 再运行，避免只读挂载带来的干扰。
  local staging="$work/staging"
  mkdir -p "$staging"
  local staged_app=""
  if [ -n "$app" ]; then
    if command -v ditto >/dev/null 2>&1; then
      ditto "$app" "$staging/$(basename "$app")" >/dev/null 2>&1
    else
      cp -R "$app" "$staging/" >/dev/null 2>&1
    fi
    staged_app="$staging/$(basename "$app")"
  fi

  check_macos_signing "$app" "$dmg"
  hdiutil detach "$mnt" >/dev/null 2>&1 || true

  if [ -z "$staged_app" ] || [ ! -x "$staged_app/Contents/MacOS/flashcast" ]; then
    add_check "installer.app.launch" "macOS 应用启动" "$STATUS_SKIP" \
      "dmg 未挂载出可运行的 .app，无法启动" "open -a $(basename "${staged_app:-$PRODUCT.app}")"
    return
  fi

  build_launcher || true
  : >"$work/macos-launch.log"
  (
    exec "$staged_app/Contents/MacOS/flashcast"
  ) >"$work/macos-launch.log" 2>&1 &
  local pid=$!
  local waited=0
  while [ "$waited" -lt "$LAUNCH_SECONDS" ]; do
    kill -0 "$pid" 2>/dev/null || break
    sleep 1
    waited=$((waited + 1))
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill -TERM "$pid" 2>/dev/null || true
    sleep 1
    kill -KILL "$pid" 2>/dev/null || true
    add_check "installer.app.launch" "macOS 应用启动" "$STATUS_PASS" \
      "解包后的 .app 内可执行文件启动后存活 ${LAUNCH_SECONDS} 秒，未崩溃" \
      "$staged_app/Contents/MacOS/flashcast"
  else
    wait "$pid" 2>/dev/null
    add_check "installer.app.launch" "macOS 应用启动" "$STATUS_FAIL" \
      "启动后 ${LAUNCH_SECONDS} 秒内退出（退出码 $?）：$(tail_log "$work/macos-launch.log" 12)" \
      "$staged_app/Contents/MacOS/flashcast"
  fi
}

check_macos_signing() {
  local app=$1 dmg=$2
  if [ -z "$app" ] || ! command -v codesign >/dev/null 2>&1; then
    add_signing "macos" "未覆盖" "没有挂载出 .app 或本机没有 codesign"
    add_check "installer.macos.codesign" "macOS 代码签名" "$STATUS_SKIP" \
      "本机没有 codesign 或没有可检查的 .app" "codesign -dv --verbose=4 <app>"
    return
  fi

  local info
  info=$(codesign -dvvv "$app" 2>&1)
  printf '\n--- codesign -dvvv %s ---\n%s\n' "$app" "$info"

  local identity=""
  case "$info" in
    *"Signature=adhoc"*) identity="ad-hoc（-）" ;;
    *)
      identity=$(printf '%s' "$info" | sed -n 's/^Authority=//p' | head -1)
      [ -n "$identity" ] || identity=$(printf '%s' "$info" | sed -n 's/^TeamIdentifier=//p' | head -1)
      ;;
  esac

  if codesign --verify --deep --strict --verbose=2 "$app" >"$WORK_DIR/codesign-verify.log" 2>&1; then
    add_check "installer.macos.codesign" "macOS 代码签名" "$STATUS_PASS" \
      "codesign --verify --deep --strict 通过；签名标识：${identity:-未识别}" \
      "codesign --verify --deep --strict --verbose=2 <app>"
    if [ "$identity" = "ad-hoc（-）" ]; then
      add_signing "macos" "ad-hoc 签名（未公证）" \
        "使用签名标识 -（ad-hoc）。可避免 Apple Silicon 上被报成「已损坏」，但不绕过 Gatekeeper，用户仍需手动放行。"
    else
      add_signing "macos" "正式签名：${identity}" \
        "使用 Developer ID 证书签名；是否可用还取决于公证结果。"
    fi
  else
    add_check "installer.macos.codesign" "macOS 代码签名" "$STATUS_FAIL" \
      "codesign --verify 失败：$(tail_log "$WORK_DIR/codesign-verify.log" 8)" \
      "codesign --verify --deep --strict --verbose=2 <app>"
    add_signing "macos" "签名校验失败" "codesign --verify 未通过，Apple Silicon 上可能被报告为「已损坏」。"
  fi

  # 公证状态：ad-hoc 签名无法公证，因此没有凭证时记「未覆盖」而不是「失败」。
  if command -v stapler >/dev/null 2>&1 && stapler validate "$app" >"$WORK_DIR/stapler.log" 2>&1; then
    add_check "installer.macos.notarized" "macOS 公证" "$STATUS_PASS" \
      "stapler validate 通过，公证票据已附加" "stapler validate <app>"
  elif [ "$identity" = "ad-hoc（-）" ]; then
    add_check "installer.macos.notarized" "macOS 公证" "$STATUS_SKIP" \
      "ad-hoc 签名无法公证（需要开发者账号）；本次构建未配置公证凭证" \
      "stapler validate <app>"
  else
    add_check "installer.macos.notarized" "macOS 公证" "$STATUS_FAIL" \
      "已用证书签名但 stapler validate 未通过：$(tail_log "$WORK_DIR/stapler.log" 5)" \
      "stapler validate <app>"
  fi

  # Gatekeeper：这是真实结论，不是构建失败。ad-hoc 签名必然被拒。
  if command -v spctl >/dev/null 2>&1; then
    if spctl -a -t exec -vv "$app" >"$WORK_DIR/spctl.log" 2>&1; then
      add_check "installer.macos.gatekeeper" "macOS Gatekeeper 首启策略" "$STATUS_PASS" \
        "spctl 接受该应用（已签名且已公证）" "spctl -a -t exec -vv <app>"
    else
      add_check "installer.macos.gatekeeper" "macOS Gatekeeper 首启策略" "$STATUS_FAIL" \
        "spctl 拒绝（未公证时属预期）：$(tail_log "$WORK_DIR/spctl.log" 4)。用户需 xattr -dr com.apple.quarantine 或在「隐私与安全性」中选择仍要打开。" \
        "spctl -a -t exec -vv <app>"
    fi
  else
    add_check "installer.macos.gatekeeper" "macOS Gatekeeper 首启策略" "$STATUS_SKIP" \
      "本机没有 spctl" "spctl -a -t exec -vv <app>"
  fi

  add_note "macOS：dmg=${dmg}。ad-hoc 签名只避免 Apple Silicon 报「已损坏」，不等于通过 Gatekeeper；安装说明必须给出 xattr -dr com.apple.quarantine 或「隐私与安全性 → 仍要打开」。"
  add_note "macOS：installer.macos.gatekeeper 记为「实测失败」是 ad-hoc 签名未公证的预期结果，不是打包失败；未配置公证凭证时不应把它当作发布阻断项。"
}

# ── Windows ─────────────────────────────────────────────────────────────
# Git Bash 里的路径是 `/d/a/...` 这种形式，bash 的 `[ -f ]` 认，但 PowerShell 不认；
# 交给 powershell.exe 时要用 `cygpath -w` 的 `D:\a\...` 形式。
win_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -u "$1"
  else
    printf '%s' "$1"
  fi
}

win_style_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -w "$1"
  else
    printf '%s' "$1"
  fi
}

check_windows_signature() {
  local exe=$1
  local ps=""
  if command -v powershell.exe >/dev/null 2>&1; then
    ps=powershell.exe
  elif command -v powershell >/dev/null 2>&1; then
    ps=powershell
  else
    add_signing "windows" "未覆盖" "本机没有 powershell，无法读取 Authenticode 状态"
    return
  fi
  local win_exe out status
  win_exe=$(win_style_path "$exe")
  # 连 stderr 一起收：读不到状态时要把 powershell 的原话写进报告，而不是只写「返回空」。
  # 不用 ^...$ 锚定：powershell.exe 重定向输出时可能带 UTF-8 BOM 或前后空白。
  out=$("$ps" -NoProfile -NonInteractive -Command \
    "(Get-AuthenticodeSignature -LiteralPath '$win_exe').Status" 2>&1 | tr -d '\r')
  status=$(printf '%s' "$out" |
    grep -oE 'Valid|NotSigned|UnknownError|HashMismatch|NotTrusted|PublisherMismatch|Incompatible' |
    head -1)
  case "$status" in
    Valid)
      add_signing "windows" "已签名（Authenticode Valid）" "NSIS 安装程序带有效的 Authenticode 签名。"
      ;;
    NotSigned)
      add_signing "windows" "未签名" \
        "没有提供 Windows 代码签名证书，安装程序未签名；Windows SmartScreen 会提示「Windows 已保护你的电脑」，用户需选择「更多信息 → 仍要运行」。"
      ;;
    *)
      add_signing "windows" "未覆盖" \
        "无法读取 Authenticode 状态（powershell 输出：$(printf '%s' "$out" | tr '\n' '|')）"
      ;;
  esac
}

check_windows() {
  local work=$1
  expect_artifact nsis "nsis/${PRODUCT}_${VERSION}_${ARCH}-setup.exe" 1048576 "NSIS 安装程序"
  local exe="$BUNDLES_DIR/nsis/${PRODUCT}_${VERSION}_${ARCH}-setup.exe"
  if [ ! -f "$exe" ]; then
    add_signing "windows" "未覆盖" "没有安装程序，无法检查签名状态"
    return
  fi
  check_windows_signature "$exe"

  if ! command -v tasklist >/dev/null 2>&1; then
    add_check "installer.nsis.install" "NSIS 静默安装" "$STATUS_SKIP" \
      "本机不是 Windows（没有 tasklist），无法执行安装" "$exe /S"
    return
  fi

  local local_appdata
  local_appdata=$(win_path "${LOCALAPPDATA:-$HOME/AppData/Local}")
  # Tauri 的 NSIS 在 installMode=currentUser 下默认装到 %LOCALAPPDATA%\<productName>；
  # 把 Program Files 变体也列上，避免模板变化时误判。
  local -a candidates=("$local_appdata/$PRODUCT" "$local_appdata/Programs/$PRODUCT")
  local install_dir="${candidates[0]}"

  # 找已安装的主程序：优先 flashcast.exe（Cargo 包名），否则取安装目录里第一个非 uninstall 的 exe。
  # 返回非空即找到；不依赖 mainBinaryName 的默认值。
  find_installed_exe() {
    local dir=$1 f base
    for f in "$dir"/*.exe; do
      [ -f "$f" ] || continue
      base=$(basename "$f")
      case "$base" in
        uninstall.exe | Uninstall.exe | *.tmp) continue ;;
      esac
      if [ "$base" = "flashcast.exe" ]; then
        printf '%s' "$f"
        return 0
      fi
      FOUND_FALLBACK=$f
    done
    if [ -n "${FOUND_FALLBACK:-}" ]; then
      printf '%s' "$FOUND_FALLBACK"
      return 0
    fi
    return 1
  }

  # Git Bash 的 MSYS 会把 `/S` 当成路径改写成 `S:\`，必须写成 `//S`（MSYS 再还原成 `/S`）。
  local cmd="$exe //S"
  printf '\n--- 静默安装 %s //S ---\n' "$exe"
  if ! "$exe" //S >"$work/nsis-install.log" 2>&1; then
    add_check "installer.nsis.install" "NSIS 静默安装" "$STATUS_FAIL" \
      "安装程序 //S 返回非零：$(tail_log "$work/nsis-install.log" 8)" "$cmd"
  else
    local waited=0 found="" app_exe=""
    while [ "$waited" -lt 120 ]; do
      local dir
      for dir in "${candidates[@]}"; do
        FOUND_FALLBACK=""
        if app_exe=$(find_installed_exe "$dir"); then
          found=$dir
          break
        fi
      done
      [ -n "$found" ] && break
      sleep 2
      waited=$((waited + 2))
    done
    if [ -n "$found" ]; then
      install_dir=$found
      add_check "installer.nsis.install" "NSIS 静默安装" "$STATUS_PASS" \
        "currentUser 静默安装成功，主程序位于 $app_exe" "$cmd"
    else
      add_check "installer.nsis.install" "NSIS 静默安装" "$STATUS_FAIL" \
        "安装程序返回 0，但 120 秒内在 ${candidates[*]} 都没找到可执行文件" "$cmd"
    fi
  fi

  # 启动已安装的应用。
  if [ -n "${app_exe:-}" ] && [ -f "$app_exe" ]; then
    local exe_name
    exe_name=$(basename "$app_exe")
    printf '\n--- 启动 %s ---\n' "$app_exe"
    "$app_exe" >"$work/win-launch.log" 2>&1 &
    local pid=$!
    sleep "$LAUNCH_SECONDS"
    local running
    running=$(tasklist //FI "IMAGENAME eq $exe_name" //NH 2>/dev/null | tr -d '\r')
    if printf '%s' "$running" | grep -qi "$exe_name"; then
      add_check "installer.app.launch" "Windows 应用启动" "$STATUS_PASS" \
        "安装后的 $exe_name 启动后仍在运行（${LAUNCH_SECONDS} 秒）" "\"$app_exe\""
      taskkill //F //IM "$exe_name" >/dev/null 2>&1 || true
    else
      add_check "installer.app.launch" "Windows 应用启动" "$STATUS_FAIL" \
        "启动后 ${LAUNCH_SECONDS} 秒内 $exe_name 进程消失：$(tail_log "$work/win-launch.log" 12)" \
        "\"$app_exe\""
    fi
    kill "$pid" 2>/dev/null || true
  else
    add_check "installer.app.launch" "Windows 应用启动" "$STATUS_SKIP" \
      "没有安装成功的主程序，无法启动" "\"$install_dir\\flashcast.exe\""
  fi

  # 静默卸载，确认安装是可逆的。
  if [ -f "$install_dir/uninstall.exe" ]; then
    printf '\n--- 静默卸载 %s //S ---\n' "$install_dir/uninstall.exe"
    "$install_dir/uninstall.exe" //S >"$work/nsis-uninstall.log" 2>&1 || true
    local waited=0
    while [ "$waited" -lt 60 ]; do
      [ -n "${app_exe:-}" ] && [ -f "$app_exe" ] || break
      sleep 2
      waited=$((waited + 2))
    done
    if [ -n "${app_exe:-}" ] && [ -f "$app_exe" ]; then
      add_check "installer.nsis.uninstall" "NSIS 静默卸载" "$STATUS_FAIL" \
        "uninstall.exe //S 之后 $app_exe 仍存在" "\"$install_dir\\uninstall.exe\" //S"
    else
      add_check "installer.nsis.uninstall" "NSIS 静默卸载" "$STATUS_PASS" \
        "uninstall.exe //S 已移除已安装的程序" "\"$install_dir\\uninstall.exe\" //S"
    fi
  else
    add_check "installer.nsis.uninstall" "NSIS 静默卸载" "$STATUS_SKIP" \
      "安装目录里没有 uninstall.exe，无法验证卸载" "\"$install_dir\\uninstall.exe\" //S"
  fi
}

# ── 主流程 ──────────────────────────────────────────────────────────────
main() {
  parse_args "$@"
  [ -n "$PLATFORM" ] || die "必须给出 --platform"
  [ -n "$ARCH" ] || die "必须给出 --arch"
  [ -n "$SLUG" ] || SLUG="${PLATFORM}-${ARCH}"
  [ -n "$ARTIFACTS_DIR" ] || ARTIFACTS_DIR="$REPO_ROOT/artifacts"

  read_config

  if [ -z "$BUNDLES_DIR" ]; then
    # 本仓库是 Cargo workspace，target/ 在仓库根目录；但若哪天改成 crate 私有 target，
    # 也会落到 src-tauri/target。两处都找一下，取存在的那个。
    local candidates=()
    if [ -n "$TARGET" ]; then
      candidates+=("$REPO_ROOT/target/$TARGET/release/bundle" "$REPO_ROOT/src-tauri/target/$TARGET/release/bundle")
    else
      candidates+=("$REPO_ROOT/target/release/bundle" "$REPO_ROOT/src-tauri/target/release/bundle")
    fi
    local dir
    for dir in "${candidates[@]}"; do
      if [ -d "$dir" ]; then
        BUNDLES_DIR=$dir
        break
      fi
    done
    [ -n "$BUNDLES_DIR" ] || BUNDLES_DIR=${candidates[0]}
  fi

  if [ -z "$WORK_DIR" ]; then
    WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/flashcast-installer-check.XXXXXX")"
  fi
  mkdir -p "$WORK_DIR" "$ARTIFACTS_DIR"
  PACKAGES_DIR="$ARTIFACTS_DIR/packages"
  mkdir -p "$PACKAGES_DIR"

  case "$PLATFORM" in
    linux) EXPECTED_OS=${EXPECTED_OS:-Linux} ;;
    macos) EXPECTED_OS=${EXPECTED_OS:-macOS} ;;
    windows) EXPECTED_OS=${EXPECTED_OS:-Windows} ;;
    *) die "未知 --platform：$PLATFORM" ;;
  esac

  report_init
  trap report_cleanup EXIT

  printf '候选安装包检查\n'
  printf '  平台/架构：%s / %s（slug=%s）\n' "$PLATFORM" "$ARCH" "$SLUG"
  printf '  产品/版本：%s %s\n' "$PRODUCT" "$VERSION"
  printf '  bundle 目录：%s\n' "$BUNDLES_DIR"
  printf '  图形后端：%s；启动观察时长：%s 秒\n\n' "$DISPLAY_MODE" "$LAUNCH_SECONDS"

  check_version_consistency

  # job summary 里的表格：先写一行「产物清单」，再由 workflow 补汇总。
  {
    printf '### 候选安装包：%s\n\n' "$SLUG"
    printf '| 项目 | 值 |\n| --- | --- |\n'
    printf '| 产品 / 版本 | %s / %s |\n' "$PRODUCT" "$VERSION"
    printf '| 平台 / 架构 | %s / %s |\n' "$PLATFORM" "$ARCH"
    printf '| bundle 目录 | `%s` |\n\n' "$BUNDLES_DIR"
  } >"$ARTIFACTS_DIR/installer-summary-$SLUG.md"

  case "$PLATFORM" in
    linux) check_linux "$WORK_DIR" ;;
    macos) check_macos "$WORK_DIR" ;;
    windows) check_windows "$WORK_DIR" ;;
  esac

  add_note "本报告只覆盖本平台 runner 上真实执行的检查；其他平台由各自的 job 分别产出。"

  local meta="$WORK_DIR/meta.env"
  # meta.env 会被 source 回来，值里可能带空格（如内核版本、PRETTY_NAME），因此都加单引号，
  # 并把值里可能出现的单引号去掉。
  {
    quote() { printf '%s' "${1-}" | tr -d "'"; }
    printf "os='%s'\n" "$(quote "$EXPECTED_OS")"
    printf "os_version='%s'\n" "$(quote "$(uname -r 2>/dev/null || printf '未知')")"
    printf "arch='%s'\n" "$(quote "$ARCH")"
    printf "slug='%s'\n" "$(quote "$SLUG")"
    printf "commit='%s'\n" "$(quote "$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || printf 'unknown')")"
    printf "version='%s'\n" "$(quote "$VERSION")"
    printf "platform='%s'\n" "$(quote "$PLATFORM")"
  } >"$meta"

  local json="$ARTIFACTS_DIR/installer-check-$SLUG.json"
  printf '\n'
  report_write_json "$json" "$meta"

  # 每个产物各写一个 .sha256 单行文件，「与包放在一起」方便用户直接校验。
  local sums="$ARTIFACTS_DIR/SHA256SUMS-$SLUG.txt"
  : >"$sums"
  local kind name path size sha
  while IFS=$'\t' read -r kind name path size sha; do
    [ -z "${kind:-}" ] && continue
    printf '%s  %s\n' "$sha" "$name" >>"$sums"
    printf '%s\n' "$sha" >"$ARTIFACTS_DIR/$name.sha256"
  done <"$REPORT_ARTIFACTS"
  if [ -s "$sums" ]; then
    printf '校验和：\n'
    cat "$sums"
  fi

  if report_has_missing_artifact; then
    printf '::error::%s 的必需安装包缺失或体积异常，打包失败。\n' "$SLUG"
    return 1
  fi
  return 0
}

main "$@"
