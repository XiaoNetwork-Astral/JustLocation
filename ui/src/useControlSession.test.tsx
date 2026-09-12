// @vitest-environment jsdom
import { StrictMode } from 'react';
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Client } from './control';
import type { State } from './protocol';
import { useControlSession } from './useControlSession';

const position = { latitude: 1, longitude: 2, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
const config = { position, scope: { mode: 'all' as const } };
const idle: State = { requested_active: false, config };
const active: State = { requested_active: true, config };
function deferred() {
  let resolve!: (state: State) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<State>((accept, fail) => {
    resolve = accept;
    reject = fail;
  });
  return { promise, resolve, reject };
}
const createJoystick = () => ({
  check: vi.fn().mockResolvedValue(undefined),
  open: vi.fn().mockResolvedValue(undefined),
  close: vi.fn().mockResolvedValue(undefined),
  read: vi.fn().mockResolvedValue(false),
});

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  localStorage.clear();
});

it.each(['success', 'failure'])(
  'ignores a stale poll %s after a control operation',
  async (outcome) => {
    const poll = deferred();
    const client = vi
      .fn<Client>()
      .mockResolvedValueOnce(idle)
      .mockReturnValueOnce(poll.promise)
      .mockResolvedValue(active);
    const { result } = renderHook(() => useControlSession(client, createJoystick(), false));
    await act(async () => {});
    await act(() => vi.advanceTimersByTimeAsync(5000));
    expect(client).toHaveBeenCalledTimes(2);
    await act(() => result.current.toggle());
    expect(result.current.state).toEqual(active);

    await act(async () => {
      if (outcome === 'success') poll.resolve(idle);
      else poll.reject(new Error('outdated failure'));
    });
    expect(result.current.state).toEqual(active);
    expect(result.current.error).toBe('');
  },
);

it('pauses polling during a command and blocks repeated submissions before a render', async () => {
  const command = deferred();
  const client = vi
    .fn<Client>()
    .mockResolvedValueOnce(idle)
    .mockReturnValueOnce(command.promise)
    .mockResolvedValue(active);
  const { result } = renderHook(() => useControlSession(client, createJoystick(), false));
  await act(async () => {});
  let operation: Promise<void>;
  act(() => {
    operation = result.current.toggle();
    void result.current.toggle();
  });
  await act(() => vi.advanceTimersByTimeAsync(7000));
  expect(client).toHaveBeenCalledTimes(2);
  await act(async () => {
    command.resolve(active);
    await operation;
  });
  await act(() => vi.advanceTimersByTimeAsync(2000));
  expect(client).toHaveBeenCalledTimes(3);
});

it('ignores the discarded initial refresh in StrictMode', async () => {
  const discarded = deferred();
  const client = vi.fn<Client>().mockReturnValueOnce(discarded.promise).mockResolvedValue(active);
  const { result, unmount } = renderHook(() => useControlSession(client, createJoystick(), false), {
    wrapper: StrictMode,
  });
  await act(async () => {});
  expect(result.current.state).toEqual(active);
  await act(async () => discarded.resolve(idle));
  expect(result.current.state).toEqual(active);
  expect(result.current.busy).toBe(false);
  unmount();
  await act(() => vi.advanceTimersByTimeAsync(15000));
  expect(client).toHaveBeenCalledTimes(2);
});

it('preserves an idle position draft when polling refreshes channel readiness', async () => {
  const client = vi.fn<Client>().mockResolvedValue(idle);
  const { result } = renderHook(() => useControlSession(client, createJoystick(), false));
  await act(async () => {});
  const draft = { ...position, latitude: 3 };
  await act(() => result.current.select(draft, 'draft'));
  await act(() => vi.advanceTimersByTimeAsync(5000));
  expect(result.current.position).toEqual(draft);
  expect(result.current.name).toBe('draft');
});

it('keeps a failed joystick close visible and retryable after stopping the simulation', async () => {
  const client = vi.fn<Client>().mockResolvedValueOnce(active).mockResolvedValue(idle);
  const joystick = createJoystick();
  joystick.read.mockResolvedValue(true);
  joystick.close.mockRejectedValueOnce(new Error('close failed'));
  const { result } = renderHook(() => useControlSession(client, joystick, false));
  await act(async () => {});
  await act(() => result.current.toggle());
  expect(result.current.state).toEqual(idle);
  expect(result.current.joystickOpen).toBe(true);
  expect(result.current.error).toContain('摇杆没有关闭成功');
  await act(() => result.current.controlJoystick(false));
  expect(result.current.joystickOpen).toBe(false);
  expect(result.current.error).toBe('');
  expect(joystick.close).toHaveBeenCalledTimes(2);
});
