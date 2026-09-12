import { test, expect, type Page } from '@playwright/test';

/** 路线页的“新建路线”在后台未就绪时不可点，这里给一个最小后台桩。 */
async function withBackend(page: Page) {
  await page.addInitScript(() => {
    const host = window;
    host.__stubErrors = [];
    host.__stubCommands = [];
    try {
      host.ksu = {
        exec: (command, _options, callback) => {
          host.__stubCommands.push(command);
          const call = host[callback];
          if (typeof call !== 'function') { host.__stubErrors.push(`缺少回调 ${callback}`); return; }
          if (command.indexOf('pm path') === 0) { call(0, 'package:/data/app/companion.apk', ''); return; }
          if (command.indexOf('am start') === 0) { call(0, 'Starting: Intent', ''); return; }
          if (command.indexOf('am stopservice') === 0) { call(1, 'Service stopped', ''); return; }
          if (command.indexOf('dumpsys') === 0) { call(0, '', ''); return; }
          call(0, JSON.stringify({ version: 1, ok: true, state: { requested_active: false, config: {
            position: { latitude: 31.2, longitude: 121.5, altitude: 12, accuracy: 5, speed: 0, bearing: 0 },
            scope: { mode: 'apps', packages: ['me.idk.justlocation.probe'] },
          } } }), '');
        },
      };
    } catch (error) {
      host.__stubErrors.push(String(error));
    }
  });
}

/** 记录编辑前的位置，供“返回后表单不丢”的断言使用。 */
async function openRouteEditor(page: Page) {
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await page.getByRole('button', { name: '新建路线' }).click();
}

test.beforeEach(async ({ page }) => {
  // No requests to public map servers during automated tests.
  await page.route('https://tile.openstreetmap.org/**', route => route.fulfill({ contentType: 'image/svg+xml', body:
    '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256"><rect width="256" height="256" fill="#e8eadf"/><path d="M0 128H256M128 0V256" stroke="#fff" stroke-width="8"/></svg>' }));
  await page.setViewportSize({ width: 320, height: 740 });
  await page.goto('/');
});

test('selects a map point, returns without losing form fields, and saves WGS84 only once', async ({ page }) => {
  await page.getByRole('button', { name: '添加位置', exact: true }).click();
  await page.getByLabel('位置名称').fill('地图选点');
  await page.getByLabel('坐标系').selectOption('gcj02');
  await page.getByLabel('纬度', { exact: true }).fill('39.91640428150164');
  await page.getByLabel('经度', { exact: true }).fill('116.41024449916938');
  await page.getByLabel('海拔（米）').fill('15');
  await page.getByRole('button', { name: '地图选点', exact: true }).click();
  await expect(page.getByRole('dialog', { name: '地图选点' })).toBeVisible();
  await expect(page.getByLabel('所选坐标')).toHaveText('39.915000, 116.404000');
  const canvas = page.getByLabel('选点地图');
  await canvas.click({ position: { x: 210, y: 120 } });
  const selected = (await page.getByLabel('所选坐标').textContent())!;
  expect(selected).not.toBe('39.915000, 116.404000');
  for (const theme of ['light', 'dark']) {
    await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await expect(page.getByRole('button', { name: '使用此位置' })).toBeInViewport();
    await expect(page.getByRole('link', { name: 'OpenStreetMap' })).toBeInViewport();
    await page.screenshot({ path: `../build/webui-map-${theme}.png` });
  }
  await page.getByRole('button', { name: '使用此位置' }).click();
  await expect(page.getByRole('dialog', { name: '地图选点' })).toHaveCount(0);
  await expect(page.getByLabel('坐标系')).toHaveValue('wgs84');
  await expect(page.getByLabel('位置名称')).toHaveValue('地图选点');
  await expect(page.getByLabel('海拔（米）')).toHaveValue('15');
  await page.getByRole('button', { name: '地图选点', exact: true }).click();
  await page.goBack();
  await expect(page.getByRole('dialog', { name: '地图选点' })).toHaveCount(0);
  await page.getByRole('button', { name: '保存位置' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('justlocation.places')!));
  expect(saved[0].position.altitude).toBe(15);
  expect(`${saved[0].position.latitude.toFixed(6)}, ${saved[0].position.longitude.toFixed(6)}`).toBe(selected);
});

test('keeps coordinate entry available after a tile failure and map cancellation', async ({ page }) => {
  await page.route('https://tile.openstreetmap.org/**', route => route.abort());
  await page.getByRole('button', { name: '添加位置', exact: true }).click();
  await page.getByRole('button', { name: '地图选点', exact: true }).click();
  await expect(page.getByText(/地图加载失败/)).toBeVisible();
  await page.getByRole('button', { name: '返回位置编辑' }).click();
  await expect(page.getByLabel('纬度', { exact: true })).toHaveValue('');
  await page.getByLabel('纬度', { exact: true }).fill('0');
  await page.getByLabel('经度', { exact: true }).fill('0');
  await page.getByRole('button', { name: '保存位置' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('searches a place, moves to the current position and switches the map source', async ({ page }) => {
  await page.route('https://nominatim.openstreetmap.org/**', route => route.fulfill({ contentType: 'application/json', body:
    JSON.stringify([{ name: '上海火车站', display_name: '上海火车站, 静安区, 上海市', lat: '31.2497', lon: '121.4553' }]) }));
  await page.getByRole('button', { name: '添加位置', exact: true }).click();
  await page.getByRole('button', { name: '地图选点', exact: true }).click();
  await page.getByLabel('搜索地点').fill('上海火车站');
  await page.getByRole('button', { name: '搜索', exact: true }).click();
  await page.getByRole('listitem').first().click();
  await expect(page.getByLabel('所选坐标')).toHaveText('31.249700, 121.455300');
  await page.getByRole('button', { name: '使用此位置' }).click();
  await expect(page.getByLabel('纬度', { exact: true })).toHaveValue('31.2497');
  // 直接输入经纬度不需要联网。
  await page.getByRole('button', { name: '地图选点', exact: true }).click();
  await page.getByLabel('搜索地点').fill('30.5, 120.5');
  await page.getByRole('button', { name: '搜索', exact: true }).click();
  await expect(page.getByLabel('所选坐标')).toHaveText('30.500000, 120.500000');
  await page.screenshot({ path: '../build/webui-map-search.png' });
  // 换图源只影响底图，内部始终是 WGS84。
  await page.getByLabel('地图图源').selectOption('amap');
  await expect(page.getByText(/已换算/)).toBeVisible();
  await expect(page.getByLabel('所选坐标')).toHaveText('30.500000, 120.500000');
  await page.screenshot({ path: '../build/webui-map-amap.png' });
  await page.getByLabel('地图图源').selectOption('custom');
  await expect(page.getByLabel('自定义图源地址')).toBeVisible();
  await expect(page.getByText(/还没有可用地址/)).toBeVisible();
  await page.screenshot({ path: '../build/webui-map-providers.png' });
});

test('changes one route point on the map without changing speed or starting playback', async ({ page }) => {
  // addInitScript 只对之后的文档加载生效，所以桩要在导航之前注册。
  await withBackend(page);
  await page.goto('/');
  await openRouteEditor(page);
  await page.getByLabel('点 1 纬度', { exact: true }).fill('31.2');
  await page.getByLabel('点 1 经度', { exact: true }).fill('121.5');
  await page.getByRole('button', { name: '在地图上选择点 1', exact: true }).click();
  await page.getByLabel('选点地图').click({ position: { x: 210, y: 120 } });
  await page.getByRole('button', { name: '使用此位置' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByLabel('点 1 纬度', { exact: true })).not.toHaveValue('31.2');
  await expect(page.getByLabel('点 2 纬度', { exact: true })).toHaveValue('');
  await expect(page.getByLabel('速度（km/h）')).toHaveValue('5.4');
});

test('plans a route on the map, undoes a point, and commits only on confirmation', async ({ page }) => {
  await withBackend(page);
  await page.goto('/');
  await openRouteEditor(page);
  await page.getByLabel('点 1 纬度', { exact: true }).fill('31.2');
  await page.getByLabel('点 1 经度', { exact: true }).fill('121.5');
  await page.getByRole('button', { name: '地图规划', exact: true }).click();
  await expect(page.getByRole('button', { name: '完成路线' })).toBeDisabled();
  await page.getByRole('button', { name: '添加到路线' }).click();
  await page.getByRole('button', { name: '添加到路线' }).click();
  await expect(page.getByRole('dialog', { name: '地图规划' }).getByRole('alert')).toHaveText('相邻路线点不能相同');
  await page.getByLabel('选点地图').click({ position: { x: 210, y: 120 } });
  await page.getByRole('button', { name: '添加到路线' }).click();
  await expect(page.getByLabel('路线点数')).toHaveText('2 个点');
  await page.getByRole('button', { name: '撤销最后一个点' }).click();
  await expect(page.getByRole('button', { name: '完成路线' })).toBeDisabled();
  await page.getByRole('button', { name: '添加到路线' }).click();
  await page.getByRole('button', { name: '完成路线' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const draft = await page.evaluate(() => localStorage.getItem('justlocation.route'));
  expect(JSON.parse(draft!).points).toHaveLength(2);
  await page.getByRole('button', { name: '地图规划', exact: true }).click();
  await page.getByLabel('选点地图').click({ position: { x: 180, y: 180 } });
  await page.getByRole('button', { name: '添加到路线' }).click();
  await expect(page.getByLabel('路线点数')).toHaveText('3 个点');
  await expect(page.getByRole('button', { name: '完成路线' })).toBeInViewport();
  await page.screenshot({ path: '../build/webui-route-map.png' });
  await page.goBack();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await page.evaluate(() => localStorage.getItem('justlocation.route'))).toBe(draft);
  await expect(page.getByLabel('速度（km/h）')).toHaveValue('5.4');
});
