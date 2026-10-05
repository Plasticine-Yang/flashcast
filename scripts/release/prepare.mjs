#!/usr/bin/env node
// 冻结已验收的源码；只有 --publish 才创建并推送新的版本标签。
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCI } from "./require-ci.mjs";
import { validateMetadata } from "./metadata.mjs";

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
export function validateSource(tag) {
  if (!/^v\d+\.\d+\.\d+$/.test(tag ?? "")) throw new Error("用法：pnpm release:prepare vX.Y.Z（添加 --publish 才打标签）");
  if (git("status", "--porcelain")) throw new Error("先提交源码与验收记录；发布要求工作区干净");
  validateMetadata(tag);
  if (git("tag", "--list", tag)) throw new Error(`本地标签 ${tag} 已存在；使用新的版本号`);
  if (git("ls-remote", "--tags", "origin", `refs/tags/${tag}`)) throw new Error(`远端标签 ${tag} 已存在；使用新的版本号`);
  return git("rev-parse", "HEAD");
}
export async function prepare(tag, { publish = false } = {}) {
  const commit = validateSource(tag);
  const ci = await requireCI(commit, { wait: true });
  // 等待期间的任何源码或工作区变化都会使冻结失效。
  if (validateSource(tag) !== commit) throw new Error("等待检查期间源码发生变化，请重新准备发布");
  mkdirSync("artifacts/release", { recursive: true });
  writeFileSync("artifacts/release/frozen-source.json", JSON.stringify({ tag, commit, ci: ci.url }, null, 2) + "\n");
  console.log(`已冻结 ${tag}：${commit}`);
  if (publish) {
    git("tag", "-a", tag, commit, "-m", `发布 ${tag}`);
    git("push", "origin", `refs/tags/${tag}`);
    console.log("标签已推送；Release 工作流将只打包并发布这个提交。");
  }
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2).filter(arg => arg !== "--");
  const tag = args.find(arg => !arg.startsWith("--"));
  if (args.some(arg => arg.startsWith("--") && arg !== "--publish")) {
    console.error("未知选项；只支持 --publish"); process.exitCode = 1;
  } else prepare(tag, { publish: args.includes("--publish") }).catch(error => {
    console.error(error.message); process.exitCode = 1;
  });
}
