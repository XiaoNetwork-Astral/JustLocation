// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { AppPicker } from './AppPicker';

afterEach(cleanup);
const apps = [
  { packageName: 'example.maps', appLabel: '地图', isSystem: false },
  { packageName: 'example.settings', appLabel: '设置', isSystem: true },
];

it('searches labels and packages, hides unselected system apps, and keeps hidden selections', async () => {
  const user = userEvent.setup();
  function Harness() {
    const [selected, setSelected] = useState(['example.missing']);
    return (
      <AppPicker
        selected={selected}
        onChange={setSelected}
        disabled={false}
        loadApps={async () => apps}
      />
    );
  }
  render(<Harness />);
  expect(await screen.findByRole('checkbox', { name: /地图/ })).toHaveProperty('checked', false);
  expect(screen.queryByRole('checkbox', { name: /^设置/ })).toBeNull();
  expect(screen.getByRole('checkbox', { name: /example.missing/ })).toHaveProperty('checked', true);
  await user.click(screen.getByRole('checkbox', { name: /地图/ }));
  await user.type(screen.getByRole('searchbox'), 'maps');
  expect(screen.queryByRole('checkbox', { name: /example.missing/ })).toBeNull();
  expect(screen.getByRole('checkbox', { name: /地图/ })).toHaveProperty('checked', true);
  await user.clear(screen.getByRole('searchbox'));
  expect(screen.getByRole('checkbox', { name: /example.missing/ })).toHaveProperty('checked', true);
  await user.click(screen.getByRole('checkbox', { name: '系统应用' }));
  expect(screen.getByRole('checkbox', { name: /^设置/ })).toBeTruthy();
});

it('shows selected system apps and blocks changes while a session is active', async () => {
  const changed = vi.fn();
  render(
    <AppPicker
      selected={['example.settings']}
      onChange={changed}
      disabled
      loadApps={async () => apps}
    />,
  );
  const selected = await screen.findByRole('checkbox', { name: /^设置/ });
  expect(selected).toHaveProperty('checked', true);
  expect(selected).toHaveProperty('disabled', true);
  await userEvent.click(selected);
  expect(changed).not.toHaveBeenCalled();
});

it('keeps saved selections on load failure and allows a retry', async () => {
  const loadApps = vi.fn().mockRejectedValueOnce(new Error('读取失败')).mockResolvedValueOnce(apps);
  render(
    <AppPicker
      selected={['example.maps']}
      onChange={vi.fn()}
      disabled={false}
      loadApps={loadApps}
    />,
  );
  expect(await screen.findByRole('alert')).toHaveProperty(
    'textContent',
    expect.stringContaining('读取失败'),
  );
  expect(screen.getByRole('checkbox', { name: /example.maps/ })).toHaveProperty('checked', true);
  await userEvent.click(screen.getByRole('button', { name: '重试' }));
  expect(await screen.findByRole('checkbox', { name: /地图/ })).toHaveProperty('checked', true);
  expect(screen.queryByRole('alert')).toBeNull();
});
