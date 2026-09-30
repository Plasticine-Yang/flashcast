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
    -e "s|=/usr/include|=$PREFIX/usr/include|g"

# 4b. 补齐多架构头文件（Debian/Ubuntu 的 opensslconf.h 在
#     usr/include/<multiarch>/openssl/，而 openssl.pc 的 Cflags 只给 usr/include）。
#     系统构建时 gcc 默认搜索多架构目录，前缀内构建不会；把多架构目录下的头文件
#     软链到常规 include 目录（已存在的不覆盖），这样只需一个 -I 就能找到全部头文件。
#     注意：不能改成往 Cflags 追加多架构 -I —— 依赖 openssl-sys 的
#     `cargo:include` 只透出**一个**目录（cargo 取最后一个），libssh2-sys 拿到那个
#     目录后仍要能解析 `<openssl/macros.h>`。
if [ -d "$PREFIX/usr/include/$multiarch" ]; then
  # 先清掉早期版本脚本追加过的多架构 -I（openssl-sys 只透出最后一个 include
  # 目录，libssh2-sys 拿它解析不到 `<openssl/macros.h>`）。
  find "$PREFIX/usr/lib" "$PREFIX/usr/share" -name '*.pc' -type f -print0 2>/dev/null |
    xargs -0 -r sed -i "s| -I$PREFIX/usr/include/$multiarch||g"
  while IFS= read -r dir; do
    pkg="$(basename "$dir")"
    mkdir -p "$PREFIX/usr/include/$pkg"
    find "$dir" -maxdepth 1 -type f -print0 |
      while IFS= read -r -d '' header; do
        link="$PREFIX/usr/include/$pkg/$(basename "$header")"
        [ -e "$link" ] || ln -s "$header" "$link"
      done
  done < <(find "$PREFIX/usr/include/$multiarch" -mindepth 1 -maxdepth 1 -type d 2>/dev/null)
fi

# 5. 修复指向运行库的断裂符号链接：-dev 包提供 libfoo.so -> libfoo.so.N，
#    而 libfoo.so.N 来自运行库包。把断链改为指向系统运行库的绝对路径，供链接器使用。
while IFS= read -r link; do
  target="$(readlink "$link")"
  case "$target" in
    /*) cand="$target" ;;
    *) cand="/usr/lib/$multiarch/$target" ;;
  esac
  if [ -e "$cand" ]; then ln -sfn "$cand" "$link"; fi
done < <(find "$PREFIX/usr/lib" -xtype l -name '*.so' 2>/dev/null)

# 6. 输出环境变量。若 rustup 安装在默认的非 root 位置，一并加入 PATH。
cat <<ENV
export FLASHCAST_NATIVE_DEPS_PREFIX="$PREFIX"
export PKG_CONFIG_PATH="$libdir/pkgconfig:$PREFIX/usr/share/pkgconfig\${PKG_CONFIG_PATH:+:\$PKG_CONFIG_PATH}"
if [ -x "\$HOME/.local/share/cargo/bin/cargo" ]; then
  export CARGO_HOME="\$HOME/.local/share/cargo"
  export RUSTUP_HOME="\$HOME/.local/share/rustup"
  export PATH="\$CARGO_HOME/bin:\$PATH"
fi
ENV
