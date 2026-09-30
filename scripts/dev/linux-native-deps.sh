#!/usr/bin/env bash
# 在无法使用 sudo 的 Linux 开发机上准备 Tauri 2 所需的 GTK/WebKit 开发包。
#
# 这些软件包的运行库通常已随桌面环境安装，缺少的只是 headers 与 .pc 文件。
# 本脚本用非 root 权限下载 .deb 并解压到本地前缀，同时生成 pkg-config 包装脚本，
# 让 `--define-prefix` 生效，从而使构建脚本能找到前缀内的 headers 与 .pc。
#
# 用法：
#   scripts/dev/linux-native-deps.sh            # 安装并打印环境变量
#   eval "$(scripts/dev/linux-native-deps.sh)"  # 在当前 shell 中启用
#
# 在 CI runner（有 root）上无需本脚本，直接 apt-get install 即可。
set -euo pipefail

PREFIX="${FLASHCAST_NATIVE_DEPS_PREFIX:-$HOME/.local/share/flashcast/linux-native-deps}"
CACHE="${FLASHCAST_NATIVE_DEPS_CACHE:-$HOME/.cache/flashcast/native-debs}"
QUIET="${FLASHCAST_NATIVE_DEPS_QUIET:-0}"

PKGS=(
  libgtk-3-dev
  libwebkit2gtk-4.1-dev
  libjavascriptcoregtk-4.1-dev
  libsoup-3.0-dev
  librsvg2-dev
  libayatana-appindicator3-dev
  libxdo-dev
  libssl-dev
)

log() { [ "$QUIET" = "1" ] || echo "$@" >&2; }

mkdir -p "$PREFIX" "$CACHE"

# 1. 解析下载清单（apt 会考虑已安装的运行库，只列出真正缺失的包）。
log "==> 解析依赖清单"
mapfile -t URIS < <(
  apt-get install --print-uris -y --no-install-recommends "${PKGS[@]}" 2>/dev/null \
    | grep -oE "^'https?://[^']+'" | tr -d "'"
)
if [ "${#URIS[@]}" -eq 0 ]; then
  echo "错误：无法解析依赖清单，请检查 apt 软件源。" >&2
  exit 1
fi
log "    需要 ${#URIS[@]} 个 .deb"

# 2. 下载（跳过已缓存的包）。
fail=0
for uri in "${URIS[@]}"; do
  file="$CACHE/$(basename "${uri%%\?*}")"
  if [ ! -s "$file" ]; then
    curl -fsSL --retry 3 -o "$file.part" "$uri" || { fail=1; rm -f "$file.part"; continue; }
    mv "$file.part" "$file"
  fi
done
if [ "$fail" != "0" ]; then
  echo "错误：部分软件包下载失败。" >&2
  exit 1
fi

# 3. 解压到前缀。用 stamp 记录已解压的包，重复执行时跳过。
stamp="$PREFIX/.extracted"
touch "$stamp"
# 先删除上一次建立的数据目录链接（见步骤 6），避免 dpkg-deb -x 顺着链接写进系统目录。
linkstamp="$PREFIX/.data-dir-links"
if [ -f "$linkstamp" ]; then
  while IFS= read -r created; do
    [ -n "$created" ] && [ -L "$created" ] && rm -f "$created"
  done <"$linkstamp"
fi
: >"$linkstamp"
new=0
for uri in "${URIS[@]}"; do
  base="$(basename "${uri%%\?*}")"
  file="$CACHE/$base"
  if grep -qxF "$base" "$stamp"; then continue; fi
  dpkg-deb -x "$file" "$PREFIX"
  echo "$base" >>"$stamp"
  new=$((new + 1))
done
log "==> 新解压 $new 个包到 $PREFIX"

# 4. 改写 .pc 文件中的绝对路径，让 pkg-config 输出指向前缀内的 headers 与 .so。
#    运行库仍由系统提供，因此不设置 LD_LIBRARY_PATH。
multiarch="$(dpkg-architecture -qDEB_HOST_MULTIARCH 2>/dev/null || echo x86_64-linux-gnu)"
libdir="$PREFIX/usr/lib/$multiarch"
find "$PREFIX/usr/lib" "$PREFIX/usr/share" -name '*.pc' -type f -print0 2>/dev/null |
  xargs -0 -r sed -i \
    -e "s|^prefix=/usr\$|prefix=$PREFIX/usr|" \
    -e "s|=/usr/lib/$multiarch|=$libdir|g" \
    -e "s|=/usr/lib\$|=$PREFIX/usr/lib|" \
    -e "s|=/usr/include|=$PREFIX/usr/include|g" \
    -e "s|^libdir=.*|libdir=/usr/lib/$multiarch|" \
    -e "s|^libdir64=.*|libdir64=/usr/lib/$multiarch|"
#    最后两条把 libdir 还原成系统路径，是有意为之：`tauri build` 打包 AppImage 时，
#    linuxdeploy 的 gtk 插件会把 pkg-config 报出的 libdir 直接当成 AppDir 内的相对路径
#    做字符串替换，而前缀路径本身包含 `/usr/lib/<triplet>`，会让它的替换错位并中断打包。
#    让 pkg-config 回答系统路径后，插件的表现与 CI 上 apt 安装时一致。链接所需的
#    `libfoo.so` 开发链接来自前缀，由步骤 7 导出的 LIBRARY_PATH 提供给链接器。

# 5. 修复指向运行库的断裂符号链接：-dev 包提供 libfoo.so -> libfoo.so.N，
#    而 libfoo.so.N 来自运行库包。把断链改为指向系统运行库的绝对路径，供链接器使用。
#
#    同时在同一目录补出运行库原名（libfoo.so.N）。`tauri build` 打包 AppImage 时会先用
#    pkg-config 的 `--libs-only-L` 找到 libayatana-appindicator3-0.1 的库目录，再拼出
#    `<libdir>/libayatana-appindicator3.so.1` 一起塞进 AppImage；前缀里只有 `.so` 时该步骤
#    会以「Failed to copy custom files」失败。补上运行库原名后，此前缀才与一次真实的 -dev
#    安装（CI 上由 apt 安装）等价。
while IFS= read -r link; do
  target="$(readlink "$link")"
  case "$target" in
    /*) cand="$target" ;;
    *) cand="/usr/lib/$multiarch/$target" ;;
  esac
  if [ -e "$cand" ]; then
    ln -sfn "$cand" "$link"
    soname="$(dirname "$link")/$(basename "$cand")"
    [ -e "$soname" ] || ln -sfn "$cand" "$soname"
  fi
done < <(find "$PREFIX/usr/lib" -type l -name '*.so' 2>/dev/null)

# 6. 数据目录：`.pc` 里的目录变量（例如 gio-2.0 的 giomoduledir）在步骤 4 也被改写到前缀，
#    但 -dev 包并不提供这些目录。`tauri build` 打包 AppImage 时，linuxdeploy 的 gtk 插件会
#    对 giomoduledir 做 realpath，失败后以「Failed to run plugin: gtk」中断打包。
#    为「前缀里不存在、系统里存在」的数据目录建立链接，使前缀与真实 -dev 安装等价。
#    链接记录在 linkstamp 里，下次运行会在解压前删除，避免 dpkg-deb -x 写穿链接。
link_data_dir() {
  local prefixed=$1 system=$2
  [ -e "$prefixed" ] && return 0
  [ -e "$system" ] || return 0
  mkdir -p "$(dirname "$prefixed")"
  ln -sfn "$system" "$prefixed"
  echo "$prefixed" >>"$linkstamp"
}
link_data_dir "$libdir/gio/modules" "/usr/lib/$multiarch/gio/modules"
link_data_dir "$libdir/girepository-1.0" "/usr/lib/$multiarch/girepository-1.0"
# linuxdeploy 的 gtk 插件还会从 gtk+-3.0.pc / gdk-pixbuf-2.0.pc 拼出这两个目录。
link_data_dir "$libdir/gtk-3.0" "/usr/lib/$multiarch/gtk-3.0"
link_data_dir "$libdir/gdk-pixbuf-2.0" "/usr/lib/$multiarch/gdk-pixbuf-2.0"

# 7. 输出环境变量。若 rustup 安装在默认的非 root 位置，一并加入 PATH。
cat <<ENV
export FLASHCAST_NATIVE_DEPS_PREFIX="$PREFIX"
export PKG_CONFIG_PATH="$libdir/pkgconfig:$PREFIX/usr/share/pkgconfig\${PKG_CONFIG_PATH:+:\$PKG_CONFIG_PATH}"
# 步骤 4 让 pkg-config 报出系统 libdir，链接器就找不到只存在于前缀里的 \`libfoo.so\`，
# 因此把前缀的库目录交给 LIBRARY_PATH（ld 在 -L 之后、默认目录之前搜索它）。
export LIBRARY_PATH="$libdir\${LIBRARY_PATH:+:\$LIBRARY_PATH}"
if [ -x "\$HOME/.local/share/cargo/bin/cargo" ]; then
  export CARGO_HOME="\$HOME/.local/share/cargo"
  export RUSTUP_HOME="\$HOME/.local/share/rustup"
  export PATH="\$CARGO_HOME/bin:\$PATH"
fi
ENV
