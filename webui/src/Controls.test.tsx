// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SwitchRow } from './Controls';

afterEach(cleanup);

// 分段控件的真实用法在 App.test.tsx 的"两套外观"用例里验证，这里不重复。
it('exposes the switch as a named checkbox and blocks changes while disabled', async () => {
  const onChange = vi.fn();
  const { rerender } = render(<SwitchRow title="摇杆" summary="拖动即可移动位置" checked={false} onChange={onChange} />);
  const user = userEvent.setup();
  const box = screen.getByRole('checkbox', { name: '摇杆' });
  expect(box).toHaveProperty('checked', false);
  expect(screen.getByText('拖动即可移动位置')).toBeTruthy();
  await user.click(box);
  expect(onChange).toHaveBeenCalledWith(true);
  rerender(<SwitchRow title="摇杆" checked disabled onChange={onChange} />);
  expect(screen.getByRole('checkbox', { name: '摇杆' })).toHaveProperty('disabled', true);
});

it('passes the requested value instead of dropping the argument', async () => {
  // 这里曾经漏传参数，导致开关永远按旧值取反，属于真实出现过的缺陷。
  const onChange = vi.fn();
  render(<SwitchRow title="基站模拟" checked onChange={onChange} />);
  await userEvent.click(screen.getByRole('checkbox', { name: '基站模拟' }));
  expect(onChange).toHaveBeenCalledWith(false);
});
