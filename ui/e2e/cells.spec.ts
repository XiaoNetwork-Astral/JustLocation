import { test, expect, type Locator, type Page } from '@playwright/test';

/**
 * 切换开关。开关就是可见的 checkbox 本身，直接点它就是真实用户的操作路径；
 * 如果这条失败，说明真机上手指也点不动，属于产品缺陷而不是测试写法问题。
 * 状态已经符合期望时不再点击，避免"点两下等于没点"。
 */
async function toggleSwitch(page: Page, box: Locator, checked: boolean) {
  if (await box.isChecked() !== checked) await box.click();
  await expect(box).toBeChecked({ checked });
}

test('mobile cell panel configures detected cards, applies target data and preserves settings when toggled', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await page.addInitScript(() => {
    const host = window as any;
    const card = { id: 7, slot: 0, mcc: '460', mnc: '001', country: 'cn', carrier: '测试运营商' };
    const state: any = { requested_active: false, phone_connected: true, cell_hook_ready: true,
      detected_subscriptions: [card], telephony: { cells_enabled: false, sim_enabled: false, radius_m: 500, subscriptions: [] },
      config: { position: { latitude: 31.2, longitude: 121.5, altitude: 10, accuracy: 5, speed: 0, bearing: 0 }, scope: { mode: 'all' } } };
    const settings = { primary: 'open_cell_id', fallback: null, opencellid_configured: true, custom_endpoint: '', custom_token_configured: false, fake_location_ready: false };
    const dataset = { provider: 'open_cell_id', origin: 'https://opencellid.org', region: { center: { latitude: 31.2, longitude: 121.5 }, radius_m: 500, source: 'OpenCellID', fetched_at_ms: Date.now(),
      cells: [{ identity: { radio: 'lte', mcc: '460', mnc: '001', tac: 10, ci: 100 }, position: { latitude: 31.2, longitude: 121.5 }, range_m: 0 }] },
      attribution: { text: 'OpenCellID', source: 'https://opencellid.org', license: 'https://creativecommons.org/licenses/by-sa/4.0/', changes: null }, incomplete: false, skipped: 0, failures: [] };
    host.frames = [];
    host.ksu = { exec: (command: string, _options: string, callback: string) => {
      const frame = JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(command.split(' ').at(-1)!), c => c.charCodeAt(0))));
      host.frames.push(frame);
      if (command.includes(' cells ')) {
        host[callback](0, JSON.stringify({ version: 1, ok: true, ...(frame.op === 'query' ? { dataset, cached: true, stale: false } : { settings }) }), ''); return;
      }
      if (frame.op === 'set_telephony') state.telephony = frame.config;
      if (frame.op === 'set_cell_region') state.cell_region = frame.region;
      host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
    } };
  });
  await page.goto('/');
  // 入口变了：过去是“基站菜单 → 启用基站模拟”，后来是首页的整行开关，
  // 现在按原版放回目标卡操作行里的「基站」+ 圆形图标按钮，点它进入基站页。
  await page.getByRole('button', { name: '基站模拟设置' }).click();
  const dialog = page.getByRole('dialog', { name: '基站模拟' });
  await expect(dialog).toBeVisible();
  // 总开关在面板里；配置里还没有可用运营商时会提示并保留在面板上。
  await expect(page.getByText('使用目标位置附近的基站数据')).toBeVisible();
  await toggleSwitch(page, page.getByRole('checkbox', { name: '启用基站模拟', exact: true }), true);
  await toggleSwitch(page, page.getByRole('checkbox', { name: '模拟基站', exact: true }), true);
  await toggleSwitch(page, page.getByRole('checkbox', { name: '模拟 SIM 运营商', exact: true }), true);
  await page.getByText('修改运营商', { exact: true }).click();
  await page.getByLabel('运营商名称').fill('自己的运营商📱');
  await page.getByRole('button', { name: '保存模拟设置' }).click();
  await expect(page.getByText('设置已保存，开始位置模拟后生效。')).toBeVisible();
  // “基站模拟”开关自己也会发一帧还不带 SIM 卡的 set_telephony，这里只看带卡数据的那一帧。
  expect(await page.evaluate(() => (window as any).frames.filter((f: any) => f.op === 'set_telephony' && f.config.subscriptions.length).at(-1).config.subscriptions[0].mnc)).toBe('001');
  await page.getByText('修改运营商', { exact: true }).click();
  await page.screenshot({ path: '../build/webui-cells-settings.png', fullPage: true });
  await page.getByRole('button', { name: '查询附近基站', exact: true }).click();
  await expect(page.getByText('LTE · 460-001')).toBeVisible();
  // 查到数据只是显示出来，还没生效：后台要等“应用这份基站数据”才收到区域。
  expect(await page.evaluate(() => (window as any).frames.some((f: any) => f.op === 'set_cell_region'))).toBe(false);
  await page.getByRole('button', { name: '应用这份基站数据' }).click();
  await expect(page.getByText('基站数据已应用')).toBeVisible();
  expect(await dialog.evaluate(e => e.scrollWidth <= e.clientWidth)).toBe(true);
  await page.screenshot({ path: '../build/webui-cells-data.png', fullPage: true });
  await page.getByRole('button', { name: '返回位置模拟' }).click();
  await expect(dialog).toHaveCount(0);
  // 停用基站模拟：回到面板再关总开关（旧路径是“基站菜单 → 停用基站模拟”）。
  await page.getByRole('button', { name: '基站模拟设置' }).click();
  await toggleSwitch(page, page.getByRole('checkbox', { name: '启用基站模拟', exact: true }), false);
  const last = await page.evaluate(() => (window as any).frames.filter((f: any) => f.op === 'set_telephony').at(-1));
  expect(last.config.cells_enabled).toBe(false);
  expect(last.config.sim_enabled).toBe(true);
  expect(last.config.subscriptions[0].carrier).toBe('自己的运营商📱');
});
