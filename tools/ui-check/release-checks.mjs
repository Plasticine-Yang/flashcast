// 连续操作检查：通过页面入口执行，浏览器替身不代表原生系统行为。
export async function releaseChecks({
  page,
  check,
  url,
  linkMockWorkspace,
  openSection,
  assert,
}) {
  const input = page.getByTestId("search-input");
  const fresh = async (link = false) => {
    await page.goto(url);
    await page.waitForSelector('[data-testid="result-item"]');
    if (link) {
      await linkMockWorkspace();
      await page.getByTestId("settings-back").click();
    }
  };
  const enter = async (query, id) => {
    await input.fill(query);
    await page.waitForSelector(`[data-item-id="flashcast.plugin.${id}"]`);
    assert(
      (await page.getByTestId("plugin-page").count()) === 0,
      "输入关键词不得自动进入",
    );
    await page.locator(`[data-item-id="flashcast.plugin.${id}"]`).click();
    await input.press("Enter");
    await page.waitForSelector(
      `[data-testid="plugin-page"][data-plugin="${id}"]`,
    );
  };
  const tray = async () => {
    await page.getByTestId("actions-toggle").click();
    await page.getByTestId("action-tray").waitFor();
  };
  await check("主搜索无 Tab，书签显式进入、清空查询和 Esc 恢复", async () => {
    await fresh();
    assert(
      (await page.locator(".scope-strip").count()) === 0,
      "主搜索不得显示插件 Tab",
    );
    await enter("bookmark", "chrome-bookmarks");
    assert((await input.inputValue()) === "", "插件初始查询必须为空");
    assert(
      (await page.getByTestId("result-item").count()) === 4,
      "全部书签应可见",
    );
    await input.fill("rust");
    await page.waitForFunction(
      () => document.querySelectorAll('[data-kind="bookmark"]').length === 3,
    );
    await input.fill("");
    await page.waitForFunction(
      () => document.querySelectorAll('[data-kind="bookmark"]').length === 4,
    );
    await input.press("Escape");
    assert((await input.inputValue()) === "bookmark", "返回须恢复进入前的查询");
  });
  await check("备忘录标签第一次回车进入，第二次回车粘贴选中正文", async () => {
    await fresh(true);
    await input.fill("工作");
    await page.waitForSelector('[data-kind="memo"]');
    const id = await page
      .locator('[data-selected="true"][data-item-id]')
      .getAttribute("data-item-id");
    await input.press("Enter");
    await page.getByTestId("plugin-page").waitFor();
    assert(
      (await page.evaluate(() => window.__flashcastMock.lastCopied)) === null,
      "第一次回车不得复制",
    );
    assert(
      (await page
        .locator('[data-selected="true"][data-item-id]')
        .getAttribute("data-item-id")) === id,
      "必须保留标签匹配选中项",
    );
    await input.press("Enter");
    await page.waitForFunction(
      () => window.__flashcastMock.lastCopied !== null,
    );
    assert(
      (await page.getByTestId("memo-preview-body").textContent()).includes(
        await page.evaluate(() => window.__flashcastMock.lastCopied),
      ),
      "粘贴必须对应预览",
    );
  });
  await check("备忘录弹窗新增、改名后实时预览及确认删除", async () => {
    await fresh(true);
    await enter("备忘录", "memo");
    await page.getByTestId("memo-new").click();
    await page.getByTestId("memo-title").fill("发版回复");
    await page.getByTestId("memo-tags").fill("memo、发版");
    await page.getByTestId("memo-body").fill("版本已发布。");
    await page.getByTestId("memo-save").click();
    await page.getByTestId("memo-editor").waitFor({ state: "hidden" });
    await page.waitForFunction(
      () =>
        document.querySelector('[data-testid="memo-preview-body"]')
          ?.textContent === "版本已发布。",
    );
    await tray();
    await page.getByTestId("memo-edit").click();
    await page.getByTestId("memo-title").fill("更新回复");
    await page.getByTestId("memo-tags").fill("新标签");
    await page.getByTestId("memo-body").fill("当前正文，已更新。");
    await page.getByTestId("memo-save").click();
    await page.getByTestId("memo-editor").waitFor({ state: "hidden" });
    await page.waitForFunction(
      () =>
        document.querySelector('[data-testid="memo-preview-body"]')
          ?.textContent === "当前正文，已更新。",
    );
    await tray();
    await page.getByTestId("memo-delete").click();
    assert(await page.getByTestId("confirm-delete").isVisible(), "删除须确认");
    await page.getByTestId("confirm-delete").click();
    await page.waitForFunction(
      () =>
        !Array.from(
          document.querySelectorAll('[data-testid="result-item"]'),
        ).some((n) => n.textContent.includes("更新回复")),
    );
  });
  await check(
    "保存失败后草稿、选中项与插件页面保留，修正后可重试",
    async () => {
      await fresh(true);
      await enter("memo", "memo");
      await tray();
      await page.getByTestId("memo-edit").click();
      await page.getByTestId("memo-title").fill("保留草稿");
      await page.getByTestId("memo-body").fill("草稿正文");
      await page.evaluate(() =>
        window.__flashcastMock.simulateMemoSaveError("写入失败：目录只读"),
      );
      await page.getByTestId("memo-save").click();
      await page.waitForSelector('[data-testid="memo-editor"] [role="alert"]');
      assert(
        (await page.getByTestId("memo-body").inputValue()) === "草稿正文",
        "失败应保留正文草稿",
      );
      await page.evaluate(() =>
        window.__flashcastMock.simulateMemoSaveError(null),
      );
      await page.getByTestId("memo-save").click();
      await page.getByTestId("memo-editor").waitFor({ state: "hidden" });
      await page.waitForFunction(
        () =>
          document.querySelector('[data-testid="memo-preview-body"]')
            ?.textContent === "草稿正文",
      );
    },
  );
  await check(
    "同名标签优先结果，IME 不执行，进入后方向键与动作托盘",
    async () => {
      await fresh(true);
      await enter("memo", "memo");
      await page.getByTestId("memo-new").click();
      await page.getByTestId("memo-title").fill("同名标签");
      await page.getByTestId("memo-tags").fill("memo");
      await page.getByTestId("memo-body").fill("同名标签正文");
      await page.getByTestId("memo-save").click();
      await page.getByTestId("memo-editor").waitFor({ state: "hidden" });
      await input.press("Escape");
      await input.fill("memo");
      await page.waitForFunction(
        () =>
          document.querySelector('[data-testid="result-item"]')?.dataset
            .kind === "memo",
      );
      await input.dispatchEvent("compositionstart");
      await input.press("Enter");
      assert(
        (await page.getByTestId("plugin-page").count()) === 0,
        "IME 确认不得进入",
      );
      await input.dispatchEvent("compositionend");
      await input.press("Enter");
      await page.getByTestId("plugin-page").waitFor();
      await input.fill("");
      await page.waitForFunction(
        () =>
          document.querySelectorAll('[data-testid="result-item"]').length === 3,
      );
      await input.press("ArrowDown");
      await input.press("Control+k");
      await page.getByTestId("action-tray").waitFor();
      await input.press("Escape");
      await page.getByTestId("action-tray").waitFor({ state: "hidden" });
      assert(
        await page.getByTestId("plugin-page").isVisible(),
        "Esc 应先关闭动作托盘",
      );
    },
  );
  await check("书签标题、网址和目录检索，回车传独立 Chrome 参数", async () => {
    await fresh();
    await enter("bookmarks", "chrome-bookmarks");
    for (const query of ["文档", "rust-lang.org", "开发"]) {
      await input.fill(query);
      await page.waitForFunction(
        () => document.querySelectorAll('[data-kind="bookmark"]').length > 0,
      );
    }
    await input.press("Enter");
    await page.waitForFunction(
      () => window.__flashcastMock.lastChromeLaunch !== null,
    );
    const launch = await page.evaluate(
      () => window.__flashcastMock.lastChromeLaunch,
    );
    assert(
      launch.args.some((a) => a.startsWith("--profile-directory=")),
      "须传 profile",
    );
    assert(
      launch.args.at(-1).includes("中文&x=1"),
      "链接须作为独立参数完整传入",
    );
  });
  await check(
    "剪切板配置无历史和副本入口，启停后搜索入口立即变化",
    async () => {
      await fresh(true);
      await page.getByTestId("open-settings").click();
      await openSection(page, "plugins");
      const plugin = page.locator('[data-plugin-id="clipboard"]');
      await plugin.getByTestId("plugin-toggle").click();
      await page.waitForFunction(
        () =>
          document.querySelector('[data-plugin-id="clipboard"]')?.dataset
            .enabled === "true",
      );
      await openSection(page, "clipboard");
      assert(
        (await page.getByTestId("clipboard-section").textContent()).includes(
          "剪切板",
        ),
        "设置名称须为剪切板",
      );
      assert(
        (await page.locator('[data-testid="clipboard-entry"]').count()) === 0,
        "配置页不得展示历史",
      );
      assert(
        !(await page.getByTestId("clipboard-section").textContent()).includes(
          "保存本机副本",
        ),
        "不得提供副本入口",
      );
      await page.getByTestId("clipboard-pause").click();
      await page.waitForFunction(
        () =>
          document.querySelector('[data-testid="clipboard-pause"]')
            ?.textContent === "继续记录",
      );
      await page.getByTestId("clipboard-retention").fill("14");
      await page.getByTestId("clipboard-capacity").fill("200");
      await page.getByTestId("clipboard-save-limits").click();
      await page.waitForFunction(
        () => window.__flashcastMock.settings.clipboard.capacity === 200,
      );
      await page.getByTestId("settings-back").click();
      await enter("剪切板", "clipboard");
      assert(
        await page.getByTestId("memo-preview-body").isVisible(),
        "右侧应提供完整文字",
      );
      await page.getByTestId("open-settings").click();
      await openSection(page, "plugins");
      await plugin.getByTestId("plugin-toggle").click();
      await page.waitForFunction(
        () =>
          document.querySelector('[data-plugin-id="clipboard"]')?.dataset
            .enabled === "false",
      );
      await page.getByTestId("settings-back").click();
      await input.fill("剪切板");
      await page.waitForSelector('[data-testid="empty-state"]');
    },
  );
  await check("插件快捷键保存、冲突保留草稿、清除及恢复默认", async () => {
    await fresh(true);
    await page.getByTestId("open-settings").click();
    await openSection(page, "plugins");
    await page
      .locator('[data-plugin-id="clipboard"]')
      .getByTestId("plugin-toggle")
      .click();
    await openSection(page, "clipboard");
    const field = page.getByTestId("command-shortcut-clipboard");
    await field.fill("Ctrl+Alt+V");
    await page
      .locator(".command-shortcut")
      .getByRole("button", { name: "保存", exact: true })
      .click();
    await page.waitForFunction(
      () =>
        document.querySelector('[data-testid="command-shortcut-clipboard"]')
          ?.value === "Ctrl+Alt+V",
    );
    const main = await page.evaluate(
      () => window.__flashcastMock.settings.hotkey,
    );
    await field.fill(main);
    await page
      .locator(".command-shortcut")
      .getByRole("button", { name: "保存", exact: true })
      .click();
    await page.waitForFunction(() =>
      document
        .querySelector('[data-testid="settings-message"]')
        ?.textContent.includes("冲突"),
    );
    assert((await field.inputValue()) === main, "冲突应保留输入");
    assert(
      (await page.evaluate(
        () =>
          window.__flashcastMock.settings.commandShortcuts[
            "flashcast.plugin.clipboard"
          ].linux,
      )) === "Ctrl+Alt+V",
      "冲突不得覆盖已保存配置",
    );
    await page
      .locator(".command-shortcut")
      .getByRole("button", { name: "清除", exact: true })
      .click();
    await page.waitForFunction(
      () =>
        document.querySelector('[data-testid="command-shortcut-clipboard"]')
          ?.value === "",
    );
    await page
      .locator(".command-shortcut")
      .getByRole("button", { name: "恢复默认", exact: true })
      .click();
    await page.waitForFunction(
      () =>
        document.querySelector('[data-testid="command-shortcut-clipboard"]')
          ?.value === "Ctrl+Alt+C",
    );
  });

  const clipboardPage = async () => {
    await fresh(true);
    await page.getByTestId("open-settings").click();
    await openSection(page, "plugins");
    await page
      .locator('[data-plugin-id="clipboard"]')
      .getByTestId("plugin-toggle")
      .click();
    await page.getByTestId("settings-back").click();
    await enter("剪切板", "clipboard");
  };
  await check("剪切板完整图片、惰性富文本与文本恢复", async () => {
    await clipboardPage();
    await page.locator('[data-item-id="clipboard:clip-mock-3"]').click();
    await page.getByTestId("image-preview-body").waitFor();
    assert(
      (
        await page.getByTestId("image-preview-body").getAttribute("src")
      ).startsWith("data:image/png"),
      "完整图片必须来自本机 data URL",
    );
    await page.locator('[data-item-id="clipboard:clip-mock-1"]').click();
    await page.getByTestId("memo-preview-body").waitFor();
    assert(
      (await page.getByTestId("memo-preview-body").textContent()).includes(
        "确认一下参加人",
      ),
      "须显示完整正文",
    );
    assert(
      (await page
        .locator(
          '.plugin-detail script,.plugin-detail iframe,.plugin-detail img[src^="http"]',
        )
        .count()) === 0,
      "富文本载荷不得执行标记或远端资源",
    );
    await input.press("Enter");
    await page.waitForFunction(
      () => window.__flashcastMock.lastCopied !== null,
    );
    assert(
      (await page.evaluate(() => window.__flashcastMock.lastCopied)).includes(
        "确认一下参加人",
      ),
      "恢复须提供纯文本",
    );
  });
  await check("剪切板置顶保留选中预览、确认删除和清空", async () => {
    await clipboardPage();
    await page.locator('[data-item-id="clipboard:clip-mock-3"]').click();
    await page.getByTestId("image-preview-body").waitFor();
    await tray();
    await page.getByRole("menuitem", { name: "置顶", exact: true }).click();
    await page.waitForFunction(
      () =>
        window.__flashcastMock.clipboardEntries.find(
          (i) => i.id === "clip-mock-3",
        ).pinned,
    );
    await page.waitForFunction(
      () =>
        document.querySelector('[data-item-id="clipboard:clip-mock-3"]')
          ?.dataset.selected === "true",
    );
    assert(
      await page.getByTestId("image-preview-body").isVisible(),
      "置顶后仍应预览同一条图片",
    );
    await tray();
    await page.getByRole("menuitem", { name: "删除", exact: true }).click();
    await page.getByTestId("confirm-delete").click();
    await page.waitForFunction(
      () => !document.querySelector('[data-item-id="clipboard:clip-mock-3"]'),
    );
    await tray();
    await page.getByRole("menuitem", { name: "清空历史", exact: true }).click();
    await page.getByTestId("confirm-delete").click();
    await page.waitForSelector('[data-testid="empty-state"]');
    assert(
      await page.getByTestId("plugin-page").isVisible(),
      "清空后仍应留在插件页面",
    );
  });
  await check("文件引用失效如实拒绝，完整路径列表按文件恢复", async () => {
    await clipboardPage();
    await page.locator('[data-item-id="clipboard:clip-mock-5"]').click();
    await input.press("Enter");
    await page.waitForFunction(() =>
      document
        .querySelector('[data-testid="notice"]')
        ?.textContent.includes("不可恢复"),
    );
    await page.locator('[data-item-id="clipboard:clip-mock-4"]').click();
    await input.press("Enter");
    await page.waitForFunction(
      () => window.__flashcastMock.lastCopiedFiles !== null,
    );
    assert(
      JSON.stringify(
        await page.evaluate(() => window.__flashcastMock.lastCopiedFiles),
      ) === JSON.stringify(["报告 草稿.pdf", "照片 一.png", "视频 片段.mp4"]),
      "恢复须使用完整文件路径列表",
    );
    assert(
      (await page.evaluate(() => window.__flashcastMock.lastCopied)) === null,
      "文件不得降为文字",
    );
  });
  await check(
    "插件页显示后台记录失败原因，保存主快捷键不覆盖插件配置",
    async () => {
      await clipboardPage();
      await page.getByTestId("open-settings").click();
      await openSection(page, "clipboard");
      const field = page.getByTestId("command-shortcut-clipboard");
      await field.fill("Ctrl+Alt+V");
      await page
        .locator(".command-shortcut")
        .getByRole("button", { name: "保存", exact: true })
        .click();
      await page.waitForFunction(
        () =>
          window.__flashcastMock.settings.commandShortcuts[
            "flashcast.plugin.clipboard"
          ].linux === "Ctrl+Alt+V",
      );
      await openSection(page, "hotkey");
      await page.getByTestId("hotkey-input").fill("Ctrl+Alt+Space");
      await page.getByTestId("hotkey-save").click();
      await page.waitForFunction(
        () => window.__flashcastMock.settings.hotkey === "Ctrl+Alt+Space",
      );
      assert(
        (await page.evaluate(
          () =>
            window.__flashcastMock.settings.commandShortcuts[
              "flashcast.plugin.clipboard"
            ].linux,
        )) === "Ctrl+Alt+V",
        "主快捷键保存不得覆盖命令配置",
      );
      await page.evaluate(() =>
        window.__flashcastMock.simulateClipboardCaptureFailure(
          "当前桌面不支持后台读取，已有历史仍可使用",
        ),
      );
      await page.getByTestId("settings-back").click();
      await input.press("Escape");
      await enter("剪切板", "clipboard");
      await page.getByTestId("clipboard-warning").waitFor();
      await page.getByTestId("clipboard-warning").locator("summary").click();
      assert(
        (await page.getByTestId("clipboard-warning").textContent()).includes(
          "已有历史仍可使用",
        ),
        "能力失败必须有真实原因",
      );
    },
  );
  await check("深浅模式与三种风格独立切换", async () => {
    await fresh();
    await page.getByTestId("open-settings").click();
    await openSection(page, "theme");
    const modes = page.getByRole("group", { name: "深浅模式" }),
      styles = page.getByRole("group", { name: "表面风格" });
    for (const [name, appearance] of [
      ["浅色", "light"],
      ["深色", "dark"],
    ]) {
      await modes.getByRole("button", { name, exact: true }).click();
      for (const [label, renderer] of [
        ["毛玻璃", "frosted"],
        ["液态玻璃", "liquid"],
        ["实底", "solid"],
      ]) {
        await styles.getByRole("button", { name: label, exact: true }).click();
        await page.waitForFunction(
          ({ appearance, renderer }) =>
            document.documentElement.dataset.themeAppearance === appearance &&
            document.documentElement.dataset.surfaceRenderer === renderer,
          { appearance, renderer },
        );
        assert(
          (await page
            .locator('[data-theme-id="flashcast.theme.arc"]')
            .getAttribute("data-selected")) === "true",
          "模式和风格不得改变主题",
        );
      }
    }
  });
}
