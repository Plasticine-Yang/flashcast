// 经由真实页面操作检查发布最容易遗漏的连续路径；模拟宿主只负责提供数据。
export async function releaseChecks({ page, check, url, linkMockWorkspace, openSection, assert }) {
  const input = page.getByTestId("search-input");
  const back = async () => {
    await page.getByTestId("settings-back").click();
    await page.waitForSelector('[data-testid="search-input"]');
  };
  const freshWorkspace = async () => {
    await page.goto(url);
    await page.waitForSelector('[data-testid="result-item"]');
    await linkMockWorkspace();
  };
  const scopeButton = name => page.locator('.scope-strip').getByRole("button", { name, exact: true });

  await check("范围点击→方向键→回车执行选中备忘录", async () => {
    await freshWorkspace();
    await back();
    await scopeButton("备忘录").click();
    await page.waitForFunction(() => document.querySelectorAll('[data-testid="result-item"]').length > 1);
    assert(await input.evaluate(node => node === document.activeElement), "范围切换后搜索框应获得焦点");
    await page.keyboard.press("ArrowDown");
    await page.waitForFunction(() => document.querySelectorAll('[data-testid="result-item"]')[1]?.dataset.selected === "true");
    const selected = await page.locator('[data-testid="result-item"][data-selected="true"]').getAttribute('data-item-id');
    await page.evaluate(() => { window.__flashcastMock.lastCopied = null; });
    await page.keyboard.press("Enter");
    await page.waitForFunction(() => window.__flashcastMock.lastCopied !== null);
    const body = await page.getByTestId("memo-preview-body").textContent();
    const copied = await page.evaluate(() => window.__flashcastMock.lastCopied);
    assert(body?.includes(copied), `回车必须复制选中项 ${selected} 的正文`);
  });

  await check("功能插件启停立即更新首页入口", async () => {
    await freshWorkspace();
    await openSection(page, "plugins");
    const plugin = page.locator('[data-plugin-id="clipboard"]');
    await plugin.getByTestId("plugin-toggle").click();
    await page.waitForFunction(() => document.querySelector('[data-plugin-id="clipboard"]')?.dataset.enabled === "true");
    await back();
    await scopeButton("剪贴板历史").click();
    await page.waitForFunction(() => document.querySelector('[data-testid="scope-label"]')?.textContent?.includes("剪贴板"));
    await page.waitForSelector('[data-testid="result-item"][data-kind="clipboardEntry"]');
    await page.getByTestId("open-settings").click();
    await openSection(page, "plugins");
    await plugin.getByTestId("plugin-toggle").click();
    await page.waitForFunction(() => document.querySelector('[data-plugin-id="clipboard"]')?.dataset.enabled === "false");
    await back();
    assert(await scopeButton("剪贴板历史").count() === 0, "停用后入口仍然存在");
  });

  await check("同名标签不劫持显式功能插件范围", async () => {
    await freshWorkspace();
    await openSection(page, "memos");
    await page.getByTestId("memo-title-input").fill("同名标签检查");
    await page.getByTestId("memo-tags-input").fill("备忘录");
    await page.getByTestId("memo-body-input").fill("同名标签的正文");
    await page.getByTestId("memo-save").click();
    await page.waitForFunction(() => document.querySelector('[data-testid="memo-title-input"]')?.value === "");
    const count = await page.getByTestId("memo-item").count();
    await back();
    await scopeButton("备忘录").click();
    await page.waitForFunction(expected => document.querySelectorAll('[data-testid="result-item"][data-kind="memo"]').length === expected, count);
    assert((await page.getByTestId("scope-label").textContent())?.trim() === "备忘录 范围", "显式范围应进入功能插件");
  });

  await check("保存失败保留草稿和编辑目标，重试成功可读回", async () => {
    await freshWorkspace();
    await openSection(page, "memos");
    const entry = page.getByTestId("memo-item").first();
    const id = await entry.getAttribute("data-memo-id");
    await entry.getByTestId("memo-edit").click();
    await page.getByTestId("memo-title-input").fill("");
    await page.getByTestId("memo-tags-input").fill("保留草稿");
    await page.getByTestId("memo-body-input").fill("失败后不能丢失的正文");
    await page.getByTestId("memo-save").click();
    await page.waitForFunction(() => document.querySelector('[data-testid="settings-message"]')?.textContent?.includes("标题不能为空"));
    assert(await page.getByTestId("memo-body-input").inputValue() === "失败后不能丢失的正文", "保存失败丢失正文");
    assert(await page.getByTestId("memo-tags-input").inputValue() === "保留草稿", "保存失败丢失标签");
    assert(await entry.getAttribute("data-editing") === "true", "保存失败丢失编辑目标");
    await page.getByTestId("memo-title-input").fill("重试后的标题");
    await page.getByTestId("memo-save").click();
    await page.waitForFunction(() => document.querySelector('[data-testid="memo-title-input"]')?.value === "");
    await page.locator(`[data-memo-id="${id}"]`).getByTestId("memo-edit").click();
    assert(await page.getByTestId("memo-body-input").inputValue() === "失败后不能丢失的正文", "保存成功未读回正文");
  });

  await check("深浅×三种风格与减少透明度独立，非玻璃主题适配", async () => {
    await freshWorkspace();
    await openSection(page, "theme");
    const modes = page.getByRole("group", { name: "深浅模式" });
    const styles = page.getByRole("group", { name: "表面风格" });
    for (const [name, appearance] of [["浅色", "light"], ["深色", "dark"]]) {
      await modes.getByRole("button", { name, exact: true }).click();
      for (const [label, renderer] of [["毛玻璃", "frosted"], ["液态玻璃", "liquid"], ["实底", "solid"]]) {
        await styles.getByRole("button", { name: label, exact: true }).click();
        await page.waitForFunction(({ appearance, renderer }) => document.documentElement.dataset.themeAppearance === appearance && document.documentElement.dataset.surfaceRenderer === renderer, { appearance, renderer });
        assert(await page.locator('[data-theme-id="flashcast.theme.arc"]').getAttribute("data-selected") === "true", "切模式或风格改变了主题身份");
      }
    }
    await styles.getByRole("button", { name: "液态玻璃", exact: true }).click();
    const reduce = page.getByRole("checkbox", { name: /减少透明度/ });
    await reduce.check();
    await page.waitForFunction(() => document.documentElement.dataset.surfaceRenderer === "solid");
    assert(await styles.getByRole("button", { name: "液态玻璃", exact: true }).getAttribute("aria-pressed") === "true", "减少透明度改变了风格偏好");
    await reduce.uncheck();
    await page.waitForFunction(() => document.documentElement.dataset.surfaceRenderer === "liquid");
    await page.getByTestId("theme-package-input").fill("/home/user/themes/solarized");
    await page.getByTestId("theme-install").click();
    const installed = page.locator('[data-theme-id="example.solarized"]');
    await installed.getByTestId("theme-select").click();
    await page.waitForFunction(() => document.documentElement.dataset.themeSelected === "example.solarized");
    assert(await styles.count() === 0, "只有一种风格的主题应隐藏风格选择");
    await modes.getByRole("button", { name: "浅色", exact: true }).click();
    await page.waitForFunction(() => document.documentElement.dataset.themeAppearance === "light");
    await modes.getByRole("button", { name: "深色", exact: true }).click();
    await page.waitForFunction(() => document.documentElement.dataset.themeAppearance === "dark");
    await page.getByTestId("theme-package-input").fill("/home/user/themes/broken");
    await page.getByTestId("theme-install").click();
    await page.waitForFunction(() => document.querySelector('[data-testid="settings-message"]')?.textContent?.includes("主题无效"));
    assert(await page.getByTestId("theme-package-input").inputValue() === "/home/user/themes/broken", "安装失败应保留路径");
    assert(await page.evaluate(() => document.documentElement.dataset.themeSelected) === "example.solarized", "安装失败改变了主题");
  });
}
