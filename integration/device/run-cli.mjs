import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

// Current module required. Exercise the public CLI, restore stopped settings, keep enabled.
const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
const binary = '/data/adb/modules/justlocation/bin/justlocationd';
const quote = (s) => "'" + String(s).replaceAll("'", "'\\''") + "'";
function shell(command, input) {
  const result = spawnSync(adb, ['-s', serial, 'shell', command], {
    input,
    encoding: 'utf8',
    timeout: 30_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  return {
    code: result.status,
    out: result.stdout.replaceAll('\r\n', '\n').trim(),
    err: result.stderr.trim(),
  };
}
function run(args, input, expected = 0) {
  const result = shell('su -c ' + quote([binary, '--json', ...args].map(quote).join(' ')), input);
  assert.equal(result.code, expected, result.err || result.out);
  return result.out;
}
const cli = (args, input) => JSON.parse(run(args, input));
function request(command) {
  const encoded = Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64');
  const reply = cli(['request', encoded]);
  assert.equal(reply.ok, true, reply.error);
  return reply.state;
}
const before = cli(['status']);
assert.equal(before.requested_active, false, 'Stop simulation before CLI acceptance.');
assert.equal(before.recording, null, 'An active recording must be preserved.');
assert.equal(before.recorded, null, 'Export the existing recording before CLI acceptance.');
const temporary = '/data/adb/justlocation/cli-acceptance';
let savedPlace, savedRoute;
try {
  run(['scope', 'set', '--app', 'example.cli.selected']);
  run(['realism', 'set', '--enabled', 'false']);
  run(['steps', 'set', '--enabled', 'false']);
  run(['wifi', 'enable', 'false']);
  run(['sim', 'cells', 'false']);
  run(['sim', 'operator', 'false']);
  const started = cli(['start', '--lat', '31.2304', '--lon', '121.4737']);
  assert.equal(started.requested_active, true);
  assert.deepEqual(started.config.scope.packages, ['example.cli.selected']);
  assert.equal(
    cli(['update', '--lat', '31.231', '--lon', '121.474']).config.position.latitude,
    31.231,
  );
  run(['drive', '--speed', '0.75', '--bearing', '90', '--seconds', '1']);
  const watch = run(['status', '--watch', '0.1', '--count', '2']).split('\n').map(JSON.parse);
  assert.equal(watch.length, 2);
  assert.ok(watch[1].config.position.longitude > 121.474);
  assert.equal(cli(['stop']).requested_active, false);
  console.log('PASS static start/update/leased movement/JSON watch/stop');

  const steps = cli([
    'steps',
    'set',
    '--enabled',
    'true',
    '--cadence',
    '3',
    '--daily-reset',
    'true',
  ]).steps;
  const disabled = cli(['steps', 'set', '--enabled', 'false']).steps;
  assert.deepEqual(disabled, { ...steps, enabled: false });
  assert.equal(cli(['gnss', 'set', '--gnss', 'true', '--nmea', 'false']).gnss.gnss_enabled, true);
  assert.equal(cli(['gnss', 'set', '--gnss', 'false']).gnss.nmea_enabled, false);
  assert.equal(cli(['realism', 'set', '--seed', '123']).realism.seed, 123);
  assert.equal(cli(['realism', 'set', '--random-seed']).realism.seed, null);
  const wifi = cli(['wifi', 'add', 'CLI fixture', '--bssid', '02:11:22:33:44:55']).wifi;
  const target = wifi.targets.at(-1);
  assert.equal(target.rssi, -45);
  run(['wifi', 'remove', target.id]);
  run([
    'sim',
    'upsert',
    '--id',
    '1234567',
    '--slot',
    '0',
    '--mcc',
    '460',
    '--mnc',
    '01',
    '--carrier',
    'CLI fixture',
    '--enabled',
    'false',
  ]);
  run(['sim', 'remove', '1234567']);
  console.log('PASS independent setting patches, explicit false, seed reset, Wi-Fi and SIM edits');

  savedPlace = cli([
    '--data-dir',
    temporary,
    'place',
    'save',
    'CLI location',
    '--lat',
    '31.23',
    '--lon',
    '121.47',
  ]);
  const code = run(['--data-dir', temporary, 'place', 'export', savedPlace.id]);
  const imported = cli(['--data-dir', temporary, 'place', 'import'], code);
  assert.notEqual(imported.id, savedPlace.id);
  run(['--data-dir', temporary, 'place', 'pin', savedPlace.id, 'true']);
  run(['--data-dir', temporary, 'place', 'rename', savedPlace.id, 'Renamed']);
  const p = (lat) => ({
    latitude: lat,
    longitude: 121.47,
    altitude: 0,
    accuracy: 5,
    speed: 0,
    bearing: 0,
  });
  const plan = { points: [p(31.23), p(31.231)], speed: 1.4, repeat_count: 2, repeat_delay: 1 };
  savedRoute = cli(['--data-dir', temporary, 'route', 'import', 'CLI route'], JSON.stringify(plan));
  const exported = run(['--data-dir', temporary, 'route', 'export', savedRoute.id]);
  assert.deepEqual(JSON.parse(exported), plan);
  cli(['route', 'start', '--input', '-'], exported);
  assert.equal(cli(['route', 'pause']).route.paused, true);
  assert.equal(cli(['scope', 'set', '--app', 'example.cli.other']).route.paused, true);
  assert.equal(cli(['route', 'resume']).route.paused, false);
  run(['route', 'stop']);
  const gpx = run(['--data-dir', temporary, 'route', 'export-gpx', savedRoute.id]);
  assert.equal(cli(['--data-dir', temporary, 'route', 'import-gpx'], gpx).length, 1);
  const backup = run(['--data-dir', temporary, 'backup', 'export']);
  assert.equal(
    cli(['--data-dir', temporary, 'backup', 'import', '--replace'], backup).replaced,
    true,
  );
  console.log('PASS saved locations/S codes/GPX/backup and route playback across scope changes');

  run(['record', 'start']);
  assert.notEqual(cli(['record', 'status']).recording, null);
  run(['record', 'discard']);
  assert.equal(cli(['record', 'status']).recording, null);
  run(['record', 'start', '--manual']);
  run(['record', 'point', '--lat', '31.23', '--lon', '121.47', '--seconds', '0']);
  run(['record', 'point', '--lat', '31.231', '--lon', '121.47', '--seconds', '10']);
  assert.equal(cli(['record', 'stop', '--manual']).recorded.points.length, 2);
  assert.equal(cli(['record', 'export']).points.length, 2);
  run(['record', 'take']);
  assert.equal(cli(['record', 'status']).recorded, null);
  run(['start', '--lat', '31.23', '--lon', '121.47']);
  run(['joystick', 'open', '--speed', '0.75']);
  await delay(1000);
  const service = shell('dumpsys activity services me.idk.justlocation.joystick').out;
  assert.ok(service.includes('JoystickService'), 'joystick did not start');
  run(['joystick', 'close']);
  run(['stop']);
  console.log('PASS Android GPS recorder lifecycle, manual recording/export, sub-1m/s joystick');

  const oldPid = shell('pidof justlocationd').out;
  assert.equal(cli(['service', 'restart']).running, true);
  const newPid = shell('pidof justlocationd').out;
  assert.notEqual(oldPid, newPid);
  for (let n = 0; n < 20; n++) {
    const state = cli(['status']);
    if (state.hook_connected && state.phone_connected) break;
    await delay(1000);
  }
  const state = cli(['status']);
  assert.equal(state.hook_connected, true);
  assert.equal(state.phone_connected, true);
  assert.deepEqual(state.config.scope.packages, ['example.cli.other']);
  assert.equal(state.requested_active, false);
  assert.equal(cli(['module', 'status']).enabled_next_boot, true);
  assert.ok(cli(['apps']).includes('me.idk.justlocation.joystick'));
  run(['maps', 'settings']);
  run(['cells', 'settings']);
  run(['cells', 'dataset', 'status']);
  run(['scope', 'set', '--all', '--app', 'bad'], undefined, 2);
  run(['steps', 'set', '--cadence', '999'], undefined, 1);
  console.log(
    'PASS service restart/reconnection/persistence, module status, apps, local services, error exits',
  );
} finally {
  const errors = [];
  const restore = (action) => {
    try {
      action();
    } catch (error) {
      errors.push(error);
    }
  };
  restore(() => request({ op: 'stop' }));
  restore(() => run(['joystick', 'close']));
  restore(() => run(['record', 'discard']));
  for (const [key, op] of [
    ['realism', 'set_realism'],
    ['steps', 'set_steps'],
    ['gnss', 'set_gnss'],
    ['wifi', 'set_wifi'],
    ['telephony', 'set_telephony'],
  ]) {
    restore(() => request({ op, config: before[key] }));
  }
  if (before.config) {
    restore(() => request({ op: 'update', position: before.config.position }));
    restore(() => request({ op: 'set_scope', scope: before.config.scope }));
  }
  const cleanup = shell(
    'su -c ' +
      quote(`rm -f ${temporary}/library.json ${temporary}/library.lock && rmdir ${temporary}`),
  );
  assert.equal(cleanup.code, 0, cleanup.err);
  assert.equal(cli(['module', 'status']).enabled_next_boot, true);
  assert.equal(cli(['status']).requested_active, false);
  if (errors.length) {
    for (const error of errors) console.error('RESTORE FAILED:', error.message);
    process.exitCode = 1;
  } else {
    console.log('RESTORED stopped settings; module remains enabled');
  }
}
