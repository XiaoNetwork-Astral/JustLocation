import { test, expect } from '@playwright/test';

test('saved routes survive reload and fit a small screen with a long name', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 740 });

  await page.addInitScript(() => {
    const host = window as any;
    const state = {
      requested_active: false,
      config: {
        position: {
          latitude: 31.2,
          longitude: 121.5,
          altitude: 10,
          accuracy: 5,
          speed: 0,
          bearing: 0,
        },
        scope: { mode: 'all' },
      },
    };
    host.ksu = {
      exec: (_command: string, _options: string, callback: string) => {
        host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
      },
    };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();

  await page.getByRole('button', { name: '新建路线' }).click();
  await page.getByLabel('导入 GPX').setInputFiles({
    name: 'walk.gpx',
    mimeType: 'application/gpx+xml',
    buffer: Buffer.from(
      '<gpx><rte><rtept lat="31.2" lon="121.5"/><rtept lat="31.201" lon="121.5"/></rte></gpx>',
    ),
  });
  await page.getByRole('button', { name: '使用这条路线' }).click();
  await page.getByLabel('播放次数').fill('3');
  await page.getByLabel('每次间隔（秒）').fill('10');
  const name = '晚饭后绕小区散步，再沿着河边走到桥头';
  await page.getByRole('button', { name: '保存路线' }).click();
  await page.getByLabel('路线名称').fill(name);
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await expect(page.getByRole('button', { name: `使用${name}` })).toBeVisible();
  for (const theme of ['light', 'dark']) {
    await page.evaluate((theme) => (document.documentElement.dataset.theme = theme), theme);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
    await page.screenshot({ path: `../build/webui-route-library-${theme}.png`, fullPage: true });
  }
  await page.getByLabel('点 1 纬度', { exact: true }).fill('20');
  await page.reload();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await page.getByText('已保存的路线（1）').click();
  await page.getByRole('button', { name: `使用${name}` }).click();

  await page.getByRole('button', { name: `编辑${name}` }).click();
  await expect(page.getByLabel('点 1 纬度', { exact: true })).toHaveValue('31.2');
  await expect(page.getByLabel('播放次数')).toHaveValue('3');
  await page.getByRole('button', { name: `删除${name}` }).click();
  await page.getByRole('button', { name: '撤销' }).click();
  await expect(page.getByRole('button', { name: `使用${name}` })).toBeVisible();
});
