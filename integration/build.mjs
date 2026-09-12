import { join } from 'node:path';
import { exe, root, run, sdk } from '../tool/build.mjs';
import { build as backend } from '../backend/build.mjs';
import { probe } from '../zygisk/build.mjs';

export function testDevice(serial) {
  if (!serial) throw new Error('Usage: node build.mjs test:device <adb-serial>');
  backend();
  backend(true);
  probe();
  run(process.execPath, [
    join(root, 'integration/device/run.mjs'),
    join(sdk(), 'platform-tools', `adb${exe}`),
    serial,
  ]);
}
