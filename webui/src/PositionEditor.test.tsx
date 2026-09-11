// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { PositionEditor } from './PositionEditor';
afterEach(cleanup);

it('converts explicitly selected GCJ coordinates once before saving', async () => {
  const save = vi.fn().mockResolvedValue(undefined);
  render(<PositionEditor initial={null} name="" onClose={() => {}} onSave={save} />);
  const user = userEvent.setup();
  await user.selectOptions(screen.getByLabelText('坐标系'), 'gcj02');
  await user.type(screen.getByLabelText('纬度'), '39.91640428150164');
  await user.type(screen.getByLabelText('经度'), '116.41024449916938');
  await user.click(screen.getByRole('button', { name: '保存位置' }));
  const p = save.mock.calls[0][0];
  expect(p.latitude).toBeCloseTo(39.915, 8);
  expect(p.longitude).toBeCloseTo(116.404, 8);
});

it('keeps invalid inputs editable and does not save or open a map with partial coordinates', async () => {
  const save = vi.fn();
  render(<PositionEditor initial={null} name="" onClose={() => {}} onSave={save} />);
  const user = userEvent.setup();
  await user.type(screen.getByLabelText('纬度'), '91');
  await user.click(screen.getByRole('button', { name: '地图选点' }));
  expect(screen.getByRole('alert').textContent).toContain('坐标');
  expect(screen.queryByRole('dialog', { name: '地图选点' })).toBeNull();
  await user.click(screen.getByRole('button', { name: '保存位置' }));
  expect(save).not.toHaveBeenCalled();
  expect(screen.getByLabelText('纬度')).toHaveProperty('value', '91');
});
