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
  const joystick = { check: vi.fn().mockResolvedValue(undefined), open: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined) };
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
  const joystick = { check: vi.fn().mockResolvedValue(undefined), open: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined) };
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
  const client = vi.fn().mockResolvedValueOnce({ requested_active: true,
    config: { position, scope: { mode: 'all' } } }).mockRejectedValueOnce(new Error('更新失败'));
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
  expect(await screen.findByText('还没有基站数据')).toBeTruthy();
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
