// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { BackupPanel } from './BackupPanel';
afterEach(() => {
  cleanup();
  localStorage.clear();
});

it('previews a backup before merging and leaves the current session alone', async () => {
  const onImported = vi.fn();
  render(<BackupPanel onImported={onImported} />);
  const user = userEvent.setup();
  const position = { latitude: 31, longitude: 121, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
  await user.upload(
    screen.getByLabelText('导入备份'),
    new File(
      [
        JSON.stringify({
          format: 'justlocation',
          version: 1,
          places: [{ id: '1', name: '家', position, pinned: true }],
          routes: [],
        }),
      ],
      'backup.json',
      { type: 'application/json' },
    ),
  );
  await screen.findByText('1 个位置 · 0 条路线');
  expect(localStorage.getItem('justlocation.places')).toBeNull();
  expect(onImported).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '合并导入' }));
  expect(onImported).toHaveBeenCalledTimes(1);
  expect(screen.getByRole('status').textContent).toContain('已添加 1 个位置、0 条路线');
  expect(JSON.parse(localStorage.getItem('justlocation.places')!)[0].name).toBe('家');
});

it('reports export errors and clears an old preview after choosing a broken file', async () => {
  const save = vi.fn().mockRejectedValue(new Error('导出失败'));
  render(<BackupPanel onImported={vi.fn()} save={save} />);
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '导出备份' }));
  expect((await screen.findByRole('alert')).textContent).toContain('导出失败');
  expect(screen.queryByRole('status')).toBeNull();
  await user.upload(
    screen.getByLabelText('导入备份'),
    new File(['{}'], 'bad.json', { type: 'application/json' }),
  );
  expect((await screen.findByRole('alert')).textContent).toContain('备份格式');
  expect(screen.queryByRole('button', { name: '合并导入' })).toBeNull();
});
