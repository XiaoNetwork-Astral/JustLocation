// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { RoutePanel } from './RoutePanel';

afterEach(() => { cleanup(); localStorage.clear(); });

it('keeps repeat settings in the draft, validates them, and includes them when starting', async () => {
  localStorage.setItem('justlocation.route', JSON.stringify({ speed: '36', points: [
    { latitude: '0', longitude: '0', altitude: '0' }, { latitude: '0', longitude: '0.001', altitude: '0' },
  ] }));
  const command = vi.fn().mockResolvedValue(undefined);
  const props = { state: { requested_active: false, config: null }, busy: false, scope: { mode: 'all' as const }, onCommand: command, onScope: () => {} };
  const user = userEvent.setup();
  const view = render(<RoutePanel {...props} />);
  expect(screen.getByLabelText('播放次数')).toHaveProperty('value', '1');
  await user.clear(screen.getByLabelText('播放次数'));
  await user.type(screen.getByLabelText('播放次数'), '3');
  await user.clear(screen.getByLabelText('每次间隔（秒）'));
  await user.type(screen.getByLabelText('每次间隔（秒）'), '10');
  view.unmount();
  render(<RoutePanel {...props} />);
  expect(screen.getByLabelText('播放次数')).toHaveProperty('value', '3');
  expect(screen.getByLabelText('每次间隔（秒）')).toHaveProperty('value', '10');
  await user.click(screen.getByRole('button', { name: '开始路线' }));
  expect(command).toHaveBeenCalledWith(expect.objectContaining({ route: expect.objectContaining({ speed: 10, repeat_count: 3, repeat_delay: 10 }) }));
  command.mockClear();
  await user.clear(screen.getByLabelText('播放次数'));
  await user.type(screen.getByLabelText('播放次数'), '1.5');
  await user.click(screen.getByRole('button', { name: '开始路线' }));
  expect(screen.getByRole('alert').textContent).toContain('整数');
  expect(command).not.toHaveBeenCalled();
});

it('shows the current lap and waiting countdown, with pause and stop still available', async () => {
  const command = vi.fn().mockResolvedValue(undefined);
  const position = { latitude: 0, longitude: 0, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
  render(<RoutePanel state={{ requested_active: true, config: null, route: {
    plan: { points: [position, { ...position, longitude: 1 }], speed: 10, repeat_count: 3, repeat_delay: 10 },
    distance: 111, total_distance: 111, paused: false, completed: false, lap: 1, waiting_seconds: 9.2,
  } }} busy={false} scope={{ mode: 'all' }} onCommand={command} onScope={() => {}} />);
  expect(screen.getByText('等待下一轮')).toBeTruthy();
  expect(screen.getByText(/第 1 \/ 3 次/)).toBeTruthy();
  expect(screen.getByText(/10 秒后回到起点/)).toBeTruthy();
  expect(screen.getByLabelText('播放次数')).toHaveProperty('disabled', true);
  await userEvent.click(screen.getByRole('button', { name: '暂停路线' }));
  expect(command).toHaveBeenLastCalledWith({ op: 'pause_route' });
  await userEvent.click(screen.getByRole('button', { name: '停止路线' }));
  expect(command).toHaveBeenLastCalledWith({ op: 'stop' });
});

it('previews an import, replaces the draft only on selection and starts with the existing speed and scope', async () => {
  const command = vi.fn().mockResolvedValue(undefined);
  const scope = { mode: 'apps' as const, packages: ['example.selected'] };
  render(<RoutePanel state={{ requested_active: false, config: null }} busy={false} scope={scope} onCommand={command} onScope={() => {}} />);
  const user = userEvent.setup();
  await user.type(screen.getByLabelText('点 1 纬度'), '12');
  await user.upload(screen.getByLabelText('导入 GPX'), new File([
    '<gpx><rte><name>散步</name><rtept lat="31.2" lon="121.5"/><rtept lat="31.3" lon="121.5"/></rte></gpx>',
  ], 'walk.gpx', { type: 'application/gpx+xml' }));
  await screen.findByText('散步 · 2 个点');
  expect(screen.getByLabelText('点 1 纬度')).toHaveProperty('value', '12');
  expect(command).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '使用这条路线' }));
  expect(screen.getByLabelText('点 1 纬度')).toHaveProperty('value', '31.2');
  expect(JSON.parse(localStorage.getItem('justlocation.route')!).points).toHaveLength(2);
  await user.upload(screen.getByLabelText('导入 GPX'), new File(['<gpx>'], 'broken.gpx', { type: 'application/gpx+xml' }));
  expect((await screen.findByRole('alert')).textContent).toContain('无法读取');
  expect(screen.getByLabelText('点 1 纬度')).toHaveProperty('value', '31.2');
  await user.click(screen.getByRole('button', { name: '开始路线' }));
  expect(command).toHaveBeenCalledWith({ op: 'start_route', scope, route: { speed: 1.5, points: [
    { latitude: 31.2, longitude: 121.5, altitude: 0, accuracy: 5, speed: 0, bearing: 0 },
    { latitude: 31.3, longitude: 121.5, altitude: 0, accuracy: 5, speed: 0, bearing: 0 },
  ] } });
});

it('disables file import while simulation is active', () => {
  render(<RoutePanel state={{ requested_active: true, config: null }} busy={false} scope={{ mode: 'all' }} onCommand={vi.fn()} onScope={() => {}} />);
  expect(screen.getByLabelText('导入 GPX')).toHaveProperty('disabled', true);
});

it('saves a named route and restores points, speed and repeats without starting simulation', async () => {
  localStorage.setItem('justlocation.route', JSON.stringify({ speed: '36', repeatCount: '3', repeatDelay: '10', points: [
    { latitude: '31', longitude: '121', altitude: '5' }, { latitude: '31.01', longitude: '121', altitude: '9' },
  ] }));
  const command = vi.fn();
  const props = { state: { requested_active: false, config: null }, busy: false, scope: { mode: 'all' as const }, onCommand: command, onScope: () => {} };
  const user = userEvent.setup();
  const view = render(<RoutePanel {...props} />);
  await user.click(screen.getByRole('button', { name: '保存路线' }));
  await user.type(screen.getByLabelText('路线名称'), '晚间散步');
  await user.click(screen.getByRole('button', { name: '保存' }));
  view.unmount();
  localStorage.removeItem('justlocation.route');
  render(<RoutePanel {...props} />);
  await user.click(screen.getByText('已保存的路线（1）'));
  await user.click(screen.getByRole('button', { name: '使用晚间散步' }));
  expect(screen.getByLabelText('点 1 纬度')).toHaveProperty('value', '31');
  expect(screen.getByLabelText('点 2 海拔')).toHaveProperty('value', '9');
  expect(screen.getByLabelText('速度（km/h）')).toHaveProperty('value', '36');
  expect(screen.getByLabelText('播放次数')).toHaveProperty('value', '3');
  expect(screen.getByLabelText('每次间隔（秒）')).toHaveProperty('value', '10');
  expect(command).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '删除晚间散步' }));
  expect(screen.queryByRole('button', { name: '使用晚间散步' })).toBeNull();
  await user.click(screen.getByRole('button', { name: '撤销' }));
  expect(screen.getByRole('button', { name: '使用晚间散步' })).toBeTruthy();
});

it('keeps the saved library when storage fails, and rejects an incomplete route', async () => {
  render(<RoutePanel state={{ requested_active: false, config: null }} busy={false} scope={{ mode: 'all' }} onCommand={vi.fn()} onScope={() => {}} />);
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '保存路线' }));
  await user.type(screen.getByLabelText('路线名称'), '散步');
  await user.click(screen.getByRole('button', { name: '保存' }));
  expect(screen.getByRole('alert').textContent).toContain('点 1');
  for (const [label, value] of [['点 1 纬度', '31'], ['点 1 经度', '121'], ['点 2 纬度', '32'], ['点 2 经度', '121']]) {
    await user.type(screen.getByLabelText(label), value);
  }
  const write = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('full'); });
  try {
    await user.click(screen.getByRole('button', { name: '保存' }));
    expect(screen.getByRole('alert').textContent).toContain('保存失败');
    expect(screen.queryByRole('button', { name: '使用散步' })).toBeNull();
  } finally { write.mockRestore(); }
});
