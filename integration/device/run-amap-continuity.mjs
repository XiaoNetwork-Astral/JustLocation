import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';

// Requires the installed probe, its private Android key and in-app SDK consent.
const [
  adb,
  serial,
  output,
  gnssText = 'false',
  durationText = '65',
  modesText = 'gps,high,network',
  cachesText = 'false,true',
] = process.argv.slice(2);
const duration = Number(durationText);
const modes = modesText.split(',');
const caches = cachesText.split(',');
assert.ok(modes.every((mode) => ['gps', 'high', 'network'].includes(mode)));
assert.ok(caches.every((cache) => ['false', 'true'].includes(cache)));
assert.ok(adb && serial && output && ['true', 'false'].includes(gnssText));
assert.ok(Number.isFinite(duration) && duration >= 30 && duration <= 180);
const gnss = gnssText === 'true';
const app = 'me.idk.justlocation.probe';
const binary = '/data/adb/modules/justlocation/bin/justlocationd';
const quote = (value) => "'" + String(value).replaceAll("'", "'\\''") + "'";
function shell(command) {
  const result = spawnSync(adb, ['-s', serial, 'shell', command], {
    encoding: 'utf8',
    timeout: 30000,
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.replaceAll('\r\n', '\n').trim();
}
function request(command) {
  const encoded = Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64');
  const reply = JSON.parse(shell('su -c ' + quote(`${binary} request ${encoded}`)));
  assert.equal(reply.ok, true, reply.error);
  return reply.state;
}
function readCapture(name) {
  return shell(
    'su -c ' +
      quote(
        `if test -f /data/user/0/${app}/files/${name}; then cat /data/user/0/${app}/files/${name}; fi`,
      ),
  )
    .split('\n')
    .filter(Boolean)
    .map(JSON.parse);
}
const uptime = () => Number(shell('cat /proc/uptime').split(' ')[0]) * 1000;
const before = request({ op: 'status' });
assert.equal(before.requested_active, false, 'Preserve an existing simulation.');
assert.equal(before.recording, null, 'Preserve an existing recording.');
assert.equal(before.hook_connected, true, 'System bridge unavailable.');
const position = {
  latitude: 39.907333,
  longitude: 116.391083,
  altitude: 44,
  accuracy: 5,
  speed: 0,
  bearing: 0,
};
const runs = [];
let interrupted = false;
process.on('SIGINT', () => {
  interrupted = true;
});
const saved = { position: before.config.position, scope: before.config.scope, gnss: before.gnss };
try {
  request({ op: 'set_gnss', config: { gnss_enabled: gnss, nmea_enabled: gnss } });
  request({ op: 'start', config: { position, scope: { mode: 'apps', packages: [app] } } });
  for (const cacheText of caches) {
    const cache = cacheText === 'true';
    for (const mode of modes) {
      assert.ok(!interrupted, 'Capture interrupted.');
      shell(`am force-stop ${app}`);
      request({ op: 'update', position });
      const launched = uptime();
      const run = { mode, cache, gnss, trace: [], sdk: [], platform: [] };
      runs.push(run);
      try {
        shell(
          `am start -W -n ${app}/.ChannelCheckActivity --ez autorun true --ez continuity_only true --es amap_mode ${mode} --ez amap_cache ${cache} --el duration_ms ${(duration + 15) * 1000}`,
        );
        let ready = false;
        for (let attempt = 0; attempt < 30; attempt++) {
          await delay(1000);
          const rows = readCapture('amap-continuity.jsonl').filter(
            (row) => row.received_ms >= launched,
          );
          assert.ok(!rows.some((row) => row.event === 'error'), 'SDK initialization failed.');
          if (rows.some((row) => row.event === 'start')) {
            ready = true;
            break;
          }
        }
        assert.ok(ready, 'SDK did not start; check foreground, key and consent.');
        const begin = performance.now();
        let reported = -1;
        while (performance.now() - begin < duration * 1000) {
          assert.ok(!interrupted, 'Capture interrupted.');
          const elapsed = (performance.now() - begin) / 1000;
          const state =
            elapsed >= duration / 3 && elapsed < (duration * 2) / 3
              ? request({ op: 'drive', speed: 1.5, bearing: 90 })
              : request({ op: 'status' });
          run.trace.push({
            time: uptime(),
            elapsed,
            position: state.config.position,
            active: state.requested_active,
            hook: state.hook_connected,
            phone: state.phone_connected,
          });
          const progress = Math.floor(elapsed / 30);
          if (progress !== reported) {
            reported = progress;
            console.log(
              `${mode} cache=${cache} gnss=${gnss} t=${elapsed.toFixed(0)}s system=${state.hook_connected}`,
            );
          }
          await delay(800);
        }
      } finally {
        run.sdk = readCapture('amap-continuity.jsonl').filter((row) => row.received_ms >= launched);
        run.platform = readCapture('location-continuity.jsonl').filter(
          (row) => row.received_ms >= launched,
        );
        writeFileSync(output, JSON.stringify({ saved, runs }, null, 2));
      }
      const fixes = run.sdk.filter((row) => row.event === 'fix');
      assert.ok(
        !fixes.some((row) => /INVALID_USER_(SCODE|KEY)/.test(row.error_info ?? '')),
        'AMap key authentication failed; this run cannot validate location behavior.',
      );
      console.log(
        JSON.stringify({
          mode,
          cache,
          gnss,
          callbacks: fixes.length,
          errors: [...new Set(fixes.map((row) => row.error_code))],
          types: [...new Set(fixes.map((row) => row.location_type))],
          satellites: [...new Set(fixes.map((row) => row.satellites))],
        }),
      );
    }
  }
} finally {
  try {
    request({ op: 'stop' });
    request({ op: 'set_gnss', config: before.gnss });
    request({ op: 'update', position: before.config.position });
    request({ op: 'set_scope', scope: before.config.scope });
  } finally {
    shell(`am force-stop ${app}`);
    writeFileSync(output, JSON.stringify({ saved, runs }, null, 2));
  }
  console.log('RESTORED: simulation stopped; original GNSS, position and scope restored.');
}
