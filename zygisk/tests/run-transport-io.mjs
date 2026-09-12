import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const [sdk, serial, ndkVersion] = process.argv.slice(2);
if (!sdk || !serial || !ndkVersion) throw new Error('Provide SDK, explicit serial and NDK version.');
const root = fileURLToPath(new URL('../../', import.meta.url));
const win = process.platform === 'win32';
const adb = join(sdk, 'platform-tools', win ? 'adb.exe' : 'adb');
function run(program, args) {
  const result = spawnSync(program, args, { encoding: 'utf8', timeout: 30_000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  return result.stdout.trim();
}
const device = args => run(adb, ['-s', serial, ...args]);
const triples = { 'x86_64': 'x86_64-linux-android35', 'arm64-v8a': 'aarch64-linux-android35' };
const triple = triples[device(['shell', 'getprop ro.product.cpu.abi'])];
assert.ok(triple, 'Transport I/O probe supports ARM64 and x86_64.');
const host = { win32: 'windows-x86_64', linux: 'linux-x86_64', darwin: 'darwin-x86_64' }[process.platform];
const clang = join(sdk, 'ndk', ndkVersion, 'toolchains/llvm/prebuilt', host, 'bin', win ? 'clang++.exe' : 'clang++');
const output = join(root, 'build/probe', `transport-io-${triple}`);
mkdirSync(join(root, 'build/probe'), { recursive: true });
run(clang, [`--target=${triple}`, '-std=c++17', '-static-libstdc++', '-Wall', '-Wextra', '-Werror',
  '-I', join(root, 'zygisk/src'), join(root, 'zygisk/tests/transport-io.cpp'), '-o', output]);
const before = device(['shell', 'pidof system_server']);
assert.ok(before);
assert.equal(device(['shell', 'getprop sys.boot_completed']), '1');
const remote = device(['shell', 'mktemp -d /data/local/tmp/justlocation-io.XXXXXX']);
assert.match(remote, /^\/data\/local\/tmp\/justlocation-io\.[A-Za-z0-9]{6}$/);
try {
  device(['push', output, `${remote}/probe`]);
  device(['shell', `chmod 500 ${remote}/probe`]);
  console.log(device(['shell', `timeout 15 ${remote}/probe`]));
} finally {
  device(['shell', `rm -f ${remote}/probe; rmdir ${remote}`]);
  assert.equal(device(['shell', 'pidof system_server']), before);
  assert.equal(device(['shell', 'getprop sys.boot_completed']), '1');
  console.log('PASS: isolated I/O probe removed; system_server unchanged');
}
