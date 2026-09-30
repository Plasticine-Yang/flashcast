#!/usr/bin/env bash
# ticket 04：汇总所有平台产物的 SHA256，并复核各平台 job 自己算出的校验和。
#
# 汇总 job 把各平台的 workflow artifact 下载下来后运行本脚本：
#   1. 重新计算每个安装包的 SHA256，写出 SHA256SUMS.txt（ticket 18 会把它作为 Release 资产）；
#   2. 与各 job 上传的 SHA256SUMS-<slug>.txt 逐条比对——下载/上传链路若出错，这里会暴露；
#   3. 为每个安装包再写一个 <name>.sha256 单行文件，方便用户直接核对。
#
# 用法：scripts/ci/verify-sha256sums.sh <产物目录> [输出目录]
# 退出码：找不到产物、校验和不匹配时为 1。

set -uo pipefail

here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/ci/installer-report.sh
. "$here/installer-report.sh"

DIR=${1-}
OUT=${2-$DIR}

if [ -z "$DIR" ] || [ ! -d "$DIR" ]; then
  echo "用法：$0 <产物目录> [输出目录]" >&2
  exit 2
fi
mkdir -p "$OUT"

# 先清掉上一次的汇总，避免把旧提交的产物混进来（ticket 18 明确要求不能混）。
rm -f "$OUT/SHA256SUMS.txt"

cd "$DIR" || exit 2

mapfile -t FILES < <(find . -maxdepth 3 -type f \
  \( -name '*.AppImage' -o -name '*.deb' -o -name '*.exe' -o -name '*.dmg' \) \
  -printf '%P\n' 2>/dev/null | sort)

if [ "${#FILES[@]}" -eq 0 ]; then
  # macOS 的 find 没有 -printf，退回 -print + sed。
  mapfile -t FILES < <(find . -maxdepth 3 -type f \
    \( -name '*.AppImage' -o -name '*.deb' -o -name '*.exe' -o -name '*.dmg' \) \
    -print 2>/dev/null | sed 's|^\./||' | sort)
fi

if [ "${#FILES[@]}" -eq 0 ]; then
  echo "::error::$DIR 下没有找到任何安装包（*.AppImage / *.deb / *.exe / *.dmg）。" >&2
  exit 1
fi

printf '找到 %d 个安装包：\n' "${#FILES[@]}"

: >"$OUT/SHA256SUMS.txt"
declare -A HASHES=()
fail=0
for f in "${FILES[@]}"; do
  sha=$(sha256_of "$f")
  if ! is_sha256 "$sha"; then
    echo "::error::无法为 $f 计算 SHA256。" >&2
    fail=1
    continue
  fi
  HASHES["$(basename "$f")"]=$sha
  printf '%s  %s\n' "$sha" "$(basename "$f")" >>"$OUT/SHA256SUMS.txt"
  printf '%s\n' "$sha" >"$OUT/$(basename "$f").sha256"
  printf '  %s  %s（%s 字节）\n' "$sha" "$(basename "$f")" "$(file_size "$f")"
done

# 复核各平台 job 自己算的校验和。
for sums in "$DIR"/SHA256SUMS-*.txt; do
  [ -f "$sums" ] || continue
  printf '\n复核 %s：\n' "$(basename "$sums")"
  while read -r expected name; do
    [ -n "${expected:-}" ] || continue
    name=$(basename "$name")
    if [ -z "${HASHES[$name]:-}" ]; then
      printf '  ::warning::%s 在汇总目录里找不到文件，跳过。\n' "$name"
      continue
    fi
    if [ "${HASHES[$name]}" = "$expected" ]; then
      printf '  一致：%s\n' "$name"
    else
      printf '  ::error::%s 校验和不一致：job 报告 %s，汇总重算 %s\n' "$name" "$expected" "${HASHES[$name]}"
      fail=1
    fi
  done < <(sed 's/  */ /' "$sums" | grep -v '^[[:space:]]*$')
done

printf '\nSHA256SUMS.txt：\n'
cat "$OUT/SHA256SUMS.txt"

if [ "$fail" != "0" ]; then
  exit 1
fi
printf '\n全部校验和一致。\n'
