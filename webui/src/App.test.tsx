// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { App } from './App';

afterEach(() => { cleanup(); localStorage.clear(); });

it('starts location and opens the joystick in one action, and closes only the overlay', async () => {
  const config = { position: { latitude: 1, longitude: 2, altitude: 0, accuracy: 5, speed: 0, bearing: 0 }, scope: { mode: 'all' as const } };
  const client = vi.fn().mockResolvedValueOnce({ requested_active: false, config })
    .mockResolvedValue({ requested_active: true, config });
  const joystick = { check: vi.fn().mockResolvedValue(undefined), open: vi.fn().mockResolvedValue(undefined), close: vi.fn().mockResolvedValue(undefined) };
  render(<App client={client} joystick={joystick} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '摇杆菜单' }));
  await user.click(screen.getByRole('button', { name: '打开摇杆' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'start', config });
  expect(joystick.open).toHaveBeenCalledWith(5.4);
  await user.click(screen.getByRole('button', { name: '关闭摇杆' }));
  expect(joystick.close).toHaveBeenCalledOnce();
  expect(client).not.toHaveBeenCalledWith({ op: 'stop' });
});

it('keeps selected apps when switching through all apps and sends them on the next start', async () => {
  const position = { latitude: 1, longitude: 2, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
  const client = vi.fn().mockResolvedValue({ requested_active: false,
    config: { position, scope: { mode: 'apps', packages: ['example.selected'] } } });
  render(<App client={client} loadApps={async () => [{ packageName: 'example.selected', appLabel: '地图', isSystem: false }]} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  expect(screen.queryByRole('button', { name: '作用范围' })).toBeNull();
  await user.click(screen.getByRole('button', { name: '独立模拟菜单' }));
  await user.click(screen.getByRole('button', { name: '作用范围' }));
  expect(await screen.findByRole('checkbox', { name: /地图/ })).toHaveProperty('checked', true);
  await user.click(screen.getByRole('radio', { name: /全部应用/ }));
  await user.click(screen.getByRole('radio', { name: /指定应用/ }));
  expect(await screen.findByRole('checkbox', { name: /地图/ })).toHaveProperty('checked', true);
  await user.click(screen.getByRole('button', { name: '完成' }));
  await user.click(screen.getByRole('button', { name: '开始模拟' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'start', config: { position,
    scope: { mode: 'apps', packages: ['example.selected'] } } });
});

it('starts a route with the selected scope and restores pause controls from backend state', async () => {
  const position = { latitude: 0, longitude: 0, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
  const scope = { mode: 'apps', packages: ['example.selected'] };
  const points = [position, { ...position, longitude: 0.001 }];
  const plan = { points, speed: 1.5 };
  const client = vi.fn().mockResolvedValueOnce({ requested_active: false, config: { position, scope } })
    .mockResolvedValueOnce({ requested_active: true, config: { position, scope },
      route: { plan, distance: 0, total_distance: 111.2, paused: false, completed: false } })
    .mockResolvedValueOnce({ requested_active: true, config: { position, scope },
      route: { plan, distance: 0, total_distance: 111.2, paused: true, completed: false } })
    .mockResolvedValue({ requested_active: false, config: { position, scope }, route: null });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '路线模拟' }));
  await user.type(screen.getByLabelText('点 1 纬度'), '0');
  await user.type(screen.getByLabelText('点 1 经度'), '0');
  await user.type(screen.getByLabelText('点 2 纬度'), '0');
  await user.type(screen.getByLabelText('点 2 经度'), '0.001');
  await user.clear(screen.getByLabelText('速度（km/h）'));
  await user.type(screen.getByLabelText('速度（km/h）'), '5.4');
  await user.click(screen.getByRole('button', { name: '开始路线' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'start_route', route: plan, scope });
  expect(screen.getByLabelText('点 1 纬度')).toHaveProperty('disabled', true);
  await user.click(screen.getByRole('button', { name: '暂停路线' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'pause_route' });
  expect(screen.getByRole('button', { name: '继续路线' })).toBeTruthy();
  await user.click(screen.getByRole('button', { name: '停止路线' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'stop' });
  expect(screen.getByLabelText('点 1 纬度')).toHaveProperty('disabled', false);
});

it('submits the checked apps as the start scope and blocks an empty selection', async () => {
  const position = { latitude: 1, longitude: 2, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
  const client = vi.fn().mockResolvedValue({ requested_active: false,
    config: { position, scope: { mode: 'apps', packages: ['example.old'] } } });
  render(<App client={client} loadApps={async () => [
    { packageName: 'example.old', appLabel: '旧应用', isSystem: false },
    { packageName: 'example.new', appLabel: '新应用', isSystem: false },
  ]} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '独立模拟菜单' }));
  await user.click(screen.getByRole('button', { name: '作用范围' }));
  await user.click(await screen.findByRole('checkbox', { name: /旧应用/ }));
  await user.click(screen.getByRole('button', { name: '完成' }));
  expect(screen.getByRole('button', { name: '开始模拟' })).toHaveProperty('disabled', true);
  await user.click(screen.getByRole('button', { name: '独立模拟菜单' }));
  await user.click(screen.getByRole('button', { name: '作用范围' }));
  await user.click(await screen.findByRole('checkbox', { name: /新应用/ }));
  await user.click(screen.getByRole('button', { name: '完成' }));
  await user.click(screen.getByRole('button', { name: '开始模拟' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'start', config: { position,
    scope: { mode: 'apps', packages: ['example.new'] } } });
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
  const position = { latitude: 1, longitude: 2, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
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
  const position = { latitude: 1, longitude: 2, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
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



it('keeps scope choices and opens the cell data page while its simulation hook is unavailable', async () => {
  const config = { position: { latitude: 1, longitude: 2, altitude: 0, accuracy: 5, speed: 0, bearing: 0 }, scope: { mode: 'apps', packages: ['example.selected'] } };
  const client = vi.fn().mockResolvedValue({ requested_active: false, config });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  expect(screen.queryByRole('region', { name: '悬浮摇杆' })).toBeNull();
  await user.click(screen.getByRole('button', { name: '独立模拟菜单' }));
  await user.click(screen.getByRole('button', { name: '禁用独立模拟' }));
  expect(JSON.parse(localStorage.getItem('justlocation.scope')!)).toEqual({ mode: 'all', packages: ['example.selected'] });
  await user.click(screen.getByRole('button', { name: '启用独立模拟' }));
  expect(JSON.parse(localStorage.getItem('justlocation.scope')!).mode).toBe('apps');
  await user.click(screen.getByRole('button', { name: '基站菜单' }));
  await user.click(screen.getByRole('button', { name: '启用基站模拟' }));
  expect(screen.getByText('还没有基站数据')).toBeTruthy();
  expect(client).toHaveBeenCalledTimes(1);
  await user.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
});

it('toggles configured cell simulation without stopping location or losing the operator settings', async () => {
  const telephony = { cells_enabled: true, sim_enabled: true, radius_m: 700,
    subscriptions: [{ id: 7, slot: 0, mcc: '460', mnc: '001', country: 'cn', carrier: 'Test', enabled: true }] };
  const client = vi.fn().mockResolvedValue({ requested_active: true, config: null, telephony });
  render(<App client={client} />);
  await screen.findByText('后台已连接');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '基站菜单' }));
  await user.click(screen.getByRole('button', { name: '停用基站模拟' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'set_telephony', config: { ...telephony, cells_enabled: false } });
  expect(client).not.toHaveBeenCalledWith({ op: 'stop' });
});
