#!/usr/bin/env bash
# ticket 18：把一次 tag 构建的产物发布成正式 GitHub Release。
#
# 这个脚本是发布路径上唯一的判定点，按顺序做四件事，任何一步不满足都**不发布**：
#   1. 一致性：tag / Cargo.toml / tauri.conf.json / package.json / 安装包文件名 / 各平台
#      installer-check JSON 里的 version 与 commit 全部对齐（防止混入别的提交的旧产物）；
#   2. 完整性：五个目标安装包（Linux AppImage + deb、Windows exe、macOS arm64 + x64 dmg）
#      一个不少，且产物目录里没有多余的安装包；
#   3. 校验和：为每个安装包**重新计算** SHA256，写出 SHA256SUMS.txt，并与 CI 汇总 job 的
#      SHA256SUMS.txt 逐条比对；
#   4. 发布：幂等地创建或更新 Release（`gh release edit` + `gh release upload --clobber`），
#      发布后用 `gh release view` 复核资产名称、字节数与 digest。
#
# 用法：
#   scripts/ci/publish-release.sh --tag v0.1.0 --artifacts <目录> [选项]
#
# 选项：
#   --tag <vX.Y.Z>        必填。tag 名，去掉前导 v 后必须等于各处版本号。
#   --artifacts <目录>    必填。合并后的产物目录（download-artifact 的结果）。
#   --commit <sha>        必填（除 --check-only 外）。本次 tag 指向的提交。
#   --notes <文件>        发布说明正文。默认 docs/release/<tag>.md。
#   --out <目录>          生成物目录。默认 <artifacts>/release。
#   --repo <owner/name>   目标仓库。默认取 `gh repo view`。
#   --run-url <url>       写进发布说明的 Actions 运行链接。
#   --check-only          只做第 1–3 步并把生成的说明写到 --out，不调用 gh。
#
# 退出码：任何校验失败或发布失败都是 1。

set -euo pipefail

HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$HERE/../.." && pwd)

die() {
  printf '::error::%s\n' "$*" >&2
  exit 1
}
info() { printf '%s\n' "$*"; }

TAG=""
ARTIFACTS=""
COMMIT=""
NOTES=""
OUT=""
REPO=""
RUN_URL=""
CHECK_ONLY=no

while [ $# -gt 0 ]; do
  case "$1" in
    --tag) TAG=${2-}; shift 2 ;;
    --artifacts) ARTIFACTS=${2-}; shift 2 ;;
    --commit) COMMIT=${2-}; shift 2 ;;
    --notes) NOTES=${2-}; shift 2 ;;
    --out) OUT=${2-}; shift 2 ;;
    --repo) REPO=${2-}; shift 2 ;;
    --run-url) RUN_URL=${2-}; shift 2 ;;
    --check-only|--dry-run) CHECK_ONLY=yes; shift ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) die "未知参数：$1（--help 查看用法）" ;;
  esac
done

[ -n "$TAG" ] || die "缺少 --tag。"
[ -n "$ARTIFACTS" ] || die "缺少 --artifacts。"
[ -d "$ARTIFACTS" ] || die "--artifacts 目录不存在：$ARTIFACTS"

# 绝对路径：脚本中间会 cd，相对路径会算错（ticket 04 踩过 artifacts/artifacts/ 的坑）。
abspath() {
  case "$1" in
    /*) printf '%s' "$1" ;;
    *) printf '%s/%s' "$(pwd)" "$1" ;;
  esac
}
ARTIFACTS=$(abspath "$ARTIFACTS")
[ -n "$OUT" ] || OUT="$ARTIFACTS/release"
OUT=$(abspath "$OUT")
mkdir -p "$OUT"

VERSION=${TAG#v}
case "$TAG" in
  v[0-9]*.[0-9]*.[0-9]*) ;;
  *) die "tag 必须是 vX.Y.Z 形式，收到：$TAG" ;;
esac
case "$VERSION" in
  *[!0-9.]*|"") die "tag 里的版本号必须是纯数字与点：$TAG" ;;
esac

[ "$CHECK_ONLY" = yes ] || [ -n "$COMMIT" ] || die "缺少 --commit（除非 --check-only）。"
if [ -n "$COMMIT" ]; then
  head_sha=$(git -C "$REPO_ROOT" rev-parse HEAD)
  [ "$head_sha" = "$COMMIT" ] || die "HEAD=$head_sha 与 --commit=$COMMIT 不一致：产物不属于本次 tag 提交。"
  # annotated tag 的 tag 对象必须指向同一个提交（轻量 tag 指向提交本身，这里都能解析）。
  if git -C "$REPO_ROOT" rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    tag_sha=$(git -C "$REPO_ROOT" rev-parse "$TAG^{commit}")
    [ "$tag_sha" = "$COMMIT" ] || die "tag $TAG 指向 $tag_sha，与 --commit=$COMMIT 不一致。"
  fi
fi

# ── 1. 版本一致性 ────────────────────────────────────────────────────────
cargo_version=$(awk '
  /^\[workspace\.package\]/ { in_block = 1; next }
  /^\[/ { in_block = 0 }
  in_block && /^[[:space:]]*version[[:space:]]*=/ { gsub(/[^0-9.]/, "", $0); print; exit }
' "$REPO_ROOT/Cargo.toml")
tauri_conf="$REPO_ROOT/src-tauri/tauri.conf.json"
product=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["productName"])' "$tauri_conf")
tauri_version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$tauri_conf")
package_version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$REPO_ROOT/package.json")

info "版本一致性：tag=$TAG Cargo.toml=$cargo_version tauri.conf.json=$tauri_version package.json=$package_version"
[ -n "$cargo_version" ] || die "无法从 Cargo.toml 的 [workspace.package] 读出 version。"
[ "$cargo_version" = "$VERSION" ] || die "Cargo.toml workspace.package.version=$cargo_version 与 tag 版本 $VERSION 不一致。"
[ "$tauri_version" = "$VERSION" ] || die "src-tauri/tauri.conf.json version=$tauri_version 与 tag 版本 $VERSION 不一致。"
[ "$package_version" = "$VERSION" ] || die "package.json version=$package_version 与 tag 版本 $VERSION 不一致。"

# ── 2. 资产完整性与来源 ──────────────────────────────────────────────────
# 文件名由 tauri-bundler 决定：<productName>_<version>_<arch>.<ext>。
# 这里的后缀表与 ticket 04 的四个 bundle job 一一对应；新增目标时必须同步更新。
EXPECTED_SUFFIXES=(
  "amd64.AppImage"
  "amd64.deb"
  "x64-setup.exe"
  "aarch64.dmg"
  "x64.dmg"
)
EXPECTED_SLUGS=(linux-x64 windows-x64 macos-arm64 macos-x64)

declare -a ASSETS=()
for suffix in "${EXPECTED_SUFFIXES[@]}"; do
  ASSETS+=("${product}_${VERSION}_${suffix}")
done

info ""
info "目标资产（${#ASSETS[@]} 个安装包）："
missing=0
for name in "${ASSETS[@]}"; do
  found=$(find "$ARTIFACTS" -maxdepth 3 -type f -name "$name" -print -quit)
  if [ -z "$found" ]; then
    printf '  ::error::缺少资产 %s\n' "$name" >&2
    missing=1
  else
    printf '  %s（%s 字节）\n' "$name" "$(wc -c <"$found" | tr -d ' ')"
  fi
done
[ "$missing" = "0" ] || die "安装包不完整，拒绝发布（缺一个也不发，不允许残缺 Release）。"

# 产物目录里不该出现别的安装包：那说明混进了别的提交或别的版本的构建。
stale=0
while IFS= read -r path; do
  [ -n "$path" ] || continue
  base=$(basename "$path")
  keep=no
  for name in "${ASSETS[@]}"; do
    [ "$base" = "$name" ] && keep=yes
  done
  if [ "$keep" = no ]; then
    printf '  ::error::产物目录里有预期外的安装包：%s\n' "$base" >&2
    stale=1
  fi
done < <(find "$ARTIFACTS" -maxdepth 3 -type f \
  \( -name '*.AppImage' -o -name '*.deb' -o -name '*.exe' -o -name '*.dmg' \) -print)
[ "$stale" = "0" ] || die "产物目录里混入了非目标安装包，拒绝发布。"

# 来源核对：每个 bundle job 都写了环境信息（version + commit），必须与本次 tag 提交一致。
for slug in "${EXPECTED_SLUGS[@]}"; do
  report=$(find "$ARTIFACTS" -maxdepth 3 -type f -name "installer-check-$slug.json" -print -quit)
  [ -n "$report" ] || die "缺少 $slug 的安装包检查报告（installer-check-$slug.json）。"
  read -r r_version r_commit < <(python3 - "$report" <<'PY'
import json, sys
d = json.load(open(sys.argv[1], encoding="utf-8"))
env = d.get("environment", {})
print(d.get("version", ""), env.get("commit", ""))
PY
)
  [ "$r_version" = "$VERSION" ] || die "$slug 的报告版本是 $r_version，与 tag 版本 $VERSION 不一致（旧产物？）。"
  if [ -n "$COMMIT" ]; then
    [ "$r_commit" = "$COMMIT" ] || die "$slug 的报告来自提交 $r_commit，与本次提交 $COMMIT 不一致（旧产物？）。"
  fi
  info "  来源核对通过：$slug version=$r_version commit=${r_commit:0:8}"
done

# ── 3. 重新计算 SHA256 ───────────────────────────────────────────────────
SUMS="$OUT/SHA256SUMS.txt"
REPORT="$OUT/sha256-report.txt"
rm -f "$SUMS"
: >"$SUMS"
: >"$REPORT"
declare -a SHA_LIST=()
info ""
info "重新计算 SHA256："
for name in "${ASSETS[@]}"; do
  path=$(find "$ARTIFACTS" -maxdepth 3 -type f -name "$name" -print -quit)
  if command -v sha256sum >/dev/null 2>&1; then
    sha=$(sha256sum "$path" | awk '{print $1}')
  else
    sha=$(shasum -a 256 "$path" | awk '{print $1}')
  fi
  case "$sha" in
    [0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]*) ;;
    *) die "无法为 $name 计算 SHA256（得到：$sha）。" ;;
  esac
  [ "${#sha}" = "64" ] || die "SHA256 长度异常（$name）：$sha"
  printf '%s  %s\n' "$sha" "$name" >>"$SUMS"
  printf '%s  %s\n' "$sha" "$name" >>"$REPORT"
  printf '%s\n' "$sha" >"$OUT/$name.sha256"
  SHA_LIST+=("$sha")
  printf '  %s  %s\n' "$sha" "$name"
done

# 与 CI 汇总 job 产出的 SHA256SUMS.txt 比对：两边都从同一批原始产物算，必须一致。
# 必须跳过 --out 目录里的那一份：它是本脚本刚写的，拿它自比会让这项检查变成空转
# （失败重试运行时 --out 就在产物目录里，尤其容易踩到）。
ci_sums=""
if [ -f "$ARTIFACTS/SHA256SUMS.txt" ]; then
  # download-artifact --merge-multiple 把汇总 job 的这份放在产物目录顶层。
  ci_sums="$ARTIFACTS/SHA256SUMS.txt"
else
  while IFS= read -r candidate; do
    case "$candidate" in
      "$OUT"/*) continue ;;
    esac
    ci_sums=$candidate
    break
  done < <(find "$ARTIFACTS" -maxdepth 3 -type f -name 'SHA256SUMS.txt' -print)
fi
if [ -n "$ci_sums" ]; then
  info ""
  info "与 CI 汇总的 SHA256SUMS.txt 比对："
  while read -r expected name; do
    [ -n "${expected:-}" ] || continue
    name=$(basename "$name")
    got=""
    i=0
    for asset in "${ASSETS[@]}"; do
      [ "$asset" = "$name" ] && got=${SHA_LIST[$i]}
      i=$((i + 1))
    done
    [ -n "$got" ] || die "CI 的 SHA256SUMS.txt 里有未知资产：$name"
    [ "$got" = "$expected" ] || die "$name 的 SHA256 与 CI 汇总不一致：CI=$expected 发布时重算=$got"
    printf '  一致：%s\n' "$name"
  done < <(sed 's/  */ /' "$ci_sums" | grep -v '^[[:space:]]*$')
else
  die "没有找到 CI 汇总 job 产出的 SHA256SUMS.txt。"
fi

# ── 生成发布说明 ─────────────────────────────────────────────────────────
[ -n "$NOTES" ] || NOTES="$REPO_ROOT/docs/release/$TAG.md"
[ -f "$NOTES" ] || die "缺少发布说明正文：$NOTES（拒绝发布没有中文说明的 Release）。"
NOTES_FINAL="$OUT/RELEASE_NOTES.md"

generated_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
{
  cat "$NOTES"
  printf '\n## 发布溯源\n\n'
  printf -- '- tag：`%s`\n' "$TAG"
  [ -n "$COMMIT" ] && printf -- '- 提交：`%s`\n' "$COMMIT"
  printf -- '- 版本元数据：Cargo.toml / `src-tauri/tauri.conf.json` / `package.json` 均为 `%s`\n' "$VERSION"
  if [ -n "$RUN_URL" ]; then
    printf -- '- 构建运行：%s\n' "$RUN_URL"
  fi
  printf -- '- 发布流水线生成时间：%s\n\n' "$generated_at"
  printf '全部资产来自上面这一次 tag 构建的同一个提交；发布前会拒绝混入其他提交或版本的旧产物。\n'
  printf '\n## 资产与 SHA256\n\n'
  printf '| 资产 | 平台 / 架构 | 字节 | SHA256 |\n'
  printf '| --- | --- | --- | --- |\n'
  platforms=("Linux x64" "Linux x64" "Windows x64" "macOS arm64 (Apple Silicon)" "macOS x64 (Intel)")
  i=0
  for name in "${ASSETS[@]}"; do
    path=$(find "$ARTIFACTS" -maxdepth 3 -type f -name "$name" -print -quit)
    printf '| `%s` | %s | %s | `%s` |\n' "$name" "${platforms[$i]}" "$(wc -c <"$path" | tr -d ' ')" "${SHA_LIST[$i]}"
    i=$((i + 1))
  done
  printf '\n`SHA256SUMS.txt`：\n\n```text\n'
  cat "$SUMS"
  printf '```\n'
} >"$NOTES_FINAL"

printf '\n生成的发布说明：%s\n' "$NOTES_FINAL"

if [ "$CHECK_ONLY" = yes ]; then
  printf '\n--check-only：已完成一致性、完整性与校验和检查，未调用 gh，未发布。\n'
  printf 'SHA256SUMS.txt：%s\n' "$SUMS"
  exit 0
fi

# ── 4. 幂等发布并复核 ────────────────────────────────────────────────────
command -v gh >/dev/null 2>&1 || die "找不到 gh，无法发布。"
REPO_ARG=()
[ -n "$REPO" ] && REPO_ARG=(--repo "$REPO")
TITLE="Flashcast $TAG"
declare -a UPLOADS=()
for name in "${ASSETS[@]}"; do
  UPLOADS+=("$(find "$ARTIFACTS" -maxdepth 3 -type f -name "$name" -print -quit)")
done
UPLOADS+=("$SUMS")

if gh release view "$TAG" "${REPO_ARG[@]}" >/dev/null 2>&1; then
  info "Release $TAG 已存在：更新说明与资产（--clobber，不产生重复 Release）。"
  gh release edit "$TAG" "${REPO_ARG[@]}" \
    --title "$TITLE" --notes-file "$NOTES_FINAL" \
    --draft=false --prerelease=false --latest
else
  info "创建 Release $TAG（公开、非草稿）。"
  gh release create "$TAG" "${REPO_ARG[@]}" \
    --verify-tag --title "$TITLE" --notes-file "$NOTES_FINAL" --latest
fi

info "上传 ${#UPLOADS[@]} 个资产（--clobber 保证重试幂等）。"
gh release upload "$TAG" "${REPO_ARG[@]}" --clobber "${UPLOADS[@]}"

info ""
info "复核已发布的 Release："
gh release view "$TAG" "${REPO_ARG[@]}" \
  --json tagName,name,isDraft,isPrerelease,url,assets >"$OUT/published-release.json"
python3 - "$OUT/published-release.json" "$SUMS" <<'PY'
import json, sys

data = json.load(open(sys.argv[1], encoding="utf-8"))
expected = {}
for line in open(sys.argv[2], encoding="utf-8"):
    line = line.strip()
    if not line:
        continue
    sha, name = line.split(None, 1)
    expected[name.strip()] = sha

if data.get("isDraft"):
    sys.exit("::error::Release 仍是草稿（draft），不算公开发布。")

assets = {a["name"]: a for a in data.get("assets", [])}
expected["SHA256SUMS.txt"] = None

problems = []
for name in expected:
    a = assets.get(name)
    if a is None:
        problems.append(f"缺少资产 {name}")
        continue
    if a.get("size", 0) <= 0:
        problems.append(f"{name} 的字节数为 {a.get('size')}")
    digest = (a.get("digest") or "")
    if expected[name] and digest.startswith("sha256:"):
        if digest.split(":", 1)[1] != expected[name]:
            problems.append(f"{name} 的 digest 与本地重算不一致：{digest}")

extra = [n for n in assets if n not in expected]
if extra:
    problems.append("出现预期外的资产：" + ", ".join(sorted(extra)))

if problems:
    sys.exit("::error::发布后复核失败：" + "；".join(problems))

print(f"  公开 Release：{data['url']}")
print(f"  名称：{data['name']}（draft={data['isDraft']}, prerelease={data['isPrerelease']}）")
for name in expected:
    a = assets[name]
    print(f"  {name}  {a['size']} 字节  digest={a.get('digest') or '（未提供）'}")
print("所有必需资产均已公开且校验一致。")
PY

info ""
info "发布完成：$TAG"
