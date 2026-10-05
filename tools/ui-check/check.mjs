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
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright-core";
import { releaseChecks } from "./release-checks.mjs";

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
// 运行环境与能力报告（ticket 17）。
const CAPABILITY_SECTION = '[data-testid="capability-section"]';
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
// Chrome 书签（ticket 13）。
const CHROME_SECTION = '[data-testid="chrome-section"]';
const CHROME_AVAILABILITY = '[data-testid="chrome-availability"]';
const CHROME_USER_DATA_DIR = '[data-testid="chrome-user-data-dir"]';
const CHROME_ASSOCIATION = '[data-testid="chrome-association"]';
const CHROME_BOOKMARKS_STATUS = '[data-testid="chrome-bookmarks-status"]';
const CHROME_BOOKMARKS_PATH = '[data-testid="chrome-bookmarks-path"]';
const CHROME_REFRESH = '[data-testid="chrome-refresh"]';
const CHROME_ERROR = '[data-testid="chrome-error"]';
const CHROME_PROFILE = '[data-testid="chrome-profile"]';
const CHROME_ASSOCIATE = '[data-testid="chrome-associate"]';
const CHROME_ASSOCIATED_BADGE = '[data-testid="chrome-associated-badge"]';
const BOOKMARK_ICON = '[data-testid="bookmark-icon"]';
const PLUGIN_FAILURE = '[data-testid="plugin-failure"]';

const THEME_ARC = "flashcast.theme.arc";
const INSTALLED_THEME = "example.solarized";

// 浏览器模拟宿主认得的主题包路径（见 src/api.ts / src/mockThemes.ts）。
const MOCK_THEME_PACKAGE = "/home/user/themes/solarized";
const MOCK_BROKEN_THEME_PACKAGE = "/home/user/themes/broken";

// 内置主题在浏览器模拟宿主里的表面色（见 src/mockThemes.ts）。
const SURFACE_LIGHT = "#e7eef5";
const SURFACE_DARK = "#222c39";
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
// Chrome 书签（见 src/api.ts 的 MOCK_BOOKMARKS）。
const MOCK_CHROME_KEYWORD_EN = "chrome bookmarks";
const MOCK_CHROME_KEYWORD_ZH = "chrome 书签";
const MOCK_CHROME_DEFAULT_PROFILE = "Profile 1";

const lines = [];
function log(line) {
  lines.push(line);
  if (!line.startsWith("· 开始：")) console.log(line.startsWith("PASS ") ? line.split(" — ")[0] : line);
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
  const serverLines = [];
  for (const stream of [child.stdout, child.stderr]) stream.on("data", data => {
    serverLines.push(data.toString());
    writeFileSync(resolve(ARTIFACTS, "vite.log"), serverLines.join(""));
  });
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

async function shot(page, name, force = false) {
  // 默认只留代表画面与失败证据；视觉精查可选择全部截图。
  if (!force && process.env.FLASHCAST_UI_SCREENSHOTS !== "all" &&
      !["01-home-empty-query.png", "14-theme-light.png", "16-theme-dark.png", "27-scaling-200.png"].includes(name)) return "未截图";
  const path = resolve(ARTIFACTS, name);
  // 唤起动画结束后再取色，避免把半透明过渡帧当成稳定外观。
  await page.evaluate(async () => {
    const finite = document.getAnimations().filter(animation => animation.effect?.getComputedTiming().iterations !== Infinity);
    await Promise.all(finite.map(animation => animation.finished.catch(() => {})));
  });
  await page.screenshot({ path });
  return name;
}

/**
 * 打开设置页的某个区块。
 *
 * 设置页从「10 个区块平铺在一个滚动列里」改成「左侧分组目录 + 右侧内容区」之后，同一
 * 时刻只有当前区块可见（其余是 `hidden`，见 `styles.css` 的 `.settings-body > [hidden]`）。
 * 所以每个用例都必须先导航到自己要操作的区块，不能再靠「滚下去就能看到」。
 *
 * 区块始终留在 DOM 里，所以进入设置页之后只需点一下目录项。
 */
async function openSection(page, id) {
  await page.click(`[data-testid="settings-nav-${id}"]`);
  await page.waitForFunction(
    (sectionId) => {
      const node = document.querySelector(`[data-section="${sectionId}"]`);
      return node !== null && node.offsetParent !== null;
    },
    id,
  );
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
    '[data-testid="search-input"]',
    '[data-testid="result-item"]',
    '[data-testid="action-bar"]',
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

/**
 * 富文本剪贴板条目在结果列表里的下标（副标题同时标注文字与 HTML）。
 *
 * 鼠标点击结果会**执行**条目，因此选中必须用键盘移动，这个下标就是按键次数。
 */
async function richClipboardIndex(page) {
  return page.$$eval(ROW, (nodes) =>
    nodes.findIndex((node) =>
      (node.querySelector(".result-subtitle")?.textContent ?? "").includes("HTML"),
    ),
  );
}

/** 等到结果列表里出现图片（副标题标注「图片」）条目：条数由样例决定，不写死。 */
async function waitForImageClipboardRow(page) {
  await page.waitForFunction(
    (selector) =>
      [...document.querySelectorAll(selector)].some((row) =>
        (row.querySelector(".result-subtitle")?.textContent ?? "").includes("图片"),
      ),
    ROW,
  );
}

/**
 * 图片剪贴板条目在结果列表里的下标（副标题标注「图片」格式）。
 *
 * 与富文本同理：鼠标点击会**执行**条目，选中必须用键盘移动。
 */
async function imageClipboardIndex(page) {
  return page.$$eval(ROW, (nodes) =>
    nodes.findIndex((node) =>
      (node.querySelector(".result-subtitle")?.textContent ?? "").includes("图片"),
    ),
  );
}

/** 等到结果列表里出现富文本（HTML）条目：条数由样例决定，不写死。 */
async function waitForRichClipboardRow(page) {
  await page.waitForFunction(
    (selector) =>
      [...document.querySelectorAll(selector)].some((row) =>
        (row.querySelector(".result-subtitle")?.textContent ?? "").includes("HTML"),
      ),
    ROW,
  );
}

/**
 * 把键盘选择移到指定下标的条目上。
 *
 * 每次按键后必须等选择真的移动：`move_selection` 是异步命令，连按会读到同一个
 * 起始下标而只前进一格。目标在当前选择上方时按 ArrowUp（下标 0 表示不动）。
 */
async function selectRow(page, index) {
  const currentIndex = () =>
    page.evaluate(
      (selector) =>
        [...document.querySelectorAll(selector)].findIndex(
          (row) => row.dataset.selected === "true",
        ),
      ROW,
    );
  await page.click(INPUT);
  let at = await currentIndex();
  while (at !== index) {
    const down = at < index;
    const expected = down ? at + 1 : at - 1;
    await page.keyboard.press(down ? "ArrowDown" : "ArrowUp");
    await page.waitForFunction(
      ({ selector, expected }) =>
        [...document.querySelectorAll(selector)].findIndex(
          (row) => row.dataset.selected === "true",
        ) === expected,
      { selector: ROW, expected },
    );
    at = expected;
  }
}

/**
 * 结果列表里标题包含 `needle` 的条目下标（键盘选中要按这个次数移动）。
 *
 * 与 [`richClipboardIndex`] 同理：鼠标点击会执行条目，选中只能走键盘。
 */
async function clipboardRowIndex(page, needle) {
  return page.$$eval(
    ROW,
    (nodes, needle) =>
      nodes.findIndex((node) => (node.textContent ?? "").includes(needle)),
    needle,
  );
}

async function main() {
  rmSync(ARTIFACTS, { recursive: true, force: true });
  mkdirSync(ARTIFACTS, { recursive: true });
  const url = process.env.FLASHCAST_UI_URL || DEFAULT_URL;
  const teardown = await ensureServer(url);

  let browser;
  const results = [];

  try {
    browser = await chromium.launch({
      executablePath: CHROME_PATH,
      headless: true,
      args: ["--no-sandbox"],
    });
    const page = await browser.newPage({ viewport: { width: 900, height: 620 } });
    const pageErrors = [];
    page.on("pageerror", error => { pageErrors.push(error.message); log(`  ! 页面异常：${error.message}`); });
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
        assert(pageErrors.length === 0, `页面异常：${pageErrors.join("；")}`);
        results.push({ name, ok: true, detail });
        log(`PASS ${name}${detail ? ` — ${detail}` : ""}`);
      } catch (error) {
        results.push({ name, ok: false, detail: error.message });
        log(`FAIL ${name} — ${error.message}`);
        await shot(page, "failure.png", true).catch(() => {});
        throw error;
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

    /** 进入设置页并关联浏览器模拟宿主的工作区（每次 goto 后模拟宿主会重置）。
     *  结束时停在「配置工作区」区块，调用方若要操作别的区块需自行 `openSection`。 */
    const linkMockWorkspace = async () => {
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "workspace");
      await page.fill(WORKSPACE_PATH_INPUT, MOCK_REPO);
      await page.click('[data-testid="workspace-select"]');
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: WORKSPACE_VALIDITY, expected: "已关联 Git 仓库" },
      );
      // 变更行现在属于「变更与提交」区块（隐藏时仍在 DOM 里），所以只等它出现、不等它可见。
      await page.waitForSelector(CHANGE_ROW, { state: "attached" });
    };

    await releaseChecks({ page, check, url, linkMockWorkspace, openSection, assert });
    await page.goto(url, { waitUntil: "load" });
    await page.waitForSelector(ROW);

    // 1. 空查询显示快速访问项。
    await check("空查询显示快速访问项", async () => {
      const items = await rows(page);
      assert(items.length > 0, "首屏列表为空");
      const commands = items.filter((item) => item.kind === "command").length;
      const applications = items.filter((item) => item.kind === "application").length;
      assert(items[0].kind === "application", "首屏应优先显示软件");
      assert(applications > 0, "首屏没有快速访问的软件项");
      assert(
        items.some((item) => item.title.includes("Firefox")),
        `首屏缺少预期软件：${items.map((i) => i.title).join("/")}`,
      );
      const file = await shot(page, "01-home-empty-query.png");
      return `${items.length} 项（${commands} 命令 + ${applications} 软件），截图 ${file}`;
    });

    // 1a. 唤起后输入框自动获得焦点（用户故事 6「窗口打开后立即输入查询」）。
    //     这条曾经是坏的：`inputRef` 没有绑到输入框上，`focusInput()` 一直在对 null 调
    //     focus()，因此必须先点一下搜索框才能打字。旧用例都先 page.click(INPUT)，
    //     所以一直没被发现——这里刻意**不点**输入框，直接打字。
    await check("唤起后输入框已聚焦，可直接输入", async () => {
      const focused = await page.evaluate(
        () => document.activeElement?.getAttribute("data-testid") ?? null,
      );
      assert(focused === "search-input", `唤起后焦点不在搜索框：${focused}`);
      const before = (await rows(page)).length;
      await page.keyboard.type("终端");
      await waitForRowCount(page, 1);
      const items = await rows(page);
      assert(items[0].title.includes("终端"), `直接输入没有过滤：${items[0]?.title}`);
      // 复原首屏，避免影响后续用例。
      await page.keyboard.press("Control+A");
      await page.keyboard.press("Backspace");
      await waitForRowCount(page, before);
      return `activeElement=search-input，未点击即可输入并过滤，已复原 ${before} 项首屏`;
    });

    // 1b. 首屏命令项：宿主对命令返回的是**带反馈的** done，所以执行后必须给出反馈并且
    //     不隐藏窗口。这条路径曾经是漏的：模拟宿主落到兜底 done + 无 message，被 UI 读成
    //     「启动成功、外壳关窗」，在浏览器里点一下整页就变白。
    await check("执行首屏命令给出反馈且不隐藏窗口", async () => {
      await page.click('[data-item-id="flashcast.command.rescan"]');
      await page.waitForSelector(NOTICE);
      const notice = (await page.textContent(NOTICE))?.trim() ?? "";
      assert(notice.includes("已重新扫描"), `命令反馈文案不对：${notice}`);
      const visible = await page.getAttribute(
        '[data-testid="app-root"]',
        "data-window-visible",
      );
      assert(visible === "true", `执行命令后窗口被隐藏了：${visible}`);
      const file = await shot(page, "01b-command-feedback.png");

      // 反馈还在时 Escape 只清反馈，不能把窗口关掉（Escape 的优先级）。
      await page.keyboard.press("Escape");
      await page.waitForFunction(
        (selector) => document.querySelector(selector) === null,
        NOTICE,
      );
      const afterEscape = await page.getAttribute(
        '[data-testid="app-root"]',
        "data-window-visible",
      );
      assert(afterEscape === "true", `清反馈时窗口被关闭了：${afterEscape}`);
      await page.fill(INPUT, "");
      await page.waitForSelector(ROW);
      return `反馈「${notice}」且窗口保持可见；Escape 只清反馈，截图 ${file}`;
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
      await openSection(page, "workspace");

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
      await openSection(page, "hotkey");
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
      await openSection(page, "workspace");
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
    //
    // 设置页改成侧栏之后，「滚下去就能看到下一个区块」不再成立，所以这个用例改成逐个
    // 导航到区块再断言控件可见、且不超出真实窗口宽度；顺带把新外壳的主要收益
    // ——「10 个区块在侧栏里一屏内全部可见」——钉住。
    await check("设置页在 640×420 窗口内可操作", async () => {
      await page.setViewportSize({ width: 640, height: 420 });
      // 上一个用例结束时通常已经在设置页里（此时搜索行是隐藏的），所以按需进入。
      if (await page.isVisible(SETTINGS_BUTTON)) {
        await page.click(SETTINGS_BUTTON);
      }
      await page.waitForSelector(SETTINGS_SCREEN);

      // 工作区：路径输入框、初始化与克隆入口。
      await openSection(page, "workspace");
      assert(await page.isVisible(WORKSPACE_PATH_INPUT), "路径输入框不可见");
      await page.locator(CLONE_URL_INPUT).scrollIntoViewIfNeeded();
      assert(await page.isVisible(CLONE_URL_INPUT), "克隆远端地址输入框不可见");
      assert(await page.isVisible(CLONE_BUTTON), "克隆按钮不可见");
      const cloneBox = await page.locator(CLONE_BUTTON).boundingBox();
      assert(
        cloneBox && cloneBox.x >= 0 && cloneBox.x + cloneBox.width <= 640,
        `克隆按钮超出窗口宽度：${JSON.stringify(cloneBox)}`,
      );

      // 快捷键：输入框与保存按钮。
      await openSection(page, "hotkey");
      assert(await page.isVisible(HOTKEY_INPUT), "快捷键输入框不可见");
      assert(await page.isVisible('[data-testid="hotkey-save"]'), "保存按钮不可见");
      const box = await page.locator('[data-testid="hotkey-save"]').boundingBox();
      assert(
        box && box.x >= 0 && box.x + box.width <= 640,
        `保存按钮超出窗口宽度：${JSON.stringify(box)}`,
      );

      // 变更与提交：变更列表与提交入口。
      await openSection(page, "changes");
      assert(await page.isVisible('[data-testid="changes-section"]'), "变更区段不可见");
      assert(await page.isVisible(CHANGE_ROW), "变更列表不可见");
      await page.locator(COMMIT_MESSAGE).scrollIntoViewIfNeeded();
      assert(await page.isVisible(COMMIT_MESSAGE), "提交说明输入框不可见");
      const commitButton = await page.locator(COMMIT_SUBMIT).boundingBox();
      assert(
        commitButton && commitButton.x >= 0 && commitButton.x + commitButton.width <= 640,
        `创建提交按钮超出窗口宽度：${JSON.stringify(commitButton)}`,
      );

      // 同步：拉取与重新检测。
      await openSection(page, "sync");
      assert(await page.isVisible(SYNC_SECTION), "同步区段不可见");
      assert(await page.isVisible(SYNC_PULL), "拉取按钮不可见");
      assert(await page.isVisible(SYNC_REDETECT), "重新检测按钮不可见");
      const pullBox = await page.locator(SYNC_PULL).boundingBox();
      assert(
        pullBox && pullBox.x >= 0 && pullBox.x + pullBox.width <= 640,
        `拉取按钮超出窗口宽度：${JSON.stringify(pullBox)}`,
      );

      // 侧栏：10 个区块在真实窗口里必须一屏内全部可见、不被裁切。
      // 这是「10 个区块平铺 = 10.57 屏」那个问题的直接回归断言。
      const nav = await page.evaluate(() => {
        const el = document.querySelector(".settings-nav");
        const items = [...document.querySelectorAll('[data-testid^="settings-nav-"]')];
        const navBox = el.getBoundingClientRect();
        const clipped = items
          .filter((item) => {
            const itemBox = item.getBoundingClientRect();
            return itemBox.top < navBox.top - 0.5 || itemBox.bottom > navBox.bottom + 0.5;
          })
          .map((item) => item.textContent.trim());
        return {
          count: items.length,
          clipped,
          overflow: el.scrollHeight - el.clientHeight,
        };
      });
      assert(nav.count === 10, `侧栏应有 10 个区块，实际 ${nav.count}`);
      assert(
        nav.clipped.length === 0,
        `640×420 下侧栏不得被裁切：${nav.clipped.join("/")}（溢出 ${nav.overflow}px）`,
      );

      const file = await shot(page, "13-settings-compact-window.png");
      await page.setViewportSize({ width: 900, height: 620 });
      return `640×420 下工作区、快捷键、变更与同步四个区块逐个可达且控件不超出窗口；侧栏 10 个区块一屏内全部可见（溢出 ${nav.overflow}px），截图 ${file}`;
    });

    // 12b. 设置页外壳的行为契约：侧栏分组、一次只看一个区块、会话内记住上次区块、
    //      提示条常驻头部。这几条是「10 个区块平铺」那轮改动的直接产物。
    await check("设置页侧栏一次只显示一个区块并记住上次区块", async () => {
      await page.setViewportSize({ width: 640, height: 420 });
      const visibleSections = () =>
        page.evaluate(() =>
          [...document.querySelectorAll("[data-section]")]
            .filter((node) => node.offsetParent !== null)
            .map((node) => node.dataset.section),
        );

      await openSection(page, "theme");
      const only = await visibleSections();
      assert(
        JSON.stringify(only) === JSON.stringify(["theme"]),
        `同一时刻只应显示当前区块，实际 ${JSON.stringify(only)}`,
      );
      assert(
        (await page.getAttribute('[data-testid="settings-nav-theme"]', "data-current")) === "true",
        "目录必须标记当前区块",
      );
      assert(
        (await page.getAttribute('[data-testid="settings-nav-memos"]', "data-current")) === "false",
        "非当前区块不得被标记为当前",
      );

      // ↑↓ 在目录里移动（键盘优先，与首屏一致）。
      await page.keyboard.press("ArrowDown");
      await page.waitForFunction(
        () =>
          document.querySelector('[data-testid="settings-nav-plugins"]')?.dataset.current ===
          "true",
        undefined,
        { timeout: 5000 },
      );
      const afterDown = await visibleSections();
      assert(
        JSON.stringify(afterDown) === JSON.stringify(["plugins"]),
        `↓ 之后应只显示下一个区块，实际 ${JSON.stringify(afterDown)}`,
      );

      // 切走再回来：会话内记住上次访问的区块。
      await page.click('[data-testid="settings-back"]');
      await page.waitForFunction(
        () =>
          document.querySelector('[data-testid="app-root"]')?.dataset.screen === "search",
        undefined,
        { timeout: 10_000 },
      );
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      const restored = await visibleSections();
      assert(
        JSON.stringify(restored) === JSON.stringify(["plugins"]),
        `回到设置页应停在上次区块，实际 ${JSON.stringify(restored)}`,
      );

      // 提示条常驻头部：原来它在整页最底部，靠前区块操作完看不到反馈。
      await openSection(page, "hotkey");
      await page.fill(HOTKEY_INPUT, "Ctrl+Shift+F2");
      await page.click('[data-testid="hotkey-save"]');
      await page.waitForFunction(
        (sel) => document.querySelector(sel)?.textContent?.includes("立即生效"),
        SETTINGS_MESSAGE,
      );
      const bannerBox = await page.locator(SETTINGS_MESSAGE).boundingBox();
      const screenBox = await page.locator(SETTINGS_SCREEN).boundingBox();
      assert(
        bannerBox && screenBox && bannerBox.y < screenBox.y + screenBox.height / 2,
        `提示条必须在窗口上半部可见：${JSON.stringify(bannerBox)}`,
      );
      const file = await shot(page, "12b-settings-nav.png");

      // 复原快捷键，避免影响后续用例。
      await page.fill(HOTKEY_INPUT, "Ctrl+Shift+F1");
      await page.click('[data-testid="hotkey-save"]');
      await page.waitForFunction(
        (sel) => document.querySelector(sel)?.textContent?.includes("Ctrl+Shift+F1"),
        HOTKEY_STATUS,
      );
      await page.setViewportSize({ width: 900, height: 620 });

      return `一次只显示 ${only.join("/")} 一个区块、目录标记当前项、↑↓ 可移动、切走再回来记住上次区块；提示条在窗口上半部可见，截图 ${file}`;
    });

    // -----------------------------------------------------------------------
    // 运行环境与能力报告（ticket 17）。
    //
    // 检查的是「设置页如实显示平台层探测结果」，不是「平台真的支持」：
    // 模拟宿主里 hotkey / clipboard 是「未覆盖」，autoPaste 是「不支持」或「支持」。
    // 同时断言这份诊断不含任何剪贴板内容或凭证（与真实机器上的报告要求一致）。
    // -----------------------------------------------------------------------

    /** 读取能力区段的关键字段。 */
    const readCapabilityPanel = () =>
      page.evaluate((sel) => {
        const root = document.querySelector(sel);
        const text = (id) =>
          root.querySelector(`[data-testid="capability-${id}"]`)?.textContent?.trim() ?? "";
        const status = (id) =>
          root.querySelector(`[data-testid="capability-${id}"]`)?.dataset.supportStatus ?? "";
        return {
          os: text("os"),
          arch: text("arch"),
          session: text("session"),
          hotkey: text("hotkey"),
          hotkeyStatus: status("hotkey"),
          clipboard: text("clipboard"),
          clipboardStatus: status("clipboard"),
          autoPaste: text("auto-paste"),
          autoPasteStatus: status("auto-paste"),
          notes: text("notes"),
          disclaimer: text("disclaimer"),
          whole: root.textContent ?? "",
        };
      }, CAPABILITY_SECTION);

    // 诊断不得包含历史内容或凭证：这些是模拟宿主里的剪贴板与备忘录样例。
    const DIAGNOSTIC_CANARIES = [
      "会议纪要草稿",
      "https://example.com/report",
      "上午十点在三楼会议室",
      "sekret",
      "alice:",
    ];

    // 10b. 运行环境与能力：如实区分「未覆盖」与「不支持」。
    await check("设置页显示运行环境与能力状态，未覆盖不等于支持", async () => {
      await openSection(page, "capabilities");
      await page.locator(CAPABILITY_SECTION).scrollIntoViewIfNeeded();
      assert(await page.isVisible(CAPABILITY_SECTION), "能力区段不可见");
      const panel = await readCapabilityPanel();
      assert(panel.os.includes("Linux"), `应显示操作系统：${panel.os}`);
      assert(panel.os.includes("浏览器模拟环境"), `应显示系统版本：${panel.os}`);
      assert(panel.arch === "x86_64", `应显示架构：${panel.arch}`);
      assert(panel.session.includes("X11"), `应显示 Linux 会话类型：${panel.session}`);
      assert(
        panel.hotkeyStatus === "unknown" && panel.hotkey.includes("未覆盖"),
        `全局快捷键应如实报未覆盖：${panel.hotkeyStatus}/${panel.hotkey}`,
      );
      assert(
        panel.clipboardStatus === "unknown" && panel.clipboard.includes("未覆盖"),
        `剪贴板应如实报未覆盖：${panel.clipboardStatus}/${panel.clipboard}`,
      );
      assert(
        panel.autoPasteStatus === "unsupported" && panel.autoPaste.includes("不支持"),
        `自动粘贴应报不支持并给出原因：${panel.autoPasteStatus}/${panel.autoPaste}`,
      );
      assert(
        panel.disclaimer.includes("未覆盖") && panel.disclaimer.includes("不代表支持"),
        `必须写明未覆盖不等于支持：${panel.disclaimer}`,
      );
      const leaked = DIAGNOSTIC_CANARIES.filter((needle) => panel.whole.includes(needle));
      assert(leaked.length === 0, `能力报告不得包含剪贴板内容或凭证：${leaked.join("、")}`);
      const file = await shot(page, "83-settings-capabilities.png");
      return `系统 ${panel.os} / ${panel.arch} / ${panel.session}；快捷键与剪贴板未覆盖，自动粘贴不支持；无内容泄露，截图 ${file}`;
    });

    // 10c. 同一份报告在能力可用时必须改口（不能永远显示「未覆盖」）。
    await check("能力可用时报告显示支持，仍不泄露历史内容", async () => {
      await page.evaluate(() => window.__flashcastMock.setAutoPasteSupported(true));
      await page.click('[data-testid="settings-back"]');
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "capabilities");
      await page.waitForFunction(
        (sel) =>
          document.querySelector(`${sel} [data-testid="capability-auto-paste"]`)?.dataset
            .supportStatus === "supported",
        CAPABILITY_SECTION,
      );
      const panel = await readCapabilityPanel();
      assert(panel.autoPaste === "支持", `自动粘贴应显示支持：${panel.autoPaste}`);
      assert(
        panel.hotkeyStatus === "unknown",
        `其余能力不应被连带改口：${panel.hotkeyStatus}`,
      );
      const leaked = DIAGNOSTIC_CANARIES.filter((needle) => panel.whole.includes(needle));
      assert(leaked.length === 0, `能力报告不得包含剪贴板内容或凭证：${leaked.join("、")}`);
      await page.evaluate(() => window.__flashcastMock.setAutoPasteSupported(false));
      const file = await shot(page, "84-settings-capabilities-supported.png");
      return `自动粘贴切换为「支持」后报告随之更新，截图 ${file}`;
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
        // 滚动容器现在是内容区（`.settings` 本身只做 Grid 布局，不再滚动）。
        const el = document.querySelector(".settings-body");
        if (el) el.scrollTop = 0;
        const nav = document.querySelector(".settings-nav");
        if (nav) nav.scrollTop = 0;
      });
    };

    /** 打开设置页并关联一个工作区（goto 之后模拟宿主是全新的）。 */
    const openSettingsWithWorkspace = async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      // 设置页现在一次只显示一个区块，关联工作区必须先导航到工作区块。
      await openSection(page, "workspace");
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

    // 14. 电弧的三种深浅偏好：可读、布局稳定、高频操作无动画。
    await check("电弧深浅偏好可切换且布局与高频操作无动画规则不变", async () => {
      await page.setViewportSize({ width: 900, height: 620 });
      await page.emulateMedia({ colorScheme: "light", reducedMotion: "no-preference" });
      await openSettingsWithWorkspace();
      await openSection(page, "theme");

      // 设置页一次只显示一个区块，所以跨主题比较的是「侧栏 + 当前区块」，不能再拿
      // 来自不同区块的控件（工作区输入框 / 快捷键输入框）来比较——它们现在不同屏。
      const settingsSelectors = [
        THEME_SECTION,
        '[data-testid="theme-item"]',
        '[data-testid="settings-nav-theme"]',
        '[data-testid="settings-nav-workspace"]',
      ];
      const searchSelectors = [INPUT, SETTINGS_BUTTON, ROW, '[data-testid="action-bar"]'];
      const themes = [
        { id: THEME_ARC, preference: "浅色", appearance: "light", surface: SURFACE_LIGHT, settings: "14-theme-light.png", list: "15-theme-light-list.png" },
        { id: THEME_ARC, preference: "深色", appearance: "dark", surface: SURFACE_DARK, settings: "16-theme-dark.png", list: "17-theme-dark-list.png" },
        { id: THEME_ARC, preference: "跟随系统", appearance: "light", surface: SURFACE_LIGHT, settings: "18-theme-follow-system-light.png", list: "19-theme-follow-system-light-list.png" },
      ];

      let settingsBaseline = null;
      let searchBaseline = null;
      let geometry = null;
      const shots = [];
      const details = [];

      for (const theme of themes) {
        await page.getByRole("group", { name: "深浅模式" }).getByRole("button", { name: theme.preference, exact: true }).click();
        await waitForTheme(page, theme.id);
        await page.waitForFunction(appearance => document.documentElement.dataset.themeAppearance === appearance, theme.appearance);

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
        await page.waitForSelector(SETTINGS_SCREEN);
        await openSection(page, "theme");
      }

      return `${details.join("，")}；布局与几何在所有主题下一致；截图 ${shots.join(" / ")}`;
    });

    // 15. 跟随系统：OS 外观在运行时变化时立即切换，无需重启。
    await check("跟随系统在运行时响应系统外观变化", async () => {
      await openSection(page, "theme");
      await page.getByRole("group", { name: "深浅模式" }).getByRole("button", { name: "跟随系统", exact: true }).click();
      await waitForTheme(page, THEME_ARC);
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
      assert(light.selected === THEME_ARC, "系统外观变化不得改变主题选择");
      assert(light.surface === SURFACE_LIGHT, "切回浅色后表面色应恢复");
      return `系统深色 → ${dark.surface}，系统浅色 → ${light.surface}，主题选择保持 ${light.selected}；截图 ${settingsShot} / ${listShot}`;
    });

    // 16. 减少动态效果：高频操作在 reduced-motion 下同样没有动画。
    await check("尊重减少动态效果设置且高频操作无动画", async () => {
      await page.emulateMedia({ reducedMotion: "reduce" });
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "theme");
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
      await openSection(page, "theme");
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
      await page.getByRole("group", { name: "深浅模式" }).getByRole("button", { name: "深色", exact: true }).click();
      await waitForTheme(page, INSTALLED_THEME);
      const installed = await assertThemeIsUsable(INSTALLED_THEME);
      assert(
        installed.surface === SURFACE_SOLARIZED,
        `安装的主题必须真的生效：期望 ${SURFACE_SOLARIZED}，实际 ${installed.surface}`,
      );
      const installedShot = await shot(page, "24-theme-installed.png");
      await backToSearch();
      const installedListShot = await shot(page, "25-theme-installed-list.png");

      // 移除：回退电弧并保留用户的深浅偏好。
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "theme");
      await page.click(`[data-theme-id="${INSTALLED_THEME}"] ${THEME_REMOVE}`);
      await page.waitForFunction(
        (id) => !document.querySelector(`[data-theme-id="${id}"]`),
        INSTALLED_THEME,
        { timeout: 10_000 },
      );
      await waitForTheme(page, THEME_ARC);
      const removed = await themeFacts(page);
      assert(removed.surface === SURFACE_DARK, "移除后回到电弧并保留深色偏好");
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
        await scaledPage.waitForSelector(SETTINGS_SCREEN);
        await openSection(scaledPage, "theme");
        assert(await scaledPage.isVisible(THEME_SECTION), "缩放下主题区不可见");
        await scaledPage.getByRole("group", { name: "深浅模式" }).getByRole("button", { name: "深色", exact: true }).click();
        await scaledPage.waitForFunction(() => document.documentElement.dataset.themeAppearance === "dark");
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
      await openSection(page, "changes");

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
        diff.includes('-hotkey = "Alt+Space"') && diff.includes("+++ b/settings.toml"),
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
      await openSection(page, "sync");
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
      await openSection(page, "sync");
      return `鉴权「${auth.slice(0, 24)}…」与离线「${offline.slice(0, 24)}…」不同，本地搜索仍返回 ${results.length} 条，截图 ${authShot} / ${offlineShot}`;
    });

    // 22. 快进拉取：工作区先显示进行中，完成后重新加载设置、主题与备忘录。
    await check("快进拉取展示进行中并重新加载设置、主题与备忘录", async () => {
      await page.evaluate(() => window.__flashcastMock.simulateSyncScenario("ready"));
      await page.click(SYNC_REDETECT);
      await page.evaluate(() => window.__flashcastMock.simulateRemoteCommit("Ctrl+Alt+Space"));
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
        after === "Ctrl+Alt+Space" && before !== after,
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
      await openSection(page, "memos");
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
      await openSection(page, "plugins");
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
      await openSection(page, "memos");
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
      await openSection(page, "plugins");
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

    // 30. Chrome 书签：两个关键词别名进入范围，结果带网址、目录与浏览器图标。
    await check("Chrome 书签按中英文别名进入范围并给出网址与目录", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);

      for (const alias of [MOCK_CHROME_KEYWORD_EN, MOCK_CHROME_KEYWORD_ZH]) {
        await page.fill(INPUT, alias);
        await page.waitForFunction(
          ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
          { sel: SCOPE_LABEL, expected: `${alias} 范围` },
        );
        const inScope = await rows(page);
        assert(inScope.length === 4, `${alias} 范围应列出 4 条书签，实际 ${inScope.length} 条`);
        assert(
          inScope.every((row) => row.kind === "bookmark"),
          `范围内的结果必须都是书签条目：${inScope.map((row) => row.kind).join("/")}`,
        );
      }

      const action = await text(DEFAULT_ACTION_LABEL);
      assert(action === "在 Chrome 打开", `书签的默认操作必须是在 Chrome 打开，实际「${action}」`);

      // 每条结果都要有网址与目录，并有浏览器图标（不是首字符占位）。
      const subtitles = await page.$$eval(ROW, (nodes) =>
        nodes.map((node) => node.querySelector(".result-subtitle")?.textContent ?? ""),
      );
      assert(
        subtitles.some((line) => line.includes("https://www.rust-lang.org/")),
        `结果里必须能看到网址：${subtitles.join(" | ")}`,
      );
      assert(
        subtitles.some((line) => line.includes("目录：书签栏 / 开发")),
        `结果里必须能看到目录路径：${subtitles.join(" | ")}`,
      );
      const icons = await page.$$(BOOKMARK_ICON);
      assert(icons.length === 4, `每条书签结果都要有浏览器图标，实际 ${icons.length} 个`);

      // 预览给出完整链接与目录（打开前可以确认目标）。
      // 关键词必须**完整匹配**才进入范围，因此先用英文别名进入范围，再继续输入查询。
      await page.fill(INPUT, MOCK_CHROME_KEYWORD_EN);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SCOPE_LABEL, expected: `${MOCK_CHROME_KEYWORD_EN} 范围` },
      );
      await page.fill(INPUT, `${MOCK_CHROME_KEYWORD_EN} intranet`);
      await waitForRowCount(page, 1);
      await page.waitForSelector(MEMO_PREVIEW);
      const body = await text(MEMO_PREVIEW_BODY);
      assert(
        body.includes("token=abc") && body.includes("目录：其他书签"),
        `预览必须是完整链接与目录，实际「${body}」`,
      );
      const scopeShot = await shot(page, "53-chrome-scope.png");
      return `两个别名都进入范围，各 4 条书签，网址 / 目录 / 浏览器图标 / 预览齐全，截图 ${scopeShot}`;
    });

    // 31. Chrome 书签范围内的检索：标题、网址、目录，且标题匹配优先。
    await check("Chrome 书签范围内按标题、网址与目录检索且标题优先", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.fill(INPUT, MOCK_CHROME_KEYWORD_EN);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SCOPE_LABEL, expected: `${MOCK_CHROME_KEYWORD_EN} 范围` },
      );

      await page.fill(INPUT, `${MOCK_CHROME_KEYWORD_EN} rust`);
      await waitForRowCount(page, 3);
      const ranked = await rows(page);
      assert(
        ranked[0].title === "Rust 官网" && ranked[1].title === "Rust 文档",
        `标题匹配必须排在最前：${ranked.map((row) => row.title).join("/")}`,
      );
      assert(
        ranked[2].title === "分析工具",
        `只有网址匹配的条目必须排在最后：${ranked.map((row) => row.title).join("/")}`,
      );

      await page.fill(INPUT, `${MOCK_CHROME_KEYWORD_EN} doc.rust-lang.org`);
      await waitForRowCount(page, 1);
      assert((await rows(page))[0].title === "Rust 文档", "必须能按网址命中");

      await page.fill(INPUT, `${MOCK_CHROME_KEYWORD_EN} 开发`);
      await waitForRowCount(page, 1);
      assert((await rows(page))[0].title === "Rust 文档", "必须能按目录命中");

      await page.fill(INPUT, `${MOCK_CHROME_KEYWORD_EN} 不存在的词`);
      await page.waitForSelector(EMPTY_STATE);
      return "标题优先于网址；网址与目录都能命中；无匹配时是明确的空状态";
    });

    // 32. 回车在关联的 profile 打开：参数向量与诚实反馈。
    await check("回车在关联 profile 打开的 argv 与反馈", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      // 先输入完整关键词进入范围，再继续输入查询（与真实输入顺序一致）。
      await page.fill(INPUT, MOCK_CHROME_KEYWORD_EN);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SCOPE_LABEL, expected: `${MOCK_CHROME_KEYWORD_EN} 范围` },
      );
      await page.fill(INPUT, `${MOCK_CHROME_KEYWORD_EN} intranet`);
      await waitForRowCount(page, 1);
      await page.keyboard.press("Enter");
      await page.waitForSelector(NOTICE);

      const launch = await page.evaluate(() => window.__flashcastMock.lastChromeLaunch);
      assert(launch, "必须记录一次交给 Chrome 的启动请求");
      assert(
        launch.program === "/usr/bin/google-chrome",
        `可执行文件必须是 Chrome，实际「${launch.program}」`,
      );
      assert(
        JSON.stringify(launch.args) ===
          JSON.stringify([
            `--profile-directory=${MOCK_CHROME_DEFAULT_PROFILE}`,
            "--no-first-run",
            "--no-default-browser-check",
            "https://intranet.example.com/login?token=abc&next=首页",
          ]),
        `参数必须逐元素正确（目录名带空格、URL 带查询串与非 ASCII）：${JSON.stringify(launch.args)}`,
      );
      const notice = await text(NOTICE);
      assert(notice.includes("已请求 Chrome"), `必须给出打开反馈：${notice}`);
      assert(
        notice.includes("无法据此确认页面是否已加载"),
        `反馈不得声称页面已经打开：${notice}`,
      );
      const file = await shot(page, "54-chrome-open.png");
      return `argv = ${JSON.stringify(launch.args)}；反馈如实说明无法确认页面是否加载，截图 ${file}`;
    });

    // 33. 设置页的 profile 关联：状态可见、可切换、可重新读取。
    await check("设置页显示并可切换 Chrome profile 关联", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "chrome");
      await page.locator(CHROME_SECTION).scrollIntoViewIfNeeded();

      const availability = await text(CHROME_AVAILABILITY);
      assert(availability.includes("Google Chrome"), `必须显示发现的 Chrome：${availability}`);
      const userDataDir = await text(CHROME_USER_DATA_DIR);
      assert(userDataDir.includes("google-chrome"), `必须显示用户数据目录：${userDataDir}`);
      const association = await text(CHROME_ASSOCIATION);
      assert(
        association.includes("工作") && association.includes("Profile 1"),
        `必须显示当前关联的 profile 目录名：${association}`,
      );
      const path = await text(CHROME_BOOKMARKS_PATH);
      assert(path.endsWith("Bookmarks"), `必须显示书签文件路径：${path}`);

      const profiles = await page.$$eval(CHROME_PROFILE, (nodes) =>
        nodes.map((node) => ({
          dir: node.dataset.profileDir,
          associated: node.dataset.associated,
          managed: node.dataset.managed,
          text: node.textContent ?? "",
        })),
      );
      assert(profiles.length === 2, `必须列出两个 profile，实际 ${profiles.length} 个`);
      assert(
        profiles.find((profile) => profile.dir === MOCK_CHROME_DEFAULT_PROFILE)?.associated ===
          "true",
        "当前关联的 profile 必须有标记",
      );
      assert(
        profiles.some((profile) => profile.managed === "true"),
        "企业管理的 profile 必须显示出来",
      );
      assert(
        profiles.some((profile) => profile.text.includes("me@example.com")),
        "显示名与账号必须可见",
      );
      const beforeShot = await shot(page, "55-settings-chrome-profiles.png");

      await page.click(`${CHROME_PROFILE}[data-profile-dir="Default"] ${CHROME_ASSOCIATE}`);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: CHROME_ASSOCIATION, expected: "个人" },
      );
      const status = await text(CHROME_BOOKMARKS_STATUS);
      assert(status.includes("已索引"), `索引状态必须可见：${status}`);
      const badge = await page.locator(CHROME_ASSOCIATED_BADGE).count();
      assert(badge === 1, `只有一个 profile 能被标记为已关联，实际 ${badge} 个`);

      await page.click(CHROME_REFRESH);
      const afterShot = await shot(page, "56-settings-chrome-associated.png");
      return `关联 工作（Profile 1）→ 切换到 个人（Default）；截图 ${beforeShot} / ${afterShot}`;
    });

    // 34. 书签缺失 / 损坏 / Chrome 未安装都有可操作反馈。
    await check("Chrome 缺失、书签缺失与损坏都有可操作反馈", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() => window.__flashcastMock.simulateChromeStatus("corrupt"));
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "chrome");
      await page.locator(CHROME_SECTION).scrollIntoViewIfNeeded();
      await page.click(CHROME_REFRESH);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: CHROME_BOOKMARKS_STATUS, expected: "无法解析" },
      );
      const corruptShot = await shot(page, "57-settings-chrome-corrupt.png");

      await page.evaluate(() => window.__flashcastMock.simulateChromeStatus("missing"));
      await page.click(CHROME_REFRESH);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: CHROME_BOOKMARKS_STATUS, expected: "正常空状态" },
      );

      await page.evaluate(() => window.__flashcastMock.simulateChromeStatus("unavailable"));
      await page.click(CHROME_REFRESH);
      await page.waitForSelector(CHROME_ERROR);
      const error = await text(CHROME_ERROR);
      assert(
        error.includes("没有找到 Chrome") && error.includes("/usr/bin/google-chrome"),
        `Chrome 未安装必须给出可操作线索：${error}`,
      );
      const unavailableShot = await shot(page, "58-settings-chrome-unavailable.png");

      // 恢复后状态回到正常。
      await page.evaluate(() => window.__flashcastMock.simulateChromeStatus("ok"));
      await page.click(CHROME_REFRESH);
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: CHROME_BOOKMARKS_STATUS, expected: "已索引 4 条书签" },
      );
      return `损坏 → 缺失 → 未安装 → 恢复，四种状态各有说明，截图 ${corruptShot} / ${unavailableShot}`;
    });

    // 35. 书签文件变化后重新检索能看到新书签。
    await check("书签文件变化后重新检索能看到新书签", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.fill(INPUT, MOCK_CHROME_KEYWORD_EN);
      await waitForRowCount(page, 4);

      await page.evaluate(() => window.__flashcastMock.simulateChromeBookmarkAdded());
      await page.fill(INPUT, `${MOCK_CHROME_KEYWORD_EN} 新增`);
      await waitForRowCount(page, 1);
      const found = await rows(page);
      assert(
        found[0].title === "新增书签",
        `外部新增的书签必须能被检索到：${found.map((row) => row.title).join("/")}`,
      );
      const file = await shot(page, "59-chrome-refreshed.png");
      return `追加一条书签后重新检索命中「${found[0].title}」，截图 ${file}`;
    });

    // 36. 停用 Chrome 书签插件后不再贡献结果。
    await check("停用 Chrome 书签插件后不再贡献结果", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "plugins");
      await page.locator(PLUGIN_SECTION).scrollIntoViewIfNeeded();
      const chromeItem = page.locator(
        `${PLUGIN_ITEM}[data-plugin-id="chrome-bookmarks"]`,
      );
      await chromeItem.locator(PLUGIN_TOGGLE).click();
      await page.waitForFunction(
        ({ sel, expected }) =>
          [...document.querySelectorAll(sel)].some(
            (node) =>
              node.dataset.pluginId === "chrome-bookmarks" &&
              node.textContent?.includes(expected),
          ),
        { sel: PLUGIN_ITEM, expected: "已停用" },
      );
      const disabledShot = await shot(page, "60-settings-chrome-disabled.png");

      await page.click('[data-testid="settings-back"]');
      await page.fill(INPUT, MOCK_CHROME_KEYWORD_ZH);
      const scope = await text(SCOPE_LABEL);
      assert(scope === "首屏", `停用后不得进入范围，实际「${scope}」`);
      const kinds = await page.$$eval(ROW, (nodes) => nodes.map((node) => node.dataset.kind));
      assert(!kinds.includes("bookmark"), `停用后不得出现书签结果：${kinds.join("/")}`);
      return `停用后关键词不再进入范围、结果里没有书签，截图 ${disabledShot}`;
    });

    // 37. 自动粘贴：粘贴的是刚执行的那条内容，且没有目标时如实降级为手动粘贴。
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
      const firstShot = await shot(page, "61-memo-pasted.png");

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
      const secondShot = await shot(page, "62-memo-pasted-switched.png");

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
      const fallbackShot = await shot(page, "63-memo-paste-fallback.png");
      return `粘贴「${first.content.slice(0, 6)}…」→ 切换后粘贴「${second.content.slice(0, 6)}…」→ 无目标时降级提示「${notice.slice(0, 18)}…」，截图 ${firstShot} / ${secondShot} / ${fallbackShot}`;
    });

    // 38. 关键词与标签冲突：插件入口与备忘录候选同时保留，入口仍然可用。
    await check("关键词与标签冲突时同时保留插件入口与备忘录候选", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await linkMockWorkspace();
      await openSection(page, "memos");
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
      const collisionShot = await shot(page, "64-memo-keyword-tag-collision.png");

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
      const scopeShot = await shot(page, "65-memo-collision-entered-scope.png");
      return `首屏同时给出入口「${entry.title}」与备忘录「${collided.title}」，回车后进入范围 ${inScope.length} 条，截图 ${collisionShot} / ${scopeShot}`;
    });

    // 39. 中文组合输入：回车确认候选词时不得粘贴。
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
      const file = await shot(page, "66-memo-ime-no-paste.png");

      // 正向对照：组合结束后回车必须真的执行。
      await cdp.send("Input.insertText", { text: "工作" });
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__flashcastMock.lastCopied !== null);
      const copied = await page.evaluate(() => window.__flashcastMock.lastCopied);
      return `组合中回车未复制（lastCopied=null）→ 组合结束后回车复制「${copied.slice(0, 8)}…」，截图 ${file}`;
    });

    // 67. 剪贴板历史：两个中文别名进入同一个插件，并共享同一份历史。
    await check("剪贴板与剪切板是同一个插件并共享历史", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      // 默认关闭：经设置页的插件开关显式启用（真实宿主同样要求用户先启用）。
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "plugins");
      const pluginState = page.locator(
        '[data-plugin-id="clipboard"] [data-testid="plugin-state"]',
      );
      assert(
        (await pluginState.textContent())?.includes("已停用"),
        "剪贴板历史必须默认关闭（隐私敏感）",
      );
      await page.click('[data-plugin-id="clipboard"] [data-testid="plugin-toggle"]');
      await page.waitForFunction(
        () =>
          document.querySelector('[data-plugin-id="clipboard"]')?.dataset.enabled === "true",
      );
      await page.click('[data-testid="settings-back"]');
      await page.waitForSelector(ROW);

      await page.fill(INPUT, "剪贴板");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some(
            (row) => row.dataset.kind === "clipboardEntry",
          ),
        ROW,
      );
      const zh = await rows(page);
      const subtitles = await page.$$eval(ROW, (nodes) =>
        nodes.map((node) => node.querySelector(".result-subtitle")?.textContent ?? ""),
      );
      // 模拟历史里既有文字条目也有文件条目（ticket 12）：这里只要求「范围内都是历史条目」，
      // 不把条数写死——条数是样例数据，不是这条检查要回答的问题。
      assert(
        zh.length >= 2 && zh.every((row) => row.kind === "clipboardEntry"),
        `「剪贴板」范围应全是历史条目，实际 ${zh.length} 条：${zh.map((r) => r.title).join("/")}`,
      );
      assert(
        zh[0].title.includes("example.com/report"),
        `置顶条目必须排在最前，实际「${zh[0].title}」`,
      );
      assert(
        subtitles[0].includes("已置顶") && subtitles[0].includes("文字"),
        `副标题要显示置顶与格式，实际「${subtitles[0]}」`,
      );
      const file = await shot(page, "67-clipboard-scope.png");

      // 「剪切板」是同一个插件的别名：看到完全相同的条目。
      await page.fill(INPUT, "剪切板");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some(
            (row) => row.dataset.kind === "clipboardEntry",
          ),
        ROW,
      );
      const alt = await rows(page);
      assert(
        JSON.stringify(alt.map((row) => row.id)) === JSON.stringify(zh.map((row) => row.id)),
        `两个别名必须看到同一份历史：${alt.map((r) => r.title).join("/")}`,
      );
      return `「剪贴板」与「剪切板」各 ${alt.length} 条且条目一致，截图 ${file}`;
    });

    // 68. 剪贴板历史：范围内的文字检索、完整预览与回车粘贴。
    await check("剪贴板条目可按文字检索、预览并粘贴到唤起前的应用", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );
      await page.evaluate(() => window.__flashcastMock.setAutoPasteSupported(true));

      // 先输入关键词进入范围，再在同一个输入框里继续写检索词（与 Chrome / 备忘录同一路径）。
      await page.fill(INPUT, "剪贴板");
      await page.waitForFunction(
        ({ sel, expected }) => document.querySelector(sel)?.textContent?.includes(expected),
        { sel: SCOPE_LABEL, expected: "剪贴板 范围" },
      );
      await page.fill(INPUT, "剪贴板 会议室");
      await waitForRowCount(page, 1);
      const hits = await rows(page);
      assert(hits.length === 1, `按正文检索应命中 1 条，实际 ${hits.length}`);
      assert(
        hits[0].title.includes("会议纪要"),
        `命中的应是会议纪要那条，实际「${hits[0].title}」`,
      );

      // 预览给出完整正文。
      await page.waitForSelector(MEMO_PREVIEW_BODY);
      const body = await page.textContent(MEMO_PREVIEW_BODY);
      assert(
        body.includes("确认一下参加人"),
        `预览必须是完整正文，实际「${body}」`,
      );
      const file = await shot(page, "68-clipboard-preview.png");

      // 回车：外壳顺序必须是 复制 → 关窗 → 恢复目标 → 注入粘贴。
      await page.evaluate(() => {
        window.__flashcastMock.lastPaste = null;
      });
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__flashcastMock.lastPaste !== null);
      const pasted = await page.evaluate(() => window.__flashcastMock.lastPaste);
      assert(
        pasted.content.includes("确认一下参加人"),
        `粘贴的必须是这条历史的完整正文，实际「${pasted.content}」`,
      );
      assert(
        pasted.sequence.join(" → ") === "copied → windowHidden → restored → pasted",
        `外壳顺序不对：${pasted.sequence.join(" → ")}`,
      );
      return `检索命中 1 条、预览完整、粘贴到「${pasted.target}」，截图 ${file}`;
    });

    // 69. 设置页的剪贴板历史管理：状态、暂停、置顶、删除与清空。
    await check("设置页展示剪贴板状态并可暂停、置顶、删除与清空", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "clipboard");
      await page.waitForSelector('[data-testid="clipboard-section"]');
      await page.waitForSelector('[data-testid="clipboard-item"]');

      const summary = await page.textContent('[data-testid="clipboard-state-summary"]');
      assert(
        summary.includes("后台捕获运行中") && summary.includes("本机条目"),
        `状态摘要必须说明捕获与容量，实际「${summary}」`,
      );
      const storageHint = await page.textContent('[data-testid="clipboard-section"]');
      assert(
        storageHint.includes("history.sqlite3"),
        "必须说明历史保存在本机数据目录",
      );

      // 暂停 / 恢复。
      await page.click('[data-testid="clipboard-pause"]');
      await page.waitForFunction(
        () =>
          document
            .querySelector('[data-testid="clipboard-state-summary"]')
            ?.textContent?.includes("已暂停记录") === true,
      );
      await page.click('[data-testid="clipboard-pause"]');
      await page.waitForFunction(
        () =>
          document
            .querySelector('[data-testid="clipboard-state-summary"]')
            ?.textContent?.includes("正在记录") === true,
      );

      // 置顶：挑一条未置顶的（模拟历史里有的条目本来就置顶），置顶后它排到最前。
      const unpinnedId = await page.$eval(
        '[data-testid="clipboard-item"][data-pinned="false"]',
        (node) => node.dataset.entryId,
      );
      await page.click(
        `[data-testid="clipboard-item"][data-entry-id="${unpinnedId}"] [data-testid="clipboard-pin"]`,
      );
      await page.waitForFunction(
        (id) =>
          document.querySelector(`[data-testid="clipboard-item"][data-entry-id="${id}"]`)
            ?.dataset.pinned === "true",
        unpinnedId,
      );
      const pinnedFirst = await page.$eval(
        '[data-testid="clipboard-item"]',
        (node) => node.dataset.entryId,
      );
      assert(
        pinnedFirst === unpinnedId,
        `置顶的条目必须排到最前：置顶 ${unpinnedId}，实际最前 ${pinnedFirst}`,
      );

      // 删除一条。
      const before = await page.locator('[data-testid="clipboard-item"]').count();
      await page.locator('[data-testid="clipboard-item"]').last().locator('[data-testid="clipboard-delete"]').click();
      await page.waitForFunction(
        (expected) =>
          document.querySelectorAll('[data-testid="clipboard-item"]').length === expected - 1,
        before,
      );
      const file = await shot(page, "69-settings-clipboard.png");

      // 清空：第一次点击是确认，第二次才真的清空。
      await page.click('[data-testid="clipboard-clear"]');
      await page.click('[data-testid="clipboard-clear"]');
      await page.waitForSelector('[data-testid="clipboard-empty"]');
      return `状态/暂停/置顶/删除/清空均生效（删除前 ${before} 条），截图 ${file}`;
    });

    // 70. 停用插件：不捕获、也不返回任何条目。
    await check("停用剪贴板插件后关键词不再返回条目", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      // 默认关闭状态：输入关键词也不得出现历史条目。
      await page.fill(INPUT, "剪贴板");
      await page.waitForTimeout(200);
      const items = await rows(page);
      assert(
        !items.some((item) => item.kind === "clipboardEntry"),
        `停用时不得返回历史条目：${items.map((item) => item.title).join("/")}`,
      );

      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "clipboard");
      await page.waitForSelector('[data-testid="clipboard-section"]');
      const summary = await page.textContent('[data-testid="clipboard-state-summary"]');
      assert(
        summary.includes("未启用") && summary.includes("后台捕获未运行"),
        `停用时状态必须写明未启用且没有后台活动，实际「${summary}」`,
      );
      const pausedDisabled = await page.locator('[data-testid="clipboard-pause"]').isDisabled();
      assert(pausedDisabled, "停用时不得提供暂停入口");
      const file = await shot(page, "70-settings-clipboard-disabled.png");
      return `停用时关键词不返回条目、状态显示未启用，截图 ${file}`;
    });

    // 71. 图片剪贴板条目：结果列表里给出缩略图与类型尺寸。
    await check("图片剪贴板条目在结果里显示缩略图与类型尺寸", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );

      await page.fill(INPUT, "剪贴板");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some(
            (row) => row.dataset.kind === "clipboardEntry",
          ),
        ROW,
      );
      const imageIndex = await imageClipboardIndex(page);
      assert(imageIndex >= 0, "模拟历史里必须能找到图片条目");

      const subtitles = await page.$$eval(ROW, (nodes) =>
        nodes.map((node) => node.querySelector(".result-subtitle")?.textContent ?? ""),
      );
      assert(
        subtitles[imageIndex].includes("图片"),
        `图片条目的副标题必须标注格式「图片」，实际「${subtitles[imageIndex]}」`,
      );
      assert(
        subtitles[imageIndex].includes("PNG 12×8"),
        `图片条目的副标题必须标注类型与尺寸，实际「${subtitles[imageIndex]}」`,
      );

      // 缩略图必须是内嵌 data URL：不发起网络请求，也不引用剪贴板提供的外部地址。
      const thumbnails = await page.$$eval('[data-testid="result-thumbnail"]', (nodes) =>
        nodes.map((node) => ({
          src: node.getAttribute("src") ?? "",
          naturalWidth: node.naturalWidth,
          naturalHeight: node.naturalHeight,
        })),
      );
      assert(thumbnails.length === 1, `缩略图只能有一张，实际 ${thumbnails.length}`);
      assert(
        thumbnails[0].src.startsWith("data:image/png;base64,"),
        `缩略图必须是内嵌 PNG data URL，实际「${thumbnails[0].src.slice(0, 32)}…」`,
      );
      assert(
        thumbnails[0].naturalWidth === 12 && thumbnails[0].naturalHeight === 8,
        `缩略图必须真的解码出 12×8 像素，实际 ${thumbnails[0].naturalWidth}×${thumbnails[0].naturalHeight}`,
      );
      const file = await shot(page, "71-clipboard-image-thumbnail.png");
      return `图片条目在第 ${imageIndex} 行，副标题「${subtitles[imageIndex]}」，缩略图 ${thumbnails[0].naturalWidth}×${thumbnails[0].naturalHeight}，截图 ${file}`;
    });

    // 72. 图片预览按需展开：完整图片来自宿主，文字结果照常可用。
    await check("图片条目按需预览完整图片且不阻塞文字结果", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );
      await page.fill(INPUT, "剪贴板");
      await waitForImageClipboardRow(page);
      const imageIndex = await imageClipboardIndex(page);
      assert(imageIndex >= 0, "模拟历史里必须能找到图片条目");

      await selectRow(page, imageIndex);
      await page.waitForSelector('[data-testid="image-preview-body"]');
      const preview = await page.evaluate(() => {
        const section = document.querySelector('[data-testid="memo-preview"]');
        const image = document.querySelector('[data-testid="image-preview-body"]');
        return {
          kind: section?.dataset.kind ?? "",
          title: document.querySelector('[data-testid="memo-preview-title"]')?.textContent ?? "",
          src: image?.getAttribute("src") ?? "",
          width: image?.naturalWidth ?? 0,
          height: image?.naturalHeight ?? 0,
        };
      });
      assert(preview.kind === "image", `预览必须是图片预览，实际「${preview.kind}」`);
      assert(
        preview.src.startsWith("data:image/png;base64,"),
        "完整图片同样必须是内嵌 data URL（按需读取本机附件）",
      );
      assert(
        preview.width === 12 && preview.height === 8,
        `完整预览必须解码出真实像素，实际 ${preview.width}×${preview.height}`,
      );
      const file = await shot(page, "72-clipboard-image-preview.png");

      // 文字结果不受影响：选中一条文字历史后预览回到惰性文本。
      const textIndex = await page.$$eval(ROW, (nodes) =>
        nodes.findIndex((node) =>
          (node.querySelector(".result-subtitle")?.textContent ?? "").includes("HTML"),
        ),
      );
      assert(textIndex >= 0, "文字条目必须仍然在结果里");
      await selectRow(page, textIndex);
      await page.waitForSelector(MEMO_PREVIEW_BODY);
      const body = await page.textContent(MEMO_PREVIEW_BODY);
      assert(
        body.includes("确认一下参加人"),
        `文字条目的预览必须仍然可用，实际「${body.slice(0, 24)}…」`,
      );
      const imageGone = await page.evaluate(
        () => document.querySelectorAll('[data-testid="image-preview-body"]').length,
      );
      assert(imageGone === 0, "切回文字条目后不得残留图片预览");
      return `图片预览 ${preview.width}×${preview.height}（第 ${imageIndex} 行），文字条目预览仍可用，截图 ${file}`;
    });

    // 73. 图片条目的恢复：自动粘贴与「已复制，请手动粘贴」两种反馈。
    await check("图片条目恢复时走同一条粘贴路径并如实反馈", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() => {
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true);
        window.__flashcastMock.setAutoPasteSupported(true);
        window.__flashcastMock.lastCopied = null;
        window.__flashcastMock.lastPaste = null;
      });
      await page.fill(INPUT, "剪贴板");
      await waitForImageClipboardRow(page);
      const imageIndex = await imageClipboardIndex(page);
      assert(imageIndex >= 0, "模拟历史里必须能找到图片条目");

      await selectRow(page, imageIndex);
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__flashcastMock.lastPaste !== null);
      const pasted = await page.evaluate(() => window.__flashcastMock.lastPaste);
      assert(
        pasted.sequence.join(" → ") === "copied → windowHidden → restored → pasted",
        `图片恢复必须复用同一条外壳顺序：${pasted.sequence.join(" → ")}`,
      );
      assert(
        pasted.content.includes("图片"),
        `写进剪贴板的必须是这条图片历史，实际「${pasted.content}」`,
      );
      const file = await shot(page, "73-clipboard-image-paste.png");

      // 降级路径：没有可注入按键的会话时必须给出手动粘贴反馈，而不是静默失败。
      await page.evaluate(() => {
        window.__flashcastMock.setAutoPasteSupported(false);
        window.__flashcastMock.lastPaste = null;
      });
      await page.fill(INPUT, "剪贴板");
      await waitForImageClipboardRow(page);
      await selectRow(page, imageIndex);
      await page.keyboard.press("Enter");
      const fallback = await page.evaluate(() => ({
        pasted: window.__flashcastMock.lastPaste,
        notice: document.querySelector('[data-testid="notice"]')?.textContent ?? "",
      }));
      assert(
        fallback.pasted === null,
        "不能自动粘贴时不得声称已经粘贴",
      );
      assert(
        fallback.notice.includes("手动粘贴"),
        `降级反馈必须说明请手动粘贴，实际「${fallback.notice}」`,
      );
      return `图片恢复自动粘贴到「${pasted.target}」，降级反馈「${fallback.notice.trim().slice(0, 20)}…」，截图 ${file}`;
    });

    // 74. 设置页的图片历史：缩略图、类型尺寸与条目状态。
    await check("设置页的剪贴板历史显示图片缩略图与尺寸", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "clipboard");
      await page.waitForSelector('[data-testid="clipboard-item"]');

      const thumbnails = await page.$$eval('[data-testid="clipboard-thumbnail"]', (nodes) =>
        nodes.map((node) => ({
          src: node.getAttribute("src") ?? "",
          width: node.naturalWidth,
          height: node.naturalHeight,
        })),
      );
      assert(thumbnails.length === 1, `设置页里的图片缩略图只能有一张，实际 ${thumbnails.length}`);
      assert(
        thumbnails[0].src.startsWith("data:image/png;base64,"),
        "设置页缩略图同样必须是内嵌 data URL",
      );
      assert(
        thumbnails[0].width === 12 && thumbnails[0].height === 8,
        `设置页缩略图必须解码出 12×8，实际 ${thumbnails[0].width}×${thumbnails[0].height}`,
      );
      const itemText = await page.textContent('[data-testid="clipboard-list"]');
      assert(
        itemText.includes("PNG 12×8"),
        `设置页必须显示图片的类型与尺寸，实际「${itemText.slice(0, 60)}…」`,
      );
      const file = await shot(page, "74-settings-clipboard-image.png");
      return `设置页显示 1 张 ${thumbnails[0].width}×${thumbnails[0].height} 缩略图与尺寸「PNG 12×8」，截图 ${file}`;
    });

    // 75. 富文本剪贴板条目：格式集合可见（同一次复制的文字 + HTML/RTF）。
    await check("富文本剪贴板条目在结果里显示完整格式集合", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );

      await page.fill(INPUT, "剪贴板");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some(
            (row) => row.dataset.kind === "clipboardEntry",
          ),
        ROW,
      );
      const subtitles = await page.$$eval(ROW, (nodes) =>
        nodes.map((node) => node.querySelector(".result-subtitle")?.textContent ?? ""),
      );
      const rich = subtitles.find(
        (subtitle) => subtitle.includes("文字") && subtitle.includes("HTML"),
      );
      assert(
        rich !== undefined,
        `必须有一条同时标注文字与 HTML 的条目：${subtitles.join(" | ")}`,
      );
      assert(rich.includes("RTF"), `同一次复制还要标注 RTF：${rich}`);
      // 只有一条富文本条目（同一次复制不得拆成多条重复记录）。
      const richCount = subtitles.filter((subtitle) => subtitle.includes("HTML")).length;
      assert(richCount === 1, `富文本条目只能有一条，实际 ${richCount} 条`);
      const file = await shot(page, "75-clipboard-richtext-scope.png");
      return `富文本条目副标题「${rich}」，同一次复制只有一条，截图 ${file}`;
    });

    // 76. 富文本预览：只渲染惰性纯文本，剪贴板提供的脚本 / 远端资源不入 DOM。
    await check("富文本剪贴板预览是惰性文本且不执行剪贴板标记", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );
      await page.fill(INPUT, "剪贴板");
      await waitForRichClipboardRow(page);

      // 鼠标点击会**执行**条目，选中必须走键盘（与真实键盘操作一致）。
      const richIndex = await richClipboardIndex(page);
      assert(richIndex >= 0, "必须能找到富文本条目");
      await page.click(INPUT);
      for (let index = 0; index < richIndex; index += 1) {
        await page.keyboard.press("ArrowDown");
      }
      // 预览元素一开始就存在（默认选中第一条），因此必须等它换成这条历史的正文。
      await page.waitForFunction(
        (selector) =>
          document.querySelector(selector)?.textContent?.includes("确认一下参加人") === true,
        MEMO_PREVIEW_BODY,
      );

      const body = await text(MEMO_PREVIEW_BODY);
      assert(
        body.includes("确认一下参加人"),
        `预览必须是这条历史的完整纯文本，实际「${body}」`,
      );

      // 预览与结果列表的 DOM 都不得含有剪贴板提供的活动内容。
      // （整页 body 里本来就有 Vite 注入的 `<script type="module">`，因此只查这两处。）
      const dom = await page.evaluate(() => {
        const preview = document.querySelector('[data-testid="memo-preview"]')?.outerHTML ?? "";
        const list = document.querySelector('[data-testid="result-list"]')?.outerHTML ?? "";
        return preview + list;
      });
      for (const needle of ["clipboard-xss", "example.invalid", "onerror", "<script"]) {
        assert(!dom.includes(needle), `剪贴板提供的内容「${needle}」不得进入 DOM`);
      }
      // 结果列表里的 `<img>` 只允许是宿主下发的缩略图（data URL）；剪贴板提供的
      // `<img src="https://…">` 一旦被当标记渲染就会带着远端地址出现。
      const remoteImages = await page.evaluate(
        () =>
          [...document.querySelectorAll('[data-testid="result-list"] img')].filter(
            (node) => !(node.getAttribute("src") ?? "").startsWith("data:image/"),
          ).length,
      );
      assert(remoteImages === 0, `结果列表里不得出现非 data URL 的图片，实际 ${remoteImages} 个`);
      const wholeBody = await page.evaluate(() => document.body.innerHTML);
      for (const needle of ["clipboard-xss", "example.invalid", "onerror"]) {
        assert(!wholeBody.includes(needle), `剪贴板提供的内容「${needle}」不得出现在页面上`);
      }
      // 页面上也不得出现由剪贴板内容创建的元素。
      const injected = await page.evaluate(
        () =>
          document.querySelectorAll(
            '[data-testid="memo-preview-body"] script, [data-testid="memo-preview-body"] img',
          ).length,
      );
      assert(injected === 0, `预览里不得由剪贴板内容创建元素，实际 ${injected} 个`);
      const file = await shot(page, "76-clipboard-richtext-preview.png");
      return `预览为惰性纯文本（${body.length} 字），DOM 中没有脚本/远端资源，截图 ${file}`;
    });

    // 77. 富文本条目粘贴：写进剪贴板的是纯文本（文本目标拿到的仍是纯文本）。
    await check("富文本剪贴板条目粘贴时提供纯文本", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() => {
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true);
        window.__flashcastMock.setAutoPasteSupported(true);
        window.__flashcastMock.lastCopied = null;
        window.__flashcastMock.lastPaste = null;
      });
      await page.fill(INPUT, "剪贴板");
      await waitForRichClipboardRow(page);
      const richIndex = await richClipboardIndex(page);
      assert(richIndex >= 0, "必须能找到富文本条目");
      await page.click(INPUT);
      for (let index = 0; index < richIndex; index += 1) {
        await page.keyboard.press("ArrowDown");
      }
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__flashcastMock.lastPaste !== null);

      const pasted = await page.evaluate(() => window.__flashcastMock.lastPaste);
      assert(
        pasted.content.includes("确认一下参加人"),
        `粘贴的必须是纯文本正文，实际「${pasted.content}」`,
      );
      for (const needle of ["clipboard-xss", "<p>", "example.invalid"]) {
        assert(
          !pasted.content.includes(needle),
          `写进剪贴板的正文不得含剪贴板标记「${needle}」：${pasted.content}`,
        );
      }
      const file = await shot(page, "77-clipboard-richtext-paste.png");
      return `富文本条目粘贴纯文本到「${pasted.target}」，截图 ${file}`;
    });

    // 79. 文件列表条目：名称、类型与引用计数在结果里可见（视频按普通文件处理）。
    await check("文件列表条目在结果里显示名称、类型与引用计数", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );
      await page.fill(INPUT, "剪贴板");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some((row) =>
            (row.textContent ?? "").includes("3 个文件"),
          ),
        ROW,
      );
      const listed = await page.$$eval(ROW, (nodes) =>
        nodes.map((node) => ({
          title: node.querySelector(".result-title")?.textContent ?? "",
          subtitle: node.querySelector(".result-subtitle")?.textContent ?? "",
        })),
      );
      const multi = listed.find((row) => row.title.includes("3 个文件"));
      assert(multi !== undefined, `必须有多文件条目：${listed.map((r) => r.title).join(" | ")}`);
      for (const name of ["报告 草稿.pdf", "照片 一.png", "视频 片段.mp4"]) {
        assert(multi.title.includes(name), `摘要要带文件名「${name}」：${multi.title}`);
      }
      assert(multi.subtitle.includes("文件"), `副标题要标注文件格式：${multi.subtitle}`);
      assert(multi.subtitle.includes("3 个引用"), `副标题要给出引用数：${multi.subtitle}`);
      const unrecoverable = listed.find((row) => row.title.includes("已归档 说明.txt"));
      assert(unrecoverable !== undefined, "样例里应有一个原文件已消失的引用");
      const file = await shot(page, "79-clipboard-file-list.png");
      return `「${multi.title}」副标题「${multi.subtitle}」，截图 ${file}`;
    });

    // 80. 设置页文件行与显式保存副本：只有用户点击才会复制原文件内容。
    await check("设置页可为一个文件引用显式保存本机副本", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "clipboard");
      const entry = '[data-testid="clipboard-item"][data-files="3"]';
      await page.waitForSelector(entry);

      const names = await page.$$eval(`${entry} [data-testid="clipboard-file"]`, (nodes) =>
        nodes.map((node) => node.textContent ?? ""),
      );
      assert(names.length === 3, `多文件条目应有 3 行文件，实际 ${names.length}`);
      for (const name of ["报告 草稿.pdf", "照片 一.png", "视频 片段.mp4"]) {
        assert(
          names.some((text) => text.includes(name)),
          `文件行缺少「${name}」：${names.join(" | ")}`,
        );
      }
      const kinds = await page.$$eval(`${entry} [data-testid="clipboard-file-kind"]`, (nodes) =>
        nodes.map((node) => node.textContent ?? ""),
      );
      assert(
        kinds.length === 3 && kinds.every((kind) => kind === "引用"),
        `捕获时默认全是引用（副本只由用户显式保存）：${kinds.join("/")}`,
      );
      const saveButtons = await page
        .locator(`${entry} [data-testid="clipboard-save-copy"]`)
        .count();
      assert(saveButtons === 3, `每个引用都应该有保存入口，实际 ${saveButtons}`);

      await page.click(
        `${entry} [data-testid="clipboard-file"][data-kind="fileReference"] [data-testid="clipboard-save-copy"]`,
      );
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some(
            (node) => node.dataset.kind === "fileCopy",
          ),
        `${entry} [data-testid="clipboard-file"]`,
      );
      const after = await page.$eval(`${entry} .theme-name`, (node) => node.textContent ?? "");
      assert(
        after.includes("2 个引用") && after.includes("1 个已保存副本"),
        `保存后引用 / 副本计数必须如实更新：${after}`,
      );
      // 副本行不再提供「保存本机副本」，但引用行仍然提供。
      const remaining = await page
        .locator(`${entry} [data-testid="clipboard-save-copy"]`)
        .count();
      assert(remaining === 2, `已保存副本不得再要求保存一次，实际剩余 ${remaining} 个入口`);
      const file = await shot(page, "80-settings-clipboard-file-copy.png");
      return `3 个引用 → 保存 1 个副本后「${after}」，截图 ${file}`;
    });

    // 81. 原文件失效：引用如实显示不可恢复，已保存副本不受影响。
    await check("原文件失效后引用显示不可恢复而副本仍可用", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() =>
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true),
      );
      await page.click(SETTINGS_BUTTON);
      await page.waitForSelector(SETTINGS_SCREEN);
      await openSection(page, "clipboard");
      const entry = '[data-testid="clipboard-item"][data-files="3"]';
      await page.waitForSelector(entry);

      // 先给「照片 一.png」保存本机副本，再让原文件消失。
      await page.click(
        `${entry} [data-testid="clipboard-file"]:has-text("照片 一.png") [data-testid="clipboard-save-copy"]`,
      );
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some(
            (node) => node.dataset.kind === "fileCopy",
          ),
        `${entry} [data-testid="clipboard-file"]`,
      );
      await page.evaluate(() => {
        window.__flashcastMock.simulateClipboardFileMissing("照片 一.png");
        window.__flashcastMock.simulateClipboardFileMissing("报告 草稿.pdf");
      });
      // 让界面拿到一份新的状态快照（置顶开关会回传完整状态）。
      await page.click(`${entry} [data-testid="clipboard-pin"]`);
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some(
            (node) => node.dataset.recoverable === "false",
          ),
        `${entry} [data-testid="clipboard-file"]`,
      );

      const rows = await page.$$eval(`${entry} [data-testid="clipboard-file"]`, (nodes) =>
        nodes.map((node) => ({
          name: node.querySelector(".theme-name")?.textContent ?? "",
          kind: node.dataset.kind,
          recoverable: node.dataset.recoverable,
          hasBadge: node.querySelector('[data-testid="clipboard-file-unrecoverable"]') !== null,
          hasSave: node.querySelector('[data-testid="clipboard-save-copy"]') !== null,
        })),
      );
      const gone = rows.find((row) => row.name.includes("报告 草稿.pdf"));
      const copy = rows.find((row) => row.name.includes("照片 一.png"));
      assert(
        gone?.recoverable === "false" && gone.hasBadge && !gone.hasSave,
        `原文件消失的引用必须显示不可恢复且不再提供保存入口：${JSON.stringify(gone)}`,
      );
      assert(
        gone.name.includes("不可恢复") && gone.name.includes("原文件已不存在"),
        `不可恢复的原因要写清楚：${gone.name}`,
      );
      assert(
        copy?.kind === "fileCopy" && copy.recoverable === "true",
        `副本必须不受原文件消失影响：${JSON.stringify(copy)}`,
      );
      const file = await shot(page, "81-settings-clipboard-unrecoverable.png");
      return `引用不可恢复、副本仍可用（${rows.filter((r) => r.recoverable === "false").length} 个失效），截图 ${file}`;
    });

    // 82. 恢复文件列表：写进剪贴板的是准确的路径列表，绝不走文字路径。
    await check("恢复文件列表时写入准确的路径列表", async () => {
      await page.goto(url, { waitUntil: "load" });
      await page.waitForSelector(ROW);
      await page.evaluate(() => {
        window.__flashcastMock.set_feature_plugin_enabled("clipboard", true);
        window.__flashcastMock.setAutoPasteSupported(true);
        window.__flashcastMock.lastCopied = null;
        window.__flashcastMock.lastCopiedFiles = null;
        window.__flashcastMock.lastPaste = null;
      });
      await page.fill(INPUT, "剪贴板");
      await page.waitForFunction(
        (selector) =>
          [...document.querySelectorAll(selector)].some((row) =>
            (row.textContent ?? "").includes("3 个文件"),
          ),
        ROW,
      );
      const index = await clipboardRowIndex(page, "3 个文件");
      assert(index >= 0, "必须能找到多文件条目");
      await page.click(INPUT);
      for (let step = 0; step < index; step += 1) {
        await page.keyboard.press("ArrowDown");
      }
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.__flashcastMock.lastPaste !== null);
      const pasted = await page.evaluate(() => ({
        files: window.__flashcastMock.lastCopiedFiles,
        text: window.__flashcastMock.lastCopied,
        target: window.__flashcastMock.lastPaste.target,
        sequence: window.__flashcastMock.lastPaste.sequence.join(" → "),
      }));
      assert(
        JSON.stringify(pasted.files) ===
          JSON.stringify(["报告 草稿.pdf", "照片 一.png", "视频 片段.mp4"]),
        `恢复的必须是同一份文件列表（含空格、非 ASCII 与视频）：${JSON.stringify(pasted.files)}`,
      );
      assert(pasted.text === null, `文件列表不得被当成文字恢复：${pasted.text}`);
      assert(
        pasted.sequence === "copied → windowHidden → restored → pasted",
        `文件列表复用 ticket 08 的粘贴路径：${pasted.sequence}`,
      );
      const file = await shot(page, "82-clipboard-files-paste.png");
      return `回车把 3 个文件写进剪贴板并粘贴到「${pasted.target}」，截图 ${file}`;
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
    await browser?.close();
    await teardown();
    writeFileSync(resolve(ARTIFACTS, "report.json"), JSON.stringify({ checks: results }, null, 2) + "\n");
    writeFileSync(resolve(ARTIFACTS, "ui-check.log"), `${lines.join("\n")}\n`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
