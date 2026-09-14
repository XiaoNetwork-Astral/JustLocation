import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { config, exe, gradle, java, output, root, run, sdk, win } from '../tool/build.mjs';

export function native(target) {
  const androidSdk = sdk();
  const bundledCmake = join(
    output,
    'tools',
    `cmake-${config.cmakeVer}-windows-x86_64`,
    'bin',
    'cmake.exe',
  );
  const cmake =
    process.env.JUSTLOCATION_CMAKE ||
    (win && existsSync(bundledCmake)
      ? bundledCmake
      : join(androidSdk, 'cmake', config.cmakeVer, 'bin', `cmake${exe}`));
  const ndk = join(androidSdk, 'ndk', config.ndkVer);
  const buildDir = join(output, 'native');
  run(
    cmake,
    [
      '-S',
      join(root, 'zygisk'),
      '-B',
      buildDir,
      '-G',
      'Ninja',
      `-DCMAKE_MAKE_PROGRAM=${join(androidSdk, 'cmake', config.ninjaSdkVer, 'bin', `ninja${exe}`)}`,
      `-DCMAKE_TOOLCHAIN_FILE=${join(ndk, 'build/cmake/android.toolchain.cmake')}`,
      `-DANDROID_ABI=${config.abi}`,
      `-DANDROID_PLATFORM=${config.platform}`,
      '-DANDROID_STL=c++_static',
      '-DCMAKE_BUILD_TYPE=Release',
      '-DCMAKE_EXPORT_COMPILE_COMMANDS=ON',
    ],
    root,
    win
      ? {
          GIT_CONFIG_COUNT: '1',
          GIT_CONFIG_KEY_0: 'http.sslBackend',
          GIT_CONFIG_VALUE_0: 'openssl',
        }
      : {},
  );
  run(cmake, ['--build', buildDir, ...(target ? ['--target', target] : [])]);
}

export function bridge() {
  gradle([':bridge:assembleRelease']);
  bridgeDex();
}

export function build() {
  native();
  bridge();
}

function bridgeDex() {
  const bridgeDir = join(output, 'bridge');
  mkdirSync(bridgeDir, { recursive: true });
  const jar = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `jar${exe}`) : `jar${exe}`;
  // Rebuild the input jar from current compiler output so d8 cannot reuse stale classes.
  const classes = join(root, 'zygisk/bridge/build/tmp/kotlin-classes/release');
  const javac = join(
    root,
    'zygisk/bridge/build/intermediates/javac/release/compileReleaseJavaWithJavac/classes',
  );
  const inputs = [classes, javac].filter(existsSync);
  if (!inputs.length)
    throw new Error('Bridge class output is missing; run the Gradle build first.');
  const staging = join(bridgeDir, 'input');
  rmSync(staging, { recursive: true, force: true });
  rmSync(join(bridgeDir, 'classes.zip'), { force: true });
  rmSync(join(bridgeDir, 'classes.dex'), { force: true });
  mkdirSync(staging, { recursive: true });
  for (const input of inputs) {
    cpSync(input, staging, { recursive: true });
  }
  run(jar, ['cf', join(bridgeDir, 'classes.jar'), '-C', staging, '.']);
  run(java(), [
    '-cp',
    join(sdk(), 'build-tools', config.buildToolsVer, 'lib/d8.jar'),
    'com.android.tools.r8.D8',
    '--min-api',
    '35',
    '--lib',
    join(sdk(), 'platforms/android-36/android.jar'),
    '--output',
    join(bridgeDir, 'classes.zip'),
    join(bridgeDir, 'classes.jar'),
  ]);
  run(jar, ['xf', join(bridgeDir, 'classes.zip'), 'classes.dex'], bridgeDir);
}

// Zygisk builds its probes; integration scripts compose artifacts and run device checks.
export function probe() {
  bridge();
  native('justlocation_probe');
  // The step state probe exercises the shipped state machine without the platform around it.
  native('justlocation_step_state_probe');
  const probeDir = join(output, 'probe');
  const classes = join(probeDir, 'classes');
  if (dirname(resolve(classes)) !== resolve(output, 'probe'))
    throw new Error('Probe classes must stay in build/probe.');
  rmSync(classes, { recursive: true, force: true });
  mkdirSync(classes, { recursive: true });
  const javac = process.env.JAVA_HOME
    ? join(process.env.JAVA_HOME, 'bin', `javac${exe}`)
    : `javac${exe}`;
  const jar = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `jar${exe}`) : `jar${exe}`;
  const androidJar = join(sdk(), 'platforms/android-36/android.jar');
  run(javac, [
    '--release',
    '17',
    '-cp',
    androidJar,
    '-d',
    classes,
    join(root, 'zygisk/tests/RuntimeProbe.java'),
  ]);
  run(jar, [
    '--create',
    '--file',
    join(probeDir, 'classes.jar'),
    '--no-manifest',
    '-C',
    classes,
    '.',
  ]);
  run(java(), [
    '-cp',
    join(sdk(), 'build-tools', config.buildToolsVer, 'lib/d8.jar'),
    'com.android.tools.r8.D8',
    '--min-api',
    '35',
    '--lib',
    androidJar,
    '--output',
    join(probeDir, 'probe.zip'),
    join(probeDir, 'classes.jar'),
  ]);
}
