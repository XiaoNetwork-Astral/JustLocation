import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
function run(args) {
  const result = spawnSync(adb, ['-s', serial, ...args], { encoding: 'utf8', timeout: 30_000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  return result.stdout.trim();
}
const before = run(['shell', 'pidof system_server']);
assert.ok(before);
assert.equal(run(['shell', 'getprop sys.boot_completed']), '1');
const remote = run(['shell', 'mktemp -d /data/local/tmp/justlocation-transport.XXXXXX']);
assert.match(remote, /^\/data\/local\/tmp\/justlocation-transport\.[A-Za-z0-9]{6}$/);
try {
  run([
    'push',
    fileURLToPath(new URL('../../build/native/justlocation_transport_probe', import.meta.url)),
    `${remote}/probe`,
  ]);
  run(['shell', `chmod 500 ${remote}/probe`]);
  // Only this native child changes UID / SELinux domain; no policy or real service is changed.
  console.log(run(['shell', `su -c 'timeout 15 ${remote}/probe'`]));
} finally {
  run(['shell', `rm -f ${remote}/probe; rmdir ${remote}`]);
  assert.equal(run(['shell', 'pidof system_server']), before);
  assert.equal(run(['shell', 'getprop sys.boot_completed']), '1');
  console.log('PASS: transport probe removed; system_server unchanged');
}
