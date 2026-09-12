import { expect, it, vi } from 'vitest';
import { createJoystick } from './joystick';

it('accepts Android stopservice success code 1 and an already stopped service', async () => {
  const exec = vi.fn().mockResolvedValueOnce({ errno: 1, stdout: 'Stopping service: Intent {}\nService stopped', stderr: '' })
    .mockResolvedValueOnce({ errno: 0, stdout: 'Service not stopped: was not running.', stderr: '' })
    .mockResolvedValueOnce({ errno: 1, stdout: '', stderr: 'SecurityException: denied' });
  const joystick = createJoystick(exec);
  await expect(joystick.close()).resolves.toBeUndefined();
  await expect(joystick.close()).resolves.toBeUndefined();
  await expect(joystick.close()).rejects.toThrow('关闭');
});

it('checks installation and sends a validated speed to the joystick caller activity', async () => {
  const exec = vi.fn().mockResolvedValue({ errno: 0, stdout: 'package:/data/app/test/base.apk', stderr: '' });
  const joystick = createJoystick(exec);
  await joystick.check();
  await joystick.open(5.4);
  // 界面给 km/h，服务要 m/s；不能直接起前台服务（shell 从后台启动会被系统拒），
  // 所以先唤起透明空壳 Activity。
  expect(exec.mock.calls[1][0]).toContain('am start');
  expect(exec.mock.calls[1][0]).toContain('CallActivity');
  expect(exec.mock.calls[1][0]).toContain('--ef speed 1.5');
  await expect(joystick.open(NaN)).rejects.toThrow();
  expect(exec).toHaveBeenCalledTimes(2);
});

it('reports a missing app and rejects Android launch errors even with exit code zero', async () => {
  const exec = vi.fn().mockResolvedValueOnce({ errno: 0, stdout: '', stderr: '' })
    .mockResolvedValue({ errno: 0, stdout: 'Error type 3\nActivity class does not exist.', stderr: '' });
  const joystick = createJoystick(exec);
  await expect(joystick.check()).rejects.toThrow('摇杆 App 未随模块安装');
  await expect(joystick.open(5.4)).rejects.toThrow('打开');
});
