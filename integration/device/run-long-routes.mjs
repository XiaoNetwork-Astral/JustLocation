import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';

const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
const binary = '/data/adb/modules/justlocation/bin/justlocationd';
const quote = (s) => "'" + String(s).replaceAll("'", "'\\''") + "'";
function shell(command, input, timeout = 30_000) {
  const result = spawnSync(adb, ['-s', serial, 'shell', command], {
    input,
    encoding: 'utf8',
    timeout,
    maxBuffer: 32 * 1024 * 1024,
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
function request(command, expected = true) {
  const reply = cli([
    'request',
    Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64'),
  ]);
  assert.equal(reply.ok, expected, reply.error);
  assert.ok(Buffer.byteLength(JSON.stringify(reply)) < 128 * 1024);
  return reply;
}
const before = cli(['status']);
assert.equal(before.requested_active, false);
assert.equal(before.recording, null);
assert.equal(before.recorded, null);
writeFileSync('build/tmp/long-route-before-device.json', JSON.stringify(before, null, 2));
const ids = [];
const point = (i) => ({
  latitude: 31 + i * 0.00001,
  longitude: 121,
  altitude: 0,
  accuracy: 5,
  speed: 0,
  bearing: 0,
});
try {
  for (const [op, key] of [
    ['set_steps', 'steps'],
    ['set_realism', 'realism'],
    ['set_wifi', 'wifi'],
  ]) {
    request({ op, config: { ...before[key], enabled: false } });
  }
  request({
    op: 'set_telephony',
    config: { ...before.telephony, cells_enabled: false, sim_enabled: false },
  });
  cli(['scope', 'set', '--app', 'example.longroute.acceptance']);
  const plan = {
    points: Array.from({ length: 10000 }, (_, i) => point(i)),
    speed: 1000,
    repeat_count: 2,
    repeat_delay: 1,
  };
  const start = performance.now();
  const saved = cli(['route', 'import', 'Long route acceptance'], JSON.stringify(plan));
  ids.push(saved.id);
  assert.equal(saved.point_count, 10000);
  assert.deepEqual(cli(['route', 'export', saved.id]), plan);
  const state = cli(['route', 'start', '--id', saved.id]);
  assert.equal(state.route.point_count, 10000);
  assert.equal(state.route.plan, null);
  assert.equal(cli(['route', 'page', '--offset', '9984']).points.length, 16);
  assert.equal(cli(['route', 'pause']).route.paused, true);
  const paused = cli(['route', 'status']).distance;
  await delay(700);
  assert.equal(cli(['route', 'status']).distance, paused);
  assert.equal(cli(['route', 'resume']).route.paused, false);
  let completed = false;
  for (let i = 0; i < 60; i++) {
    await delay(500);
    const route = cli(['route', 'status']);
    if (route.completed) {
      assert.equal(route.lap, 2);
      completed = true;
      break;
    }
  }
  assert.equal(completed, true);
  assert.equal(cli(['status']).config.position.latitude, plan.points.at(-1).latitude);
  const memory = shell(
    'su -c ' +
      quote(
        'for p in $(pidof justlocationd); do echo PID=$p; cat /proc/$p/status | grep -E "VmRSS|VmHWM"; done',
      ),
  );
  assert.equal(memory.code, 0);
  console.log(
    `PASS 10000-point import/export, referenced playback, pages, pause, two complete laps (${((performance.now() - start) / 1000).toFixed(2)}s)`,
  );
  console.log(memory.out);
  cli(['stop']);
  const gpx = run(['route', 'export-gpx', saved.id]);
  const imported = cli(['route', 'import-gpx', '--speed', '1000'], gpx);
  ids.push(...imported.map((r) => r.id));
  assert.deepEqual(cli(['route', 'export', imported[0].id]).points, plan.points);
  console.log('PASS 10000-point GPX round trip');

  const upload = cli(['route', 'upload', 'begin', '--point-count', '257']);
  run(['route', 'upload', 'finish', upload.upload_id, 'Incomplete'], undefined, 1);
  for (const offset of [256, 0, 128]) {
    const end = Math.min(offset + 128, 257);
    const page = {
      offset,
      total: 257,
      next: end < 257 ? end : null,
      points: plan.points.slice(offset, end),
      breaks: offset === 128 ? [129] : [],
    };
    cli(['route', 'upload', 'append', upload.upload_id], JSON.stringify(page));
    cli(['route', 'upload', 'append', upload.upload_id], JSON.stringify(page));
  }
  const uploaded = cli(['route', 'upload', 'finish', upload.upload_id, 'Chunk acceptance']);
  ids.push(uploaded.id);
  assert.deepEqual(cli(['route', 'export', uploaded.id]).breaks, [129]);
  console.log('PASS missing/reordered/retried upload chunks');

  cli(['record', 'start', '--manual']);
  for (let i = 0; i < 257; i++) {
    if (i === 129) {
      cli(['service', 'restart']);
      assert.equal(cli(['record', 'status']).recording.paused, true);
      assert.equal(cli(['record', 'status']).recording.points, 129);
      request({ op: 'record_point', position: point(i), seconds: 0 }, false);
      cli(['record', 'resume', '--manual']);
    }
    request({ op: 'record_point', position: point(i), seconds: i < 129 ? i : i - 129 });
  }
  cli(['record', 'pause', '--manual']);
  const stopped = cli(['record', 'stop', '--manual']);
  assert.equal(stopped.recorded.point_count, 257);
  assert.deepEqual(stopped.recorded.points, []);
  const recorded = cli(['record', 'export']);
  assert.deepEqual(recorded.points, plan.points.slice(0, 257));
  assert.deepEqual(recorded.breaks, [129]);
  const recordedGpx = run(['record', 'export', '--gpx']);
  assert.equal((recordedGpx.match(/<trkseg>/g) ?? []).length, 2);
  const recording = cli(['record', 'save', 'Recorded acceptance']);
  ids.push(recording.id);
  assert.deepEqual(cli(['route', 'export', recording.id]), recorded);
  assert.equal(cli(['record', 'status']).recorded, null);
  console.log(
    'PASS durable recorder restart, resume, paged export, segment GPX and save/acknowledge',
  );

  cli(['record', 'start']);
  await delay(3000);
  cli(['record', 'pause']);
  const first = cli(['record', 'status']).recording;
  assert.equal(first.paused, true);
  await delay(1000);
  assert.equal(cli(['record', 'status']).recording.points, first.points);
  cli(['record', 'resume']);
  await delay(3000);
  const active = cli(['record', 'status']).recording;
  assert.equal(active.paused, false);
  assert.equal(active.id, first.id);
  if (active.points > 0) {
    const stopped = cli(['record', 'stop']);
    assert.equal(stopped.recorded.point_count, active.points);
    console.log(
      `PASS Android GPS capture pause/resume/stop (${active.points} accepted real samples)`,
    );
  } else {
    throw new Error('Android recorder lifecycle ran, but no real location sample arrived.');
  }
} finally {
  const errors = [];
  const restore = (action) => {
    try {
      action();
    } catch (error) {
      errors.push(error);
    }
  };
  restore(() => cli(['stop']));
  restore(() => cli(['record', 'discard']));
  for (const id of ids) restore(() => cli(['route', 'remove', id]));
  for (const [key, op] of [
    ['realism', 'set_realism'],
    ['steps', 'set_steps'],
    ['wifi', 'set_wifi'],
    ['telephony', 'set_telephony'],
  ]) {
    restore(() => request({ op, config: before[key] }));
  }
  if (before.config) {
    restore(() => request({ op: 'update', position: before.config.position }));
    restore(() => request({ op: 'set_scope', scope: before.config.scope }));
  }
  if (errors.length) throw new AggregateError(errors, 'Device settings restore failed');
  assert.equal(cli(['status']).requested_active, false);
  console.log('RESTORED stopped settings');
}
