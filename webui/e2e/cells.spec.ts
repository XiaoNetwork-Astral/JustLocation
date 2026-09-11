import { test, expect } from '@playwright/test';

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
  await page.getByRole('button', { name: '基站菜单' }).click();
  await page.getByRole('button', { name: '启用基站模拟' }).click();
  const dialog = page.getByRole('dialog', { name: '附近基站' });
  await expect(dialog).toBeVisible();
  await page.getByLabel('模拟基站', { exact: true }).check();
  await page.getByLabel('模拟 SIM 运营商', { exact: true }).check();
  await page.getByText('修改运营商', { exact: true }).click();
  await page.getByLabel('运营商名称').fill('自己的运营商📱');
  await page.getByRole('button', { name: '保存模拟设置' }).click();
  await expect(page.getByText('设置已保存，开始位置模拟后生效。')).toBeVisible();
  expect(await page.evaluate(() => (window as any).frames.find((f: any) => f.op === 'set_telephony').config.subscriptions[0].mnc)).toBe('001');
  await page.getByText('修改运营商', { exact: true }).click();
  await page.screenshot({ path: '../build/webui-cells-settings.png', fullPage: true });
  await page.getByRole('button', { name: '查询附近基站', exact: true }).click();
  await expect(page.getByText('LTE · 460-001')).toBeVisible();
  await page.getByRole('button', { name: '应用这份基站数据' }).click();
  await expect(page.getByText('基站数据已应用')).toBeVisible();
  expect(await dialog.evaluate(e => e.scrollWidth <= e.clientWidth)).toBe(true);
  await page.screenshot({ path: '../build/webui-cells-data.png', fullPage: true });
  await page.getByRole('button', { name: '返回位置模拟' }).click();
  await expect(dialog).toHaveCount(0);
  await page.getByRole('button', { name: '基站菜单' }).click();
  await page.getByRole('button', { name: '停用基站模拟' }).click();
  const last = await page.evaluate(() => (window as any).frames.filter((f: any) => f.op === 'set_telephony').at(-1));
  expect(last.config.cells_enabled).toBe(false);
  expect(last.config.sim_enabled).toBe(true);
  expect(last.config.subscriptions[0].carrier).toBe('自己的运营商📱');
});
