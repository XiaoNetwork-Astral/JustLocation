import { copyFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { gradle, root } from '../tool/build.mjs';

export function build() {
  gradle([':joystick:assembleDebug', ':probe:assembleDebug']);
  mkdirSync(join(root, 'dist'), { recursive: true });
  copyFileSync(join(root, 'app/joystick/build/outputs/apk/debug/joystick-debug.apk'),
    join(root, 'dist/justlocation-joystick.apk'));
  copyFileSync(join(root, 'app/probe/build/outputs/apk/debug/probe-debug.apk'),
    join(root, 'dist/justlocation-probe-debug.apk'));
}
