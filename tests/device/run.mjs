import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
function run(args, input) {
  const result = spawnSync(adb, ['-s', serial, ...args], { input, encoding: 'utf8', timeout: 60_000, maxBuffer: 2 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`adb ${args[0]} failed (${result.status}):\n${result.stdout}\n${result.stderr}`);
  return result.stdout.trim();
}

const before = run(['shell', 'pidof system_server']);
const since = run(['shell', "date '+%m-%d %H:%M:%S.000'"]);
assert.ok(before, 'system_server must already be running');
const remote = run(['shell', 'mktemp -d /data/local/tmp/justlocation-probe.XXXXXX']);
assert.match(remote, /^\/data\/local\/tmp\/justlocation-probe\.[A-Za-z0-9]{6}$/);
const files = [
  ['build/cargo/aarch64-linux-android/release/justlocationd', 'justlocationd'],
  ['build/probe/backend-tests', 'backend-tests'],
  ['build/native/justlocation_loader_probe', 'loader-probe'],
  ['build/native/justlocation_veneer_probe', 'veneer-probe'],
  ['build/native/libjustlocation_bootstrap_probe.so', 'libjustlocation_bootstrap_probe.so'],
  ['build/native/libjustlocation_probe.so', 'libjustlocation_probe.so'],
  ['build/native/libjustlocation_runtime.so', 'libjustlocation_runtime.so'],
  ['build/bridge/classes.dex', 'classes.dex'],
  ['build/native/_deps/shadowhook-build/libshadowhook.so', 'libshadowhook.so'],
  ['build/native/_deps/shadowhook-build/libshadowhook_nothing.so', 'libshadowhook_nothing.so'],
  ['build/probe/probe.zip', 'probe.zip'],
];
try {
  for (const [source, name] of files) run(['push', join(root, source), `${remote}/${name}`]);
  run(['shell', `chmod 500 ${remote}/justlocationd ${remote}/backend-tests ${remote}/loader-probe ${remote}/veneer-probe; chmod 400 ${remote}/probe.zip ${remote}/classes.dex`]);
  console.log(run(['shell', `env -u LD_LIBRARY_PATH timeout 15 ${remote}/loader-probe ${remote}`]));
  console.log(run(['shell', `LD_LIBRARY_PATH=${remote} timeout 10 ${remote}/veneer-probe`]));
  console.log(run(['shell', `TMPDIR=${remote} timeout 15 ${remote}/backend-tests --nocapture`]));
  const position = { latitude: 31.2, longitude: 121.5, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
  const frames = [
    { version: 1, op: 'status' },
    { version: 1, op: 'start', config: { position, scope: { mode: 'apps', packages: ['me.idk.justlocation.probe'] } } },
    { version: 1, op: 'update', position: { ...position, latitude: 999 } },
    { version: 1, op: 'status' },
    { version: 1, op: 'stop' },
    { version: 1, op: 'shutdown' },
  ];
  // stdio owns in-memory state only: no root, socket, saved config or Android location calls.
  const replies = run(['shell', `${remote}/justlocationd stdio`], frames.map(frame => JSON.stringify(frame)).join('\n') + '\n')
    .split(/\r?\n/).map(line => JSON.parse(line));
  assert.equal(replies.length, frames.length);
  assert.equal(replies[0].state.requested_active, false);
  assert.equal(replies[1].ok, true);
  assert.equal(replies[2].ok, false);
  assert.deepEqual(replies[3].state.config.position, position);
  assert.equal(replies[3].state.requested_active, true);
  assert.equal(replies[3].state.hook_connected, false);
  assert.equal(replies[4].state.requested_active, false);
  assert.equal(replies[5].state.requested_active, false);
  console.log('PASS: Android ARM64 backend start/rejected update/stop/shutdown (memory only)');
  const output = run(['shell', `CLASSPATH=${remote}/probe.zip LD_LIBRARY_PATH=${remote} timeout 45 app_process /system/bin --nice-name=justlocation-probe me.idk.justlocation.bridge.RuntimeProbe ${remote}`]);
  console.log(output);
  assert.ok(output.includes('JUSTLOCATION_PROBE_OK'));
  const bridge = run(['shell', `env -u LD_LIBRARY_PATH CLASSPATH=${remote}/probe.zip timeout 45 app_process /system/bin --nice-name=justlocation-probe me.idk.justlocation.bridge.RuntimeProbe ${remote} bridge`]);
  console.log(bridge);
  assert.ok(bridge.includes('JUSTLOCATION_BRIDGE_PROBE_OK'));
} catch (error) {
  console.error(run(['logcat', '-d', '-T', since, '-v', 'brief', '-s', 'AndroidRuntime:E', 'JustLocation:V', 'LSPlant:V', 'shadowhook_tag:V', 'appproc:V']));
  throw error;
} finally {
  // Only exact files created above; never remove modules, app data or a shared directory.
  run(['shell', `rm -f ${files.map(([, name]) => `${remote}/${name}`).join(' ')}; rmdir ${remote}`]);
  assert.equal(run(['shell', 'pidof system_server']), before, 'system_server PID changed during probe');
  assert.equal(run(['shell', 'getprop sys.boot_completed']), '1');
  console.log('PASS: temporary files removed; system_server unchanged');
}
