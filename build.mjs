import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const config = JSON.parse(readFileSync(join(root, 'project-config.json'), 'utf8'));
const command = process.argv[2] ?? 'help';
const win = process.platform === 'win32';
const exe = win ? '.exe' : '';
const output = join(root, 'build');

function run(program, args, cwd = root, extraEnv = {}, capture = false) {
  const temp = join(output, 'tmp');
  mkdirSync(temp, { recursive: true });
  console.log(`> ${program} ${args.join(' ')}`);
  const result = spawnSync(program, args, {
    cwd, stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit', encoding: 'utf8', env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: join(output, 'cache/playwright'), ...(win ? { TEMP: temp, TMP: temp } : {}), ...extraEnv },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} failed (${result.status ?? result.signal})`);
  return result.stdout;
}

/** 面板（ui/）是独立目录：自带 package.json、测试与参考材料，只通过后台协议与本模块耦合。 */
function npm(args) {
  const cli = join(dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js');
  const cache = ['--cache', join(output, 'cache', 'npm'), '--no-audit', '--no-fund'];
  if (win) run(process.execPath, [cli, ...args, ...cache], join(root, 'ui'));
  else run('npm', [...args, ...cache], join(root, 'ui'));
}

function sdk() {
  const properties = join(root, 'android', 'local.properties');
  const localSdk = existsSync(properties)
    ? readFileSync(properties, 'utf8').match(/^sdk\.dir=(.+)$/m)?.[1].trim().replaceAll('\\\\', '\\').replaceAll('\\:', ':')
    : undefined;
  const path = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT || localSdk;
  if (!path || !existsSync(path)) throw new Error('Set ANDROID_HOME or sdk.dir in android/local.properties.');
  return resolve(path);
}

function java() {
  return process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `java${exe}`) : `java${exe}`;
}

function gradle(tasks) {
  run(java(), ['-classpath', join(root, 'android/gradle/wrapper/gradle-wrapper.jar'),
    'org.gradle.wrapper.GradleWrapperMain', '--console=plain', ...tasks], join(root, 'android'),
  { ANDROID_HOME: sdk() });
}

async function setupCmake() {
  if (!win) return;
  const name = `cmake-${config.cmakeVer}-windows-x86_64`;
  const toolsDir = join(output, 'tools');
  if (existsSync(join(toolsDir, name, 'bin/cmake.exe'))) return;
  mkdirSync(toolsDir, { recursive: true });
  const url = `https://github.com/Kitware/CMake/releases/download/v${config.cmakeVer}/${name}.zip`;
  console.log(`> Download ${url}`);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`CMake download failed: ${response.status}`);
  const archive = join(toolsDir, `${name}.zip`);
  writeFileSync(archive, Buffer.from(await response.arrayBuffer()));
  const jar = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', 'jar.exe') : 'jar.exe';
  run(jar, ['xf', archive], toolsDir);
}

function native(target) {
  const androidSdk = sdk();
  const bundledCmake = join(output, 'tools', `cmake-${config.cmakeVer}-windows-x86_64`, 'bin', 'cmake.exe');
  const cmake = process.env.JUSTLOCATION_CMAKE || (win && existsSync(bundledCmake) ? bundledCmake : join(androidSdk, 'cmake', config.cmakeVer, 'bin', `cmake${exe}`));
  const ndk = join(androidSdk, 'ndk', config.ndkVer);
  const buildDir = join(output, 'native');
  run(cmake, ['-S', join(root, 'native'), '-B', buildDir, '-G', 'Ninja',
    `-DCMAKE_MAKE_PROGRAM=${join(androidSdk, 'cmake', config.ninjaSdkVer, 'bin', `ninja${exe}`)}`,
    `-DCMAKE_TOOLCHAIN_FILE=${join(ndk, 'build/cmake/android.toolchain.cmake')}`,
    `-DANDROID_ABI=${config.abi}`, `-DANDROID_PLATFORM=${config.platform}`,
    '-DANDROID_STL=c++_static', '-DCMAKE_BUILD_TYPE=Release', '-DCMAKE_EXPORT_COMPILE_COMMANDS=ON'], root,
    win ? { GIT_CONFIG_COUNT: '1', GIT_CONFIG_KEY_0: 'http.sslBackend', GIT_CONFIG_VALUE_0: 'openssl' } : {});
  run(cmake, ['--build', buildDir, ...(target ? ['--target', target] : [])]);
}

function testDevice() {
  const serial = process.argv[3];
  if (!serial) throw new Error('Usage: node build.mjs test:device <adb-serial>');
  backend();
  backend(true);
  gradle([':bridge:assembleRelease']);
  bridgeDex();
  native('justlocation_probe');
  const probeDir = join(output, 'probe');
  const classes = join(probeDir, 'classes');
  if (dirname(resolve(classes)) !== resolve(output, 'probe')) throw new Error('Probe classes must stay in build/probe.');
  rmSync(classes, { recursive: true, force: true });
  mkdirSync(classes, { recursive: true });
  const javac = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `javac${exe}`) : `javac${exe}`;
  const jar = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `jar${exe}`) : `jar${exe}`;
  const androidJar = join(sdk(), 'platforms/android-36/android.jar');
  run(javac, ['--release', '17', '-cp', androidJar, '-d', classes,
    join(root, 'tests/device/RuntimeProbe.java')]);
  run(jar, ['--create', '--file', join(probeDir, 'classes.jar'), '--no-manifest', '-C', classes, '.']);
  run(java(), ['-cp', join(sdk(), 'build-tools', config.buildToolsVer, 'lib/d8.jar'),
    'com.android.tools.r8.D8', '--min-api', '35', '--lib', androidJar,
    '--output', join(probeDir, 'probe.zip'), join(probeDir, 'classes.jar')]);
  run(process.execPath, [join(root, 'tests/device/run.mjs'), join(sdk(), 'platform-tools', `adb${exe}`), serial]);
}

function backend(tests = false) {
  const host = { win32: 'windows-x86_64', linux: 'linux-x86_64', darwin: 'darwin-x86_64' }[process.platform];
  if (!host) throw new Error(`Unsupported NDK host: ${process.platform}`);
  const clang = join(sdk(), 'ndk', config.ndkVer, 'toolchains/llvm/prebuilt', host, 'bin', `clang${exe}`);
  const androidTarget = `aarch64-linux-android${config.platform.replace('android-', '')}`;
  const result = run(`cargo${exe}`, [tests ? 'test' : 'build', ...(tests ? ['--lib', '--no-run', '--message-format=json'] : []), '--locked', '--release', '--target', config.rustTarget,
    '--target-dir', join(output, 'cargo'), '--manifest-path', join(root, 'backend/Cargo.toml')], root, {
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER: clang,
    CC_aarch64_linux_android: clang,
    AR_aarch64_linux_android: join(dirname(clang), `llvm-ar${exe}`),
    CFLAGS_aarch64_linux_android: `--target=${androidTarget}`,
    CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS:
      `-C link-arg=--target=${androidTarget} -C link-arg=-Wl,-z,max-page-size=16384`,
  }, tests);
  if (tests) {
    const artifact = result.trim().split(/\r?\n/).map(line => JSON.parse(line))
      .find(item => item.reason === 'compiler-artifact' && item.profile.test && item.executable);
    if (!artifact) throw new Error('Android backend test executable was not produced.');
    mkdirSync(join(output, 'probe'), { recursive: true });
    copyFileSync(artifact.executable, join(output, 'probe/backend-tests'));
  }
}

function android() {
  gradle([':bridge:assembleRelease', ':companion:assembleDebug']);
  bridgeDex();
  mkdirSync(join(root, 'dist'), { recursive: true });
  copyFileSync(join(root, 'android/companion/build/outputs/apk/debug/companion-debug.apk'),
    join(root, 'dist/justlocation-companion-debug.apk'));
}

function bridgeDex() {
  const bridgeDir = join(output, 'bridge');
  mkdirSync(bridgeDir, { recursive: true });
  const jar = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `jar${exe}`) : `jar${exe}`;
  run(jar, ['xf', join(root, 'android/bridge/build/outputs/aar/bridge-release.aar'), 'classes.jar'], bridgeDir);
  run(java(), ['-cp', join(sdk(), 'build-tools', config.buildToolsVer, 'lib/d8.jar'),
    'com.android.tools.r8.D8', '--min-api', '35', '--lib', join(sdk(), 'platforms/android-36/android.jar'),
    '--output', join(bridgeDir, 'classes.zip'), join(bridgeDir, 'classes.jar')]);
  run(jar, ['xf', join(bridgeDir, 'classes.zip'), 'classes.dex'], bridgeDir);
}

function test() {
  run(`cargo${exe}`, ['test', '--locked', '--manifest-path', join(root, 'backend/Cargo.toml'),
    '--target-dir', join(output, 'cargo')]);
  npm(['run', 'check']);
  npm(['test']);
}

function pack() {
  const stage = resolve(output, 'module');
  if (dirname(stage) !== resolve(output)) throw new Error('Module staging must stay inside build/.');
  rmSync(stage, { recursive: true, force: true });
  cpSync(join(root, 'module'), stage, { recursive: true });
  const inputs = [
    ['build/native/libjustlocation.so', 'zygisk/arm64-v8a.so'],
    ['build/cargo/aarch64-linux-android/release/justlocationd', 'bin/justlocationd'],
    ['build/bridge/classes.dex', 'bridge/classes.dex'],
    ['build/native/libjustlocation_runtime.so', 'lib/libjustlocation_runtime.so'],
    ['build/native/_deps/shadowhook-build/libshadowhook.so', 'lib/libshadowhook.so'],
    ['build/native/_deps/shadowhook-build/libshadowhook_nothing.so', 'lib/libshadowhook_nothing.so'],
    ['build/native/_deps/lsplant-src/LICENSE', 'licenses/lsplant.txt'],
    ['build/native/_deps/lsplant-src/lsplant/src/main/jni/external/dex_builder/LICENSE', 'licenses/dexbuilder.txt'],
    ['build/native/_deps/lsplant-src/lsplant/src/main/jni/external/dex_builder/external/parallel_hashmap/LICENSE', 'licenses/parallel-hashmap.txt'],
    ['build/native/_deps/shadowhook-src/LICENSE', 'licenses/shadowhook.txt'],
    ['build/native/_deps/shadowhook-src/shadowhook/src/main/cpp/third_party/xdl/LICENSE', 'licenses/xdl.txt'],
    ['build/native/_deps/shadowhook-src/shadowhook/src/main/cpp/third_party/lss/LICENSE', 'licenses/lss.txt'],
    ['native/include/zygisk.hpp', 'licenses/zygisk.hpp'],
    ['ui/node_modules/react/LICENSE', 'licenses/react.txt'],
    ['ui/node_modules/react-dom/LICENSE', 'licenses/react-dom.txt'],
    ['ui/node_modules/kernelsu/package.json', 'licenses/kernelsu-package.json'],
    ['ui/node_modules/lucide-react/LICENSE', 'licenses/lucide.txt'],
    ['ui/node_modules/leaflet/LICENSE', 'licenses/leaflet.txt'],
  ];
  for (const [source, target] of inputs) {
    const destination = join(stage, target);
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(join(root, source), destination);
  }
  cpSync(join(root, 'ui/dist'), join(stage, 'webroot'), { recursive: true });
  writeChecksums(stage);
  mkdirSync(join(root, 'dist'), { recursive: true });
  const jar = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `jar${exe}`) : `jar${exe}`;
  const zip = join(root, 'dist/justlocation-0.1.0-dev-arm64.zip');
  run(jar, ['--create', '--file', zip, '--no-manifest', '-C', stage, '.']);
  run(jar, ['--list', '--file', zip]);
}

/**
 * 给组装出来的每个文件写一份 `.sha256` 清单，与模块包一起分发。
 *
 * 这样安装脚本可以在设备上逐个核对（模块自带的 util_functions.sh 提供 verify_tree），
 * 运行脚本也能在启动后台前确认二进制没被改动。清单必须在打包时生成，不能提交到仓库：
 * 它描述的是这一次构建的字节内容。
 */
function writeChecksums(stage) {
  const walk = directory => readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const full = join(directory, entry.name);
    if (entry.isDirectory()) return walk(full);
    // 清单文件自身不再生成清单，否则会无限递归。
    return entry.name.endsWith('.sha256') ? [] : [full];
  });
  let count = 0;
  for (const file of walk(stage)) {
    const digest = createHash('sha256').update(readFileSync(file)).digest('hex');
    writeFileSync(`${file}.sha256`, `${digest}\n`);
    count += 1;
  }
  console.log(`checksums: ${count} files`);
}

try {
  switch (command) {
    case 'setup':
      await setupCmake();
      npm(['ci']);
      run(`rustup${exe}`, ['target', 'add', config.rustTarget]);
      break;
    case 'test': test(); break;
    case 'test:e2e': npm(['run', 'test:e2e']); break;
    case 'test:android': gradle([':bridge:testDebugUnitTest', ':companion:testDebugUnitTest']); break;
    case 'test:device': testDevice(); break;
    case 'test:transport-io':
      if (!process.argv[3]) throw new Error('Usage: node build.mjs test:transport-io <adb-serial>');
      run(process.execPath, [join(root, 'tests/device/run-transport-io.mjs'), sdk(), process.argv[3], config.ndkVer]);
      break;
    case 'test:transport':
      if (!process.argv[3]) throw new Error('Usage: node build.mjs test:transport <adb-serial>');
      native('justlocation_transport_probe');
      run(process.execPath, [join(root, 'tests/device/run-transport.mjs'), join(sdk(), 'platform-tools', `adb${exe}`), process.argv[3]]);
      break;
    case 'native': native(); break;
    case 'backend': backend(); break;
    case 'ui': npm(['run', 'build']); break;
    case 'android': android(); break;
    case 'pack': pack(); break;
    case 'build': native(); backend(); npm(['run', 'build']); android(); pack(); break;
    case 'help':
      console.log('node build.mjs <setup|test|test:e2e|test:android|test:device <serial>|test:transport <serial>|test:transport-io <serial>|build|pack|native|backend|ui|android>');
      break;
    default: throw new Error(`Unknown command: ${command}`);
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
