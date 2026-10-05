// 真实页面的连续操作；模拟宿主只控制平台返回结果，无 UI 单元测试。
export async function hotkeyConflictChecks({ page, check, url, linkMockWorkspace, openSection, assert }) {
  const open = async scenario => {
    await page.goto(url);
    await page.waitForSelector('[data-testid="result-item"]');
    await linkMockWorkspace();
    await page.evaluate(value => { window.__flashcastMock.hotkeyConflictScenario = value; }, scenario);
    await openSection(page, "hotkey");
    await page.waitForSelector('[data-testid="hotkey-conflict"]');
  };
  const panel = () => page.locator('.hotkey-conflict-panel');
  const click = name => panel().getByRole('button', { name, exact: true }).click();
  const dialog = () => page.getByRole('dialog', { name: '把 Alt+Space 留给 Flashcast？' });
  await check("快捷键冲突→确认取消与 Escape 焦点恢复", async () => {
    await open("conflict");
    assert((await panel().textContent()).includes('GNOME 窗口菜单占用'), "必须说明占用来源");
    await click("解除冲突");
    await dialog().waitFor();
    assert((await dialog().textContent()).includes('保留其他按键'), "必须交代修改范围");
    await page.keyboard.press('Escape');
    assert(await page.getByTestId('hotkey-section').isVisible(), "关闭确认不应退出设置");
    assert(await page.evaluate(() => document.activeElement?.textContent === '解除冲突'), "焦点必须返回原按钮");
    await click("解除冲突");
    await dialog().getByRole('button', { name: '暂不修改' }).click();
    assert(await page.getByTestId('hotkey-conflict').isVisible(), "取消不得修改系统状态");
  });
  await check("解除冲突→核验提示→撤销原系统绑定", async () => {
    await open("conflict"); await click("解除冲突");
    await dialog().getByRole('button', { name: '解除占用并绑定' }).click();
    await page.waitForSelector('.hotkey-success');
    assert((await panel().textContent()).includes('请按一次 Alt+Space'), "不能把设置成功当成人工唤起通过");
    await click('已确认可以唤起');
    assert((await panel().textContent()).includes('快捷键可以正常唤起'), "确认状态未显示");
    await click('撤销系统修改');
    await page.waitForSelector('[data-testid="hotkey-conflict"]');
    assert((await page.getByTestId("hotkey-status").textContent()).includes('Ctrl+Alt+Space'), "撤销应恢复原绑定说明");
  });
  await check("自动修改失败→手动步骤→重新检测→保留错误", async () => {
    await open('failure'); await click('解除冲突');
    await dialog().getByRole('button', { name: '解除占用并绑定' }).click();
    await page.waitForSelector('.hotkey-manual');
    assert((await panel().textContent()).includes('已恢复原来的系统设置'), "失败必须返回恢复结果");
    assert((await panel().textContent()).includes('Backspace'), "必须提供可执行的手动步骤");
    await click('我已修改，重新检测');
    assert((await panel().textContent()).includes('仍需处理'), "检测到仍冲突不得伪报成功");
    await page.evaluate(() => { window.__flashcastMock.hotkeyConflictScenario = 'conflict'; });
    await click('解除冲突'); await dialog().getByRole('button', { name: '解除占用并绑定' }).click();
    await page.waitForSelector('.hotkey-success');
  });
  await check("系统只读或不可检测时只提供手动处理", async () => {
    for (const scenario of ['readonly', 'unknown']) {
      await open(scenario);
      assert(await panel().getByRole('button', { name: '解除冲突', exact: true }).count() === 0, '不可自动修改时不得显示按钮');
      assert(await panel().locator('.hotkey-manual').isVisible(), '必须显示手动指引');
    }
  });
  await check("系统实际绑定不同→重新绑定并核验", async () => {
    await open('mismatch'); await click('重新绑定');
    await dialog().getByRole('button', { name: '解除占用并绑定' }).click();
    await page.waitForSelector('.hotkey-success');
    assert((await page.getByTestId("hotkey-status").textContent()).includes('Alt+Space'), '必须显示系统实际绑定');
  });
  await check("保留系统菜单→替代快捷键写入配置", async () => {
    await open('conflict'); await click('改用 Ctrl+Alt+Space');
    await page.waitForFunction(() => document.querySelector('[data-testid="hotkey-input"]')?.value === 'Ctrl+Alt+Space');
    assert(await panel().count() === 0, '换用其他快捷键后不应继续显示 Alt+Space 冲突');
  });
  await page.goto(url);
}
