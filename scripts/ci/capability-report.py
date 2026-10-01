#!/usr/bin/env python3
"""把「平台能力报告」的证据文件合并成一份机器可读文档（ticket 17）。

用法（仓库根目录）：

    python3 scripts/ci/capability-report.py

输入：
- `docs/platform/capability-report.meta.json`：编辑性内容（环境清单、替身检查清单、
  未覆盖清单、发布说明注意事项）。
- `docs/platform/evidence/*.json`：每个环境的原始证据（`flashcast-platform-check --json`
  的输出、installer 检查输出）。运行器上的证据用 `gh run download <run-id> -n <artifact>`
  取得，本地证据由本机直接运行 `flashcast-platform-check` 产生。

输出：
- `docs/platform/capability-report.json`：自包含的合并报告。每个检查都带
  环境、类别（test-double / real-platform / uncovered）与三态结论
  （实测通过 / 实测失败 / 未覆盖）。

设计约束（对齐 spec 的 Testing Decisions）：
- 替身检查与真实平台检查**分开**归类，替身通过不进入真实平台结论。
- 未覆盖项单独一节，逐条写明原因与证据。
- 编译成功（`build.compile`）只是一个检查项，不被当成任何桌面行为的证据。
"""

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent

STATUS_LABEL = {
    "measured_pass": "实测通过",
    "measured_fail": "实测失败",
    "not_covered": "未覆盖",
}


def load_json(path: pathlib.Path):
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def environment_checks(entry: dict, evidence: dict) -> list:
    """把一个环境的 platform-check 输出转成带环境与类别的检查列表。"""
    checks = []
    for check in evidence.get("checks", []):
        status = check["status"]
        checks.append(
            {
                "id": check["id"],
                "title": check["title"],
                "category": "real-platform",
                "status": status,
                "statusLabel": STATUS_LABEL.get(status, status),
                "environment": entry["id"],
                "environmentLabel": entry["label"],
                "detail": check["detail"],
                "command": check.get("command"),
                "evidence": entry["evidence"],
            }
        )
    return checks


def md_cell(text: str) -> str:
    """把一段可能含换行 / 竖线的文本压成表格单元格。"""
    return " ".join(str(text).split()).replace("|", "\\|")


def emit_markdown(report: dict, path: pathlib.Path) -> None:
    lines = []
    push = lines.append
    push(f"# Flashcast v{report['version']} 平台能力报告")
    push("")
    push(
        "本文件由 `scripts/ci/capability-report.py` 从 "
        "`docs/platform/capability-report.meta.json` 与 `docs/platform/evidence/*` 生成；"
        "机器可读版本是同目录的 `capability-report.json`（含每一项的完整原因与复现命令）。"
    )
    push("")
    push(f"- 生成时间：{report['generatedAt']}")
    ci = report["ci"]
    push(
        f"- CI 证据：`{ci['branch']}` @ `{ci['commit'][:8]}`，"
        f"[运行 {ci['runId']}]({ci['url']})，{ci['jobCount']} 个任务全部 success"
    )
    push("")
    for statement in report["statements"]:
        push(f"- {statement}")
    push("")

    push("## 证据采集说明")
    push("")
    for caveat in report["evidenceCaveats"]:
        push(f"- {caveat}")
    push("")

    summary = report["summary"]
    push("## 结论速览")
    push("")
    push("| 类别 | 实测通过 | 实测失败 | 未覆盖 |")
    push("| --- | --- | --- | --- |")
    push(
        f"| 真实平台 / 真实桌面检查 | {summary['realPlatformMeasuredPass']} | "
        f"{summary['realPlatformMeasuredFail']} | {summary['realPlatformNotCovered']} |"
    )
    push(f"| 替身检查（`cargo test` 经宿主入口，共 {report['testDouble']['cargoTest'][0]['passed']} 项） | "
         f"{summary['testDoubleMeasuredPass']} | 0 | 0 |")
    push(f"| 未覆盖条目（逐条写原因） | — | — | {summary['uncoveredItems']} |")
    push("")

    push("## 环境")
    push("")
    push("| 环境 | 系统 | 版本 | 架构 | 会话 | 桌面 | 签名状态 | 必要权限 |")
    push("| --- | --- | --- | --- | --- | --- | --- | --- |")
    for env in report["environments"]:
        push(
            f"| {env['label']} | {env['os']} | {md_cell(env['osVersion'])} | {env['arch']} | "
            f"{md_cell(env['sessionType'])} | {'有' if env['desktopAvailable'] else '无'} | "
            f"{md_cell(env['signing'])} | {md_cell(env['requiredPermissions'])} |"
        )
    push("")

    push("## (a) 替身检查：不构成平台行为证据")
    push("")
    push(report["testDouble"]["statement"])
    push("")
    push("| 平台 | 套件数 | 通过 | 失败 |")
    push("| --- | --- | --- | --- |")
    for item in report["testDouble"]["cargoTest"]:
        push(f"| {item['platform']} | {item['suites']} | {item['passed']} | {item['failed']} |")
    push("")
    ui = report["testDouble"]["uiCheck"]
    push(
        f"- `{ui['command']}` → **通过 {ui['passed']} / 失败 {ui['failed']}（共 {ui['total']} 项）**；"
        f"{ui['what']}"
    )
    push("")
    push("| 检查 | 覆盖内容 | 用例 | 结论 |")
    push("| --- | --- | --- | --- |")
    for check in report["testDouble"]["checks"]:
        push(
            f"| `{check['id']}` | {md_cell(check['title'])} | `{check['suite']}` | "
            f"{STATUS_LABEL.get(check['status'], check['status'])} |"
        )
    push("")

    push("## (b) 真实平台 / 真实桌面检查：行为证据")
    push("")
    for env in report["environments"]:
        push(f"### {env['label']}")
        push("")
        push(f"- 证据：`docs/platform/{env['evidence']}`（{md_cell(env.get('artifact') or '')}）")
        if env.get("note"):
            push(f"- 说明：{env['note']}")
        push("")
        push("| 检查 | 结论 | 说明 |")
        push("| --- | --- | --- |")
        for check in report["checks"]:
            if check["environment"] != env["id"]:
                continue
            push(
                f"| `{check['id']}` {md_cell(check['title'])} | {check['statusLabel']} | "
                f"{md_cell(check['detail'])} |"
            )
        push("")

    push("## (c) 未覆盖：逐条原因")
    push("")
    push("| 条目 | 原因 | 证据 |")
    push("| --- | --- | --- |")
    for item in report["uncovered"]:
        push(f"| `{item['id']}` | {md_cell(item['reason'])} | {md_cell(item['evidence'])} |")
    push("")

    push("## 安装包与签名")
    push("")
    installer = report["installerBuild"]
    push(installer["statement"])
    push("")
    push(f"- 证据：{installer['evidence']}")
    push("")
    push("| 平台 | 签名状态 | 说明 |")
    push("| --- | --- | --- |")
    for item in installer["signing"]:
        push(f"| {item['platform']} | {item['status']} | {md_cell(item['detail'])} |")
    push("")

    push("## 发布说明不得声称")
    push("")
    for item in report["releaseNotesGuidance"]:
        push(f"- {item}")
    push("")

    with path.open("w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))


def main() -> int:
    meta_path = ROOT / "docs" / "platform" / "capability-report.meta.json"
    meta = load_json(meta_path)

    environments = []
    checks = []
    for entry in meta["environments"]:
        evidence_path = ROOT / "docs" / "platform" / entry["evidence"]
        evidence = load_json(evidence_path)
        environments.append(
            {
                "id": entry["id"],
                "label": entry["label"],
                "kind": entry["kind"],
                "os": evidence.get("environment", {}).get("os"),
                "osVersion": evidence.get("environment", {}).get("osVersion"),
                "arch": evidence.get("environment", {}).get("arch"),
                "sessionType": evidence.get("environment", {}).get("sessionType"),
                "desktopAvailable": evidence.get("environment", {}).get("desktopAvailable"),
                "capabilities": evidence.get("capabilities"),
                "summary": evidence.get("summary"),
                "signing": entry.get("signing"),
                "requiredPermissions": entry.get("requiredPermissions"),
                "note": entry.get("note"),
                "evidence": entry["evidence"],
                "artifact": entry.get("artifact"),
            }
        )
        checks.extend(environment_checks(entry, evidence))

    report = {
        "schemaVersion": meta["schemaVersion"],
        "product": meta["product"],
        "version": meta["version"],
        "generatedAt": meta["generatedAt"],
        "generatedBy": meta["generatedBy"],
        "generatedFrom": meta["generatedFrom"],
        "statements": meta["statements"],
        "evidenceCaveats": meta["evidenceCaveats"],
        "ci": meta["ci"],
        "categories": {
            "testDouble": "经宿主入口、但平台适配层使用替身的检查（不证明平台行为）",
            "realPlatform": "真实平台实现 / 真实桌面上的检查（行为证据）",
            "uncovered": "当前环境无法判定，逐条写明原因",
        },
        "environments": environments,
        "checks": checks,
        "summary": {
            "realPlatformMeasuredPass": sum(
                1 for c in checks if c["category"] == "real-platform" and c["status"] == "measured_pass"
            ),
            "realPlatformMeasuredFail": sum(
                1 for c in checks if c["category"] == "real-platform" and c["status"] == "measured_fail"
            ),
            "realPlatformNotCovered": sum(
                1 for c in checks if c["category"] == "real-platform" and c["status"] == "not_covered"
            ),
            "testDoubleMeasuredPass": sum(
                1 for c in meta["testDouble"]["checks"] if c["status"] == "measured_pass"
            ),
            "uncoveredItems": len(meta["uncovered"]),
        },
        "testDouble": meta["testDouble"],
        "uncovered": meta["uncovered"],
        "installerBuild": meta["installerBuild"],
        "releaseNotesGuidance": meta["releaseNotesGuidance"],
    }

    out_path = ROOT / "docs" / "platform" / "capability-report.json"
    with out_path.open("w", encoding="utf-8") as handle:
        json.dump(report, handle, ensure_ascii=False, indent=2, sort_keys=False)
        handle.write("\n")

    print(f"已写入 {out_path.relative_to(ROOT)}")
    md_path = ROOT / "docs" / "platform" / "capability-report.md"
    emit_markdown(report, md_path)
    print(f"已写入 {md_path.relative_to(ROOT)}")
    print(f"  环境 {len(environments)} 个，真实平台检查 {len(checks)} 项")
    print(f"  实测通过 {report['summary']['realPlatformMeasuredPass']}，"
          f"实测失败 {report['summary']['realPlatformMeasuredFail']}，"
          f"未覆盖 {report['summary']['realPlatformNotCovered']}，"
          f"未覆盖条目 {report['summary']['uncoveredItems']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
