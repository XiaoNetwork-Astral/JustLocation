import { test, expect, type Locator, type Page } from '@playwright/test';

/* Toggle the native checkbox with the keyboard, as a keyboard user would. */
async function toggleSwitch(page: Page, box: Locator) {
  await box.focus();
  await page.keyboard.press('Space');
}

test('feature switches fit small screens and stop color follows confirmed state', async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.addInitScript(() => {
    const host = window as any;
    let state = {
      requested_active: false,
      location_hook_ready: true,
      config: {
        position: {
          latitude: 31.2,
          longitude: 121.5,
          altitude: 12,
          accuracy: 5,
          speed: 0,
          bearing: 0,
        },
        scope: { mode: 'apps', packages: ['me.idk.justlocation.probe'] },
      },
    };
    host.commands = [];
    host.ksu = {
      exec: (command: string, _options: string, callback: string) => {
        host.commands.push(command);
        if (command.startsWith('pm path'))
          return host[callback](0, 'package:/data/app/companion.apk', '');
        if (command.startsWith('am start')) return host[callback](0, 'Starting: Intent', '');
        if (command.startsWith('am stopservice')) return host[callback](1, 'Service stopped', '');
        const frame = JSON.parse(atob(command.split(' ').at(-1)!));
        const reply = () => host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
        if (frame.op === 'start') {
          host.confirmStart = () => {
            state = { ...state, requested_active: true, config: frame.config };
            reply();
          };
        } else {
          if (frame.op === 'stop') state.requested_active = false;
          reply();
        }
      },
    };
  });
  await page.goto('/');
  const primary = page.locator('.target-actions > .primary');
  const initialColor = await primary.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  await primary.click();
  await expect(primary).toHaveText('处理中…');
  await expect(primary).not.toHaveClass(/stop/);
  await page.evaluate(() => (window as any).confirmStart());
  await expect(primary).toHaveText('停止模拟');
  expect(await primary.evaluate((element) => getComputedStyle(element).backgroundColor)).not.toBe(
    initialColor,
  );

  for (const theme of ['light', 'dark']) {
    await page.evaluate((theme) => (document.documentElement.dataset.theme = theme), theme);
    for (const selector of ['.switch-row:has(input[aria-label="摇杆"])', '.anchor-entry', '.fab']) {
      const row = page.locator(selector).first();
      await expect(row).toBeVisible();
      const bounds = (await row.boundingBox())!;
      expect(bounds.x).toBeGreaterThanOrEqual(0);
      expect(bounds.x + bounds.width).toBeLessThanOrEqual(320);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
        true,
      );
    }
    await page.screenshot({ path: `../build/webui-feature-switches-${theme}.png`, fullPage: true });
  }

  const joystick = page.getByRole('checkbox', { name: '摇杆' });
  await toggleSwitch(page, joystick);
  await expect(page.getByText('摇杆已打开，拖动即可移动位置。')).toBeVisible();
  expect(
    await page.evaluate(() =>
      (window as any).commands.some(
        (command: string) =>
          command.includes('am start') &&
          command.includes('CallActivity') &&
          command.includes('--ef speed 1.5'),
      ),
    ),
  ).toBe(true);
  await page.getByLabel('最高速度（km/h）').fill('7.2');
  await toggleSwitch(page, joystick);
  await expect(page.getByText('摇杆已关闭，位置模拟会保持当前状态。')).toBeVisible();
  await toggleSwitch(page, joystick);
  await expect(page.getByText('摇杆已打开，拖动即可移动位置。')).toBeVisible();
  expect(
    await page.evaluate(() =>
      (window as any).commands.some((command: string) => command.includes('--ef speed 2')),
    ),
  ).toBe(true);
  await primary.click();
  await expect(primary).toHaveText('开始模拟');
  await expect(primary).not.toHaveClass(/stop/);
});

test('GPX import previews separate segments and keeps the draft after reload', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.addInitScript(() => {
    const host = window as any;
    const state = {
      requested_active: false,
      location_hook_ready: true,
      config: {
        position: {
          latitude: 31.2,
          longitude: 121.5,
          altitude: 12,
          accuracy: 5,
          speed: 0,
          bearing: 0,
        },
        scope: { mode: 'all' },
      },
    };
    host.ksu = {
      exec: (command: string, _options: string, callback: string) => {
        host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
      },
    };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();

  await page.getByRole('button', { name: '新建路线' }).click();
  await page.locator('input[type=file]').setInputFiles({
    name: 'walk.gpx',
    mimeType: 'application/gpx+xml',
    buffer: Buffer.from(
      '<gpx><trk><name>散步</name><trkseg><trkpt lat="31.2" lon="121.5"/><trkpt lat="31.201" lon="121.5"/></trkseg><trkseg><trkpt lat="30" lon="120"/><trkpt lat="30.001" lon="120"/></trkseg></trk></gpx>',
    ),
  });
  await expect(page.getByLabel('选择路线')).toBeVisible();
  await page.getByLabel('选择路线').selectOption('1');
  await page.screenshot({
    path: '../build/webui-gpx-preview.png',
    fullPage: true,
    animations: 'disabled',
  });
  await page.getByRole('button', { name: '使用这条路线' }).click();
  await expect(page.getByLabel('点 1 纬度', { exact: true })).toHaveValue('30');
  await page.reload();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();

  await page.getByRole('button', { name: '保存路线' }).click();
  await page.getByLabel('路线名称').fill('导入的路线');
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await page.getByRole('button', { name: '编辑导入的路线' }).click();
  await expect(page.getByLabel('点 2 纬度', { exact: true })).toHaveValue('30.001');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('mobile route editor reorders points, validates input and restores a running route after reload', async ({
  page,
}) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.addInitScript(() => {
    const host = window as any;
    let state = JSON.parse(sessionStorage.getItem('test.route.state') || 'null') || {
      requested_active: false,
      config: {
        position: {
          latitude: 31.2,
          longitude: 121.5,
          altitude: 12,
          accuracy: 5,
          speed: 0,
          bearing: 0,
        },
        scope: { mode: 'apps', packages: ['me.idk.justlocation.probe'] },
      },
    };
    host.ksu = {
      exec: (command: string, _options: string, callback: string) => {
        const frame = JSON.parse(atob(command.split(' ').at(-1)!));
        if (frame.op === 'start_route')
          state = {
            ...state,
            requested_active: true,
            config: { position: frame.route.points[0], scope: frame.scope },
            route: {
              plan: frame.route,
              distance: 111.2,
              total_distance: 111.2,
              paused: false,
              completed: false,
              lap: 1,
              waiting_seconds: 9.2,
            },
          };
        if (frame.op === 'pause_route') state.route.paused = true;
        if (frame.op === 'resume_route') state.route.paused = false;
        if (frame.op === 'stop') state = { ...state, requested_active: false, route: null };
        sessionStorage.setItem('test.route.state', JSON.stringify(state));
        host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
      },
    };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();

  await page.getByRole('button', { name: '新建路线' }).click();
  await page.getByRole('button', { name: '开始路线' }).click();
  await expect(page.getByRole('alert')).toContainText('点 1');
  for (const index of [1, 2]) {
    await page.getByLabel(`点 ${index} 纬度`, { exact: true }).fill('31.2');
    await page
      .getByLabel(`点 ${index} 经度`, { exact: true })
      .fill(index === 1 ? '121.5' : '121.501');
  }
  await page.getByRole('button', { name: '下移点 1', exact: true }).click();
  await expect(page.getByLabel('点 1 经度', { exact: true })).toHaveValue('121.501');
  await page.getByLabel('播放次数').fill('3');
  await page.getByLabel('每次间隔（秒）').fill('10');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: '../build/webui-route-editor.png', fullPage: true });
  await page.getByRole('button', { name: '开始路线' }).click();

  await expect(page.getByLabel('点 1 经度', { exact: true })).toBeDisabled();
  await expect(page.getByLabel('播放次数')).toHaveValue('3');
  await page.getByRole('button', { name: '返回路线管理' }).click();
  await expect(page.getByRole('button', { name: '暂停路线' })).toBeVisible();
  await page.reload();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();

  await expect(page.getByRole('heading', { name: '等待下一轮' })).toBeVisible();
  await expect(page.getByText('10 秒后回到起点')).toBeVisible();
  await expect(page.getByText(/第 1 \/ 3 次/)).toBeVisible();
  await page.getByRole('button', { name: '暂停路线' }).click();
  await expect(page.getByRole('heading', { name: '路线已暂停' })).toBeVisible();
  await expect(page.getByText('剩余间隔 10 秒')).toBeVisible();
  await page.screenshot({ path: '../build/webui-route-paused.png', fullPage: true });
  await page.getByRole('button', { name: '继续路线' }).click();
  await page.getByRole('button', { name: '停止路线' }).click();
  await page.getByRole('button', { name: '新建路线' }).click();
  await expect(page.getByLabel('点 1 经度', { exact: true })).toBeEnabled();
});

test('mobile app picker uses KernelSU metadata and submits selected packages', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.addInitScript(() => {
    const host = window as unknown as Record<string, any>;
    const apps = [
      { packageName: 'me.idk.justlocation.probe', appLabel: 'JustLocation', isSystem: false },
      { packageName: 'com.android.settings', appLabel: '设置', isSystem: true },
      ...Array.from({ length: 50 }, (_, index) => ({
        packageName: `example.app${index}`,
        appLabel: `应用 ${index}`,
        isSystem: false,
      })),
    ];
    let state = {
      requested_active: false,
      config: {
        position: {
          latitude: 31.2,
          longitude: 121.5,
          altitude: 12,
          accuracy: 5,
          speed: 0,
          bearing: 0,
        },
        scope: { mode: 'apps', packages: [] as string[] },
      },
    };
    host.ksu = {
      listPackages: () => JSON.stringify(apps.map((app) => app.packageName)),
      getPackagesInfo: () => JSON.stringify(apps),
      exec: (command: string, _options: string, callback: string) => {
        const frame = JSON.parse(atob(command.split(' ').at(-1)!));
        if (frame.op === 'start') {
          state = { requested_active: true, config: frame.config };
          host.startedScope = frame.config.scope;
        }
        host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
      },
    };
  });
  await page.goto('/');

  await expect(page.getByRole('button', { name: '作用范围' })).toBeVisible();
  await page.getByRole('button', { name: '作用范围' }).click();
  await expect(page.getByRole('navigation')).toHaveCount(0);
  const topbar = page.locator('.scope-topbar');
  const top = (await topbar.boundingBox())!.y;
  await page.locator('.app-list').evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  expect((await topbar.boundingBox())!.y).toBe(top);
  expect(await page.evaluate(() => document.documentElement.scrollHeight <= innerHeight)).toBe(
    true,
  );
  await page.getByRole('searchbox').fill('justlocation');
  await page.getByRole('checkbox', { name: /JustLocation/ }).check();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeChecked();
  await page.screenshot({ path: '../build/webui-app-picker.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await expect(page.getByRole('heading', { name: '位置模拟', exact: true })).toBeVisible();

  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('justlocation.scope')!)),
  ).toEqual({ mode: 'apps', packages: ['me.idk.justlocation.probe'] });
  await page.getByRole('button', { name: '作用范围（已选 1 个）' }).click();
  await page.getByRole('button', { name: '改为全部应用' }).click();
  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('justlocation.scope')!)),
  ).toEqual({ mode: 'all', packages: ['me.idk.justlocation.probe'] });
  await page.getByRole('button', { name: '完成', exact: true }).click();
  await expect(page.getByRole('button', { name: '作用范围' })).toBeVisible();
  await page.reload();

  await expect(page.getByRole('button', { name: '作用范围' })).toBeVisible();
  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('justlocation.scope')!)),
  ).toEqual({ mode: 'all', packages: ['me.idk.justlocation.probe'] });
  await page.getByRole('button', { name: '打开导航' }).click();

  await expect(page.getByRole('navigation').getByRole('button')).toHaveText([
    '位置模拟',
    '路线模拟',
    'Wi-Fi 模拟',
    '设置',
  ]);
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();

  await page.getByRole('button', { name: '作用范围', exact: true }).click();
  await page.getByRole('button', { name: '完成', exact: true }).click();
  await expect(page.getByRole('heading', { name: '路线模拟', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '位置模拟', exact: true }).click();

  await page.getByRole('button', { name: '作用范围' }).click();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeChecked();
  await page.getByRole('button', { name: '完成', exact: true }).click();
  await page.getByRole('button', { name: '开始模拟' }).click();
  await expect(page.getByRole('button', { name: '停止模拟' })).toBeVisible();
  expect(await page.evaluate(() => (window as any).startedScope)).toEqual({
    mode: 'apps',
    packages: ['me.idk.justlocation.probe'],
  });

  await page.getByRole('button', { name: '作用范围（已选 1 个）' }).click();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeDisabled();
});

test('scope page drops the all/apps choice and follows the feature switch', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.addInitScript(() => {
    const host = window as unknown as Record<string, any>;
    const apps = [
      { packageName: 'me.idk.justlocation.probe', appLabel: 'JustLocation', isSystem: false },
      { packageName: 'com.example.maps', appLabel: '地图', isSystem: false },
    ];
    const state = {
      requested_active: false,
      location_hook_ready: true,
      config: {
        position: {
          latitude: 31.2,
          longitude: 121.5,
          altitude: 12,
          accuracy: 5,
          speed: 0,
          bearing: 0,
        },
        scope: { mode: 'apps', packages: ['me.idk.justlocation.probe'] },
      },
    };
    host.ksu = {
      listPackages: () => JSON.stringify(apps.map((app) => app.packageName)),
      getPackagesInfo: () => JSON.stringify(apps),
      exec: (command: string, _options: string, callback: string) => {
        host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
      },
    };
  });
  await page.goto('/');

  await expect(page.getByRole('button', { name: '作用范围（已选 1 个）' })).toBeVisible();
  await expect(page.getByRole('radio')).toHaveCount(0);
  await page.getByRole('button', { name: '作用范围（已选 1 个）' }).click();
  await expect(page.getByRole('heading', { name: '作用范围', exact: true })).toBeVisible();

  await expect(page.getByRole('radio')).toHaveCount(0);
  await expect(page.getByText('全部应用', { exact: true })).toHaveCount(0);
  await expect(page.getByText('指定应用', { exact: true })).toHaveCount(0);
  await expect(
    page.getByText('只有勾选的应用会使用模拟位置，其他应用保持真实定位。'),
  ).toBeVisible();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeChecked();
  await page.screenshot({ path: '../build/webui-scope-apps.png', fullPage: true });
  await page.getByRole('button', { name: '完成', exact: true }).click();
  await expect(page.getByRole('heading', { name: '位置模拟', exact: true })).toBeVisible();

  await page.getByRole('button', { name: '作用范围（已选 1 个）' }).click();
  await page.getByRole('button', { name: '改为全部应用' }).click();
  expect(
    await page.evaluate(() => JSON.parse(localStorage.getItem('justlocation.scope')!)),
  ).toEqual({ mode: 'all', packages: ['me.idk.justlocation.probe'] });
  await expect(page.getByText('所有应用都会使用模拟位置。之前勾选的应用已保留。')).toBeVisible();
  await page.getByRole('button', { name: '完成', exact: true }).click();
  await expect(page.getByRole('button', { name: '作用范围' })).toBeVisible();

  await page.getByRole('button', { name: '作用范围' }).click();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeChecked();
  await expect(page.getByRole('searchbox')).toHaveCount(1);
  await expect(page.getByRole('radio')).toHaveCount(0);
  await page.screenshot({ path: '../build/webui-scope-all-apps.png', fullPage: true });
});

test('imports a map link and plain coordinates on a narrow screen', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.goto('/');
  await page.getByRole('button', { name: '关闭提示' }).click();

  await page.getByRole('button', { name: '导入位置' }).click();
  await page.getByLabel('粘贴地图链接或经纬度').fill('31.2304, 121.4737');
  await expect(page.getByText(/按纬度在前、经度在后读入/)).toBeVisible();
  await expect(page.getByText(/将保存为 WGS84：31\.230400, 121\.473700/)).toBeVisible();
  await page.getByRole('button', { name: '导入到历史位置' }).click();
  await expect(page.getByText('31.230400, 121.473700')).toBeVisible();

  await page.getByRole('button', { name: '导入位置' }).click();
  await page
    .getByLabel('粘贴地图链接或经纬度')
    .fill('https://uri.amap.com/marker?position=116.404,39.915');
  await expect(page.getByLabel('坐标类型')).toHaveValue('gcj02');
  await expect(page.getByText(/将保存为 WGS84：39\.91/)).toBeVisible();
  await page.screenshot({ path: '../build/webui-import.png', fullPage: true });
  await page.getByRole('button', { name: '导入到历史位置' }).click();
  await expect(page.getByText('来自高德地图')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('mobile coordinate entry, navigation and dark theme', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.goto('/');
  await expect(page.getByRole('alert')).toContainText('请从 KernelSU');
  await page.getByRole('button', { name: '关闭提示' }).click();
  await page.screenshot({ path: '../build/webui-mobile.png', fullPage: true });
  await page.getByRole('button', { name: '添加位置', exact: true }).click();
  await page.getByLabel('位置名称').fill('江边散步');
  await page.getByLabel('纬度').fill('31.2304');
  await page.getByLabel('经度').fill('121.4737');
  await page.screenshot({ path: '../build/webui-editor.png', fullPage: true });
  await page.getByRole('button', { name: '保存位置' }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await expect(page.getByRole('button', { name: '开始模拟' })).toBeDisabled();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('button', { name: '深色', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '位置模拟', exact: true }).click();
  await page.screenshot({ path: '../build/webui-dark.png', fullPage: true });
  await page.setViewportSize({ width: 1360, height: 960 });
  await expect(page.getByRole('navigation')).toBeVisible();
  await page.screenshot({ path: '../build/webui-desktop.png', fullPage: true });
});
