import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';

// Collects one real environment snapshot on the device, stores it through an isolated daemon and
// checks the stored place. The installed module's configuration is not modified.
const [adbPath, serial, hostBinary] = process.argv.slice(2);
if (!adbPath || !serial || !hostBinary)
  throw new Error('Usage: node run-environment.mjs ADB SERIAL ARM64_BINARY');
const adb = (...args) =>
  execFileSync(adbPath, ['-s', serial, ...args], {
    encoding: 'utf8',
    // One collection waits for a real fix, so CLI calls need more than the default timeout.
    timeout: 120_000,
    maxBuffer: 4 * 1024 * 1024,
  }).trim();
const quote = (value) => "'" + String(value).replaceAll("'", "'\\''") + "'";
const root = (command) => adb('shell', 'su -c ' + quote(command));
const directory = '/data/local/tmp/justlocation-environment-' + Date.now();
assert.match(directory, /^\/data\/local\/tmp\/justlocation-environment-\d+$/);
const binary = directory + '/justlocationd';
const invoke = (...args) =>
  root([binary, '--data-dir', directory, '--json', ...args].map(quote).join(' '));
const cli = (...args) => JSON.parse(invoke(...args));
let pid;
async function start() {
  pid = root(
    `${quote(binary)} --data-dir ${quote(directory)} serve > ${quote(directory + '/service.log')} 2>&1 < /dev/null & echo $!`,
  );
  assert.match(pid, /^\d+$/);
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      return cli('status');
    } catch (error) {
      if (attempt === 29) throw error;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
}
function stop() {
  if (!pid) return;
  try {
    cli('shutdown');
  } catch {
    if (root(`readlink /proc/${pid}/exe 2>/dev/null`) === binary) root(`kill ${pid}`);
  }
  pid = undefined;
}
/** Run a command that is expected to fail and return its stderr. */
function failure(...args) {
  try {
    invoke(...args);
  } catch (error) {
    return String(error.stderr ?? error.stdout ?? error.message);
  }
  throw new Error(`expected a failure from: ${args.join(' ')}`);
}

adb('shell', 'mkdir -m 700 ' + quote(directory));
try {
  adb('push', hostBinary, binary);
  root('chmod 700 ' + quote(binary));
  const initial = await start();
  assert.equal(initial.requested_active, false, 'isolated daemon must start stopped');

  // One real measurement. The device must be unlocked and not simulating.
  const snapshot = cli('place', 'collect', 'JL_ENV_Check');
  assert.equal(snapshot.name, 'JL_ENV_Check');
  assert.equal(snapshot.from, 1, 'a collected place is a local measurement');
  assert.ok(Math.abs(snapshot.latitude) > 0.0001 || Math.abs(snapshot.longitude) > 0.0001);
  assert.ok(snapshot.collected.captured_ms > 0);
  assert.equal(snapshot.collected.coordinate_system, 'wgs84');
  assert.equal(snapshot.collected.source, 'device-probe');
  assert.equal(snapshot.collected.capture.location_permission, 'granted');
  assert.equal(snapshot.collected.capture.mock_active, false, 'the probe must not report a mock fix');
  assert.ok(Array.isArray(snapshot.nearbyCells), 'cells attachment must be stored');
  assert.ok(Array.isArray(snapshot.nearbyWifis), 'Wi-Fi attachment must be stored');
  assert.equal(snapshot.collected.cells, snapshot.nearbyCells.length);
  assert.equal(snapshot.collected.wifi, snapshot.nearbyWifis.length);
  console.log(
    `collected ${snapshot.nearbyCells.length} cells, ${snapshot.nearbyWifis.length} Wi-Fi records, ` +
      `satellites extra ${JSON.stringify(snapshot.collected.extras_satellites)}`,
  );
  for (const cell of snapshot.nearbyCells) {
    assert.ok(typeof cell.radio === 'string' && cell.radio.length > 0, 'cell radio type');
    assert.ok(Number.isInteger(cell.area), 'cell area');
    assert.ok(Number.isInteger(cell.id), 'cell identity');
  }

  // A measurement carries tower identities, not tower coordinates.
  const refusal = failure('place', 'start', snapshot.id, '--cells', 'apply');
  assert.match(refusal, /tower coordinate lat/);

  // Wi-Fi carries everything the channel needs and applies without extra data.
  const applied = cli('place', 'start', snapshot.id, '--wifi', 'apply', '--all');
  assert.equal(applied.requested_active, true);
  assert.equal(applied.config.position.latitude, snapshot.latitude);
  assert.equal(applied.wifi.enabled, true);
  assert.equal(applied.wifi.targets.length, snapshot.nearbyWifis.length);
  assert.equal(applied.cells_synthesized, false, 'no cell region was applied');

  // The environment switch is one step: position, attached channels and persistence together.
  cli('stop');
  const listed = cli('place', 'list').find((place) => place.id === snapshot.id);
  assert.ok(listed, 'the collected place must persist');
  assert.deepEqual(listed.nearbyCells, snapshot.nearbyCells);
  assert.deepEqual(listed.nearbyWifis, snapshot.nearbyWifis);
  stop();
  const restored = await start();
  const persisted = cli('place', 'list').find((place) => place.id === snapshot.id);
  assert.deepEqual(persisted.nearbyWifis, snapshot.nearbyWifis, 'attachments survive a restart');
  assert.deepEqual(persisted.collected, snapshot.collected);

  // Explicit clearing is separate from keeping, and never implied by a missing attachment.
  cli('place', 'start', snapshot.id, '--all');
  assert.equal(cli('status').wifi.enabled, true, 'keep leaves Wi-Fi untouched');
  cli('place', 'start', snapshot.id, '--wifi', 'clear', '--all');
  assert.equal(cli('status').wifi.enabled, false);
  assert.equal(cli('status').wifi.targets.length, 0);
  cli('stop');

  // Recording and the environment switch do not overlap.
  const recording = cli('record', 'start', '--manual');
  assert.ok(recording.recording, 'the recording must be active');
  assert.match(failure('place', 'start', snapshot.id, '--all'), /stop recording/);
  cli('record', 'discard');
  assert.equal(cli('status').recording, null);

  cli('place', 'remove', snapshot.id);
  assert.equal(
    cli('place', 'list').some((place) => place.id === snapshot.id),
    false,
    'the test place must be removed',
  );
  console.log('PASS: measured place, attachment refusal, apply/keep/clear, restart persistence');
} finally {
  stop();
  root('rm -r -- ' + quote(directory));
}
