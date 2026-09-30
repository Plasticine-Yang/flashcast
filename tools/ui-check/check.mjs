// Flashcast UI 浏览器交互检查。
//
// 用真实的 Chrome（playwright-core + 系统 Chrome）驱动 Vite 开发服务器上的 UI，
// 走一遍主流程并截图。仓库不为 UI 写单元测试，这里是 UI 的验证手段。
//
// 只覆盖「浏览器里的 React UI + 浏览器模拟宿主」。它不是 Tauri webview，
// 因此**不能**证明托盘、全局快捷键或真实软件启动可用。
//
// 用法：
//   pnpm ui-check                                   # 仓库根目录
//   pnpm --dir tools/ui-check run check             # 等价写法
//   FLASHCAST_UI_URL=http://localhost:1420 pnpm ui-check   # 复用已启动的 Vite
//
// 环境变量：
//   FLASHCAST_UI_URL  复用已运行的开发服务器（不再自己启动 Vite）
//   CHROME_PATH       Chrome 可执行文件，默认 /usr/bin/google-chrome
//
// 产物：artifacts/ui/*.png 与 artifacts/ui/ui-check.log（artifacts/ 已被 gitignore）。

import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright-core";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(HERE, "..", "..");
const ARTIFACTS = resolve(REPO_ROOT, "artifacts", "ui");
const DEFAULT_URL = "http://localhost:1420";
const CHROME_PATH = process.env.CHROME_PATH || "/usr/bin/google-chrome";
const INPUT = '[data-testid="search-input"]';
const ROW = '[data-testid="result-item"]';
const SETTINGS_BUTTON = '[data-testid="open-settings"]';
const SETTINGS_SCREEN = '[data-testid="settings-screen"]';
const WORKSPACE_PATH_INPUT = '[data-testid="workspace-path-input"]';
const WORKSPACE_PATH_VALUE = '[data-testid="workspace-path-value"]';
const WORKSPACE_VALIDITY = '[data-testid="workspace-validity"]';
const SETTINGS_MESSAGE = '[data-testid="settings-message"]';
const CHANGE_ROW = '[data-testid="change-row"]';
const CHANGE_DIFF = '[data-testid="change-diff"]';
const COMMIT_MESSAGE = '[data-testid="commit-message"]';
const COMMIT_SUBMIT = '[data-testid="commit-submit"]';
const HOTKEY_INPUT = '[data-testid="hotkey-input"]';
const HOTKEY_STATUS = '[data-testid="hotkey-status"]';
const CLONE_URL_INPUT = '[data-testid="clone-url-input"]';
const CLONE_BUTTON = '[data-testid="workspace-clone"]';
const CLONE_CANCEL = '[data-testid="workspace-clone-cancel"]';
const CLONE_PROGRESS = '[data-testid="clone-progress"]';
const WORKSPACE_REMOTE = '[data-testid="workspace-remote"]';

// 浏览器模拟宿主认得的路径（见 src/api.ts）。
const MOCK_REPO = "/home/user/.config/flashcast";
const MOCK_BROKEN_REPO = "/home/user/broken-repo";
const MOCK_NEW_DIR = "/home/user/flashcast-config";
const MOCK_NON_EMPTY_DIR = "/home/user/Documents";
// 克隆用的模拟地址（见 src/api.ts 的 MockHost.clone_workspace）。
const MOCK_CLONE_TARGET = "/home/user/flashcast-clone";
const MOCK_CLONE_URL = "https://github.com/me/flashcast-config.git";
const MOCK_CLONE_BAD_URL = "https://github.com/me/not-found-config.git";
const MOCK_CLONE_SECRET_URL = "https://alice:sekret@github.com/me/flashcast-config.git";

const lines = [];
function log(line) {
  lines.push(line);
  console.log(line);
}

/** 极简断言：不引入任何测试框架，与「不为 UI 加单元测试」的约定保持一致。 */
function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

async function probe(url) {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(2000) });
    return response.ok;
  } catch {
    return false;
  }
}

async function waitForServer(url, timeoutMs = 60000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await probe(url)) return true;
    await new Promise((r) => setTimeout(r, 300));
  }
  return false;
}

/** 启动 Vite；若端口上已有服务则直接复用。返回 teardown 函数。 */
async function ensureServer(url) {
  if (process.env.FLASHCAST_UI_URL) {
    log(`· 复用 FLASHCAST_UI_URL=${url}`);
    return async () => {};
  }
  if (await probe(url)) {
    log(`· ${url} 上已有开发服务器，直接复用（本脚本不会关掉它）`);
    return async () => {};
  }
  log("· 启动 Vite（pnpm dev）…");
  const child = spawn("pnpm", ["dev"], {
    cwd: REPO_ROOT,
    stdio: ["ignore", "pipe", "pipe"],
    detached: true,
  });
  child.stdout.on("data", () => {});
  child.stderr.on("data", () => {});
  const teardown = async () => {
    if (child.exitCode !== null) return;
    try {
      process.kill(-child.pid, "SIGTERM");
    } catch {
      child.kill("SIGTERM");
    }
  };
  if (!(await waitForServer(url))) {
    await teardown();
    throw new Error(`Vite 在 60 秒内没有响应 ${url}`);
  }
  log("· Vite 已就绪");
  return teardown;
}

/** 读取当前选中行的下标与条目 id。 */
async function selection(page) {
  return page.evaluate((rowSelector) => {
    const rows = [...document.querySelectorAll(rowSelector)];
    const index = rows.findIndex((row) => row.dataset.selected === "true");
    return index < 0 ? null : { index, id: rows[index].dataset.itemId };
  }, ROW);
}

async function rows(page) {
  return page.$$eval(ROW, (nodes) =>
    nodes.map((node) => ({
      id: node.dataset.itemId,
      kind: node.dataset.kind,
      selected: node.dataset.selected === "true",
      title: node.querySelector(".result-title")?.textContent ?? "",
    })),
  );
}

async function shot(page, name) {
  const path = resolve(ARTIFACTS, name);
  await page.screenshot({ path });
  return name;
}

async function waitForRowCount(page, expected, timeout = 5000) {
  await page.waitForFunction(
    ({ selector, expected }) =>
      document.querySelectorAll(selector).length === expected,
    { selector: ROW, expected },
    { timeout },
  );
}

async function main() {
  mkdirSync(ARTIFACTS, { recursive: true });
  const url = process.env.FLASHCAST_UI_URL || DEFAULT_URL;
  const teardown = await ensureServer(url);

  const browser = await chromium.launch({
    executablePath: CHROME_PATH,
    headless: true,
    args: ["--no-sandbox"],
  });
  const results = [];

  try {
    const page = await browser.newPage({ viewport: { width: 900, height: 620 } });
    page.on("pageerror", (error) => log(`  ! 页面异常：${error.message}`));
    await page.goto(url, { waitUntil: "load" });
    await page.waitForSelector(INPUT);
    // 首屏是一次空查询，等第一批结果渲染出来。
    await page.waitForSelector(ROW);

    log(`· Chrome ${browser.version()}，URL ${url}`);

    const check = async (name, body) => {
      try {
        const detail = await body();
        results.push({ name, ok: true, detail });
        log(`PASS ${name}${detail ? ` — ${detail}` : ""}`);
      } catch (error) {
        results.push({ name, ok: false, detail: error.message });
        log(`FAIL ${name} — ${error.message}`);
      }
    };

    /** 变更为某个仓库相对路径的勾选框。 */
    const changeCheck = (path) =>
      page.locator(`${CHANGE_ROW}[data-path="${path}"] [data-testid="change-check"]`);

    /** 读取当前变更列表的关键状态。 */
    const changeRows = () =>
      page.$$eval(CHANGE_ROW, (nodes) =>
        nodes.map((node) => ({
          path: node.dataset.path,
          code: node.dataset.code,
          selected: node.dataset.selected === "true",
          staged: node.dataset.staged === "true",
          untracked: node.dataset.untracked === "true",
          status: node.querySelector('[data-testid="change-status"]')?.textContent ?? "",
        })),
      );

    /** 进入设置页并关联浏览器模拟宿主的工作区（每次 goto 后模拟宿主会重置）。 */
    const linkMockWorkspace = async () => {
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await page.fill(WORKSPACE_PATH_INPUT, MOCK_REPO);
      await page.click('[data-testid="workspace-select"]');
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: WORKSPACE_VALIDITY, expected: "已关联 Git 仓库" },
      );
      await page.waitForSelector(CHANGE_ROW);
    };

    // 1. 空查询显示快速访问项。
    await check("空查询显示快速访问项", async () => {
      const items = await rows(page);
      assert(items.length > 0, "首屏列表为空");
      const commands = items.filter((item) => item.kind === "command").length;
      const applications = items.filter((item) => item.kind === "application").length;
      assert(commands > 0, "首屏没有内置命令项");
      assert(applications > 0, "首屏没有快速访问的软件项");
      assert(
        items.some((item) => item.title.includes("Firefox")),
        `首屏缺少预期软件：${items.map((i) => i.title).join("/")}`,
      );
      const file = await shot(page, "01-home-empty-query.png");
      return `${items.length} 项（${commands} 命令 + ${applications} 软件），截图 ${file}`;
    });

    // 2. 输入过滤列表。
    await check("输入过滤列表", async () => {
      const before = (await rows(page)).length;
      await page.click(INPUT);
      await page.keyboard.type("终端", { delay: 20 });
      await waitForRowCount(page, 1);
      const items = await rows(page);
      assert(items.length < before, `过滤后条目数没有减少（${before} → ${items.length}）`);
      assert(items[0].title.includes("终端"), `过滤结果不是「终端」：${items[0].title}`);
      const file = await shot(page, "02-filtered.png");
      return `“终端” → ${items.length} 项（首屏 ${before} 项），截图 ${file}`;
    });

    // 3. 方向键移动选择。
    await check("方向键上下移动选择", async () => {
      await page.click(INPUT);
      await page.keyboard.press("Control+A");
      await page.keyboard.press("Backspace");
      await page.waitForFunction(
        (selector) => document.querySelectorAll(selector).length > 1,
        ROW,
      );
      const start = await selection(page);
      assert(start?.index === 0, `初始选中下标应为 0，实际 ${JSON.stringify(start)}`);
      await page.keyboard.press("ArrowDown");
      await page.waitForFunction(
        (selector) => document.querySelectorAll(selector)[1]?.dataset.selected === "true",
        ROW,
      );
      const down = await selection(page);
      assert(down?.index === 1, `ArrowDown 后应选中第 2 行，实际 ${JSON.stringify(down)}`);
      assert(down.id !== start.id, "ArrowDown 后条目 id 未变化");
      await page.keyboard.press("ArrowUp");
      await page.waitForFunction(
        (selector) => document.querySelectorAll(selector)[0]?.dataset.selected === "true",
        ROW,
      );
      const up = await selection(page);
      assert(up?.index === 0, `ArrowUp 后应回到第 1 行，实际 ${JSON.stringify(up)}`);
      const file = await shot(page, "03-arrow-selection.png");
      return `选中 0 → 1（${down.id}）→ 0，截图 ${file}`;
    });

    // 4. Escape 关闭窗口，再次唤起后查询被清空、回到首屏。
    await check("Escape 关闭并在唤起后清空查询", async () => {
      await page.keyboard.press("Escape");
      await page.waitForFunction(
        () => document.querySelector('[data-testid="app-root"]')?.dataset.windowVisible === "false",
        undefined,
      );
      const closed = await page.evaluate(() => ({
        visible: document.querySelector('[data-testid="app-root"]').dataset.windowVisible,
        hidden: window.__flashcastMock.hidden,
      }));
      assert(closed.visible === "false", `Escape 后窗口仍标记为可见：${closed.visible}`);
      assert(closed.hidden === true, "Escape 没有触发宿主隐藏窗口");
      const file = await shot(page, "04-escape-dismissed.png");

      await page.evaluate(() => window.__flashcastMock.summon());
      await page.waitForFunction(
        () => document.querySelector('[data-testid="app-root"]')?.dataset.windowVisible === "true",
        undefined,
      );
      const value = await page.inputValue(INPUT);
      const items = await rows(page);
      assert(value === "", `再次唤起后输入框未清空：${JSON.stringify(value)}`);
      assert(items.length > 1, `再次唤起后没有回到首屏列表（${items.length} 项）`);
      return `窗口 hidden=true 后由托盘/快捷键唤起，输入框回到空、首屏 ${items.length} 项，截图 ${file}`;
    });

    // 5. 中文输入法组合期间回车不执行；组合结束后回车才执行（正向对照）。
    await check("输入法组合期间回车不执行", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() => {
        const el = document.querySelector('[data-testid="search-input"]');
        window.__events = [];
        for (const type of ["compositionstart", "compositionend"]) {
          el.addEventListener(type, () => window.__events.push(type), true);
        }
      });
      const cdp = await page.context().newCDPSession(page);
      await page.click(INPUT);
      // 真实的浏览器级 IME 组合（不是手工 dispatch 的合成事件）：
      // 组合文本 "fire" 会让列表里出现 Firefox，所以下面的断言不是空过。
      await cdp.send("Input.imeSetComposition", {
        text: "fire",
        selectionStart: 4,
        selectionEnd: 4,
      });
      await waitForRowCount(page, 1);
      const composing = await page.evaluate(() => ({
        events: window.__events,
        value: document.querySelector('[data-testid="search-input"]').value,
      }));
      assert(
        composing.events.includes("compositionstart"),
        "没有触发 compositionstart",
      );
      assert(composing.value === "fire", `组合文本未进入输入框：${composing.value}`);
      const file = await shot(page, "05-ime-composing.png");

      await page.evaluate(() => {
        window.__flashcastMock.lastLaunched = null;
      });
      await page.keyboard.press("Enter");
      assert(
        (await page.evaluate(() => window.__flashcastMock.lastLaunched)) === null,
        "组合期间回车竟然执行了条目",
      );

      await cdp.send("Input.insertText", { text: "fire" });
      await page.waitForFunction(() => window.__events.includes("compositionend"), undefined, {
        timeout: 5000,
      });

      // 正向对照：组合结束后回车必须执行，否则上面的「不执行」没有说服力。
      await page.keyboard.press("Enter");
      const launched = await page.evaluate(() => window.__flashcastMock.lastLaunched);
      assert(launched === "app:firefox", `组合结束后回车没有执行：lastLaunched=${launched}`);
      return `组合中回车 lastLaunched=null → compositionend 后回车 lastLaunched=${launched}，截图 ${file}`;
    });

    // 6. 鼠标悬停不改变键盘选择。
    await check("鼠标悬停不改变键盘选择", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      const before = await selection(page);
      const target = await page.evaluate((selector) => {
        const nodes = [...document.querySelectorAll(selector)];
        const index = nodes.findIndex((node) => node.dataset.selected !== "true");
        return index < 0 ? null : { index, id: nodes[index].dataset.itemId };
      }, ROW);
      assert(target, "没有可悬停的未选中行");
      await page.hover(`[data-item-id="${target.id}"]`);
      await page.waitForTimeout(150);
      const after = await selection(page);
      const hovered = (await rows(page)).find((item) => item.id === target.id);
      assert(
        after?.index === before.index && after?.id === before.id,
        `悬停后选择被抢走：${JSON.stringify(before)} → ${JSON.stringify(after)}`,
      );
      assert(hovered.selected === false, "被悬停的行被标记为选中");
      const file = await shot(page, "06-hover-keeps-selection.png");
      return `悬停第 ${target.index + 1} 行（${target.id}），选中仍为第 ${after.index + 1} 行，截图 ${file}`;
    });

    // 7. 从启动器进入设置页，关联一个现有本地 Git 仓库。
    await check("设置页可关联本地 Git 仓库", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);

      const before = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(
        before.includes("尚未关联"),
        `初始状态应显示尚未关联工作区：${before}`,
      );
      const unlinked = await shot(page, "07-settings-unlinked.png");

      await page.fill(WORKSPACE_PATH_INPUT, MOCK_REPO);
      await page.click('[data-testid="workspace-select"]');
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: WORKSPACE_VALIDITY, expected: "已关联 Git 仓库" },
      );
      const linked = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(linked === MOCK_REPO, `当前工作区应为 ${MOCK_REPO}，实际 ${linked}`);
      const settingsFile = (
        await page.textContent('[data-testid="workspace-settings-file"]')
      ).trim();
      assert(
        settingsFile.endsWith("settings.toml"),
        `设置文件路径不对：${settingsFile}`,
      );
      const file = await shot(page, "08-settings-workspace-linked.png");
      return `未关联 → ${linked}（${settingsFile}），截图 ${unlinked} / ${file}`;
    });

    // 8. 目标工作区配置无效：显示中文原因，并保留当前工作区。
    await check("无效配置显示原因且不切换工作区", async () => {
      await page.fill(WORKSPACE_PATH_INPUT, MOCK_BROKEN_REPO);
      await page.click('[data-testid="workspace-select"]');
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "配置无效" },
      );
      const message = (await page.textContent(SETTINGS_MESSAGE)).trim();
      const current = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(current === MOCK_REPO, `失败后必须保留当前工作区，实际 ${current}`);
      const file = await shot(page, "09-settings-validation-error.png");
      return `显示「${message}」，当前工作区仍为 ${current}，截图 ${file}`;
    });

    // 9. 初始化新目录：非空目录被拒绝，空目录建好工作区。
    await check("初始化新目录并拒绝覆盖非空目录", async () => {
      await page.fill(WORKSPACE_PATH_INPUT, MOCK_NON_EMPTY_DIR);
      await page.click('[data-testid="workspace-init"]');
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "非空" },
      );
      const refused = (await page.textContent(SETTINGS_MESSAGE)).trim();
      assert(
        refused.includes("不会覆盖") || refused.includes("以免覆盖"),
        `拒绝原因必须是可读的中文说明：${refused}`,
      );
      const still = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(still === MOCK_REPO, `被拒绝后不得切换工作区，实际 ${still}`);

      await page.fill(WORKSPACE_PATH_INPUT, MOCK_NEW_DIR);
      await page.click('[data-testid="workspace-init"]');
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: WORKSPACE_PATH_VALUE, expected: MOCK_NEW_DIR },
      );
      const created = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(created === MOCK_NEW_DIR, `应切换到新建工作区，实际 ${created}`);
      const file = await shot(page, "10-settings-initialised.png");
      return `非空目录被拒绝（${refused}），新目录 ${created} 初始化成功，截图 ${file}`;
    });

    // 10. 编辑快捷键：立即生效；无效写法被拒绝并保留上次有效值。
    await check("修改快捷键立即生效，无效写法被拒绝", async () => {
      await page.fill(HOTKEY_INPUT, "Ctrl+Shift+F1");
      await page.click('[data-testid="hotkey-save"]');
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: HOTKEY_STATUS, expected: "Ctrl+Shift+F1" },
      );
      const applied = (await page.textContent(SETTINGS_MESSAGE)).trim();
      assert(applied.includes("立即生效"), `应提示立即生效：${applied}`);

      await page.fill(HOTKEY_INPUT, "这不是快捷键");
      await page.click('[data-testid="hotkey-save"]');
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "快捷键无效" },
      );
      const rejected = (await page.textContent(SETTINGS_MESSAGE)).trim();
      const status = (await page.textContent(HOTKEY_STATUS)).trim();
      assert(
        status.includes("Ctrl+Shift+F1"),
        `无效写法必须保留上次有效快捷键：${status}`,
      );
      const file = await shot(page, "11-settings-hotkey.png");
      return `生效 ${applied}；无效写法「${rejected}」后仍为 ${status}，截图 ${file}`;
    });

    // 11. 外部修改设置文件：有效则重新加载，无效则保留并显示原因。
    await check("外部修改重新加载，错误配置保留上次有效状态", async () => {
      await page.evaluate(() =>
        window.__flashcastMock.simulateExternalEdit("Ctrl+Alt+K"),
      );
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: HOTKEY_STATUS, expected: "Ctrl+Alt+K" },
      );
      const applied = (await page.textContent(SETTINGS_MESSAGE)).trim();
      assert(applied.includes("重新加载"), `应提示重新加载：${applied}`);

      await page.evaluate(() =>
        window.__flashcastMock.simulateExternalEdit("不是快捷键"),
      );
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "配置无效" },
      );
      const error = (await page.textContent(SETTINGS_MESSAGE)).trim();
      const status = (await page.textContent(HOTKEY_STATUS)).trim();
      assert(
        status.includes("Ctrl+Alt+K"),
        `错误配置必须保留上次有效快捷键：${status}`,
      );
      const file = await shot(page, "12-settings-external-reload.png");

      // Escape 从设置页返回搜索首屏。
      await page.keyboard.press("Escape");
      await page.waitForFunction(
        () =>
          document.querySelector('[data-testid="app-root"]')?.dataset.screen === "search",
        undefined,
      );
      const back = await page.inputValue(INPUT);
      assert(back === "", `返回后输入框应回到空查询：${JSON.stringify(back)}`);
      return `重载 ${applied}；错误「${error}」后仍为 ${status}，Escape 返回首屏，截图 ${file}`;
    });

    // 13. 从远端克隆：进度、完成状态、远端关系与恢复内容都可见。
    await check("从远端克隆并显示进度与完成状态", async () => {
      // 上一项检查用 Escape 回到了首屏，这里重新进入设置页。
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      const before = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      await page.fill(WORKSPACE_PATH_INPUT, MOCK_CLONE_TARGET);
      await page.fill(CLONE_URL_INPUT, MOCK_CLONE_URL);
      await page.click(CLONE_BUTTON);
      await page.waitForFunction(
        ({ sel }) => /正在/.test(document.querySelector(sel)?.textContent ?? ""),
        { sel: CLONE_PROGRESS },
      );
      // 尽量截到带真实计数的中间态（已接收对象 / 已检出文件）。
      await page
        .waitForFunction(
          ({ sel }) => /已接收|已检出/.test(document.querySelector(sel)?.textContent ?? ""),
          { sel: CLONE_PROGRESS },
          { timeout: 5000 },
        )
        .catch(() => {});
      const during = (await page.textContent(CLONE_PROGRESS)).trim();
      const progressShot = await shot(page, "20-clone-progress.png");

      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "已从远端克隆并关联工作区" },
      );
      const linked = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(linked === MOCK_CLONE_TARGET, `克隆后应关联 ${MOCK_CLONE_TARGET}，实际 ${linked}`);
      const remote = (await page.textContent(WORKSPACE_REMOTE)).trim();
      assert(remote.includes("origin") && remote.includes("main"), `远端信息不对：${remote}`);
      const message = (await page.textContent(SETTINGS_MESSAGE)).trim();
      assert(
        message.includes("origin") && message.includes("主题") && message.includes("插件"),
        `完成说明应报告远端与本机覆盖情况：${message}`,
      );
      const doneShot = await shot(page, "21-clone-completed.png");
      return `${before} → ${during} → ${linked}（${remote}），截图 ${progressShot} / ${doneShot}`;
    });

    // 14. 克隆失败：显示中文原因，保留当前工作区。
    await check("克隆失败显示中文原因且不切换工作区", async () => {
      const before = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      await page.fill(WORKSPACE_PATH_INPUT, "/home/user/flashcast-clone-failed");
      await page.fill(CLONE_URL_INPUT, MOCK_CLONE_BAD_URL);
      await page.click(CLONE_BUTTON);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "克隆失败" },
      );
      const message = (await page.textContent(SETTINGS_MESSAGE)).trim();
      const current = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(current === before, `失败后必须保留当前工作区，实际 ${current}`);
      const file = await shot(page, "22-clone-failed.png");
      return `显示「${message}」，当前工作区仍为 ${current}，截图 ${file}`;
    });

    // 15. 克隆中取消：显示已取消，保留当前工作区。
    await check("克隆过程中取消并保留当前工作区", async () => {
      const before = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      await page.fill(WORKSPACE_PATH_INPUT, "/home/user/flashcast-clone-cancelled");
      await page.fill(CLONE_URL_INPUT, MOCK_CLONE_URL);
      await page.click(CLONE_BUTTON);
      await page.waitForFunction(
        ({ sel }) => /正在/.test(document.querySelector(sel)?.textContent ?? ""),
        { sel: CLONE_PROGRESS },
      );
      await page.click(CLONE_CANCEL);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "取消" },
      );
      const message = (await page.textContent(SETTINGS_MESSAGE)).trim();
      const current = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(current === before, `取消后必须保留当前工作区，实际 ${current}`);
      const file = await shot(page, "23-clone-cancelled.png");
      return `显示「${message}」，当前工作区仍为 ${current}，截图 ${file}`;
    });

    // 16. 目标目录已有文件：拒绝克隆，不覆盖。
    await check("目标目录非空时拒绝克隆且不覆盖", async () => {
      const before = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      await page.fill(WORKSPACE_PATH_INPUT, MOCK_NON_EMPTY_DIR);
      await page.fill(CLONE_URL_INPUT, MOCK_CLONE_URL);
      await page.click(CLONE_BUTTON);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "非空" },
      );
      const message = (await page.textContent(SETTINGS_MESSAGE)).trim();
      const current = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      assert(current === before, `拒绝后必须保留当前工作区，实际 ${current}`);
      const file = await shot(page, "24-clone-non-empty.png");
      return `显示「${message}」，当前工作区仍为 ${current}，截图 ${file}`;
    });

    // 17. 克隆地址里带口令：拒绝，并且口令不出现在界面文本里。
    await check("克隆地址包含密码时拒绝且不泄露", async () => {
      await page.fill(WORKSPACE_PATH_INPUT, "/home/user/flashcast-clone-secret");
      await page.fill(CLONE_URL_INPUT, MOCK_CLONE_SECRET_URL);
      await page.click(CLONE_BUTTON);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "密码" },
      );
      const message = (await page.textContent(SETTINGS_MESSAGE)).trim();
      assert(!message.includes("sekret"), `界面文本不得含口令：${message}`);
      const body = await page.textContent("body");
      assert(!body.includes("sekret"), "页面任何位置都不得出现口令");
      const file = await shot(page, "25-clone-secret-refused.png");
      return `显示「${message}」，页面无口令文本，截图 ${file}`;
    });

    // 12. 真实窗口尺寸（640×420）下设置页仍可操作。
    await check("设置页在 640×420 窗口内可操作", async () => {
      await page.setViewportSize({ width: 640, height: 420 });
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      assert(await page.isVisible('[data-testid="workspace-section"]'), "工作区区段不可见");
      assert(await page.isVisible(WORKSPACE_PATH_INPUT), "路径输入框不可见");

      // 快捷键区段可能在折叠内容下方：滚动后仍必须可见、可点击。
      await page.locator(HOTKEY_INPUT).scrollIntoViewIfNeeded();
      assert(await page.isVisible(HOTKEY_INPUT), "快捷键输入框不可见");
      assert(await page.isVisible('[data-testid="hotkey-save"]'), "保存按钮不可见");
      const box = await page.locator('[data-testid="hotkey-save"]').boundingBox();
      assert(
        box && box.x >= 0 && box.x + box.width <= 640,
        `保存按钮超出窗口宽度：${JSON.stringify(box)}`,
      );

      // 克隆区段同样必须可用：滚动到克隆按钮，检查可见且不超出窗口宽度。
      await page.locator(CLONE_URL_INPUT).scrollIntoViewIfNeeded();
      assert(await page.isVisible(CLONE_URL_INPUT), "克隆远端地址输入框不可见");
      assert(await page.isVisible(CLONE_BUTTON), "克隆按钮不可见");
      const cloneBox = await page.locator(CLONE_BUTTON).boundingBox();
      assert(
        cloneBox && cloneBox.x >= 0 && cloneBox.x + cloneBox.width <= 640,
        `克隆按钮超出窗口宽度：${JSON.stringify(cloneBox)}`,
      );

      // 变更与提交区段同样必须在真实窗口尺寸下可见、可操作。
      await page.locator('[data-testid="changes-section"]').scrollIntoViewIfNeeded();
      assert(await page.isVisible('[data-testid="changes-section"]'), "变更区段不可见");
      assert(await page.isVisible(CHANGE_ROW), "变更列表不可见");
      await page.locator(COMMIT_MESSAGE).scrollIntoViewIfNeeded();
      assert(await page.isVisible(COMMIT_MESSAGE), "提交说明输入框不可见");
      const commitButton = await page.locator(COMMIT_SUBMIT).boundingBox();
      assert(
        commitButton && commitButton.x >= 0 && commitButton.x + commitButton.width <= 640,
        `创建提交按钮超出窗口宽度：${JSON.stringify(commitButton)}`,
      );
      const file = await shot(page, "13-settings-compact-window.png");
      await page.setViewportSize({ width: 900, height: 620 });
      return `640×420 下工作区、快捷键与克隆区段均可见可操作，截图 ${file}`;
    });

    // 13. 变更区展示分支、差异基准、每个文件的状态与真实差异。
    await check("变更区展示分支、基准、状态与真实差异", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await linkMockWorkspace();

      const branch = (await page.textContent('[data-testid="changes-branch"]')).trim();
      assert(branch === "main", `必须显示当前分支，实际 ${branch}`);
      const base = (await page.textContent('[data-testid="changes-diff-base"]')).trim();
      assert(
        base.includes("HEAD") && base.includes("工作区"),
        `差异基准必须透明呈现，实际 ${base}`,
      );
      const count = (await page.textContent('[data-testid="changes-count"]')).trim();
      assert(count.includes("3 个文件"), `待提交数量不对：${count}`);

      const list = await changeRows();
      assert(list.length === 3, `应有 3 个变更文件，实际 ${list.length}`);
      const byPath = Object.fromEntries(list.map((row) => [row.path, row]));
      assert(byPath["settings.toml"].code === " M", JSON.stringify(byPath["settings.toml"]));
      assert(byPath["settings.toml"].staged === false, "settings.toml 不应标记为已暂存");
      assert(
        byPath["settings.toml"].status.includes("未暂存"),
        `未暂存状态必须点明：${byPath["settings.toml"].status}`,
      );
      assert(byPath["theme.json"].code === "MM", JSON.stringify(byPath["theme.json"]));
      assert(byPath["theme.json"].staged === true, "已暂存改动必须单独标记");
      assert(
        byPath["theme.json"].status.includes("已暂存"),
        `已暂存状态必须点明：${byPath["theme.json"].status}`,
      );
      assert(byPath["memos/2026-10-01.md"].untracked === true, "未跟踪文件必须被识别");
      const stagedShot = await shot(page, "14-settings-changes.png");

      // 点击某个文件，展示它的真实差异内容（不是命令字符串）。
      await page.click(`${CHANGE_ROW}[data-path="settings.toml"] [data-testid="change-path"]`);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: CHANGE_DIFF, expected: '+hotkey = "Super+Space"' },
      );
      const diff = (await page.textContent(CHANGE_DIFF)).trim();
      assert(
        diff.includes('-hotkey = "Ctrl+Alt+Space"') && diff.includes("+++ b/settings.toml"),
        `必须展示真实补丁：${diff}`,
      );

      // 已暂存 + 未暂存的文件：差异必须是 HEAD → 工作区的最终内容。
      await page.click(`${CHANGE_ROW}[data-path="theme.json"] [data-testid="change-path"]`);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: CHANGE_DIFF, expected: "serif" },
      );
      const stagedDiff = (await page.textContent(CHANGE_DIFF)).trim();
      assert(
        !stagedDiff.includes("contrast"),
        `差异不得只显示已暂存的中间内容：${stagedDiff}`,
      );
      const diffShot = await shot(page, "15-settings-diff.png");
      return `分支 ${branch}，基准「${base}」，${list.length} 个文件；真实补丁含新旧两行与最终内容，截图 ${stagedShot} / ${diffShot}`;
    });

    // 14. 显式范围提交：只提交勾选的路径，未勾选的已暂存改动原样保留。
    await check("只提交勾选的路径，未勾选的暂存改动保留", async () => {
      await changeCheck("settings.toml").check();
      await page.fill(COMMIT_MESSAGE, "提交设置改动");
      const scope = (await page.textContent('[data-testid="commit-scope"]')).trim();
      assert(scope.includes("勾选的 1 / 3"), `提交范围必须透明：${scope}`);

      await page.click(COMMIT_SUBMIT);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "已创建提交" },
      );
      const message = (await page.textContent(SETTINGS_MESSAGE)).trim();
      assert(message.includes("1 个文件"), `结果说明必须点明文件数：${message}`);

      const list = await changeRows();
      const remaining = list.map((row) => row.path);
      assert(
        !remaining.includes("settings.toml"),
        `已提交的路径必须从列表消失：${remaining}`,
      );
      assert(
        remaining.includes("theme.json") && remaining.includes("memos/2026-10-01.md"),
        `未勾选的改动必须保留：${remaining}`,
      );
      const theme = list.find((row) => row.path === "theme.json");
      assert(theme.staged === true, "未勾选的已暂存改动不得被吞掉");
      const file = await shot(page, "16-settings-commit-partial.png");
      return `「${message}」，剩余 ${remaining.join("、")}（theme.json 仍为已暂存），截图 ${file}`;
    });

    // 15. 未勾选任何路径时提交被拒绝，改动不丢。
    await check("未勾选路径时提交被拒绝且改动不丢", async () => {
      const before = (await changeRows()).length;
      // 先把提交说明填好，确认失败原因确实来自「没有勾选路径」而不是空说明。
      await page.fill(COMMIT_MESSAGE, "空范围检查");
      await page.click(COMMIT_SUBMIT);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "没有选择" },
      );
      const message = (await page.textContent(SETTINGS_MESSAGE)).trim();
      assert(message.includes("勾选"), `必须点明提交范围由勾选决定：${message}`);
      const after = await changeRows();
      assert(after.length === before, `失败不得改动仓库状态：${before} → ${after.length}`);
      return `显示「${message}」，变更列表仍为 ${after.length} 项`;
    });

    // 16. 身份未配置 / 索引被占用 / 工作区异常：失败原因明确，改动保留。
    await check("提交失败时给出明确的中文原因且改动保留", async () => {
      await changeCheck("memos/2026-10-01.md").check();
      await page.fill(COMMIT_MESSAGE, "会失败的提交");

      await page.evaluate(() => window.__flashcastMock.simulateCommitError("identity"));
      await page.click(COMMIT_SUBMIT);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "user.name" },
      );
      const identity = (await page.textContent(SETTINGS_MESSAGE)).trim();
      assert(
        identity.includes("user.email"),
        `必须点明缺哪两项身份配置：${identity}`,
      );

      await page.evaluate(() => {
        window.__flashcastMock.clearCommitError();
        window.__flashcastMock.simulateCommitError("locked");
      });
      await page.click(COMMIT_SUBMIT);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "index.lock" },
      );
      const locked = (await page.textContent(SETTINGS_MESSAGE)).trim();
      const rows = await changeRows();
      assert(
        rows.some((row) => row.path === "memos/2026-10-01.md"),
        "提交失败后改动不得消失",
      );
      const errorShot = await shot(page, "17-settings-commit-error.png");

      await page.evaluate(() => window.__flashcastMock.clearCommitError());
      return `身份「${identity}」、索引「${locked}」，改动仍在，截图 ${errorShot}`;
    });

    // 17. 不是 Git 仓库 / 无变更：明确说明而不是空白或报错。
    await check("不是 Git 仓库与无变更都有明确说明", async () => {
      await page.evaluate(() => window.__flashcastMock.simulateGitUnavailable(true));
      await page.click('[data-testid="changes-refresh"]');
      await page.waitForSelector('[data-testid="changes-unavailable"]');
      const unavailable = (await page.textContent('[data-testid="changes-unavailable"]')).trim();
      assert(
        unavailable.includes("不是 Git 仓库"),
        `必须说明工作区不是 Git 仓库：${unavailable}`,
      );
      const unavailableShot = await shot(page, "18-settings-git-unavailable.png");

      await page.evaluate(() => window.__flashcastMock.simulateGitUnavailable(false));
      await page.click('[data-testid="changes-refresh"]');
      await page.waitForSelector(CHANGE_ROW);

      await page.click('[data-testid="changes-select-all"]');
      await page.fill(COMMIT_MESSAGE, "提交全部改动");
      await page.click(COMMIT_SUBMIT);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "已创建提交" },
      );
      await page.waitForSelector('[data-testid="changes-empty"]');
      const empty = (await page.textContent('[data-testid="changes-empty"]')).trim();
      const count = (await page.textContent('[data-testid="changes-count"]')).trim();
      assert(empty.includes("没有可提交"), `应说明没有可提交的变更：${empty}`);
      assert(count.includes("没有可提交"), `待提交说明不对：${count}`);
      const emptyShot = await shot(page, "19-settings-changes-empty.png");

      return `不是仓库时「${unavailable}」；全部提交后「${empty}」，截图 ${unavailableShot} / ${emptyShot}`;
    });

    const failed = results.filter((result) => !result.ok).length;
    log("");
    log(`汇总：通过 ${results.length - failed}，失败 ${failed}（共 ${results.length} 项）`);
    log("范围说明：本检查只覆盖浏览器中的 UI 与模拟宿主，不代表 Tauri 托盘、");
    log("         全局快捷键、自动粘贴或真实软件启动可用。");
    if (failed > 0) {
      process.exitCode = 1;
    }
  } finally {
    await browser.close();
    await teardown();
    writeFileSync(resolve(ARTIFACTS, "ui-check.log"), `${lines.join("\n")}\n`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
