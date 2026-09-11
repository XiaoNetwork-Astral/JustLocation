import { exec as ksuExec } from 'kernelsu';

export interface Joystick {
  check(): Promise<void>;
  open(kmh: number): Promise<void>;
  close(): Promise<void>;
  /** 查询悬浮摇杆此刻是否真的在运行；取不到结果时返回 null。 */
  read(): Promise<boolean | null>;
}
type Exec = (command: string) => Promise<{ errno: number; stdout: string; stderr: string }>;
const component = 'me.idk.justlocation.companion';
const serviceName = `${component}/.JoystickService`;
/** 解析 dumpsys 里 "ServiceRecord{... me.idk.justlocation.companion/.JoystickService}" 这类行。 */
export function parseServiceRunning(output: string, name = serviceName): boolean {
  return output.includes(name);
}
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
      const result = await exec(`am stopservice -n ${serviceName}`);
      // Android 停止运行中的服务返回 1，服务本来就没运行返回 0，两种情况都算关闭成功。
      const stopped = result.errno === 1 && /\bService stopped\b/.test(result.stdout);
      if ((result.errno !== 0 && !stopped) || /error|exception/i.test(result.stdout + result.stderr)) throw new Error('无法关闭摇杆，请重试');
    },
    async read() {
      // 页面刷新后前端不记得摇杆是否开着，只看本地状态会把开着的摇杆显示成关着的，
      // 于是再点一次"打开"就会重复启动服务，所以这里以系统记录为准。
      // 查不到就返回 null，由调用方决定要不要沿用旧状态。
      try {
        const result = await exec(`dumpsys activity services ${component}`);
        if (result.errno) return null;
        return parseServiceRunning(result.stdout);
      } catch { return null; }
    },
  };
}
export const joystickControl = createJoystick(async command => {
  if (!('ksu' in window)) throw new Error('请从 KernelSU 管理器打开模块面板');
  return ksuExec(command);
});
