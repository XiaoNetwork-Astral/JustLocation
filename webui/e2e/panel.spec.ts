import { test, expect, type Locator, type Page } from '@playwright/test';

/**
 * 功能开关是隐藏的原生 checkbox（1×1、opacity:0、pointer-events:none，行和可视开关都没有点击处理），
 * 鼠标/触摸点不到它，只能像键盘用户那样聚焦后按空格切换。
 */
async function toggleSwitch(page: Page, box: Locator) {
  await box.focus();
  await page.keyboard.press('Space');
}

test('feature switches fit small screens and stop color follows confirmed state', async ({ page }) => {
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
  // 作用范围与基站是"模拟功能"卡片里的整行开关；摇杆已按原版移进目标卡的操作行
  // （见 TargetCard），所以单独断言它那一行不溢出。
  for (const theme of ['light', 'dark']) {
    await page.evaluate(theme => document.documentElement.dataset.theme = theme, theme);
    for (const name of ['作用范围', '基站模拟', '摇杆']) {
      const row = page.locator(`.switch-row:has(input[aria-label="${name}"])`);
      await expect(row).toBeVisible();
      const bounds = (await row.boundingBox())!;
      expect(bounds.x).toBeGreaterThanOrEqual(0);
      expect(bounds.x + bounds.width).toBeLessThanOrEqual(320);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    }
    await page.screenshot({ path: `../build/webui-feature-switches-${theme}.png`, fullPage: true });
  }
  // 摇杆开关：打开会真的发出 am start 命令，关掉后位置模拟继续。
  const joystick = page.getByRole('checkbox', { name: '摇杆' });
  await toggleSwitch(page, joystick);
  await expect(page.getByText('摇杆已打开，拖动即可移动位置。')).toBeVisible();
  expect(await page.evaluate(() => (window as any).commands.some((command: string) => command.includes('--es speed 5.4 --ez open true')))).toBe(true);
  await page.getByLabel('最高速度（km/h）').fill('7.2');
  await toggleSwitch(page, joystick);
  await expect(page.getByText('摇杆已关闭，位置模拟会保持当前状态。')).toBeVisible();
  await toggleSwitch(page, joystick);
  await expect(page.getByText('摇杆已打开，拖动即可移动位置。')).toBeVisible();
  expect(await page.evaluate(() => (window as any).commands.some((command: string) => command.includes('--es speed 7.2 --ez open true')))).toBe(true);
  await primary.click();
  await expect(primary).toHaveText('开始模拟');
  await expect(primary).not.toHaveClass(/stop/);
});

test('GPX import previews separate segments and keeps the draft after reload', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.addInitScript(() => {
    const host = window as any;
    const state = { requested_active: false, location_hook_ready: true, config: {
      position: { latitude: 31.2, longitude: 121.5, altitude: 12, accuracy: 5, speed: 0, bearing: 0 },
      scope: { mode: 'all' },
    } };
    host.ksu = { exec: (command: string, _options: string, callback: string) => {
      host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
    } };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  // 路线页默认是“路线管理”，导入 GPX 要先新建一条路线进入点编辑。
  await page.getByRole('button', { name: '新建路线' }).click();
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
  // 刷新后回到路线管理。“新建路线”会主动丢弃草稿，所以用“保存路线”把恢复出来的草稿存进路线库，
  // 再打开它，确认草稿里的点是导入的那一段而不是空草稿。
  await page.getByRole('button', { name: '保存路线' }).click();
  await page.getByLabel('路线名称').fill('导入的路线');
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await page.getByRole('button', { name: '编辑导入的路线' }).click();
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
  // 点编辑是路线管理里的第二个页面，先新建路线才有点位输入。
  await page.getByRole('button', { name: '新建路线' }).click();
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
  // 路线播放中：点编辑被锁住（刷新后会回到管理页，所以在这里验证锁定）。
  await expect(page.getByLabel('点 1 经度', { exact: true })).toBeDisabled();
  await expect(page.getByLabel('播放次数')).toHaveValue('3');
  await page.getByRole('button', { name: '返回路线管理' }).click();
  await expect(page.getByRole('button', { name: '暂停路线' })).toBeVisible();
  await page.reload();
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  // 刷新后从后台恢复的运行中路线：进度、轮次、等待间隔都在管理页上。
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
  // 作用范围不再是弹层：开关打开时行内出现“选择应用”。
  await expect(page.getByRole('checkbox', { name: '作用范围' })).toBeChecked();
  await page.getByRole('button', { name: '选择应用' }).click();
  await expect(page.getByRole('navigation')).toHaveCount(0);
  const topbar = page.locator('.scope-topbar');
  const top = (await topbar.boundingBox())!.y;
  await page.locator('.app-list').evaluate(element => { element.scrollTop = element.scrollHeight; });
  expect((await topbar.boundingBox())!.y).toBe(top);
  expect(await page.evaluate(() => document.documentElement.scrollHeight <= innerHeight)).toBe(true);
  await page.getByRole('searchbox').fill('justlocation');
  await page.getByRole('checkbox', { name: /JustLocation/ }).check();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeChecked();
  await page.screenshot({ path: '../build/webui-app-picker.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await expect(page.getByRole('heading', { name: '位置模拟', exact: true })).toBeVisible();
  // 模式只能由首页开关切换：关掉再打开必须保留已选应用（原来那两个单选就是干这个的）。
  const scopeSwitch = page.getByRole('checkbox', { name: '作用范围' });
  await toggleSwitch(page, scopeSwitch);
  await expect(scopeSwitch).not.toBeChecked();
  await expect(page.getByText('所有应用都使用模拟位置')).toBeVisible();
  await expect(page.getByRole('button', { name: '选择应用' })).toHaveCount(0);
  await toggleSwitch(page, scopeSwitch);
  await expect(scopeSwitch).toBeChecked();
  await expect(page.getByText('仅在这些应用中生效 · 已选 1 个')).toBeVisible();
  await page.getByRole('button', { name: '选择应用' }).click();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeChecked();
  await page.getByRole('button', { name: '完成', exact: true }).click();
  await expect(page.getByRole('heading', { name: '位置模拟', exact: true })).toBeVisible();
  await page.reload();
  // 刷新后仍是限定应用模式（原来是断言“独立模拟菜单”处于 selected 状态）。
  await expect(page.getByRole('checkbox', { name: '作用范围' })).toBeChecked();
  await page.getByRole('button', { name: '打开导航' }).click();
  // 侧栏里仍然没有“作用范围”入口：导航只有这四个页面。
  await expect(page.getByRole('navigation').getByRole('button')).toHaveText(['位置模拟', '路线模拟', 'Wi-Fi 模拟', '设置']);
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
  // 模拟进行中不能再改应用选择，作用范围页从路线页进入。
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await page.getByRole('button', { name: '作用范围', exact: true }).click();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeDisabled();
});

test('scope page drops the all/apps choice and follows the feature switch', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.addInitScript(() => {
    const host = window as unknown as Record<string, any>;
    const apps = [
      { packageName: 'me.idk.justlocation.companion', appLabel: 'JustLocation', isSystem: false },
      { packageName: 'com.example.maps', appLabel: '地图', isSystem: false },
    ];
    const state = { requested_active: false, location_hook_ready: true, config: {
      position: { latitude: 31.2, longitude: 121.5, altitude: 12, accuracy: 5, speed: 0, bearing: 0 },
      scope: { mode: 'apps', packages: ['me.idk.justlocation.companion'] },
    } };
    host.ksu = {
      listPackages: () => JSON.stringify(apps.map(app => app.packageName)),
      getPackagesInfo: () => JSON.stringify(apps),
      exec: (command: string, _options: string, callback: string) => {
        host[callback](0, JSON.stringify({ version: 1, ok: true, state }), '');
      },
    };
  });
  await page.goto('/');
  // 模式就是首页那个开关，首页上也没有“全部应用 / 指定应用”这类单选。
  await expect(page.getByRole('checkbox', { name: '作用范围' })).toBeChecked();
  await expect(page.getByRole('radio')).toHaveCount(0);
  await page.getByRole('button', { name: '选择应用' }).click();
  await expect(page.getByRole('heading', { name: '作用范围', exact: true })).toBeVisible();
  // 限定应用时只讲清范围并给出列表，不再要求先选模式。
  await expect(page.getByRole('radio')).toHaveCount(0);
  await expect(page.getByText('全部应用', { exact: true })).toHaveCount(0);
  await expect(page.getByText('指定应用', { exact: true })).toHaveCount(0);
  await expect(page.getByText('只有勾选的应用会使用模拟位置，其他应用保持真实定位。')).toBeVisible();
  await expect(page.getByRole('checkbox', { name: /JustLocation/ })).toBeChecked();
  await page.screenshot({ path: '../build/webui-scope-apps.png', fullPage: true });
  await page.getByRole('button', { name: '完成', exact: true }).click();
  await expect(page.getByRole('heading', { name: '位置模拟', exact: true })).toBeVisible();
  // 关掉开关就是全部应用：行内不再有应用入口，页面只说明勾选被保留。
  await toggleSwitch(page, page.getByRole('checkbox', { name: '作用范围' }));
  await expect(page.getByRole('checkbox', { name: '作用范围' })).not.toBeChecked();
  await expect(page.getByRole('button', { name: '选择应用' })).toHaveCount(0);
  await page.getByRole('button', { name: '打开导航' }).click();
  await page.getByRole('button', { name: '路线模拟', exact: true }).click();
  await page.getByRole('button', { name: '作用范围', exact: true }).click();
  await expect(page.getByText('所有应用都会使用模拟位置。之前勾选的应用已保留。')).toBeVisible();
  await expect(page.getByRole('radio')).toHaveCount(0);
  await expect(page.getByRole('searchbox')).toHaveCount(0);
  await page.screenshot({ path: '../build/webui-scope-all-apps.png', fullPage: true });
});

test('imports a map link and plain coordinates on a narrow screen', async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.goto('/');
  await page.getByRole('button', { name: '关闭提示' }).click();
  // 直接粘贴经纬度。
  await page.getByRole('button', { name: '导入位置' }).click();
  await page.getByLabel('粘贴地图链接或经纬度').fill('31.2304, 121.4737');
  await expect(page.getByText(/按纬度在前、经度在后读入/)).toBeVisible();
  await expect(page.getByText(/将保存为 WGS84：31\.230400, 121\.473700/)).toBeVisible();
  await page.getByRole('button', { name: '导入到历史位置' }).click();
  await expect(page.getByText('31.230400, 121.473700')).toBeVisible();
  // 再粘贴一个地图分享链接，坐标系要自动按来源设定。
  await page.getByRole('button', { name: '导入位置' }).click();
  await page.getByLabel('粘贴地图链接或经纬度').fill('https://uri.amap.com/marker?position=116.404,39.915');
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
