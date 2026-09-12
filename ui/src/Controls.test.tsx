// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { useState } from 'react';
import userEvent from '@testing-library/user-event';
import { SwitchRow } from './Controls';

afterEach(cleanup);

it('exposes the switch as a named checkbox and blocks changes while disabled', async () => {
  const onChange = vi.fn();
  const { rerender } = render(
    <SwitchRow title="摇杆" summary="拖动即可移动位置" checked={false} onChange={onChange} />,
  );
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
  const onChange = vi.fn();
  render(<SwitchRow title="基站模拟" checked onChange={onChange} />);
  await userEvent.click(screen.getByRole('checkbox', { name: '基站模拟' }));
  expect(onChange).toHaveBeenCalledWith(false);
});

it('toggles once per click and also responds to the row title', async () => {
  function Harness() {
    const [on, setOn] = useState(false);
    return (
      <>
        <SwitchRow title="作用范围" checked={on} onChange={setOn} />
        <span data-testid="state">{on ? 'on' : 'off'}</span>
      </>
    );
  }
  render(<Harness />);
  const user = userEvent.setup();
  const box = screen.getByRole('checkbox', { name: '作用范围' }) as HTMLInputElement;
  await user.click(box);
  expect(box.checked).toBe(true);
  expect(screen.getByTestId('state').textContent).toBe('on');
  await user.click(box);
  expect(box.checked).toBe(false);
  await user.click(screen.getByText('作用范围'));
  expect(box.checked).toBe(true);
});
