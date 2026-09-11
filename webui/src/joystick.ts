import { exec as ksuExec } from 'kernelsu';

export interface Joystick { check(): Promise<void>; open(kmh: number): Promise<void>; close(): Promise<void> }
type Exec = (command: string) => Promise<{ errno: number; stdout: string; stderr: string }>;
const component = 'me.idk.justlocation.companion';
export function createJoystick(exec: Exec): Joystick {
  return {
    async check() {
      const result = await exec(`pm path ${component}`);
      if (result.errno || !result.stdout.includes('package:')) throw new Error('请先安装 JustLocation 可选 App，再打开摇杆');
    },
    async open(kmh) {
      if (!Number.isFinite(kmh) || kmh <= 0 || kmh > 3600) throw new Error('速度须大于 0、不超过 3600 km/h');
      const result = await exec(`am start -n ${component}/.JoystickActivity --es speed ${kmh} --ez open true`);
      if (result.errno || /error|exception/i.test(result.stdout + result.stderr)) throw new Error('无法打开摇杆，请检查可选 App 是否为最新版本');
    },
    async close() {
      const result = await exec(`am stopservice -n ${component}/.JoystickService`);
      // Android returns 1 when it stops a running service, 0 when already stopped.
      const stopped = result.errno === 1 && /\bService stopped\b/.test(result.stdout);
      if ((result.errno !== 0 && !stopped) || /error|exception/i.test(result.stdout + result.stderr)) throw new Error('无法关闭摇杆，请重试');
    },
  };
}
export const joystickControl = createJoystick(async command => {
  if (!('ksu' in window)) throw new Error('请从 KernelSU 管理器打开模块面板');
  return ksuExec(command);
});
