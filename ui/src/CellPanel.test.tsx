// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CellPanel } from './CellPanel';

afterEach(cleanup);
const settings = {
  primary: 'open_cell_id',
  fallback: 'custom',
  opencellid_configured: true,
  custom_endpoint: '',
  custom_token_configured: false,
};
const dataset = {
  provider: 'open_cell_id',
  origin: 'https://opencellid.org',
  region: {
    center: { latitude: 1, longitude: 2 },
    radius_m: 500,
    source: 'OpenCellID',
    fetched_at_ms: 1000,
    cells: [],
  },
  attribution: {
    text: 'OpenCellID',
    source: 'https://opencellid.org',
    license: 'https://creativecommons.org/licenses/by-sa/4.0/',
    changes: null,
  },
  incomplete: false,
  skipped: 0,
  failures: [],
};
const telephony = { cells_enabled: false, sim_enabled: false, radius_m: 500, subscriptions: [] };

it('applies the queried area only when requested, and reports a failed application without false success', async () => {
  const client = vi.fn().mockResolvedValueOnce({ settings }).mockResolvedValue({ dataset });
  const apply = vi
    .fn()
    .mockRejectedValueOnce(new Error('后台写入失败'))
    .mockResolvedValue(undefined);
  render(
    <CellPanel
      target={{ latitude: 1, longitude: 2 }}
      client={client}
      onApply={apply}
      onClose={() => {}}
    />,
  );
  const user = userEvent.setup();
  await screen.findByRole('checkbox', { name: '启用基站模拟' });
  await user.click(screen.getByRole('button', { name: '查询附近基站' }));
  await screen.findByText('这个范围内没有基站数据');
  expect(apply).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '应用这份基站数据' }));
  expect((await screen.findByRole('alert')).textContent).toContain('后台写入失败');
  expect(screen.queryByText('基站数据已应用')).toBeNull();
  await user.click(screen.getByRole('button', { name: '应用这份基站数据' }));
  expect(apply).toHaveBeenLastCalledWith(dataset.region);
  expect(await screen.findByText('基站数据已应用')).toBeTruthy();
});

it('queries the chosen target offline and shows a successful empty result with attribution', async () => {
  const client = vi
    .fn()
    .mockResolvedValueOnce({ settings })
    .mockResolvedValue({ dataset, cached: true, stale: false });
  render(<CellPanel target={{ latitude: 1, longitude: 2 }} client={client} onClose={() => {}} />);
  const user = userEvent.setup();
  await screen.findByRole('checkbox', { name: '启用基站模拟' });
  await user.click(screen.getByLabelText('只使用已导入的离线数据'));
  await user.click(screen.getByRole('button', { name: '查询附近基站' }));
  await screen.findByText('这个范围内没有基站数据');
  expect(client).toHaveBeenLastCalledWith({
    op: 'query',
    area: { target: { latitude: 1, longitude: 2 }, radius_m: 500 },
    mode: 'offline',
  });
  expect(screen.getByRole('link', { name: 'OpenCellID' }).getAttribute('href')).toBe(
    'https://opencellid.org/',
  );
});

it('leaves an existing key untouched when saving other settings and ignores a late result after the target changes', async () => {
  let settle!: (value: unknown) => void;
  const pending = new Promise((resolve) => {
    settle = resolve;
  });
  const client = vi.fn().mockResolvedValue({ settings });
  const { rerender } = render(
    <CellPanel target={{ latitude: 1, longitude: 2 }} client={client} onClose={() => {}} />,
  );
  const user = userEvent.setup();
  await user.click(await screen.findByText('数据来源设置'));

  await user.selectOptions(screen.getByLabelText('首选供应商'), 'custom');
  await user.click(screen.getByRole('button', { name: '保存数据来源' }));
  expect(client).toHaveBeenLastCalledWith({
    op: 'configure',
    settings: { primary: 'custom', fallback: 'custom', custom_endpoint: '' },
  });
  await screen.findByText('数据来源已保存');
  expect(
    client.mock.calls.some((call) => call[0]?.settings && 'opencellid_key' in call[0].settings),
  ).toBe(false);

  client.mockImplementationOnce(() => pending);
  await user.click(screen.getByRole('button', { name: '查询附近基站' }));
  rerender(
    <CellPanel target={{ latitude: 20, longitude: 30 }} client={client} onClose={() => {}} />,
  );
  settle({ dataset, cached: false, stale: false });
  await waitFor(() =>
    expect(screen.getByRole('button', { name: '查询附近基站' })).toHaveProperty('disabled', false),
  );
  expect(screen.queryByText('这个范围内没有基站数据')).toBeNull();
});

it('writes the master switch straight to the backend instead of waiting for a save', async () => {
  const client = vi.fn().mockResolvedValue({ settings });
  const configure = vi.fn().mockResolvedValue(undefined);
  render(
    <CellPanel
      target={{ latitude: 1, longitude: 2 }}
      client={client}
      state={{ requested_active: false, config: null, telephony } as never}
      onConfigure={configure}
      onClose={() => {}}
    />,
  );
  const user = userEvent.setup();
  const master = await screen.findByRole('checkbox', { name: '启用基站模拟' });
  expect(master).toHaveProperty('checked', false);
  await user.click(master);
  await waitFor(() =>
    expect(configure).toHaveBeenCalledWith({ ...telephony, cells_enabled: true }),
  );
  expect(await screen.findByText('基站模拟已启用')).toBeTruthy();
});

it('says what is missing instead of failing silently when there is no target', async () => {
  const client = vi.fn().mockResolvedValue({ settings });
  render(<CellPanel target={null} client={client} onClose={() => {}} />);
  const user = userEvent.setup();
  await screen.findByRole('checkbox', { name: '启用基站模拟' });
  expect(screen.getByText('还没有目标位置，先在位置模拟里选一个点。')).toBeTruthy();
  await user.click(screen.getByRole('button', { name: '查询附近基站' }));
  expect((await screen.findByRole('alert')).textContent).toContain('先在位置模拟里选一个位置');
});
