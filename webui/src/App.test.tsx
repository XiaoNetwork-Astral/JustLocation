// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { App } from './App';

afterEach(() => { cleanup(); localStorage.clear(); });

const position = { latitude: 1, longitude: 2, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };

it('starts location and opens the joystick from one switch, and closing it keeps the simulation', async () => {
  const config = { position, scope: { mode: 'all' as const } };
  const client = vi.fn().mockResolvedValueOnce({ requested_active: false, config })
    .mockResolvedValue({ requested_active: true, config });
  const joystick = { check: vi.fn().mockResolvedValue(undefined), open: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined), read: vi.fn().mockResolvedValue(false) };
  render(<App client={client} joystick={joystick} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  const toggle = await screen.findByRole('checkbox', { name: '摇杆' });
  expect(toggle).toHaveProperty('checked', false);
  await user.click(toggle);
  expect(client).toHaveBeenLastCalledWith({ op: 'start', config });
  expect(joystick.open).toHaveBeenCalledWith(5.4);
  expect(screen.getByRole('checkbox', { name: '摇杆' })).toHaveProperty('checked', true);
  await user.click(screen.getByRole('checkbox', { name: '摇杆' }));
  expect(joystick.close).toHaveBeenCalledOnce();
  expect(client).not.toHaveBeenCalledWith({ op: 'stop' });
  expect(screen.getByRole('checkbox', { name: '摇杆' })).toHaveProperty('checked', false);
});

it('closes the joystick together with the simulation', async () => {
  const config = { position, scope: { mode: 'all' as const } };
  const client = vi.fn().mockResolvedValue({ requested_active: true, config });
  const joystick = { check: vi.fn().mockResolvedValue(undefined), open: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined), read: vi.fn().mockResolvedValue(false) };
  render(<App client={client} joystick={joystick} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  // 模拟已在运行：打开摇杆不会再发一次 start。
  await user.click(await screen.findByRole('checkbox', { name: '摇杆' }));
  expect(joystick.open).toHaveBeenCalledOnce();
  await user.click(screen.getByRole('button', { name: '停止模拟' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'stop' });
  expect(joystick.close).toHaveBeenCalledOnce();
  expect(screen.getByRole('checkbox', { name: '摇杆' })).toHaveProperty('checked', false);
});

it('keeps the application scope and sends it on the next start', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false,
    config: { position, scope: { mode: 'apps', packages: ['example.selected'] } } });
  render(<App client={client} loadApps={async () => [{ packageName: 'example.selected', appLabel: '地图', isSystem: false }]} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  // 作用范围与基站在默认视图里就是开关，不再需要先展开菜单。
  expect(screen.getByRole('checkbox', { name: '作用范围' })).toHaveProperty('checked', true);
  expect(screen.getByRole('checkbox', { name: '基站模拟' })).toHaveProperty('checked', false);
  await user.click(screen.getByRole('button', { name: '选择应用' }));
  expect(await screen.findByRole('checkbox', { name: /地图/ })).toHaveProperty('checked', true);
  await user.click(screen.getByRole('button', { name: '完成' }));
  await user.click(screen.getByRole('button', { name: '开始模拟' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'start', config: { position,
    scope: { mode: 'apps', packages: ['example.selected'] } } });
});

it('switches the scope between all apps and the saved selection without losing it', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false,
    config: { position, scope: { mode: 'apps', packages: ['example.selected'] } } });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('checkbox', { name: '作用范围' }));
  expect(JSON.parse(localStorage.getItem('justlocation.scope')!)).toEqual({ mode: 'all', packages: ['example.selected'] });
  await user.click(screen.getByRole('checkbox', { name: '作用范围' }));
  expect(JSON.parse(localStorage.getItem('justlocation.scope')!).mode).toBe('apps');
  expect(JSON.parse(localStorage.getItem('justlocation.scope')!).packages).toEqual(['example.selected']);
});

it('refuses to limit the scope while no application is selected', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false,
    config: { position, scope: { mode: 'all' } } });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  expect(screen.getByRole('checkbox', { name: '作用范围' })).toHaveProperty('checked', false);
  await user.click(screen.getByRole('checkbox', { name: '作用范围' }));
  expect(await screen.findByRole('alert')).toHaveProperty('textContent', expect.stringContaining('请先选择要模拟的应用'));
  expect(JSON.parse(localStorage.getItem('justlocation.scope') || 'null')?.mode ?? 'all').toBe('all');
});

it('submits the checked apps as the start scope and blocks an empty selection', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false,
    config: { position, scope: { mode: 'apps', packages: ['example.old'] } } });
  render(<App client={client} loadApps={async () => [
    { packageName: 'example.old', appLabel: '旧应用', isSystem: false },
    { packageName: 'example.new', appLabel: '新应用', isSystem: false },
  ]} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '选择应用' }));
  await user.click(await screen.findByRole('checkbox', { name: /旧应用/ }));
  await user.click(screen.getByRole('button', { name: '完成' }));
  expect(screen.getByRole('button', { name: '开始模拟' })).toHaveProperty('disabled', true);
  await user.click(screen.getByRole('button', { name: '选择应用' }));
  await user.click(await screen.findByRole('checkbox', { name: /新应用/ }));
  await user.click(screen.getByRole('button', { name: '完成' }));
  await user.click(screen.getByRole('button', { name: '开始模拟' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'start', config: { position,
    scope: { mode: 'apps', packages: ['example.new'] } } });
});

it('renames a saved place without moving the simulation', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false, config: null });
  localStorage.setItem('justlocation.places', JSON.stringify([
    { id: 'p1', name: '公司', position, pinned: false },
  ]));
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '编辑 公司' }));
  const nameField = screen.getByLabelText('位置名称');
  await user.clear(nameField);
  await user.type(nameField, '新公司');
  await user.click(screen.getByRole('button', { name: '保存位置' }));
  await screen.findByText('新公司');
  expect(JSON.parse(localStorage.getItem('justlocation.places')!)[0].name).toBe('新公司');
  expect(client).toHaveBeenCalledTimes(1);
});

it('starts and stops a route from the route manager', async () => {
  const scope = { mode: 'apps' as const, packages: ['example.selected'] };
  const points = [position, { ...position, longitude: 2.001 }];
  const plan = { points, speed: 1.5 };
  const client = vi.fn().mockResolvedValueOnce({ requested_active: false, config: { position, scope } })
    .mockResolvedValueOnce({ requested_active: true, config: { position, scope },
      route: { plan, distance: 0, total_distance: 111.2, paused: false, completed: false } })
    .mockResolvedValue({ requested_active: false, config: { position, scope }, route: null });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '路线模拟' }));
  await user.click(screen.getByRole('button', { name: '新建路线' }));
  await user.type(screen.getByLabelText('点 1 纬度'), '1');
  await user.type(screen.getByLabelText('点 1 经度'), '2');
  await user.type(screen.getByLabelText('点 2 纬度'), '1');
  await user.type(screen.getByLabelText('点 2 经度'), '2.001');
  await user.clear(screen.getByLabelText('速度（km/h）'));
  await user.type(screen.getByLabelText('速度（km/h）'), '5.4');
  await user.click(screen.getByRole('button', { name: '开始路线' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'start_route', route: plan, scope });
  await user.click(screen.getByRole('button', { name: '返回路线管理' }));
  await user.click(screen.getByRole('button', { name: '停止路线' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'stop' });
  expect(await screen.findByRole('button', { name: '新建路线' })).toBeTruthy();
});

it('reports which system channels are ready instead of hiding them', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false, config: null,
    hook_connected: true, location_hook_ready: true, gnss_hook_ready: true, nmea_hook_ready: false,
    cell_query_hook_ready: false, cell_callback_hook_ready: false, phone_connected: true });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '设置' }));
  // 已就绪与尚未接入要一眼能区分开，不能让用户以为所有通道都可用。
  expect(screen.getByText('GNSS 状态').nextElementSibling?.textContent).toBe('已就绪');
  expect(screen.getByText('NMEA 报文').nextElementSibling?.textContent).toBe('尚未接入');
  expect(screen.getByText('基站查询').nextElementSibling?.textContent).toBe('尚未接入');
  expect(screen.getByText('电话服务').nextElementSibling?.textContent).toBe('已连接');
});

it('imports a pasted position into the history without touching the backend', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false, config: null });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '导入位置' }));
  await user.type(screen.getByLabelText('粘贴地图链接或经纬度'), '31.2, 121.5');
  expect(await screen.findByText(/按纬度在前、经度在后读入/)).toBeTruthy();
  await user.click(screen.getByRole('button', { name: '导入到历史位置' }));
  await screen.findByText('31.200000, 121.500000');
  expect(JSON.parse(localStorage.getItem('justlocation.places')!)).toHaveLength(1);
  // 导入只进历史列表，不应该顺手改动模拟状态。
  expect(client.mock.calls.every(call => call[0]?.op === 'status')).toBe(true);
});

it('reads a shared map link and lets the coordinate system be corrected', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false, config: null });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '导入位置' }));
  await user.type(screen.getByLabelText('粘贴地图链接或经纬度'), 'https://uri.amap.com/marker?position=116.404,39.915');
  // 高德给的是 GCJ-02，导入时必须换算成 WGS84 再存。
  expect(await screen.findByText(/来自高德地图/)).toBeTruthy();
  expect(screen.getByLabelText('坐标类型')).toHaveProperty('value', 'gcj02');
  await user.click(screen.getByRole('button', { name: '导入到历史位置' }));
  await screen.findByText(/39\.9\d+/);
  const saved = JSON.parse(localStorage.getItem('justlocation.places')!)[0];
  expect(saved.name).toBe('来自高德地图');
  expect(saved.position.latitude).toBeLessThan(39.915);
  expect(saved.position.latitude).toBeGreaterThan(39.90);
});

it('writes the satellite switches straight to the backend', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false, config: null,
    gnss: { gnss_enabled: false, nmea_enabled: true }, gnss_hook_ready: true, nmea_hook_ready: false });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  const gnss = screen.getByRole('checkbox', { name: 'GNSS 状态' });
  // 状态以后端返回为准，不是本地默认值。
  expect(gnss).toHaveProperty('checked', false);
  expect(screen.getByRole('checkbox', { name: 'NMEA 报文' })).toHaveProperty('checked', true);
  await user.click(gnss);
  expect(client).toHaveBeenLastCalledWith({ op: 'set_gnss', config: { gnss_enabled: true, nmea_enabled: true } });
  // 未开始模拟时要说清"已启用但要等模拟开始"，不能显示成已生效。
  expect(screen.getByText('已启用，开始位置模拟后生效')).toBeTruthy();
});

it('manages saved Wi-Fi networks, writes them to the backend, and says the output is not wired up yet', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false, config: null });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: 'Wi-Fi 模拟' }));
  // 页面必须说清"已经存进配置，但还没输出"，避免误以为已经在改变应用读到的网络。
  expect(screen.getByText(/真正的输出尚未实现/)).toBeTruthy();
  await user.click(screen.getByRole('button', { name: '添加网络' }));
  await user.type(screen.getByLabelText('网络名称（SSID）'), '家里');
  await user.type(screen.getByLabelText('接入点地址（BSSID，可留空）'), 'aa:bb:cc:dd:ee:ff');
  await user.click(screen.getByRole('button', { name: '保存网络' }));
  expect(await screen.findByText('家里')).toBeTruthy();
  // 保存的网络要写进后台配置，否则系统侧没有数据可读、开关只是摆设。
  const saved = client.mock.calls.map(call => call[0]).filter(op => op?.op === 'set_wifi').at(-1);
  expect(saved.config.targets).toHaveLength(1);
  expect(saved.config.targets[0]).toMatchObject({ ssid: '家里', bssid: 'aa:bb:cc:dd:ee:ff', rssi: 200 });
  await user.click(screen.getByRole('button', { name: '删除 家里' }));
  expect(screen.queryByText('家里')).toBeNull();
  await user.click(screen.getByRole('button', { name: '撤销' }));
  expect(await screen.findByText('家里')).toBeTruthy();
  // 只碰 Wi-Fi 配置，不要顺手改动定位相关的命令。
  expect(client.mock.calls.every(call => ['status', 'set_wifi'].includes(call[0]?.op))).toBe(true);
});

it('keeps the backend status fresh while idle instead of freezing at mount', async () => {
  // 空闲时也必须继续查状态：桥接是否连上、各通道接口是否就绪都来自后台，
  // 只在"模拟进行中"才轮询会让设置页永远停在打开面板那一刻。
  vi.useFakeTimers();
  try {
    const client = vi.fn().mockResolvedValue({ requested_active: false, config: null, hook_connected: false });
    render(<App client={client} />);
    await vi.advanceTimersByTimeAsync(50);
    const atMount = client.mock.calls.length;
    expect(atMount).toBeGreaterThan(0);
    await vi.advanceTimersByTimeAsync(11_000);
    expect(client.mock.calls.length).toBeGreaterThan(atMount);
    // 空闲轮询不该像模拟进行中那样每 2 秒一次。
    expect(client.mock.calls.length).toBeLessThan(atMount + 5);
  } finally {
    vi.useRealTimers();
  }
});

it('shows a connection failure and never presents an active session', async () => {
  render(<App client={async () => { throw new Error('后台未启动'); }} />);
  expect(await screen.findByRole('alert')).toHaveProperty('textContent', expect.stringContaining('后台未启动'));
  expect(screen.getByRole('button', { name: '开始模拟' })).toHaveProperty('disabled', true);
});

it('edits coordinates without starting output and does not treat zero as empty', async () => {
  const client = vi.fn(async () => ({ requested_active: false, config: null }));
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '添加位置' }));
  await user.type(screen.getByLabelText('纬度'), '0');
  await user.type(screen.getByLabelText('经度'), '0');
  await user.click(screen.getByRole('button', { name: '保存位置' }));
  expect(screen.getAllByText('0.000000, 0.000000').length).toBeGreaterThan(0);
  expect(client.mock.calls).toHaveLength(1);
  expect(screen.getByRole('button', { name: '开始模拟' })).toHaveProperty('disabled', true);
});

it('waits for a successful start and leaves an unsuccessful start stopped', async () => {
  const client = vi.fn().mockResolvedValueOnce({ requested_active: false,
    config: { position, scope: { mode: 'apps', packages: ['example.selected'] } } })
    .mockRejectedValueOnce(new Error('保存失败'));
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  await userEvent.click(screen.getByRole('button', { name: '开始模拟' }));
  expect(await screen.findByRole('alert')).toHaveProperty('textContent', expect.stringContaining('保存失败'));
  expect(screen.queryByRole('button', { name: '停止模拟' })).toBeNull();
});

it('keeps the editor and old target when an active position update fails', async () => {
  const client = vi.fn()
    .mockResolvedValueOnce({ requested_active: true, config: { position, scope: { mode: 'all' } } })
    .mockRejectedValueOnce(new Error('更新失败'))
    // 面板会定期刷新状态，后续查询仍要成功，否则会误判成后台断开。
    .mockResolvedValue({ requested_active: true, config: { position, scope: { mode: 'all' } } });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '添加位置' }));
  await user.clear(screen.getByLabelText('纬度'));
  await user.type(screen.getByLabelText('纬度'), '3');
  await user.click(screen.getByRole('button', { name: '保存位置' }));
  expect(screen.getByRole('dialog')).toBeTruthy();
  expect(screen.getByText('1.000000, 2.000000')).toBeTruthy();
  expect(localStorage.getItem('justlocation.places')).toBeNull();
});

it('offers both appearances and keeps the chosen one', async () => {
  const client = vi.fn().mockResolvedValue({ requested_active: false, config: null });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '设置' }));
  expect(document.documentElement.dataset.style).toBe('material');
  await user.click(screen.getByRole('button', { name: '米 UI' }));
  expect(document.documentElement.dataset.style).toBe('miuix');
  await user.click(screen.getByRole('button', { name: '深色' }));
  expect(document.documentElement.dataset.theme).toBe('dark');
  expect(localStorage.getItem('justlocation.style')).toBe('miuix');
  expect(localStorage.getItem('justlocation.theme')).toBe('dark');
});

it('opens the cell data page from the cell switch when no operator is configured', async () => {
  const config = { position, scope: { mode: 'apps', packages: ['example.selected'] } };
  const client = vi.fn().mockResolvedValue({ requested_active: false, config });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('checkbox', { name: '基站模拟' }));
  expect(await screen.findByText('还没有查询结果。上面的按钮会按目标位置取这一带的基站，也可以直接导入离线数据。')).toBeTruthy();
  // 没有可配置的运营商时不应该偷偷改动后台配置。
  expect(client.mock.calls.filter(call => call[0]?.op === 'set_telephony')).toHaveLength(0);
});

it('toggles configured cell simulation without stopping location or losing the operator settings', async () => {
  const telephony = { cells_enabled: true, sim_enabled: true, radius_m: 700,
    subscriptions: [{ id: 7, slot: 0, mcc: '460', mnc: '001', country: 'cn', carrier: 'Test', enabled: true }] };
  const client = vi.fn().mockResolvedValue({ requested_active: true, config: null, telephony });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  expect(screen.getByRole('checkbox', { name: '基站模拟' })).toHaveProperty('checked', true);
  await user.click(screen.getByRole('checkbox', { name: '基站模拟' }));
  // 前端状态以后台返回为准，所以这里只断言发出的指令。
  expect(client).toHaveBeenLastCalledWith({ op: 'set_telephony', config: { ...telephony, cells_enabled: false } });
  expect(client).not.toHaveBeenCalledWith({ op: 'stop' });
});
