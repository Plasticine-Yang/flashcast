#!/usr/bin/env node
// 只接受本仓库、同一源码提交的完整 CI；发布不重复执行检查矩阵。
import { execFileSync, spawnSync } from "node:child_process";
import { appendFileSync, mkdirSync, openSync, closeSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { validateMetadata } from "./metadata.mjs";

export const REQUIRED_JOBS = [
  "Linux x64", "Windows x64", "macOS arm64 (native)", "macOS x86_64 (cross-compiled)",
  "跨目标类型检查 x86_64-pc-windows-msvc", "跨目标类型检查 x86_64-apple-darwin",
  "浏览器交互检查",
];

function gh(args) {
  return execFileSync("gh", args, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] }).trim();
}
export function assertSuccessfulCI(run, commit) {
  if (run.headSha !== commit) throw new Error("CI 源码提交不匹配");
  if (!["push", "workflow_dispatch"].includes(run.event)) throw new Error("必须使用本仓库分支 CI，不能使用 PR 检查");
  if (run.status !== "completed" || run.conclusion !== "success") throw new Error("源码 CI 尚未完整通过");
  for (const name of REQUIRED_JOBS) {
    const job = run.jobs.find(job => job.name === name);
    if (!job || job.conclusion !== "success") throw new Error(`缺少成功的必需检查：${name}`);
  }
}
export async function requireCI(commit, { wait = false, tag = null } = {}) {
  if (!/^[0-9a-f]{40}$/.test(commit)) throw new Error("需要完整的 40 位源码 SHA");
  if (tag) validateMetadata(tag);
  const runs = JSON.parse(gh(["run", "list", "--workflow", "ci.yml", "--commit", commit,
    "--limit", "50", "--json", "databaseId,headSha,event,status,conclusion,url"]));
  const candidate = runs.find(run => run.headSha === commit && ["push", "workflow_dispatch"].includes(run.event));
  if (!candidate) throw new Error("此提交没有分支 CI。先推送源码；纯文档提交可手动运行 CI。");
  if (candidate.status !== "completed" && wait) {
    console.log(`等待源码 CI：${candidate.url}`);
    mkdirSync("artifacts/release", { recursive: true });
    const fd = openSync("artifacts/release/ci-watch.log", "w");
    try {
      // 单一持续监听；详细轮询写入文件，终端仅显示最终结果。
      const result = spawnSync("gh", ["run", "watch", String(candidate.databaseId), "--exit-status", "--interval", "30"],
        { stdio: ["ignore", fd, fd], timeout: 12 * 60 * 1000 });
      if (result.error || result.status !== 0) throw new Error("源码 CI 未通过，详情见 artifacts/release/ci-watch.log");
    } finally { closeSync(fd); }
  }
  const run = JSON.parse(gh(["run", "view", String(candidate.databaseId), "--json", "headSha,event,status,conclusion,jobs,url"]));
  assertSuccessfulCI(run, commit);
  console.log(`源码检查已通过：${run.url}`);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `ci-run-url=${run.url}\n`);
  return run;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  requireCI(process.argv[2], { wait: process.argv.includes("--wait"),
    tag: process.env.GITHUB_REF_TYPE === "tag" ? process.env.GITHUB_REF_NAME : null }).catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
