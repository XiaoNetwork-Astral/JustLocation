// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CellPanel } from './CellPanel';

afterEach(cleanup);
const settings = { primary: 'open_cell_id', fallback: 'fake_location', opencellid_configured: true, custom_endpoint: '', custom_token_configured: false, fake_location_ready: false };
const dataset = { provider: 'open_cell_id', origin: 'https://opencellid.org', region: { center: { latitude: 1, longitude: 2 }, radius_m: 500, source: 'OpenCellID', fetched_at_ms: 1000, cells: [] }, attribution: { text: 'OpenCellID', source: 'https://opencellid.org', license: 'https://creativecommons.org/licenses/by-sa/4.0/', changes: null }, incomplete: false, skipped: 0, failures: [] };

it('applies the queried area only when requested, and reports a failed application without false success', async () => {
  const client = vi.fn().mockResolvedValueOnce({ settings }).mockResolvedValue({ dataset });
  const apply = vi.fn().mockRejectedValueOnce(new Error('后台写入失败')).mockResolvedValue(undefined);
  render(<CellPanel target={{ latitude: 1, longitude: 2 }} client={client} onApply={apply} onClose={() => {}} />);
  const user = userEvent.setup();
  await screen.findByText('供应商设置');
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
  const client = vi.fn().mockResolvedValueOnce({ settings }).mockResolvedValue({ dataset, cached: true, stale: false });
  render(<CellPanel target={{ latitude: 1, longitude: 2 }} client={client} onClose={() => {}} />);
  const user = userEvent.setup();
  await screen.findByText('供应商设置');
  await user.click(screen.getByLabelText('仅使用离线数据'));
  await user.click(screen.getByRole('button', { name: '查询附近基站' }));
  await screen.findByText('这个范围内没有基站数据');
  expect(client).toHaveBeenLastCalledWith({ op: 'query', area: { target: { latitude: 1, longitude: 2 }, radius_m: 500 }, mode: 'offline' });
  expect(screen.getByRole('link', { name: 'OpenCellID' }).getAttribute('href')).toBe('https://opencellid.org/');
});

it('leaves an existing key untouched when saving other settings and ignores a late result after the target changes', async () => {
  let resolve!: (value: unknown) => void;
  const client = vi.fn().mockResolvedValue({ settings });
  const { rerender } = render(<CellPanel target={{ latitude: 1, longitude: 2 }} client={client} onClose={() => {}} />);
  const user = userEvent.setup();
  await user.click(await screen.findByText('供应商设置'));
  await user.click(screen.getByRole('button', { name: '保存供应商' }));
  expect(client).toHaveBeenLastCalledWith({ op: 'configure', settings: { primary: 'open_cell_id', fallback: 'fake_location', custom_endpoint: '' } });
  await screen.findByText('供应商已保存');
  client.mockImplementationOnce(() => new Promise(r => { resolve = r; }));
  await user.click(screen.getByRole('button', { name: '查询附近基站' }));
  rerender(<CellPanel target={{ latitude: 20, longitude: 30 }} client={client} onClose={() => {}} />);
  resolve({ dataset, cached: false, stale: false });
  await waitFor(() => expect(screen.getByRole('button', { name: '查询附近基站' })).toHaveProperty('disabled', false));
  expect(screen.queryByText('这个范围内没有基站数据')).toBeNull();
});
