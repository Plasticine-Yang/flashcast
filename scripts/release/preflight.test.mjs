import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { REQUIRED_JOBS } from "./require-ci.mjs";

const prepare = resolve("scripts/release/prepare.mjs");
const requireCI = resolve("scripts/release/require-ci.mjs");

// 穿过 CLI、真实 Git 工作区及一个可控制的 gh 进程；不发布、不访问网络。
function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "flashcast-release-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const work = join(dir, "work");
  mkdirSync(work);
  const git = (...args) => execFileSync("git", args, { cwd: work, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  execFileSync("git", ["init", "--bare", join(dir, "origin.git")], { stdio: "ignore" });
  git("init"); git("config", "user.name", "Release Check"); git("config", "user.email", "release@example.invalid");
  git("remote", "add", "origin", join(dir, "origin.git"));
  mkdirSync(join(work, "src-tauri")); mkdirSync(join(work, "docs/release"), { recursive: true });
  writeFileSync(join(work, "Cargo.toml"), '[workspace.package]\nversion = "0.4.0"\n');
  writeFileSync(join(work, "package.json"), '{"version":"0.4.0"}');
  writeFileSync(join(work, "src-tauri/tauri.conf.json"), '{"version":"0.4.0"}');
  writeFileSync(join(work, "docs/release/v0.4.0.md"), '# v0.4.0\n发布说明\n');
  writeFileSync(join(work, ".gitignore"), 'artifacts/\n');
  git("add", "."); git("commit", "-m", "准备版本");
  const sha = git("rev-parse", "HEAD");
  const run = { databaseId: 17, headSha: sha, event: "push", status: "completed", conclusion: "success",
    url: "https://github.com/example/flashcast/actions/runs/17", jobs: REQUIRED_JOBS.map(name => ({ name, conclusion: "success" })) };
  const bin = join(dir, "bin"); mkdirSync(bin);
  const data = join(dir, "run.json");
  // gh 的真实命令形状被保留；只替换它与远端的边界。
  writeFileSync(join(bin, "gh"), `#!${process.execPath}\n` + `
import fs from 'node:fs';
const run = JSON.parse(fs.readFileSync(process.env.RELEASE_CHECK_RUN, 'utf8'));
const args = process.argv.slice(2);
if (args[0] !== 'run') process.exit(2);
if (args[1] === 'list') console.log(JSON.stringify(process.env.RELEASE_CHECK_MISSING ? [] : [run]));
else if (args[1] === 'view') {
  if (process.env.RELEASE_CHECK_MUTATE) fs.writeFileSync('package.json', '{"version":"0.5.0"}');
  console.log(JSON.stringify(run));
}
else process.exit(3);
`, { mode: 0o755 });
  const execute = (script, args = [], env = {}) => {
    writeFileSync(data, JSON.stringify(run));
    return spawnSync(process.execPath, [script, ...args], { cwd: work, encoding: "utf8",
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, RELEASE_CHECK_RUN: data, ...env } });
  };
  return { work, git, sha, run, execute };
}

test("完整成功 CI 才生成绑定 SHA 的冻结记录，不创建标签", t => {
  const f = fixture(t); const result = f.execute(prepare, ["v0.4.0"]);
  assert.equal(result.status, 0, result.stderr);
  const proof = JSON.parse(readFileSync(join(f.work, "artifacts/release/frozen-source.json")));
  assert.equal(proof.commit, f.sha); assert.equal(proof.tag, "v0.4.0");
  assert.equal(f.git("tag", "--list"), "");
});
for (const [label, mutate] of [
  ["源码 SHA 不一致", f => { f.run.headSha = 'a'.repeat(40); }],
  ["PR 检查不能用于发布", f => { f.run.event = "pull_request"; }],
  ["浏览器检查跳过", f => { f.run.jobs.find(j => j.name === "浏览器交互检查").conclusion = "skipped"; }],
  ["平台矩阵缺项", f => { f.run.jobs = f.run.jobs.filter(j => j.name !== "Windows x64"); }],
  ["仍在运行", f => { f.run.status = "in_progress"; }],
  ["CI 失败", f => { f.run.conclusion = "failure"; }],
]) test(`拒绝${label}`, t => {
  const f = fixture(t); mutate(f);
  const result = f.execute(requireCI, [f.sha]);
  assert.notEqual(result.status, 0); assert.notEqual(result.stderr, "");
});
for (const [label, mutate] of [
  ["未提交修改", f => writeFileSync(join(f.work, "package.json"), '{"version":"0.5.0"}')],
  ["版本元数据不一致", f => { writeFileSync(join(f.work, "package.json"), '{"version":"0.5.0"}'); f.git("add", "."); f.git("commit", "-m", "不同步版本"); }],
  ["已存在本地标签", f => f.git("tag", "v0.4.0")],
  ["已存在远端标签", f => { f.git("tag", "v0.4.0"); f.git("push", "origin", "refs/tags/v0.4.0"); f.git("tag", "-d", "v0.4.0"); }],
]) test(`准备发布拒绝${label}`, t => {
  const f = fixture(t); mutate(f); const result = f.execute(prepare, ["v0.4.0"]);
  assert.notEqual(result.status, 0);
  assert.equal(existsSync(join(f.work, "artifacts/release/frozen-source.json")), false);
});

test("此提交没有 CI 时拒绝冻结", t => {
  const f = fixture(t); const result = f.execute(prepare, ["v0.4.0"], { RELEASE_CHECK_MISSING: "1" });
  assert.notEqual(result.status, 0); assert.match(result.stderr, /没有分支 CI/);
});

test("标签推送创建绑定冻结 SHA 的注释标签", t => {
  const f = fixture(t); const result = f.execute(prepare, ["v0.4.0", "--publish"]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(f.git("rev-parse", "v0.4.0^{commit}"), f.sha);
  assert.equal(f.git("cat-file", "-t", "v0.4.0"), "tag");
  assert.match(f.git("ls-remote", "--tags", "origin", "refs/tags/v0.4.0^{}"), new RegExp(`^${f.sha}`));
});

test("绕过本地入口的错误版本标签在打包前被拒绝", t => {
  const f = fixture(t); const result = f.execute(requireCI, [f.sha], { GITHUB_REF_TYPE: "tag", GITHUB_REF_NAME: "v0.5.0" });
  assert.notEqual(result.status, 0); assert.match(result.stderr, /版本不一致/);
});


test("远端检查期间工作区变化使冻结失效", t => {
  const f = fixture(t); const result = f.execute(prepare, ["v0.4.0"], { RELEASE_CHECK_MUTATE: "1" });
  assert.notEqual(result.status, 0); assert.match(result.stderr, /工作区干净/);
  assert.equal(existsSync(join(f.work, "artifacts/release/frozen-source.json")), false);
  assert.equal(f.git("tag", "--list"), "");
});
