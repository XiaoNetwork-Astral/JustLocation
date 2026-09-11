import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

test('imports and downloads a backup, restores visible libraries and exports GPX on a narrow screen', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.goto('/');
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '设置', exact: true }).click();
  const position = { latitude: 31.2, longitude: 121.5, altitude: 8, accuracy: 5, speed: 0, bearing: 0 };
  const backup = { format: 'justlocation', version: 1,
    places: [{ id: 'p', name: '家', position, pinned: true }],
    routes: [{ id: 'r', name: '小桥 & 河边', plan: { points: [position, { ...position, latitude: 31.3 }], speed: 1.5, repeat_count: 3, repeat_delay: 8 } }],
  };
  const file = { name: 'backup.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) };
  await page.getByLabel('导入备份').setInputFiles(file);
  await expect(page.getByText('1 个位置 · 1 条路线')).toBeVisible();
  await page.getByRole('button', { name: '合并导入' }).click();
  await expect(page.getByRole('status')).toHaveText('已添加 1 个位置、1 条路线');
  await page.getByLabel('导入备份').setInputFiles(file);
  await page.getByRole('button', { name: '合并导入' }).click();
  await expect(page.getByRole('status')).toHaveText('已添加 0 个位置、0 条路线');
  const downloadEvent = page.waitForEvent('download');
  await page.getByRole('button', { name: '导出备份' }).click();
  const downloaded = await downloadEvent;
  expect(JSON.parse(await readFile((await downloaded.path())!, 'utf8'))).toEqual(backup);
  for (const theme of ['light', 'dark']) {
    await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: `../build/webui-backup-${theme}.png`, fullPage: true });
  }
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '位置模拟', exact: true }).click();
  await expect(page.getByRole('button', { name: '家 31.200000, 121.500000' })).toBeVisible();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await page.getByText('已保存的路线（1）').click();
  const gpxEvent = page.waitForEvent('download');
  await page.getByRole('button', { name: '导出小桥 & 河边为 GPX' }).click();
  expect(await readFile((await (await gpxEvent).path())!, 'utf8')).toContain('<name>小桥 &amp; 河边</name>');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
