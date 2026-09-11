import { test, expect } from '@playwright/test';

test('feature menus fit small screens and stop color follows confirmed state', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.addInitScript(() => {
    const host = window as any;
    let state = { requested_active: false, location_hook_ready: true, config: {
      position: { latitude: 31.2, longitude: 121.5, altitude: 12, accuracy: 5, speed: 0, bearing: 0 },
      scope: { mode: 'apps', packages: ['me.idk.justlocation.companion'] },
    } };
    host.commands = [];
    host.ksu = { exec: (command: string, _options: string, callback: string) => {
      host.commands.push(command);
      if (command.startsWith('pm path')) return host[callback](0, 'package:/data/app/companion.apk', '');
      if (command.startsWith('am start')) return host[callback](0, 'Starting: Intent', '');
      if (command.startsWith('am stopservice')) return host[callback](1, 'Service stopped', '');
      const frame = JSON.parse(atob(command.split(' ').at(-1)!));
      const reply = () => host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
      if (frame.op === 'start') {
        host.confirmStart = () => { state = { ...state, requested_active: true, config: frame.config }; reply(); };
      } else { if (frame.op === 'stop') state.requested_active = false; reply(); }
    } };
  });
  await page.goto('/');
  const primary = page.locator('.target-actions > .primary');
  const initialColor = await primary.evaluate(element => getComputedStyle(element).backgroundColor);
  await primary.click();
  await expect(primary).toHaveText('处理中…');
  await expect(primary).not.toHaveClass(/stop/);
  await page.evaluate(() => (window as any).confirmStart());
  await expect(primary).toHaveText('停止模拟');
  expect(await primary.evaluate(element => getComputedStyle(element).backgroundColor)).not.toBe(initialColor);
  for (const theme of ['light', 'dark']) {
    await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
    for (const name of ['独立模拟', '基站', '摇杆']) {
      await page.getByRole('button', { name: `${name}菜单` }).click();
      const popup = page.getByRole('dialog', { name, exact: true });
      await expect(popup).toBeVisible();
      const bounds = (await popup.boundingBox())!;
      expect(bounds.x).toBeGreaterThanOrEqual(0);
      expect(bounds.x + bounds.width).toBeLessThanOrEqual(320);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.screenshot({ path: `../build/webui-menu-${name}-${theme}.png`, fullPage: true });
      await page.keyboard.press('Escape');
      await expect(popup).toHaveCount(0);
    }
  }
  await page.getByRole('button', { name: '摇杆菜单' }).click();
  await page.getByLabel('最高速度（km/h）').fill('7.2');
  await page.getByRole('button', { name: '打开摇杆', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已请求打开摇杆');
  expect(await page.evaluate(() => (window as any).commands.some((command: string) => command.includes('--es speed 7.2 --ez open true')))).toBe(true);
  await page.getByRole('button', { name: '关闭摇杆', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('摇杆已关闭');
  await primary.click();
  await expect(primary).toHaveText('开始模拟');
  await expect(primary).not.toHaveClass(/stop/);
});

test('GPX import previews separate segments and keeps the draft after reload', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.goto('/');
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await page.locator('input[type=file]').setInputFiles({ name: 'walk.gpx', mimeType: 'application/gpx+xml',
    buffer: Buffer.from('<gpx><trk><name>散步</name><trkseg><trkpt lat="31.2" lon="121.5"/><trkpt lat="31.201" lon="121.5"/></trkseg><trkseg><trkpt lat="30" lon="120"/><trkpt lat="30.001" lon="120"/></trkseg></trk></gpx>') });
  await expect(page.getByLabel('选择路线')).toBeVisible();
  await page.getByLabel('选择路线').selectOption('1');
  await page.screenshot({ path: '../build/webui-gpx-preview.png', fullPage: true, animations: 'disabled' });
  await page.getByRole('button', { name: '使用这条路线' }).click();
  await expect(page.getByLabel('点 1 纬度', { exact: true })).toHaveValue('30');
  await page.reload();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await expect(page.getByLabel('点 2 纬度', { exact: true })).toHaveValue('30.001');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test('mobile route editor reorders points, validates input and restores a running route after reload', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.addInitScript(() => {
    const host = window as any;
    let state = JSON.parse(sessionStorage.getItem('test.route.state') || 'null') || { requested_active: false, config: {
      position: { latitude: 31.2, longitude: 121.5, altitude: 12, accuracy: 5, speed: 0, bearing: 0 },
      scope: { mode: 'apps', packages: ['me.idk.justlocation.companion'] },
    } };
    host.ksu = { exec: (command: string, _options: string, callback: string) => {
      const frame = JSON.parse(atob(command.split(' ').at(-1)!));
      if (frame.op === 'start_route') state = { ...state, requested_active: true,
        config: { position: frame.route.points[0], scope: frame.scope },
        route: { plan: frame.route, distance: 111.2, total_distance: 111.2, paused: false, completed: false, lap: 1, waiting_seconds: 9.2 } };
      if (frame.op === 'pause_route') state.route.paused = true;
      if (frame.op === 'resume_route') state.route.paused = false;
      if (frame.op === 'stop') state = { ...state, requested_active: false, route: null };
      sessionStorage.setItem('test.route.state', JSON.stringify(state));
      host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
    } };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await page.getByRole('button', { name: '开始路线' }).click();
  await expect(page.getByRole('alert')).toContainText('点 1');
  for (const index of [1, 2]) {
    await page.getByLabel(`点 ${index} 纬度`, { exact: true }).fill('31.2');
    await page.getByLabel(`点 ${index} 经度`, { exact: true }).fill(index === 1 ? '121.5' : '121.501');
  }
  await page.getByRole('button', { name: '下移点 1', exact: true }).click();
  await expect(page.getByLabel('点 1 经度', { exact: true })).toHaveValue('121.501');
  await page.getByLabel('播放次数').fill('3');
  await page.getByLabel('每次间隔（秒）').fill('10');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: '../build/webui-route-editor.png', fullPage: true });
  await page.getByRole('button', { name: '开始路线' }).click();
  await expect(page.getByRole('button', { name: '暂停路线' })).toBeVisible();
  await page.reload();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await expect(page.getByLabel('点 1 经度', { exact: true })).toHaveValue('121.501');
  await expect(page.getByLabel('点 1 经度', { exact: true })).toBeDisabled();
  await expect(page.getByLabel('播放次数')).toHaveValue('3');
  await expect(page.getByRole('heading', { name: '等待下一轮' })).toBeVisible();
  await expect(page.getByText('10 秒后回到起点')).toBeVisible();
  await page.getByRole('button', { name: '暂停路线' }).click();
  await expect(page.getByRole('heading', { name: '路线已暂停' })).toBeVisible();
  await expect(page.getByText('剩余间隔 10 秒')).toBeVisible();
  await page.screenshot({ path: '../build/webui-route-paused.png', fullPage: true });
  await page.getByRole('button', { name: '继续路线' }).click();
  await page.getByRole('button', { name: '停止路线' }).click();
  await expect(page.getByLabel('点 1 经度', { exact: true })).toBeEnabled();
});

test('mobile app picker uses KernelSU metadata and submits selected packages', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.addInitScript(() => {
    const host = window as unknown as Record<string, any>;
    const apps = [
      { packageName: 'me.idk.justlocation.companion', appLabel: 'JustLocation', isSystem: false },
      { packageName: 'com.android.settings', appLabel: '设置', isSystem: true },
      ...Array.from({ length: 50 }, (_, index) => ({ packageName: `example.app${index}`, appLabel: `应用 ${index}`, isSystem: false })),
    ];
    let state = { requested_active: false, config: {
      position: { latitude: 31.2, longitude: 121.5, altitude: 12, accuracy: 5, speed: 0, bearing: 0 },
      scope: { mode: 'apps', packages: [] as string[] },
    } };
    host.ksu = {
      listPackages: () => JSON.stringify(apps.map(app => app.packageName)),
      getPackagesInfo: () => JSON.stringify(apps),
      exec: (command: string, _options: string, callback: string) => {
        const frame = JSON.parse(atob(command.split(' ').at(-1)!));
        if (frame.op === 'start') { state = { requested_active: true, config: frame.config }; host.startedScope = frame.config.scope; }
        host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
      },
    };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '独立模拟菜单' }).click();
  await page.getByRole('button', { name: '作用范围', exact: true }).click();
  await expect(page.getByRole('navigation')).toHaveCount(0);
  const topbar = page.locator('.scope-topbar');
  const top = (await topbar.boundingBox())!.y;
  await page.locator('.app-list').evaluate(element => { element.scrollTop = element.scrollHeight; });
  expect((await topbar.boundingBox())!.y).toBe(top);
  expect(await page.evaluate(() => document.documentElement.scrollHeight <= innerHeight)).toBe(true);
  await page.getByRole('searchbox').fill('justlocation');
  await page.getByRole('checkbox', { name: /JustLocation/ }).check();
  await page.getByRole('radio', { name: '全部应用', exact: true }).check();
  await page.getByRole('radio', { name: '指定应用', exact: true }).check();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeChecked();
  await page.screenshot({ path: '../build/webui-app-picker.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await expect(page.getByRole('heading', { name: '位置模拟', exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByRole('button', { name: '独立模拟菜单' })).toHaveClass(/selected/);
  await page.getByRole('button', { name: '打开导航' }).click();
  await expect(page.getByRole('button', { name: '作用范围', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await page.getByRole('button', { name: '作用范围', exact: true }).click();
  await page.goBack();
  await expect(page.getByRole('heading', { name: '路线模拟', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '作用范围', exact: true }).click();
  await page.getByRole('button', { name: '完成', exact: true }).click();
  await expect(page.getByRole('heading', { name: '路线模拟', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '位置模拟', exact: true }).click();
  await page.getByRole('button', { name: '开始模拟' }).click();
  await expect(page.getByRole('button', { name: '停止模拟' })).toBeVisible();
  expect(await page.evaluate(() => (window as any).startedScope)).toEqual({ mode: 'apps', packages: ['me.idk.justlocation.companion'] });
  await page.getByRole('button', { name: '独立模拟菜单' }).click();
  await page.getByRole('button', { name: '作用范围', exact: true }).click();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeDisabled();
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

