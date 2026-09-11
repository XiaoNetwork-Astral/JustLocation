// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TelephonySettings } from './TelephonySettings';
import type { State } from './control';

afterEach(cleanup);
const card = { id: 7, slot: 0, mcc: '460', mnc: '001', country: 'cn', carrier: '测试运营商' };
const state: State = { requested_active: false, config: null, phone_connected: true, detected_subscriptions: [card],
  telephony: { cells_enabled: false, sim_enabled: false, radius_m: 500, subscriptions: [] } };

it('uses detected slot IDs and preserves three-digit MNC when enabling cell simulation', async () => {
  const save = vi.fn().mockResolvedValue(undefined);
  render(<TelephonySettings state={state} busy={false} onSave={save} />);
  const user = userEvent.setup();
  await user.click(screen.getByLabelText('模拟基站'));
  await user.click(screen.getByRole('button', { name: '保存模拟设置' }));
  expect(save).toHaveBeenCalledWith({ cells_enabled: true, sim_enabled: false, radius_m: 500, subscriptions: [{ ...card, enabled: true }] });
  expect(screen.getByText('设置已保存，开始位置模拟后生效。')).toBeTruthy();
});

it('keeps unsaved edits through status polling and a failed save', async () => {
  const save = vi.fn().mockRejectedValue(new Error('保存失败'));
  const { rerender } = render(<TelephonySettings state={state} busy={false} onSave={save} />);
  const user = userEvent.setup();
  await user.click(screen.getByText('修改运营商'));
  await user.clear(screen.getByLabelText('运营商名称'));
  await user.type(screen.getByLabelText('运营商名称'), '自定义运营商');
  rerender(<TelephonySettings state={{ ...state, location_hook_ready: true }} busy={false} onSave={save} />);
  await user.click(screen.getByRole('button', { name: '保存模拟设置' }));
  expect((await screen.findByRole('alert')).textContent).toContain('保存失败');
  expect(screen.getByLabelText('运营商名称')).toHaveProperty('value', '自定义运营商');
});

it('distinguishes missing phone connection from no inserted SIM and refuses to save after a card changes', async () => {
  const save = vi.fn();
  const { rerender } = render(<TelephonySettings state={{ ...state, phone_connected: false, detected_subscriptions: null }} busy={false} onSave={save} />);
  expect(screen.getByText('等待电话服务连接')).toBeTruthy();
  rerender(<TelephonySettings state={{ ...state, detected_subscriptions: [] }} busy={false} onSave={save} />);
  expect(screen.getByText('没有检测到可用的 SIM 卡')).toBeTruthy();
  rerender(<TelephonySettings state={state} busy={false} onSave={save} />);
  const user = userEvent.setup();
  await user.click(screen.getByLabelText('模拟基站'));
  rerender(<TelephonySettings state={{ ...state, detected_subscriptions: [{ ...card, id: 8 }] }} busy={false} onSave={save} />);
  await user.click(screen.getByRole('button', { name: '保存模拟设置' }));
  expect((await screen.findByRole('alert')).textContent).toContain('SIM 卡已变化，请重新打开此页后设置');
  expect(save).not.toHaveBeenCalled();
});
