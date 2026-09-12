import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

// Requires the current module and probe APK. Installs the probe, restores stopped settings,
// and leaves the module enabled. Cumulative step counters are never rolled back.
const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
const app = 'me.idk.justlocation.probe';
function run(args) {
  const result = spawnSync(adb, ['-s', serial, ...args], { encoding: 'utf8', timeout: 30_000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result.stdout.trim();
}
const shell = (command) => run(['shell', command]);
function request(command) {
  const encoded = Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64');
  const output = shell(
    `su -c '/data/adb/modules/justlocation/bin/justlocationd request ${encoded}'`,
  );
  const response = JSON.parse(output.slice(output.indexOf('{')));
  assert.equal(response.ok, true, response.error);
  return response.state;
}
function distance(a, b) {
  const rad = Math.PI / 180;
  const h =
    Math.sin(((a.latitude - b.latitude) * rad) / 2) ** 2 +
    Math.cos(a.latitude * rad) *
      Math.cos(b.latitude * rad) *
      Math.sin(((a.longitude - b.longitude) * rad) / 2) ** 2;
  return 2 * 6371008.8 * Math.asin(Math.sqrt(h));
}
const before = request({ op: 'status' });
assert.equal(before.requested_active, false, 'Stop simulation before this acceptance run.');
const position = {
  latitude: 31.2304,
  longitude: 121.4737,
  altitude: 42,
  accuracy: 5,
  speed: 0,
  bearing: 0,
};
let installed = false;
try {
  run(['install', '-r', '-g', 'dist/justlocation-probe-debug.apk']);
  installed = true;
  request({ op: 'set_realism', config: { enabled: true, seed: 123, period_seconds: 2 } });
  request({
    op: 'set_steps',
    config: { enabled: true, cadence: 2, movement_linked: true, stride_m: 0.75 },
  });
  request({ op: 'start', config: { position, scope: { mode: 'apps', packages: [app] } } });
  const total = request({ op: 'status' }).step_count.total;
  shell(`am force-stop ${app}`);
  shell(`am start -W -n ${app}/.ChannelCheckActivity --ez autorun true --ez movement_only true`);
  const pid = shell(`pidof ${app}`);
  let log = '';
  for (let attempt = 0; attempt < 75; attempt++) {
    await delay(1000);
    log = shell(`logcat -d --pid=${pid} -s JustLocationCheck:I '*:S'`);
    if (log.includes('CHECK_REPORT_END')) break;
  }
  assert.ok(log.includes('CHECK_REPORT_END'), 'Probe report timed out');
  const gps = log.split('\n').find((line) => /gps .*source=live/.test(line));
  assert.ok(gps, 'No live GPS samples from Android LocationManager');
  const samples = gps
    .split('all=')[1]
    .trim()
    .split('|')
    .map((pair) => {
      const [latitude, longitude] = pair.split(',').map(Number);
      return { latitude, longitude };
    });
  assert.ok(samples.length >= 3);
  assert.ok(
    samples.every((point) => distance(position, point) <= 2.2),
    gps,
  );
  assert.ok(
    new Set(samples.map((point) => JSON.stringify(point))).size > 1,
    'GPS output did not vary',
  );
  assert.equal(request({ op: 'status' }).step_count.total, total, 'Static drift added steps');
  console.log(
    `PASS: ${samples.length} live app GPS fixes stayed inside the 2m drift radius; no static steps`,
  );

  request({ op: 'stop' });
  request({
    op: 'set_realism',
    config: { enabled: true, seed: 123, drift_radius_m: 0, altitude_m: 0, bearing_degrees: 0 },
  });
  request({ op: 'start', config: { position, scope: { mode: 'apps', packages: [app] } } });
  request({ op: 'drive', speed: 0.75, bearing: 90 });
  await delay(3500);
  const stopped = request({ op: 'status' });
  const moved = distance(position, stopped.config.position);
  assert.ok(moved >= 1.35 - 0.01 && moved <= 1.65 + 0.01, `Moved ${moved}m`);
  assert.equal(stopped.config.position.speed, 0);
  console.log(`PASS: varied joystick travelled ${moved.toFixed(3)}m and stopped at its 2s lease`);
} finally {
  request({ op: 'stop' });
  request({ op: 'set_realism', config: before.realism });
  request({ op: 'set_steps', config: before.steps });
  if (before.config) {
    request({ op: 'start', config: before.config });
    request({ op: 'stop' });
  }
  if (installed) run(['uninstall', app]);
}
