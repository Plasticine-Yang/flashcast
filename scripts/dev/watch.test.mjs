import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { createServer } from "vite";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

// 使用仓库的真实配置与 watcher，在小型工作区检查产物隔离，不依赖本机 inotify 限额。
test("Vite 忽略开发产物，同时保留前端源码监听与更新", { timeout: 15_000 }, async t => {
  const root = await mkdtemp(join(tmpdir(), "flashcast-watch-"));
  let server;
  t.after(async () => {
    await server?.close();
    await rm(root, { recursive: true, force: true });
  });
  const excluded = ["target", "artifacts", "src-tauri", "dist", "node_modules", ".git"];
  const fixtures = [
    "target/debug/incremental/example/object.o",
    "target/x86_64-pc-windows-msvc/debug/object.o",
    "artifacts/ui/screenshot.png",
    "src-tauri/src/main.rs",
    "dist/index.html",
    ".git/objects/example",
  ];
  for (const file of [...fixtures, "src/main.js", "index.html"]) {
    const fullPath = join(root, file);
    await mkdir(dirname(fullPath), { recursive: true });
    await writeFile(fullPath, file === "index.html"
      ? '<script type="module" src="/src/main.js"></script>'
      : 'export const message = "before";');
  }
  // 夹具复用已安装依赖，React 插件预优化时也能正常解析包。
  await symlink(join(repoRoot, "node_modules"), join(root, "node_modules"), "junction");

  server = await createServer({
    configFile: join(repoRoot, "vite.config.ts"),
    root,
    cacheDir: join(root, ".vite"),
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  let watchError;
  server.watcher.on("error", error => { watchError = error; });
  await server.listen();

  // Vite 会分批添加 root、public 与配置依赖；首个 ready 不代表根目录已遍历完。
  let watched;
  let previousCount = -1;
  let stable = 0;
  for (let i = 0; i < 100; i++) {
    await delay(50);
    if (watchError) throw watchError;
    watched = server.watcher.getWatched();
    const count = Object.keys(watched).length;
    stable = count === previousCount ? stable + 1 : 0;
    previousCount = count;
    if (watched[join(root, "src")]?.includes("main.js") && stable >= 5) break;
  }
  assert.ok(watched[join(root, "src")]?.includes("main.js"), "前端源码未被监听");
  for (const directory of excluded) {
    const prefix = join(root, directory);
    assert.equal(Object.keys(watched).some(dir => dir === prefix || dir.startsWith(prefix + sep)),
      false, `${directory}/ 被纳入监听，会随产物积累耗尽额度`);
  }

  const url = server.resolvedUrls.local[0];
  const before = await fetch(new URL("src/main.js", url));
  assert.equal(before.status, 200);
  assert.match(await before.text(), /before/);
  const source = join(root, "src/main.js");
  const changed = new Promise((resolveChange, reject) => {
    const timer = setTimeout(() => {
      server.watcher.off("change", onChange);
      reject(new Error("前端源码修改没有触发 watcher"));
    }, 5_000);
    function onChange(file) {
      if (resolve(file) !== source) return;
      clearTimeout(timer);
      server.watcher.off("change", onChange);
      resolveChange();
    }
    server.watcher.on("change", onChange);
  });
  await writeFile(source, 'export const message = "after";');
  await changed;
  const after = await fetch(new URL("src/main.js", url));
  assert.equal(after.status, 200);
  assert.match(await after.text(), /after/);
  if (watchError) throw watchError;
});
