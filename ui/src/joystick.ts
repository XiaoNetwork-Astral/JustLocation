import { moduleExec, type Exec } from './platform';

export interface Joystick {
  check(): Promise<void>;
  open(kmh: number): Promise<void>;
  close(): Promise<void>;
  /* Return null when the service state cannot be read. */
  read(): Promise<boolean | null>;
}

const component = 'me.idk.justlocation.joystick';
const serviceName = `${component}/.JoystickService`;

export function parseServiceRunning(output: string, name = serviceName): boolean {
  return output.includes(name);
}
export function createJoystick(exec: Exec): Joystick {
  return {
    async check() {
      const result = await exec(`pm path ${component}`);
      if (result.errno || !result.stdout.includes('package:'))
        throw new Error('摇杆 App 未随模块安装，请重装模块');
    },
    async open(kmh) {
      if (!Number.isFinite(kmh) || kmh <= 0 || kmh > 3600)
        throw new Error('速度须大于 0、不超过 3600 km/h');
      // Start through the activity so the app can launch its foreground service.
      // Convert the UI speed from km/h to the service speed in m/s.
      const metersPerSecond = kmh / 3.6;
      const result = await exec(
        `am start -n ${component}/.CallActivity --ef speed ${metersPerSecond}`,
      );
      if (result.errno || /error|exception/i.test(result.stdout + result.stderr))
        throw new Error('无法打开摇杆，请检查模块是否为最新版本');
    },
    async close() {
      const result = await exec(`am stopservice -n ${serviceName}`);
      // Android returns 1 for a stopped service and 0 when it was already stopped.
      const stopped = result.errno === 1 && /\bService stopped\b/.test(result.stdout);
      if (
        (result.errno !== 0 && !stopped) ||
        /error|exception/i.test(result.stdout + result.stderr)
      )
        throw new Error('无法关闭摇杆，请重试');
    },
    async read() {
      try {
        const result = await exec(`dumpsys activity services ${component}`);
        if (result.errno) return null;
        return parseServiceRunning(result.stdout);
      } catch {
        return null;
      }
    },
  };
}
export const joystickControl = createJoystick(moduleExec);
