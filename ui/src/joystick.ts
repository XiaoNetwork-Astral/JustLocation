import { exec as ksuExec } from 'kernelsu';

export interface Joystick {
  check(): Promise<void>;
  open(kmh: number): Promise<void>;
  close(): Promise<void>;
  /** 查询悬浮摇杆此刻是否真的在运行；取不到结果时返回 null。 */
  read(): Promise<boolean | null>;
}
type Exec = (command: string) => Promise<{ errno: number; stdout: string; stderr: string }>;
/** 模块自带的摇杆 App：没有启动器与页面，只能由面板经 am 唤起它的前台服务。 */
const component = 'me.idk.justlocation.joystick';
const serviceName = `${component}/.JoystickService`;
/** 解析 dumpsys 里 "ServiceRecord{... me.idk.justlocation.joystick/.JoystickService}" 这类行。 */
export function parseServiceRunning(output: string, name = serviceName): boolean {
  return output.includes(name);
}
export function createJoystick(exec: Exec): Joystick {
  return {
    async check() {
      const result = await exec(`pm path ${component}`);
      if (result.errno || !result.stdout.includes('package:')) throw new Error('摇杆 App 未随模块安装，请重装模块');
    },
    async open(kmh) {
      if (!Number.isFinite(kmh) || kmh <= 0 || kmh > 3600) throw new Error('速度须大于 0、不超过 3600 km/h');
      // 系统不允许 shell 从后台直接启动前台服务，所以先唤起一个透明空壳 Activity，
      // 由服务自己的进程在前台状态下把服务拉起来。界面给的是 km/h，服务要 m/s。
      const metersPerSecond = kmh / 3.6;
      const result = await exec(`am start -n ${component}/.CallActivity --ef speed ${metersPerSecond}`);
      if (result.errno || /error|exception/i.test(result.stdout + result.stderr)) throw new Error('无法打开摇杆，请检查模块是否为最新版本');
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
