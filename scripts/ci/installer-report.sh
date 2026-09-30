#!/usr/bin/env bash
# ticket 04：候选安装包检查的公共函数库（被 `scripts/ci/check-installers.sh` source，不单独执行）。
#
# 三态语义与 ticket 01 的 `flashcast-platform-check` 保持一致：
#   实测通过 measured_pass / 实测失败 measured_fail / 未覆盖 not_covered
# 「编译成功」或「压缩文件已生成」都不能当作安装通过，因此这里的每一条检查都必须
# 描述一个真实执行过的动作；不能执行的动作记为未覆盖并写明原因。

# ── 状态 ────────────────────────────────────────────────────────────────
STATUS_PASS="measured_pass"
STATUS_FAIL="measured_fail"
STATUS_SKIP="not_covered"

status_label() {
  case "${1-}" in
    "$STATUS_PASS") printf '%s' "实测通过" ;;
    "$STATUS_FAIL") printf '%s' "实测失败" ;;
    "$STATUS_SKIP") printf '%s' "未覆盖" ;;
    *) printf '%s' "未知" ;;
  esac
}

# 去掉字段内的制表符，避免破坏检查记录的 TSV 结构；其余控制字符留给 json_escape 处理。
_sanitize() {
  local s=${1-}
  s=${s//$'\t'/ }
  printf '%s' "$s"
}

json_escape() {
  local s=${1-}
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\r'/\\r}
  s=${s//$'\n'/\\n}
  s=${s//$'\t'/\\t}
  # 其余 C0 控制字符在 JSON 里必须转义，这里直接丢弃（不会出现在我们的检查文案里）。
  printf '%s' "$s" | tr -d '\000-\010\013\014\016-\037'
}

# ── 报告累积 ────────────────────────────────────────────────────────────
# 用临时目录里的 TSV 文件累积记录，最后统一序列化成 JSON；字段值已 _sanitize 过。
report_init() {
  REPORT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/flashcast-installer-report.XXXXXX")"
  REPORT_CHECKS="$REPORT_DIR/checks.tsv"
  REPORT_ARTIFACTS="$REPORT_DIR/artifacts.tsv"
  REPORT_SIGNING="$REPORT_DIR/signing.tsv"
  REPORT_NOTES="$REPORT_DIR/notes.txt"
  : >"$REPORT_CHECKS"
  : >"$REPORT_ARTIFACTS"
  : >"$REPORT_SIGNING"
  : >"$REPORT_NOTES"
  rm -f "$REPORT_DIR/missing-artifact"
}

# 必需产物缺失时置位；check-installers.sh 据此以非零退出（打包失败必须让 job 变红）。
report_mark_missing_artifact() {
  : >"$REPORT_DIR/missing-artifact"
}

report_cleanup() {
  if [ -n "${REPORT_DIR:-}" ] && [ -d "$REPORT_DIR" ]; then
    rm -rf "$REPORT_DIR"
  fi
}

# add_check <id> <标题> <状态> <说明> <复现命令>
add_check() {
  local id title status detail command
  id=$(_sanitize "${1-}")
  title=$(_sanitize "${2-}")
  status=${3-}
  detail=$(_sanitize "${4-}")
  command=$(_sanitize "${5-}")
  printf '%s\t%s\t%s\t%s\t%s\n' "$id" "$title" "$status" "$detail" "$command" >>"$REPORT_CHECKS"
  printf '  [%s] %s：%s\n' "$(status_label "$status")" "$title" "$detail"
  case "$status" in
    "$STATUS_FAIL") printf '::warning::%s：%s\n' "$title" "$detail" ;;
    "$STATUS_SKIP") printf '::notice::%s：%s\n' "$title" "$detail" ;;
  esac
}

# add_artifact <类型> <文件名> <路径> <字节数> <sha256>
add_artifact() {
  printf '%s\t%s\t%s\t%s\t%s\n' \
    "$(_sanitize "${1-}")" "$(_sanitize "${2-}")" "$(_sanitize "${3-}")" \
    "$(_sanitize "${4-}")" "$(_sanitize "${5-}")" >>"$REPORT_ARTIFACTS"
}

# add_signing <范围> <状态> <说明>
add_signing() {
  printf '%s\t%s\t%s\n' \
    "$(_sanitize "${1-}")" "$(_sanitize "${2-}")" "$(_sanitize "${3-}")" >>"$REPORT_SIGNING"
}

add_note() {
  _sanitize "${1-}" >>"$REPORT_NOTES"
}

# report_counts <状态>：统计某状态的检查条数。
report_counts() {
  local want=$1 n=0 status
  while IFS=$'\t' read -r _id _title status _rest; do
    [ "$status" = "$want" ] && n=$((n + 1))
  done <"$REPORT_CHECKS"
  printf '%s' "$n"
}

# 是否存在「必需产物缺失」这类硬失败；有则 check-installers.sh 以非零退出。
report_has_missing_artifact() {
  [ -f "$REPORT_DIR/missing-artifact" ]
}

# report_write_json <输出路径> <额外元数据文件>：序列化报告。
# 元数据以 KEY=VALUE 形式逐行给出（os / osVersion / arch / slug / commit / version / platform）。
report_write_json() {
  local out=$1 meta=$2
  local os="" os_version="" arch="" slug="" commit="" version="" platform=""
  if [ -f "$meta" ]; then
    # shellcheck disable=SC1090
    . "$meta"
  fi

  {
    printf '{\n'
    printf '  "schemaVersion": 1,\n'
    printf '  "product": "flashcast",\n'
    printf '  "version": "%s",\n' "$(json_escape "$version")"
    printf '  "environment": {\n'
    printf '    "platform": "%s",\n' "$(json_escape "$platform")"
    printf '    "os": "%s",\n' "$(json_escape "$os")"
    printf '    "osVersion": "%s",\n' "$(json_escape "$os_version")"
    printf '    "arch": "%s",\n' "$(json_escape "$arch")"
    printf '    "slug": "%s",\n' "$(json_escape "$slug")"
    printf '    "commit": "%s",\n' "$(json_escape "$commit")"
    printf '    "source": "scripts/ci/check-installers.sh"\n'
    printf '  },\n'

    printf '  "artifacts": [\n'
    local first=1 kind name path size sha
    while IFS=$'\t' read -r kind name path size sha; do
      [ -z "${kind:-}" ] && continue
      [ "$first" = "1" ] || printf ',\n'
      first=0
      printf '    { "kind": "%s", "name": "%s", "path": "%s", "sizeBytes": %s, "sha256": "%s" }' \
        "$(json_escape "$kind")" "$(json_escape "$name")" "$(json_escape "$path")" \
        "${size:-0}" "$(json_escape "$sha")"
    done <"$REPORT_ARTIFACTS"
    printf '\n  ],\n'

    printf '  "signing": [\n'
    first=1
    local scope status detail
    while IFS=$'\t' read -r scope status detail; do
      [ -z "${scope:-}" ] && continue
      [ "$first" = "1" ] || printf ',\n'
      first=0
      printf '    { "scope": "%s", "status": "%s", "detail": "%s" }' \
        "$(json_escape "$scope")" "$(json_escape "$status")" "$(json_escape "$detail")"
    done <"$REPORT_SIGNING"
    printf '\n  ],\n'

    printf '  "checks": [\n'
    first=1
    local id title cstatus cdetail ccommand
    while IFS=$'\t' read -r id title cstatus cdetail ccommand; do
      [ -z "${id:-}" ] && continue
      [ "$first" = "1" ] || printf ',\n'
      first=0
      printf '    {\n'
      printf '      "id": "%s",\n' "$(json_escape "$id")"
      printf '      "title": "%s",\n' "$(json_escape "$title")"
      printf '      "status": "%s",\n' "$(json_escape "$cstatus")"
      printf '      "statusLabel": "%s",\n' "$(status_label "$cstatus")"
      printf '      "detail": "%s",\n' "$(json_escape "$cdetail")"
      printf '      "command": "%s"\n' "$(json_escape "$ccommand")"
      printf '    }'
    done <"$REPORT_CHECKS"
    printf '\n  ],\n'

    printf '  "summary": {\n'
    printf '    "measuredPass": %s,\n' "$(report_counts "$STATUS_PASS")"
    printf '    "measuredFail": %s,\n' "$(report_counts "$STATUS_FAIL")"
    printf '    "notCovered": %s\n' "$(report_counts "$STATUS_SKIP")"
    printf '  },\n'

    printf '  "notes": [\n'
    first=1
    local note
    while IFS= read -r note; do
      [ -z "$note" ] && continue
      [ "$first" = "1" ] || printf ',\n'
      first=0
      printf '    "%s"' "$(json_escape "$note")"
    done <"$REPORT_NOTES"
    printf '\n  ]\n'
    printf '}\n'
  } >"$out"

  printf '已写入 %s\n' "$out"
  printf '汇总：实测通过 %s，实测失败 %s，未覆盖 %s\n' \
    "$(report_counts "$STATUS_PASS")" \
    "$(report_counts "$STATUS_FAIL")" \
    "$(report_counts "$STATUS_SKIP")"
}

# ── 文件工具 ────────────────────────────────────────────────────────────
file_size() {
  wc -c <"$1" | tr -d '[:space:]'
}

# 跨平台 SHA256：Linux 用 sha256sum，macOS 用 shasum，Windows runner 两者之一（Git Bash 自带 sha256sum），
# 都没有时退回 PowerShell / certutil。
sha256_of() {
  local file=$1
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  elif command -v certutil >/dev/null 2>&1; then
    certutil -hashfile "$file" SHA256 | sed -n 2p | tr -d ' \r'
  else
    printf ''
  fi
}

is_sha256() {
  printf '%s' "${1-}" | grep -Eq '^[0-9a-f]{64}$'
}
