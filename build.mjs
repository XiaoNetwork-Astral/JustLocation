import { join } from 'node:path';
import { config, exe, gradle, root, run, sdk, setupCmake } from './tool/build.mjs';
import { bridge, build as zygisk, native } from './zygisk/build.mjs';
import { build as backend, test as testBackend } from './backend/build.mjs';
import { build as app } from './app/build.mjs';
import { build as ui, npm, test as testUi } from './ui/build.mjs';
import { testDevice } from './integration/build.mjs';
import { pack } from './packaging/build.mjs';

const command = process.argv[2] ?? 'help';
const serial = process.argv[3];

try {
  switch (command) {
    case 'setup':
      await setupCmake();
      npm(['ci']);
      run(`rustup${exe}`, ['target', 'add', config.rustTarget]);
      break;
    case 'test':
      testBackend();
      testUi();
      break;
    case 'test:e2e':
      npm(['run', 'test:e2e']);
      break;
    case 'test:android':
      gradle([':bridge:testDebugUnitTest', ':joystick:testDebugUnitTest']);
      break;
    case 'test:device':
      testDevice(serial);
      break;
    case 'test:checks':
      if (!serial) throw new Error('Usage: node build.mjs test:checks <adb-serial>');
      app();
      run(process.execPath, [
        join(root, 'integration/device/run-checks.mjs'),
        join(sdk(), 'platform-tools', `adb${exe}`),
        serial,
      ]);
      break;
    case 'test:transport-io':
      if (!serial) throw new Error('Usage: node build.mjs test:transport-io <adb-serial>');
      run(process.execPath, [
        join(root, 'zygisk/tests/run-transport-io.mjs'),
        sdk(),
        serial,
        config.ndkVer,
      ]);
      break;
    case 'test:transport':
      if (!serial) throw new Error('Usage: node build.mjs test:transport <adb-serial>');
      native('justlocation_transport_probe');
      run(process.execPath, [
        join(root, 'zygisk/tests/run-transport.mjs'),
        join(sdk(), 'platform-tools', `adb${exe}`),
        serial,
      ]);
      break;
    case 'zygisk':
      zygisk();
      break;
    case 'native':
      native();
      break;
    case 'backend':
      backend();
      break;
    case 'ui':
      ui();
      break;
    case 'app':
      app();
      break;
    case 'android':
      bridge();
      app();
      break;
    case 'pack':
      pack();
      break;
    case 'build':
      zygisk();
      backend();
      ui();
      app();
      pack();
      break;
    case 'help':
      console.log(
        'node build.mjs <setup|test|test:e2e|test:android|test:device <serial>|test:checks <serial>|test:transport <serial>|test:transport-io <serial>|build|pack|zygisk|backend|app|ui|native|android>',
      );
      break;
    default:
      throw new Error(`Unknown command: ${command}`);
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
