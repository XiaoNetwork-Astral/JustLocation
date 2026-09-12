import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const config = JSON.parse(readFileSync(join(root, 'project-config.json'), 'utf8'));
export const win = process.platform === 'win32';
export const exe = win ? '.exe' : '';
export const output = join(root, 'build');

export function run(program, args, cwd = root, extraEnv = {}, capture = false) {
  const temp = join(output, 'tmp');
  mkdirSync(temp, { recursive: true });
  console.log(`> ${program} ${args.join(' ')}`);
  const result = spawnSync(program, args, {
    cwd,
    stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit',
    encoding: 'utf8',
    env: {
      ...process.env,
      PLAYWRIGHT_BROWSERS_PATH: join(output, 'cache/playwright'),
      ...(win ? { TEMP: temp, TMP: temp } : {}),
      ...extraEnv,
    },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} failed (${result.status ?? result.signal})`);
  return result.stdout;
}

export function sdk() {
  const properties = join(root, 'tool/gradle/local.properties');
  const localSdk = existsSync(properties)
    ? readFileSync(properties, 'utf8')
        .match(/^sdk\.dir=(.+)$/m)?.[1]
        .trim()
        .replaceAll('\\\\', '\\')
        .replaceAll('\\:', ':')
    : undefined;
  const path = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT || localSdk;
  if (!path || !existsSync(path))
    throw new Error('Set ANDROID_HOME or sdk.dir in tool/gradle/local.properties.');
  return resolve(path);
}

export function java() {
  return process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', `java${exe}`) : `java${exe}`;
}

export function gradle(tasks) {
  run(
    java(),
    [
      '-classpath',
      join(root, 'tool/gradle/gradle/wrapper/gradle-wrapper.jar'),
      'org.gradle.wrapper.GradleWrapperMain',
      '--console=plain',
      ...tasks,
    ],
    join(root, 'tool/gradle'),
    { ANDROID_HOME: sdk() },
  );
}

export async function setupCmake() {
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
