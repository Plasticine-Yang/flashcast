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
const SYNC_SECTION = '[data-testid="sync-section"]';
const SYNC_BRANCH = '[data-testid="sync-branch"]';
const SYNC_REMOTE = '[data-testid="sync-remote"]';
const SYNC_COUNTS = '[data-testid="sync-counts"]';
const SYNC_DIRTY = '[data-testid="sync-dirty"]';
const SYNC_CAPABILITY = '[data-testid="sync-capability"]';
const SYNC_STATE = '[data-testid="sync-state"]';
const SYNC_BLOCK = '[data-testid="sync-block"]';
const SYNC_BLOCK_LABEL = '[data-testid="sync-block-label"]';
const SYNC_BLOCK_HINT = '[data-testid="sync-block-hint"]';
const SYNC_PROGRESS = '[data-testid="sync-progress"]';
const SYNC_PULL = '[data-testid="sync-pull"]';
const SYNC_PUSH = '[data-testid="sync-push"]';
const SYNC_REDETECT = '[data-testid="sync-redetect"]';
const THEME_SECTION = '[data-testid="theme-section"]';
const THEME_ITEM = '[data-testid="theme-item"]';
const THEME_APPEARANCE = '[data-testid="theme-appearance"]';
const THEME_ERROR = '[data-testid="theme-error"]';
const THEME_PACKAGE_INPUT = '[data-testid="theme-package-input"]';
const THEME_INSTALL = '[data-testid="theme-install"]';
const THEME_SELECT = '[data-testid="theme-select"]';
const THEME_REMOVE = '[data-testid="theme-remove"]';
// 备忘录与功能插件（ticket 07）。
const SCOPE_LABEL = '[data-testid="scope-label"]';
const DEFAULT_ACTION_LABEL = '[data-testid="default-action-label"]';
const NOTICE = '[data-testid="notice"]';
const EMPTY_STATE = '[data-testid="empty-state"]';
const PLUGIN_SECTION = '[data-testid="plugin-section"]';
const PLUGIN_ITEM = '[data-testid="plugin-item"]';
const PLUGIN_STATE = '[data-testid="plugin-state"]';
const PLUGIN_TOGGLE = '[data-testid="plugin-toggle"]';
const MEMO_SECTION = '[data-testid="memo-section"]';
const MEMO_DISABLED = '[data-testid="memo-disabled"]';
const MEMO_ITEM = '[data-testid="memo-item"]';
const MEMO_TITLE_INPUT = '[data-testid="memo-title-input"]';
const MEMO_TAGS_INPUT = '[data-testid="memo-tags-input"]';
const MEMO_BODY_INPUT = '[data-testid="memo-body-input"]';
const MEMO_SAVE = '[data-testid="memo-save"]';
const MEMO_PREVIEW = '[data-testid="memo-preview"]';
const MEMO_PREVIEW_BODY = '[data-testid="memo-preview-body"]';
const MEMO_PREVIEW_TOGGLE = '[data-testid="memo-preview-toggle"]';

const THEME_LIGHT = "flashcast.theme.light";
const THEME_DARK = "flashcast.theme.dark";
const THEME_SYSTEM = "flashcast.theme.system";
const INSTALLED_THEME = "example.solarized";

// 浏览器模拟宿主认得的主题包路径（见 src/api.ts / src/mockThemes.ts）。
const MOCK_THEME_PACKAGE = "/home/user/themes/solarized";
const MOCK_BROKEN_THEME_PACKAGE = "/home/user/themes/broken";

// 内置主题在浏览器模拟宿主里的表面色（见 src/mockThemes.ts）。
const SURFACE_LIGHT = "#ffffff";
const SURFACE_DARK = "#202226";
const SURFACE_SOLARIZED = "#002b36";

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

/** 读取根元素上一个主题 CSS 自定义属性的当前值。 */
async function themeVar(page, name) {
  return page.evaluate(
    (property) =>
      getComputedStyle(document.documentElement).getPropertyValue(property).trim(),
    name,
  );
}

/** 等待主题切换生效。 */
async function waitForTheme(page, id, timeout = 5000) {
  await page.waitForFunction(
    (expected) => document.documentElement.dataset.themeSelected === expected,
    id,
    { timeout },
  );
}

/**
 * 某个主题生效时，根元素上的关键 CSS 变量、对比度与状态可辨识度。
 *
 * 颜色解析与 WCAG 计算直接写在页面上下文里（与 `flashcast-core` 的校验同口径），
 * 避免把正则塞进模板字符串带来的转义问题。
 */
async function themeFacts(page) {
  return page.evaluate(() => {
    const parse = (value) => {
      const text = String(value).trim().toLowerCase();
      if (text.startsWith("#")) {
        const hex = text.slice(1);
        const expand = (part) => parseInt(part.length === 1 ? part + part : part, 16);
        if (hex.length === 3 || hex.length === 4) {
          return [
            expand(hex[0]),
            expand(hex[1]),
            expand(hex[2]),
            hex.length === 4 ? expand(hex[3]) / 255 : 1,
          ];
        }
        if (hex.length === 6 || hex.length === 8) {
          return [
            expand(hex.slice(0, 2)),
            expand(hex.slice(2, 4)),
            expand(hex.slice(4, 6)),
            hex.length === 8 ? expand(hex.slice(6, 8)) / 255 : 1,
          ];
        }
        return null;
      }
      const match = text.match(new RegExp("rgba?\\s*\\(([^)]+)\\)"));
      if (!match) return null;
      const parts = match[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
      if (parts.length < 3 || parts.some((part) => Number.isNaN(part))) return null;
      return [parts[0], parts[1], parts[2], parts.length > 3 ? parts[3] : 1];
    };
    const blend = (front, back) => {
      const alpha = front[3];
      return [
        front[0] * alpha + back[0] * (1 - alpha),
        front[1] * alpha + back[1] * (1 - alpha),
        front[2] * alpha + back[2] * (1 - alpha),
        1,
      ];
    };
    const luminance = (color) => {
      const channel = (value) => {
        const v = value / 255;
        return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
      };
      return 0.2126 * channel(color[0]) + 0.7152 * channel(color[1]) + 0.0722 * channel(color[2]);
    };
    const white = [255, 255, 255, 1];
    const contrast = (foreground, background) => {
      const front = parse(foreground);
      const back = parse(background);
      if (!front || !back) return null;
      const solidFront = blend(front, blend(back, white));
      const solidBack = blend(back, white);
      const a = luminance(solidFront);
      const b = luminance(solidBack);
      const [hi, lo] = a > b ? [a, b] : [b, a];
      return (hi + 0.05) / (lo + 0.05);
    };
    const delta = (foreground, background) => {
      const front = parse(foreground);
      const back = parse(background);
      if (!front || !back) return null;
      const solidFront = blend(front, blend(back, white));
      const solidBack = blend(back, white);
      return (
        Math.max(
          Math.abs(solidFront[0] - solidBack[0]),
          Math.abs(solidFront[1] - solidBack[1]),
          Math.abs(solidFront[2] - solidBack[2]),
        ) / 255
      );
    };

    const style = getComputedStyle(document.documentElement);
    const read = (name) => style.getPropertyValue(name).trim();
    const surface = read("--fc-surface");
    const pageBackground = read("--fc-page-bg");
    return {
      selected: document.documentElement.dataset.themeSelected,
      appearance: document.documentElement.dataset.themeAppearance,
      surface,
      pageBackground,
      text: read("--fc-text"),
      textMuted: read("--fc-text-muted"),
      selectionBackground: read("--fc-selection-bg"),
      selectionBorder: read("--fc-selection-border"),
      focusRing: read("--fc-focus-ring"),
      errorBackground: read("--fc-error-bg"),
      rowHeight: read("--fc-row-height"),
      fontBody: read("--fc-font-body"),
      spaceWindowPadding: read("--fc-space-window-padding"),
      radiusWindow: read("--fc-radius-window"),
      textContrast: contrast(read("--fc-text"), surface),
      textOnPageContrast: contrast(read("--fc-text"), pageBackground),
      mutedContrast: contrast(read("--fc-text-muted"), surface),
      selectionDelta: delta(read("--fc-selection-bg"), surface),
      focusDelta: delta(read("--fc-focus-ring"), read("--fc-selection-border")),
      errorDelta: delta(read("--fc-error-bg"), surface),
    };
  });
}

/** 高频操作元素的动画状态：任何主题下都必须是 0s。 */
async function animationFacts(page) {
  return page.evaluate((selectors) => {
    const read = (node) => {
      const style = getComputedStyle(node);
      return {
        transitionDuration: style.transitionDuration,
        transitionDelay: style.transitionDelay,
        animationDuration: style.animationDuration,
        animationName: style.animationName,
      };
    };
    const result = {};
    for (const selector of selectors) {
      const node = document.querySelector(selector);
      if (!node) {
        result[selector] = null;
        continue;
      }
      result[selector] = read(node);
    }
    return result;
  }, [
    '[data-testid="app-root"]',
    '[data-testid="search-input"]',
    '[data-testid="result-item"]',
    '[data-testid="action-bar"]',
    '[data-testid="open-settings"]',
    '[data-testid="settings-screen"]',
    '[data-testid="theme-item"]',
    '[data-testid="workspace-select"]',
  ]);
}

/** 关键控件的包围盒，用于断言切换主题不移动控件。 */
async function layoutBoxes(page, selectors) {
  const boxes = {};
  for (const selector of selectors) {
    const box = await page.locator(selector).first().boundingBox();
    boxes[selector] = box
      ? {
          x: Math.round(box.x),
          y: Math.round(box.y),
          width: Math.round(box.width),
          height: Math.round(box.height),
        }
      : null;
  }
  return boxes;
}

function sameBoxes(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
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
    page.setDefaultTimeout(20_000);
    await page.goto(url, { waitUntil: "load" });
    await page.waitForSelector(INPUT);
    // 首屏是一次空查询，等第一批结果渲染出来。
    await page.waitForSelector(ROW);

    log(`· Chrome ${browser.version()}，URL ${url}`);

    // 单项检查的硬超时：任何一项卡住都只算这一项失败，不会拖住整轮检查。
    const CHECK_TIMEOUT = 90_000;
    const check = async (name, body) => {
      log(`· 开始：${name}`);
      let timer = null;
      try {
        const detail = await Promise.race([
          body(),
          new Promise((_, reject) => {
            timer = setTimeout(
              () => reject(new Error(`检查超过 ${CHECK_TIMEOUT / 1000} 秒未完成`)),
              CHECK_TIMEOUT,
            );
          }),
        ]);
        results.push({ name, ok: true, detail });
        log(`PASS ${name}${detail ? ` — ${detail}` : ""}`);
      } catch (error) {
        results.push({ name, ok: false, detail: error.message });
        log(`FAIL ${name} — ${error.message}`);
      } finally {
        if (timer) clearTimeout(timer);
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

      // 同步区段同样必须在真实窗口尺寸下可见、可操作。
      await page.locator(SYNC_SECTION).scrollIntoViewIfNeeded();
      assert(await page.isVisible(SYNC_SECTION), "同步区段不可见");
      assert(await page.isVisible(SYNC_PULL), "拉取按钮不可见");
      assert(await page.isVisible(SYNC_REDETECT), "重新检测按钮不可见");
      const pullBox = await page.locator(SYNC_PULL).boundingBox();
      assert(
        pullBox && pullBox.x >= 0 && pullBox.x + pullBox.width <= 640,
        `拉取按钮超出窗口宽度：${JSON.stringify(pullBox)}`,
      );
      const file = await shot(page, "13-settings-compact-window.png");
      await page.setViewportSize({ width: 900, height: 620 });
      return `640×420 下工作区、快捷键、克隆与同步区段均可见可操作，截图 ${file}`;
    });

    // -----------------------------------------------------------------------
    // 主题（ticket 06）：全部默认主题、系统外观跟随、本地主题包与系统缩放。
    // -----------------------------------------------------------------------

    /** 回到搜索首屏。 */
    const backToSearch = async () => {
      await page.keyboard.press("Escape");
      await page.waitForFunction(
        () =>
          document.querySelector('[data-testid="app-root"]')?.dataset.screen === "search",
        undefined,
        { timeout: 10_000 },
      );
    };

    /** 把设置页滚回顶部，保证跨主题测量的是同一滚动位置。 */
    const resetSettingsScroll = async () => {
      await page.evaluate(() => {
        const el = document.querySelector(".settings");
        if (el) el.scrollTop = 0;
      });
    };

    /** 打开设置页并关联一个工作区（goto 之后模拟宿主是全新的）。 */
    const openSettingsWithWorkspace = async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      const current = (await page.textContent(WORKSPACE_PATH_VALUE)).trim();
      if (!current.includes(MOCK_REPO)) {
        await page.fill(WORKSPACE_PATH_INPUT, MOCK_REPO);
        await page.click('[data-testid="workspace-select"]');
        await page.waitForFunction(
          ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
          { sel: WORKSPACE_VALIDITY, expected: "已关联 Git 仓库" },
          { timeout: 10_000 },
        );
      }
    };

    /** 断言当前主题下高频操作没有动画、文字可读、状态可辨识。 */
    const assertThemeIsUsable = async (label) => {
      const facts = await themeFacts(page);
      assert(
        facts.textContrast >= 4.5,
        `${label}：正文与表面色的对比度只有 ${facts.textContrast?.toFixed(2)}:1`,
      );
      assert(
        facts.mutedContrast >= 4.5,
        `${label}：辅助文字对比度只有 ${facts.mutedContrast?.toFixed(2)}:1`,
      );
      assert(
        facts.textOnPageContrast >= 4.5,
        `${label}：正文与页面背景（${facts.pageBackground}）的对比度只有 ${facts.textOnPageContrast?.toFixed(2)}:1`,
      );
      assert(
        facts.selectionDelta >= 0.03,
        `${label}：选中背景与表面太接近（Δ=${facts.selectionDelta?.toFixed(3)}）`,
      );
      assert(
        facts.focusDelta >= 0.03,
        `${label}：焦点环与选中边框太接近（Δ=${facts.focusDelta?.toFixed(3)}）`,
      );
      assert(
        facts.errorDelta >= 0.03,
        `${label}：错误背景与表面太接近（Δ=${facts.errorDelta?.toFixed(3)}）`,
      );

      const animation = await animationFacts(page);
      for (const [selector, value] of Object.entries(animation)) {
        if (!value) continue;
        const durations = `${value.transitionDuration},${value.animationDuration}`
          .split(",")
          .map((part) => part.trim());
        assert(
          durations.every((duration) => duration === "0s"),
          `${label}：${selector} 存在动画（transition=${value.transitionDuration}，animation=${value.animationDuration}）`,
        );
        assert(
          value.transitionDelay.split(",").every((delay) => delay.trim() === "0s"),
          `${label}：${selector} 存在过渡延迟（${value.transitionDelay}）`,
        );
      }
      return facts;
    };

    // 14. 三个默认主题：可切换、外观正确、文字可读、选中/焦点可辨识、无动画、布局不变。
    await check("三个默认主题可切换且布局与无动画规则不变", async () => {
      await page.setViewportSize({ width: 900, height: 620 });
      await page.emulateMedia({ colorScheme: "light", reducedMotion: "no-preference" });
      await openSettingsWithWorkspace();

      const settingsSelectors = [WORKSPACE_PATH_INPUT, HOTKEY_INPUT, THEME_SECTION];
      const searchSelectors = [INPUT, SETTINGS_BUTTON, ROW, '[data-testid="action-bar"]'];
      const themes = [
        { id: THEME_LIGHT, appearance: "light", surface: SURFACE_LIGHT, settings: "14-theme-light.png", list: "15-theme-light-list.png" },
        { id: THEME_DARK, appearance: "dark", surface: SURFACE_DARK, settings: "16-theme-dark.png", list: "17-theme-dark-list.png" },
        { id: THEME_SYSTEM, appearance: "light", surface: SURFACE_LIGHT, settings: "18-theme-follow-system-light.png", list: "19-theme-follow-system-light-list.png" },
      ];

      let settingsBaseline = null;
      let searchBaseline = null;
      let geometry = null;
      const shots = [];
      const details = [];

      for (const theme of themes) {
        // 已选中的主题按钮是禁用的，不必再点（默认就是浅色）。
        const alreadySelected = await page.evaluate(
          (id) =>
            document.querySelector(`[data-theme-id="${id}"]`)?.dataset.selected === "true",
          theme.id,
        );
        if (!alreadySelected) {
          await page.click(`[data-theme-id="${theme.id}"] ${THEME_SELECT}`);
        }
        await waitForTheme(page, theme.id);

        const facts = await assertThemeIsUsable(theme.id);
        assert(
          facts.appearance === theme.appearance,
          `${theme.id} 的实际外观应为 ${theme.appearance}，实际 ${facts.appearance}`,
        );
        assert(
          facts.surface === theme.surface,
          `${theme.id} 的 --fc-surface 应为 ${theme.surface}，实际 ${facts.surface}`,
        );
        if (!geometry) {
          geometry = {
            rowHeight: facts.rowHeight,
            fontBody: facts.fontBody,
            spaceWindowPadding: facts.spaceWindowPadding,
            radiusWindow: facts.radiusWindow,
          };
        } else {
          for (const key of Object.keys(geometry)) {
            assert(
              facts[key] === geometry[key],
              `切换主题改变了 ${key}：${geometry[key]} → ${facts[key]}`,
            );
          }
        }

        await resetSettingsScroll();
        const settingsBoxes = await layoutBoxes(page, settingsSelectors);
        if (settingsBaseline === null) {
          settingsBaseline = settingsBoxes;
        } else {
          assert(
            sameBoxes(settingsBaseline, settingsBoxes),
            `切换主题移动了设置页控件：${JSON.stringify(settingsBaseline)} → ${JSON.stringify(settingsBoxes)}`,
          );
        }
        shots.push(await shot(page, theme.settings));

        await backToSearch();
        const searchBoxes = await layoutBoxes(page, searchSelectors);
        if (searchBaseline === null) {
          searchBaseline = searchBoxes;
        } else {
          assert(
            sameBoxes(searchBaseline, searchBoxes),
            `切换主题移动了首屏控件：${JSON.stringify(searchBaseline)} → ${JSON.stringify(searchBoxes)}`,
          );
        }
        shots.push(await shot(page, theme.list));
        details.push(`${theme.id}=${facts.appearance}/${facts.surface}`);
        await page.click(SETTINGS_BUTTON);
        await page.waitForSelector(THEME_SECTION);
      }

      return `${details.join("，")}；布局与几何在所有主题下一致；截图 ${shots.join(" / ")}`;
    });

    // 15. 跟随系统：OS 外观在运行时变化时立即切换，无需重启。
    await check("跟随系统在运行时响应系统外观变化", async () => {
      // 上一项检查已经选中「跟随系统」，此时它的选择按钮是禁用的。
      const alreadySystem = await page.evaluate(
        (id) => document.querySelector(`[data-theme-id="${id}"]`)?.dataset.selected === "true",
        THEME_SYSTEM,
      );
      if (!alreadySystem) {
        await page.click(`[data-theme-id="${THEME_SYSTEM}"] ${THEME_SELECT}`);
      }
      await waitForTheme(page, THEME_SYSTEM);
      assert(
        (await themeFacts(page)).appearance === "light",
        "初始应为浅色（模拟环境默认浅色）",
      );

      // 模拟 OS 切到深色：App 通过 prefers-color-scheme 感知并重新解析主题。
      await page.emulateMedia({ colorScheme: "dark" });
      await page.waitForFunction(
        () => document.documentElement.dataset.themeAppearance === "dark",
        undefined,
        { timeout: 10_000 },
      );
      const dark = await assertThemeIsUsable("跟随系统（系统深色）");
      assert(dark.surface === SURFACE_DARK, `系统深色下应为深色表面，实际 ${dark.surface}`);
      const settingsShot = await shot(page, "20-theme-follow-system-dark.png");
      await backToSearch();
      const listShot = await shot(page, "21-theme-follow-system-dark-list.png");

      // 切回浅色：同样立即生效，且主题选择没有被改动。
      await page.emulateMedia({ colorScheme: "light" });
      await page.waitForFunction(
        () => document.documentElement.dataset.themeAppearance === "light",
        undefined,
        { timeout: 10_000 },
      );
      const light = await themeFacts(page);
      assert(light.selected === THEME_SYSTEM, "系统外观变化不得改变主题选择");
      assert(light.surface === SURFACE_LIGHT, "切回浅色后表面色应恢复");
      return `系统深色 → ${dark.surface}，系统浅色 → ${light.surface}，主题选择保持 ${light.selected}；截图 ${settingsShot} / ${listShot}`;
    });

    // 16. 减少动态效果：高频操作在 reduced-motion 下同样没有动画。
    await check("尊重减少动态效果设置且高频操作无动画", async () => {
      await page.emulateMedia({ reducedMotion: "reduce" });
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(THEME_SECTION);
      await assertThemeIsUsable("reduced-motion（设置页）");
      await backToSearch();
      const facts = await assertThemeIsUsable("reduced-motion（首屏）");
      const file = await shot(page, "22-theme-reduced-motion.png");
      await page.emulateMedia({ reducedMotion: "no-preference" });
      return `reduced-motion 下首屏与设置页的 transition/animation 均为 0s（当前 ${facts.selected}），截图 ${file}`;
    });

    // 17. 本地主题包：无效包给出中文原因并保留外观，有效包可安装、选择、移除。
    await check("安装、选择与移除本地主题包；无效包保留外观", async () => {
      await openSettingsWithWorkspace();
      const before = await themeFacts(page);

      await page.fill(THEME_PACKAGE_INPUT, MOCK_BROKEN_THEME_PACKAGE);
      await page.click(THEME_INSTALL);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "主题无效" },
        { timeout: 10_000 },
      );
      const reason = (await page.textContent(SETTINGS_MESSAGE)).trim();
      const after = await themeFacts(page);
      assert(
        after.surface === before.surface && after.selected === before.selected,
        `无效主题包不得改变外观：${before.surface}/${before.selected} → ${after.surface}/${after.selected}`,
      );
      assert(
        !(await page.$('[data-theme-id="example.solarized"]')),
        "无效主题包不得出现在主题列表里",
      );
      const invalidShot = await shot(page, "23-theme-invalid-keeps-appearance.png");

      await page.fill(THEME_PACKAGE_INPUT, MOCK_THEME_PACKAGE);
      await page.click(THEME_INSTALL);
      await page.waitForSelector(`[data-theme-id="${INSTALLED_THEME}"]`);
      await page.click(`[data-theme-id="${INSTALLED_THEME}"] ${THEME_SELECT}`);
      await waitForTheme(page, INSTALLED_THEME);
      const installed = await assertThemeIsUsable(INSTALLED_THEME);
      assert(
        installed.surface === SURFACE_SOLARIZED,
        `安装的主题必须真的生效：期望 ${SURFACE_SOLARIZED}，实际 ${installed.surface}`,
      );
      const installedShot = await shot(page, "24-theme-installed.png");
      await backToSearch();
      const installedListShot = await shot(page, "25-theme-installed-list.png");

      // 移除：回退到浅色并说明原因。
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(THEME_SECTION);
      await page.click(`[data-theme-id="${INSTALLED_THEME}"] ${THEME_REMOVE}`);
      await page.waitForFunction(
        (id) => !document.querySelector(`[data-theme-id="${id}"]`),
        INSTALLED_THEME,
        { timeout: 10_000 },
      );
      await waitForTheme(page, THEME_LIGHT);
      const removed = await themeFacts(page);
      assert(removed.surface === SURFACE_LIGHT, "移除后必须回到浅色外观");
      const removedShot = await shot(page, "26-theme-removed.png");

      return `无效包「${reason}」后外观保持 ${before.surface}；安装 ${INSTALLED_THEME} → ${installed.surface}；移除后回到 ${removed.surface}；截图 ${invalidShot} / ${installedShot} / ${installedListShot} / ${removedShot}`;
    });

    // 18. 系统缩放：200% 缩放（deviceScaleFactor=2）下设置页与主题区仍可操作。
    await check("系统缩放 200% 下主题与设置页仍可操作", async () => {
      const scaled = await browser.newContext({
        viewport: { width: 640, height: 420 },
        deviceScaleFactor: 2,
      });
      try {
        const scaledPage = await scaled.newPage();
        await scaledPage.goto(url, { waitUntil: "load" });
        await scaledPage.waitForSelector(ROW);
        await scaledPage.click(SETTINGS_BUTTON);
        await scaledPage.waitForSelector(THEME_SECTION);
        assert(await scaledPage.isVisible(THEME_SECTION), "缩放下主题区不可见");
        await scaledPage.locator(`[data-theme-id="${THEME_DARK}"] ${THEME_SELECT}`).scrollIntoViewIfNeeded();
        assert(
          await scaledPage.isVisible(`[data-theme-id="${THEME_DARK}"] ${THEME_SELECT}`),
          "缩放下主题选择按钮不可见",
        );
        await scaledPage.click(`[data-theme-id="${THEME_DARK}"] ${THEME_SELECT}`);
        await scaledPage.waitForFunction(
          (expected) => document.documentElement.dataset.themeSelected === expected,
          THEME_DARK,
          { timeout: 10_000 },
        );
        const box = await scaledPage.locator(THEME_SECTION).boundingBox();
        assert(
          box && box.x >= 0 && box.x + box.width <= 640,
          `缩放下主题区超出窗口宽度：${JSON.stringify(box)}`,
        );
        const file = await shot(scaledPage, "27-scaling-200.png");
        const facts = await themeFacts(scaledPage);
        assert(facts.surface === SURFACE_DARK, `缩放下主题必须生效，实际 ${facts.surface}`);
        return `deviceScaleFactor=2、视口 640×420 下主题区可见可选（${facts.surface}），截图 ${file}`;
      } finally {
        await scaled.close();
      }
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

    /** 切换同步场景并点「重新检测」，等待该场景对应的状态文案出现。 */
    const setSyncScenario = async (scenario, blockLabel) => {
      await page.evaluate((value) => window.__flashcastMock.simulateSyncScenario(value), scenario);
      await page.click(SYNC_REDETECT);
      if (blockLabel) {
        await page.waitForFunction(
          ({ sel, expected }) => document.querySelector(sel)?.textContent?.trim() === expected,
          { sel: SYNC_BLOCK_LABEL, expected: blockLabel },
        );
      } else {
        await page.waitForFunction(
          ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
          { sel: SYNC_BLOCK, expected: "没有阻塞" },
        );
      }
    };

    const text = async (selector) => (await page.textContent(selector)).trim();

    // 18. 同步区展示分支、远端、领先 / 落后、未提交改动、同步能力与进行中操作。
    await check("同步区展示分支、远端、待同步与同步能力", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await linkMockWorkspace();
      await page.waitForSelector(SYNC_BRANCH);

      const branch = await text(SYNC_BRANCH);
      assert(branch === "main", `必须显示当前分支，实际 ${branch}`);
      const remote = await text(SYNC_REMOTE);
      assert(
        remote.includes("origin") && remote.includes(MOCK_CLONE_URL),
        `必须显示远端名与地址：${remote}`,
      );
      const counts = await text(SYNC_COUNTS);
      assert(counts.includes("领先 0") && counts.includes("落后 0"), `待同步数量不对：${counts}`);
      const dirty = await text(SYNC_DIRTY);
      assert(dirty.includes("工作区干净"), `干净工作区必须说明：${dirty}`);
      const capability = await text(SYNC_CAPABILITY);
      assert(
        capability.includes("拉取可用") && capability.includes("推送可用"),
        `同步能力不对：${capability}`,
      );
      assert(capability.includes("没有需要推送的提交"), `应说明无需推送：${capability}`);
      const state = await text(SYNC_STATE);
      assert(state === "无", `没有进行中的操作时应显示「无」，实际 ${state}`);
      assert(
        (await text(SYNC_BLOCK)).includes("没有阻塞"),
        "没有阻塞时必须明确说明可以同步",
      );
      const file = await shot(page, "36-settings-sync.png");
      return `分支 ${branch}，远端「${remote}」，${counts}，${dirty}，${capability}，截图 ${file}`;
    });

    // 19. 未提交修改阻塞拉取（推送仍然可用），并给出外部处理指引。
    await check("未提交修改阻塞拉取并给出指引，推送仍可用", async () => {
      await setSyncScenario("dirty", "有未提交修改");
      const label = await text(SYNC_BLOCK_LABEL);
      assert(label === "有未提交修改", `阻塞标签不对：${label}`);
      const hint = await text(SYNC_BLOCK_HINT);
      assert(hint.includes("提交") && hint.includes("重新检测"), `指引不可操作：${hint}`);
      assert(await page.locator(SYNC_PULL).isDisabled(), "阻塞时拉取按钮必须禁用");
      assert(
        !(await page.locator(SYNC_PUSH).isDisabled()),
        "未提交修改只搬运已提交对象，推送不应被禁用",
      );
      const dirty = await text(SYNC_DIRTY);
      assert(dirty.includes("已暂存") && dirty.includes("未跟踪"), `脏状态要分类点明：${dirty}`);
      const file = await shot(page, "37-settings-sync-blocked.png");
      return `阻塞「${label}」，指引指向外部提交后重新检测，截图 ${file}`;
    });

    // 20. 分叉：两边都不动，明确不给自动合并。
    await check("分叉阻塞拉取且不自动合并", async () => {
      await setSyncScenario("diverged", "历史已分叉");
      const hint = await text(SYNC_BLOCK_HINT);
      assert(
        hint.includes("不强推") && hint.includes("三方合并编辑器"),
        `分叉指引不完整：${hint}`,
      );
      const counts = await text(SYNC_COUNTS);
      assert(counts.includes("领先 1") && counts.includes("落后 1"), `分叉数量不对：${counts}`);
      assert(await page.locator(SYNC_PULL).isDisabled(), "分叉时拉取必须禁用");
      const file = await shot(page, "38-settings-sync-diverged.png");
      return `阻塞「${await text(SYNC_BLOCK_LABEL)}」，${counts}，不自动合并，截图 ${file}`;
    });

    // 21. 鉴权失败与离线分类不同，且本地功能不受影响。
    await check("鉴权失败与离线分类不同且本地功能可用", async () => {
      await setSyncScenario("auth", "authFailed".replace("authFailed", ""));
      await page.click(SYNC_PULL);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "鉴权失败" },
      );
      const auth = await text(SETTINGS_MESSAGE);
      assert(auth.includes("令牌"), `鉴权失败要给出令牌指引：${auth}`);
      const authShot = await shot(page, "39-settings-sync-auth.png");

      await setSyncScenario("offline");
      await page.click(SYNC_PULL);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "网络不可用" },
      );
      const offline = await text(SETTINGS_MESSAGE);
      assert(offline !== auth, "鉴权失败与离线必须是不同的反馈");
      assert(offline.includes("网络"), `离线反馈不对：${offline}`);
      const offlineShot = await shot(page, "40-settings-sync-offline.png");

      // 本地功能不降级：回到搜索页仍能查询并得到结果。
      await page.click('[data-testid="settings-back"]');
      await page.fill(INPUT, "firefox");
      await page.waitForSelector(ROW);
      const results = await rows(page);
      assert(results.length > 0, "离线时本地搜索仍必须可用");
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      return `鉴权「${auth.slice(0, 24)}…」与离线「${offline.slice(0, 24)}…」不同，本地搜索仍返回 ${results.length} 条，截图 ${authShot} / ${offlineShot}`;
    });

    // 22. 快进拉取：工作区先显示进行中，完成后重新加载设置、主题与备忘录。
    await check("快进拉取展示进行中并重新加载设置、主题与备忘录", async () => {
      await page.evaluate(() => window.__flashcastMock.simulateSyncScenario("ready"));
      await page.click(SYNC_REDETECT);
      await page.evaluate(() => window.__flashcastMock.simulateRemoteCommit("Alt+Space"));
      await page.click(SYNC_REDETECT);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SYNC_COUNTS, expected: "落后 1" },
      );
      const before = await page.inputValue(HOTKEY_INPUT);

      await page.click(SYNC_PULL);
      // 拉取期间必须能看到进行中的状态。
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SYNC_PROGRESS, expected: "拉取中" },
      );
      const progress = await text(SYNC_PROGRESS);
      const progressShot = await shot(page, "41-settings-sync-pulling.png");

      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "已快进拉取到" },
      );
      const message = await text(SETTINGS_MESSAGE);
      assert(message.includes("生效主题：深色"), `主题必须随拉取重新加载：${message}`);
      assert(message.includes("备忘录 2 篇"), `备忘录必须随拉取重新读取：${message}`);

      const after = await page.inputValue(HOTKEY_INPUT);
      assert(
        after === "Alt+Space" && before !== after,
        `设置必须随拉取重新加载：${before} → ${after}`,
      );
      const counts = await text(SYNC_COUNTS);
      assert(counts.includes("领先 0") && counts.includes("落后 0"), `拉取后应同步：${counts}`);
      const file = await shot(page, "42-settings-sync-pulled.png");
      return `进度「${progress}」，生效快捷键 ${before} → ${after}，${message}，截图 ${progressShot} / ${file}`;
    });

    // 23. 推送成功，以及「没有需要推送的提交」是独立反馈。
    await check("推送成功且无需推送时给出独立说明", async () => {
      await page.evaluate(() => window.__flashcastMock.simulateLocalCommit());
      await page.click(SYNC_REDETECT);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SYNC_COUNTS, expected: "领先 1" },
      );
      await page.click(SYNC_PUSH);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "已推送到" },
      );
      const pushed = await text(SETTINGS_MESSAGE);
      assert(
        pushed.includes("refs/heads/main → refs/heads/main"),
        `推送结果必须点明引用：${pushed}`,
      );
      const file = await shot(page, "43-settings-sync-pushed.png");

      await page.click(SYNC_PUSH);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SETTINGS_MESSAGE, expected: "没有需要推送的提交" },
      );
      const nothing = await text(SETTINGS_MESSAGE);
      assert(nothing !== pushed, "无需推送必须与推送成功区分");
      return `推送「${pushed}」；再次推送「${nothing}」，截图 ${file}`;
    });

    // 24. 进行中的 Git 操作可见；外部处理后重新检测恢复同步。
    await check("重新检测在外部处理后恢复同步", async () => {
      await setSyncScenario("inProgress", "Git 操作进行中");
      const state = await text(SYNC_STATE);
      assert(state.includes("变基"), `进行中的操作必须可见：${state}`);
      const hint = await text(SYNC_BLOCK_HINT);
      assert(hint.includes("rebase --abort"), `中止指引不完整：${hint}`);
      assert(await page.locator(SYNC_PULL).isDisabled(), "进行中的操作必须阻塞拉取");
      const blockedShot = await shot(page, "44-settings-sync-in-progress.png");

      // 外部处理完毕 → 重新检测 → 阻塞消失。
      await page.evaluate(() => window.__flashcastMock.simulateSyncScenario("ready"));
      await page.click(SYNC_REDETECT);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SYNC_BLOCK, expected: "没有阻塞" },
      );
      const recovered = await text(SYNC_CAPABILITY);
      assert(
        recovered.includes("拉取可用") && recovered.includes("推送可用"),
        `重新检测后必须恢复同步能力：${recovered}`,
      );
      const file = await shot(page, "45-settings-sync-redetected.png");
      return `阻塞时「${state}」，重新检测后「${recovered}」，截图 ${blockedShot} / ${file}`;
    });

    // 25. 备忘录：关键词别名进入范围，列表与完整预览可用（ticket 07）。
    await check("备忘录范围按别名进入并给出完整预览", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);

      await page.fill(INPUT, "备忘录");
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SCOPE_LABEL, expected: "备忘录 范围" },
      );
      const inScope = await rows(page);
      assert(inScope.length >= 2, `范围内应列出备忘录，实际 ${inScope.length} 条`);
      assert(
        inScope.every((row) => row.kind === "memo"),
        `范围内的结果必须都是备忘录条目：${inScope.map((row) => row.kind).join("/")}`,
      );
      assert(
        inScope.some((row) => row.title === "常用回复"),
        `范围内缺少预期备忘录：${inScope.map((row) => row.title).join("/")}`,
      );
      const action = await text(DEFAULT_ACTION_LABEL);
      assert(action === "粘贴", `备忘录的默认操作必须是粘贴，实际「${action}」`);

      // 完整预览：正文必须是完整内容，而不是列表里的摘要。
      await page.waitForSelector(MEMO_PREVIEW);
      const body = await text(MEMO_PREVIEW_BODY);
      assert(body === "收到，我看一下再回复你。", `预览必须是完整正文，实际「${body}」`);
      const scopeShot = await shot(page, "46-memo-scope.png");

      // 英文别名同样进入范围。
      await page.fill(INPUT, "memo");
      const memoScope = await text(SCOPE_LABEL);
      assert(memoScope.includes("memo 范围"), `别名 memo 必须进入范围：${memoScope}`);
      await page.fill(INPUT, "memos");
      const memosScope = await text(SCOPE_LABEL);
      assert(memosScope.includes("memos 范围"), `别名 memos 必须进入范围：${memosScope}`);

      // 范围内按标题 / 标签检索（在同一个输入框里继续输入）。
      await page.fill(INPUT, "memos 会议");
      await page.waitForFunction(
        (selector) => document.querySelectorAll(selector).length === 1,
        ROW,
      );
      const filtered = await rows(page);
      assert(
        filtered[0].title === "会议邀请",
        `范围内检索结果不对：${filtered.map((row) => row.title).join("/")}`,
      );

      // 预览可以按需收起 / 展开，列表与操作栏保持不变。
      await page.click(MEMO_PREVIEW_TOGGLE);
      assert(!(await page.isVisible(MEMO_PREVIEW_BODY)), "收起后不应显示正文");
      await page.click(MEMO_PREVIEW_TOGGLE);
      assert(await page.isVisible(MEMO_PREVIEW_BODY), "再次展开必须恢复正文");
      return `范围「备忘录 / memo / memos」均可用，${inScope.length} 条，正文完整，截图 ${scopeShot}`;
    });

    // 26. 备忘录：首屏完整标签命中；回车复制并给出准确反馈。
    await check("首屏按完整标签命中备忘录，回车复制并提示手动粘贴", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);

      await page.fill(INPUT, "工作");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some((row) => row.dataset.kind === "memo"),
        ROW,
      );
      const homeScope = await text(SCOPE_LABEL);
      assert(homeScope === "首屏", `标签命中必须留在首屏，实际「${homeScope}」`);
      const tagged = await rows(page);
      const memoRows = tagged.filter((row) => row.kind === "memo");
      assert(memoRows.length === 2, `完整标签应命中全部两条，实际 ${memoRows.length} 条`);
      const homeShot = await shot(page, "47-memo-home-tag.png");

      // 进入范围并回车：状态必须是「已复制，需手动粘贴」。
      await page.fill(INPUT, "备忘录");
      await page.waitForSelector(MEMO_PREVIEW);
      const first = await selection(page);
      const copiable = (await rows(page)).find((row) => row.title === "常用回复");
      assert(copiable, "范围内必须有「常用回复」");
      assert(first.id === copiable.id, `第一条应为选中的备忘录，实际 ${first.id}`);
      await page.keyboard.press("Enter");
      await page.waitForSelector(NOTICE);
      const notice = await text(NOTICE);
      assert(
        notice.includes("已复制") && notice.includes("手动粘贴"),
        `复制反馈必须说明已复制且需手动粘贴：${notice}`,
      );
      const copied = await page.evaluate(() => window.__flashcastMock.lastCopied);
      assert(copied === "收到，我看一下再回复你。", `剪贴板内容必须是正文，实际「${copied}」`);
      const copyShot = await shot(page, "48-memo-copied.png");
      return `标签命中 ${memoRows.length} 条，反馈「${notice.slice(0, 24)}…」，截图 ${homeShot} / ${copyShot}`;
    });

    // 27. 备忘录：返回首屏恢复此前的查询、选择与范围。
    await check("从备忘录范围返回首屏恢复查询、选择与范围", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.click(INPUT);
      await page.keyboard.press("ArrowDown");
      await page.waitForFunction(
        (selector) => document.querySelectorAll(selector)[1]?.dataset.selected === "true",
        ROW,
      );
      await page.keyboard.press("ArrowDown");
      await page.waitForFunction(
        (selector) => document.querySelectorAll(selector)[2]?.dataset.selected === "true",
        ROW,
      );
      const before = await selection(page);
      assert(before && before.index === 2, `应先选中第 3 项：${JSON.stringify(before)}`);

      await page.fill(INPUT, "备忘录");
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SCOPE_LABEL, expected: "备忘录 范围" },
      );
      const inScope = await selection(page);
      assert(inScope.index === 0, `进入范围后选择必须归零：${JSON.stringify(inScope)}`);

      await page.keyboard.press("Escape");
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.trim() === expected,
        { sel: SCOPE_LABEL, expected: "首屏" },
      );
      const after = await selection(page);
      assert(
        after && after.id === before.id && after.index === 2,
        `返回必须恢复此前的选择：${JSON.stringify(after)} 应为 ${JSON.stringify(before)}`,
      );
      const restoredInput = await page.inputValue(INPUT);
      assert(restoredInput === "", `返回必须恢复此前的查询，实际「${restoredInput}」`);

      // 已在最外层：再按一次 Escape 关闭窗口。
      await page.keyboard.press("Escape");
      await page.waitForFunction(
        () =>
          document.querySelector('[data-testid="app-root"]')?.dataset.windowVisible === "false",
      );
      return `选择恢复为第 ${after.index} 项（${after.id}），查询恢复为空，再次 Escape 关闭窗口`;
    });

    // 28. 设置页里管理备忘录：创建、编辑、删除。
    await check("设置页创建、编辑并删除备忘录", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await linkMockWorkspace();
      await page.locator(MEMO_SECTION).scrollIntoViewIfNeeded();
      await page.waitForSelector(MEMO_ITEM);
      const seed = await page.$$eval(MEMO_ITEM, (nodes) => nodes.length);
      assert(seed === 2, `工作区里应有两条初始备忘录，实际 ${seed}`);

      // 创建。
      await page.fill(MEMO_TITLE_INPUT, "报平安");
      await page.fill(MEMO_TAGS_INPUT, "家人、日常");
      await page.fill(MEMO_BODY_INPUT, "我到家了，一切都好。");
      await page.click(MEMO_SAVE);
      await page.waitForFunction(
        ({ sel, expected }) =>
          [...document.querySelectorAll(sel)].some((node) => node.textContent?.includes(expected)),
        { sel: MEMO_ITEM, expected: "报平安" },
      );
      const created = await page.$$eval(MEMO_ITEM, (nodes) =>
        nodes.map((node) => ({
          id: node.dataset.memoId,
          title: node.querySelector('[data-testid="memo-item-title"]')?.textContent ?? "",
          tags: node.querySelector('[data-testid="memo-item-tags"]')?.textContent ?? "",
          body: node.querySelector('[data-testid="memo-item-body"]')?.textContent ?? "",
        })),
      );
      const fresh = created.find((memo) => memo.title === "报平安");
      assert(fresh, "新建的备忘录必须出现在列表里");
      assert(
        fresh.tags.includes("家人") && fresh.tags.includes("日常"),
        `多个标签必须保存：${fresh.tags}`,
      );
      assert(fresh.body.includes("我到家了"), `正文必须保存：${fresh.body}`);
      assert(/^memo-\d+$/.test(fresh.id), `标识必须稳定可追踪：${fresh.id}`);
      const createShot = await shot(page, "49-settings-memo-created.png");

      // 编辑：标识不变，内容更新。
      await page.click(`${MEMO_ITEM}[data-memo-id="${fresh.id}"] [data-testid="memo-edit"]`);
      await page.fill(MEMO_TITLE_INPUT, "报平安（改）");
      await page.fill(MEMO_BODY_INPUT, "路上堵车，刚到家。");
      await page.click(MEMO_SAVE);
      await page.waitForFunction(
        ({ sel, expected }) =>
          [...document.querySelectorAll(sel)].some((node) => node.textContent?.includes(expected)),
        { sel: MEMO_ITEM, expected: "报平安（改）" },
      );
      const edited = await page.$$eval(MEMO_ITEM, (nodes) =>
        nodes.map((node) => ({
          id: node.dataset.memoId,
          body: node.querySelector('[data-testid="memo-item-body"]')?.textContent ?? "",
        })),
      );
      const stillThere = edited.find((memo) => memo.id === fresh.id);
      assert(stillThere, "编辑后标识必须保持不变");
      assert(stillThere.body.includes("路上堵车"), `编辑后正文必须更新：${stillThere.body}`);
      const editShot = await shot(page, "50-settings-memo-edited.png");

      // 删除。
      await page.click(`${MEMO_ITEM}[data-memo-id="${fresh.id}"] [data-testid="memo-delete"]`);
      await page.waitForFunction(
        ({ sel, id }) =>
          ![...document.querySelectorAll(sel)].some((node) => node.dataset.memoId === id),
        { sel: MEMO_ITEM, id: fresh.id },
      );
      return `创建 ${fresh.id}（标签「${fresh.tags.replace("标签：", "")}」）→ 编辑 → 删除，截图 ${createShot} / ${editShot}`;
    });

    // 29. 停用备忘录插件：不再贡献结果、管理入口拒绝写入、可再启用。
    await check("停用备忘录插件后不再贡献结果与管理入口", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await linkMockWorkspace();
      await page.locator(PLUGIN_SECTION).scrollIntoViewIfNeeded();
      await page.waitForSelector(PLUGIN_ITEM);
      const state = await text(PLUGIN_STATE);
      assert(state === "已启用", `默认应启用功能插件，实际「${state}」`);

      await page.click(PLUGIN_TOGGLE);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: PLUGIN_STATE, expected: "已停用" },
      );
      const disabledShot = await shot(page, "51-settings-plugin-disabled.png");

      // 管理入口如实说明并拒绝写入。
      await page.locator(MEMO_SECTION).scrollIntoViewIfNeeded();
      assert(await page.isVisible(MEMO_DISABLED), "停用后必须说明备忘录不可用");
      assert(await page.locator(MEMO_SAVE).isDisabled(), "停用后不得创建备忘录");

      // 搜索页：标签不再命中备忘录，关键词也不再进入范围。
      await page.click('[data-testid="settings-back"]');
      await page.fill(INPUT, "工作");
      await page.waitForFunction(
        (selector) => document.querySelectorAll(selector).length === 0,
        ROW,
      );
      assert(await page.isVisible(EMPTY_STATE), "停用后标签不应命中任何结果");
      await page.fill(INPUT, "备忘录");
      const disabledScope = await text(SCOPE_LABEL);
      assert(disabledScope === "首屏", `停用后不得进入范围，实际「${disabledScope}」`);
      const searchShot = await shot(page, "52-memo-disabled-search.png");

      // 重新启用后恢复（状态记录在清单里，可以再切换回来）。
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await page.locator(PLUGIN_SECTION).scrollIntoViewIfNeeded();
      await page.click(PLUGIN_TOGGLE);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: PLUGIN_STATE, expected: "已启用" },
      );
      await page.click('[data-testid="settings-back"]');
      await page.fill(INPUT, "工作");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some((row) => row.dataset.kind === "memo"),
        ROW,
      );
      const recovered = (await rows(page)).filter((row) => row.kind === "memo").length;
      return `停用后标签与关键词都不再命中、管理入口拒绝写入；重新启用后命中 ${recovered} 条，截图 ${disabledShot} / ${searchShot}`;
    });

    // 30. 自动粘贴：粘贴的是刚执行的那条内容，且没有目标时如实降级为手动粘贴。
    await check("自动粘贴可用时粘贴刚才执行的那条；没有目标时提示手动粘贴", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      // 浏览器模拟宿主默认不能自动粘贴；这里打开它来验证 UI 的粘贴路径。
      await page.evaluate(() => window.__flashcastMock.setAutoPasteSupported(true));

      await page.fill(INPUT, "工作");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some((row) => row.dataset.kind === "memo"),
        ROW,
      );
      /** 用方向键把选择移到标题为 `title` 的那一行（鼠标移动不算数）。 */
      const selectRow = async (title) => {
        const listed = await rows(page);
        const index = listed.findIndex((row) => row.title === title);
        assert(index >= 0, `列表里必须有「${title}」：${listed.map((r) => r.title).join("/")}`);
        let current = (await selection(page)).index;
        while (current !== index) {
          const step = index > current ? 1 : -1;
          await page.keyboard.press(step > 0 ? "ArrowDown" : "ArrowUp");
          current += step;
        }
        await page.waitForFunction(
          ({ selector, index }) =>
            document.querySelectorAll(selector)[index]?.dataset.selected === "true",
          { selector: ROW, index },
        );
      };

      // 执行第一条：外壳顺序必须是 复制 → 关窗 → 恢复目标 → 注入粘贴。
      await selectRow("常用回复");
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__flashcastMock.lastPaste !== null);
      const first = await page.evaluate(() => window.__flashcastMock.lastPaste);
      assert(
        first.content === "收到，我看一下再回复你。",
        `粘贴的必须是刚执行的正文，实际「${first.content}」`,
      );
      assert(
        first.sequence.join(" → ") === "copied → windowHidden → restored → pasted",
        `外壳顺序不对：${first.sequence.join(" → ")}`,
      );
      assert(
        first.target === "Visual Studio Code",
        `粘贴目标必须是唤起前的应用，实际「${first.target}」`,
      );
      const firstShot = await shot(page, "53-memo-pasted.png");

      // 切换到另一条再执行：粘贴的必须变成新的那条，不能是上一次的选择。
      await page.evaluate(() => {
        window.__flashcastMock.lastPaste = null;
      });
      await selectRow("会议邀请");
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__flashcastMock.lastPaste !== null);
      const second = await page.evaluate(() => window.__flashcastMock.lastPaste);
      assert(
        second.content === "下午三点在三楼会议室，麻烦确认一下时间。",
        `切换结果后粘贴的必须是新选中那条，实际「${second.content}」`,
      );
      const secondShot = await shot(page, "54-memo-pasted-switched.png");

      // 没有记录到唤起前的应用：内容照样进剪贴板，界面必须说清要手动粘贴。
      await page.evaluate(() => {
        window.__flashcastMock.setPreviousApp(null);
        window.__flashcastMock.lastCopied = null;
        window.__flashcastMock.lastPaste = null;
      });
      await page.keyboard.press("Enter");
      await page.waitForSelector(NOTICE);
      const notice = await text(NOTICE);
      assert(notice.includes("已复制"), `降级反馈必须先说清已复制：${notice}`);
      assert(
        notice.includes("没有记录到唤起前的应用") && notice.includes("手动粘贴"),
        `降级反馈必须说清原因与下一步：${notice}`,
      );
      const fallback = await page.evaluate(() => ({
        copied: window.__flashcastMock.lastCopied,
        pasted: window.__flashcastMock.lastPaste,
      }));
      assert(
        fallback.copied === "下午三点在三楼会议室，麻烦确认一下时间。" && fallback.pasted === null,
        `降级时仍必须复制内容且不注入按键：${JSON.stringify(fallback)}`,
      );
      const fallbackShot = await shot(page, "55-memo-paste-fallback.png");
      return `粘贴「${first.content.slice(0, 6)}…」→ 切换后粘贴「${second.content.slice(0, 6)}…」→ 无目标时降级提示「${notice.slice(0, 18)}…」，截图 ${firstShot} / ${secondShot} / ${fallbackShot}`;
    });

    // 31. 关键词与标签冲突：插件入口与备忘录候选同时保留，入口仍然可用。
    await check("关键词与标签冲突时同时保留插件入口与备忘录候选", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await linkMockWorkspace();
      // 建一条标签与插件关键词（memo）完全相同的备忘录。
      await page.locator(MEMO_SECTION).scrollIntoViewIfNeeded();
      await page.waitForSelector(MEMO_ITEM);
      await page.fill(MEMO_TITLE_INPUT, "关键词冲突样例");
      await page.fill(MEMO_TAGS_INPUT, "memo");
      await page.fill(MEMO_BODY_INPUT, "标签与关键词同名时的正文。");
      await page.click(MEMO_SAVE);
      await page.waitForFunction(
        ({ sel, expected }) =>
          [...document.querySelectorAll(sel)].some((node) => node.textContent?.includes(expected)),
        { sel: MEMO_ITEM, expected: "关键词冲突样例" },
      );
      await page.click('[data-testid="settings-back"]');

      await page.fill(INPUT, "memo");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some((row) => row.dataset.kind === "memo"),
        ROW,
      );
      const scope = await text(SCOPE_LABEL);
      assert(scope === "首屏", `冲突时必须留在首屏，实际「${scope}」`);
      const listed = await rows(page);
      const entry = listed.find((row) => row.kind === "command" && row.title === "备忘录");
      assert(entry, `冲突时必须给出插件入口：${listed.map((row) => row.title).join("/")}`);
      assert(
        listed[0].id === entry.id,
        `插件入口必须排在最前，实际第一条是「${listed[0].title}」`,
      );
      const collided = listed.find((row) => row.title === "关键词冲突样例");
      assert(collided, `标签命中的备忘录不得消失：${listed.map((row) => row.title).join("/")}`);
      const collisionShot = await shot(page, "56-memo-keyword-tag-collision.png");

      // 插件入口就在第一条（输入后选择归零）：直接回车必须真的进入范围，
      // 而不是又留在首屏。
      const atEntry = await selection(page);
      assert(atEntry && atEntry.id === entry.id, `选择应停在插件入口：${JSON.stringify(atEntry)}`);
      await page.keyboard.press("Enter");
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SCOPE_LABEL, expected: "memo 范围" },
      );
      const inScope = await rows(page);
      assert(
        inScope.every((row) => row.kind === "memo"),
        `进入范围后只应有备忘录：${inScope.map((row) => row.kind).join("/")}`,
      );
      const scopeShot = await shot(page, "57-memo-collision-entered-scope.png");
      return `首屏同时给出入口「${entry.title}」与备忘录「${collided.title}」，回车后进入范围 ${inScope.length} 条，截图 ${collisionShot} / ${scopeShot}`;
    });

    // 32. 中文组合输入：回车确认候选词时不得粘贴。
    await check("输入法组合期间回车不粘贴备忘录", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() => window.__flashcastMock.setAutoPasteSupported(true));
      const cdp = await page.context().newCDPSession(page);
      await page.click(INPUT);
      await cdp.send("Input.imeSetComposition", {
        text: "工作",
        selectionStart: 2,
        selectionEnd: 2,
      });
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some((row) => row.dataset.kind === "memo"),
        ROW,
      );
      await page.evaluate(() => {
        window.__flashcastMock.lastCopied = null;
        window.__flashcastMock.lastPaste = null;
      });
      await page.keyboard.press("Enter");
      const composing = await page.evaluate(() => ({
        copied: window.__flashcastMock.lastCopied,
        pasted: window.__flashcastMock.lastPaste,
      }));
      assert(
        composing.copied === null && composing.pasted === null,
        `组合期间回车不得复制或粘贴：${JSON.stringify(composing)}`,
      );
      const file = await shot(page, "58-memo-ime-no-paste.png");

      // 正向对照：组合结束后回车必须真的执行。
      await cdp.send("Input.insertText", { text: "工作" });
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__flashcastMock.lastCopied !== null);
      const copied = await page.evaluate(() => window.__flashcastMock.lastCopied);
      return `组合中回车未复制（lastCopied=null）→ 组合结束后回车复制「${copied.slice(0, 8)}…」，截图 ${file}`;
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
