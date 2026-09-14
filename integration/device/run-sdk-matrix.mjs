import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';

// Runs the SDK source matrix on a connected device while the module simulates one position:
// every location mode with and without its cache, each observed for the requested duration.
// Installed module and probe required. Everything is read through root; nothing is installed.
//
// Work is resumable: --modes/--caches select the configurations, every configuration is written
// to the report as soon as it finishes, and each finished configuration removes its own capture
// file. The report records only results captured during this invocation, so a later run cannot
// count an earlier run's data.
const [adb, serial, output, durationText = '95', ...options] = process.argv.slice(2);
const duration = Number(durationText);
if (!adb || !serial || !output || !Number.isFinite(duration) || duration < 60 || duration > 240)
  throw new Error('Provide adb, serial, local report path and per-mode duration in seconds (60..240).');
function selected(name, all) {
  const index = options.indexOf(`--${name}`);
  if (index < 0) return all;
  const value = options[index + 1];
  if (!value) throw new Error(`--${name} needs a value`);
  return value.split(',');
}
const modes = selected('modes', ['gps', 'high', 'network']);
const caches = selected('caches', ['false', 'true']).map((value) => value === 'true');
for (const mode of modes)
  if (!['gps', 'high', 'network'].includes(mode)) throw new Error(`unknown mode ${mode}`);
const app = 'me.idk.justlocation.probe';
const binary = '/data/adb/modules/justlocation/bin/justlocationd';
const rows = (name) => `/data/user/0/${app}/files/${name}`;
const capture = rows('amap-continuity.jsonl');
const quote = (value) => "'" + String(value).replaceAll("'", "'\\''") + "'";
function shell(command) {
  const result = spawnSync(adb, ['-s', serial, 'shell', command], {
    encoding: 'utf8',
    timeout: 60_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result.stdout.replaceAll('\r\n', '\n').trim();
}
/** Root helper that tolerates a missing file, so it can be used to clear or size a capture. */
function rootRead(command) {
  const result = spawnSync(adb, ['-s', serial, 'shell', 'su -c ' + quote(command)], {
    encoding: 'utf8',
    timeout: 60_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  return (result.stdout || '').replaceAll('\r\n', '\n').trim();
}
function request(command) {
  const encoded = Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64');
  const reply = JSON.parse(shell('su -c ' + quote(`${binary} request ${encoded}`)));
  assert.equal(reply.ok, true, reply.error);
  return reply.state;
}
function readJsonl(name) {
  const text = shell('su -c ' + quote(`cat ${rows(name)} 2>/dev/null`));
  return text
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}
/** Device wall clock in milliseconds; the SDK's own monotonic field is not usable (see below). */
function deviceClockMs() {
  return Number(shell('date +%s%3N'));
}
const position = {
  latitude: 39.907333,
  longitude: 116.391083,
  altitude: 44,
  accuracy: 5,
  speed: 0,
  bearing: 0,
};
const summary = [];
// Results are written after every configuration, so an interrupted run keeps what it finished.
function publish() {
  writeFileSync(output, JSON.stringify({ duration, configurations: summary }, null, 2));
}
const before = request({ op: 'status' });
assert.equal(before.requested_active, false, 'An existing simulation must be preserved.');
assert.equal(before.recording, null, 'An existing recording must be preserved.');
assert.equal(before.hook_connected, true, 'System bridge unavailable.');
try {
  request({ op: 'start', config: { position, scope: { mode: 'apps', packages: [app] } } });
  for (const mode of modes) {
    for (const cache of caches) {
      shell(`am force-stop ${app}`);
      const launchedAt = Number(shell('cat /proc/uptime').split(' ')[0]) * 1000;
      shell(
        `am start -W -n ${app}/.ChannelCheckActivity --ez autorun true ` +
          `--ez continuity_only true --es amap_mode ${mode} --ez amap_cache ${cache} ` +
          `--el duration_ms ${(duration + 5) * 1000}`,
      );
      // The app needs its registration window before it can report anything.
      const deadline = Date.now() + 60_000;
      let started = false;
      while (Date.now() < deadline) {
        await delay(2000);
        const start = readJsonl('amap-continuity.jsonl').find((row) => row.event === 'start');
        if (start && start.received_ms >= launchedAt) {
          started = true;
          break;
        }
      }
      assert.ok(started, `${mode}/${cache}: the SDK capture did not start`);
      const runState = request({ op: 'status' });
      assert.equal(runState.requested_active, true, `${mode}/${cache}: the simulation stopped`);
      console.log(`collecting ${mode} cache=${cache} for ${duration}s`);
      await delay(duration * 1000);
      const all = readJsonl('amap-continuity.jsonl');
      const startRow = all.find((row) => row.event === 'start');
      const fixes = all.filter((row) => row.event === 'fix');
      const errors = fixes.filter((row) => row.error_code !== 0);
      const good = fixes.filter((row) => row.error_code === 0);
      const clock = deviceClockMs();
      const summaryRow = {
        mode,
        cache,
        sdk_version: startRow?.sdk_version ?? null,
        app_version: startRow?.app_version ?? null,
        api_level: startRow?.api_level ?? null,
        fixes: fixes.length,
        good: good.length,
        errors: errors.length,
        error_codes: [...new Set(errors.map((row) => row.error_code))],
        location_types: [...new Set(good.map((row) => row.location_type))],
        sources: [...new Set(good.map((row) => row.provider))],
        coord_types: [...new Set(good.map((row) => row.coord_type))],
        accuracy: good.map((row) => row.accuracy).sort((a, b) => a - b).slice(0, 3),
        // AMap leaves `Location.elapsedRealtimeNanos` at zero, so the fix age comes from the wall
        // clock it does report; mixing the two would produce a meaningless age.
        elapsed_realtime_reported: [...new Set(good.map((row) => row.fix_elapsed_ms))],
        last: good.length
          ? {
              latitude: good.at(-1).latitude,
              longitude: good.at(-1).longitude,
              age_ms: clock - good.at(-1).fix_time_ms,
              location_type: good.at(-1).location_type,
            }
          : null,
      };
      console.log(
        `${mode} cache=${cache}: ${good.length} ok / ${errors.length} error ` +
          `types=${summaryRow.location_types.join('/') || '-'} ` +
          `first accuracy=${summaryRow.accuracy[0] ?? '-'}`,
      );
      summary.push(summaryRow);
      publish();
      shell(`am force-stop ${app}`);
      // Clearing the capture marks this configuration finished: the next run cannot count it, and
      // an interrupted run leaves no data behind.
      rootRead(`rm -f ${capture}`);
    }
  }
  const status = request({ op: 'status' });
  for (const row of summary) {
    assert.ok(row.sdk_version, `${row.mode}/${row.cache}: the SDK version was not reported`);
    assert.ok(row.app_version, `${row.mode}/${row.cache}: the consumer identity was not reported`);
    assert.ok(row.good > 0, `${row.mode}/${row.cache}: no successful fix`);
    // The SDK must not invent a location of its own: every result has to stay at the simulated
    // target, whichever source it used.
    if (row.last) {
      const metres = Math.hypot(
        (row.last.latitude - status.config.position.latitude) * 111_320,
        (row.last.longitude - status.config.position.longitude) *
          111_320 *
          Math.cos((row.last.latitude * Math.PI) / 180),
      );
      row.divergence_m = Number(metres.toFixed(2));
      assert.ok(
        metres < 200,
        `${row.mode}/${row.cache}: the SDK result diverged by ${metres.toFixed(1)}m`,
      );
    }
  }
  console.log('--- SDK source matrix ---');
  for (const row of summary)
    console.log(
      `${row.mode.padEnd(7)} cache=${String(row.cache).padEnd(5)} ok=${String(row.good).padEnd(4)} ` +
        `err=${String(row.errors).padEnd(3)} types=${row.location_types.join('/') || '-'} ` +
        `sources=${row.sources.join('/') || '-'} ` +
        `type4=${row.location_types.includes(4) ? 'yes' : 'no'} last_age=${row.last?.age_ms ?? '-'}ms ` +
        `divergence=${row.divergence_m ?? '-'}m`,
    );
  console.log(`SDK version ${summary[0].sdk_version}, consumer ${summary[0].app_version}`);
  console.log('PASS: SDK source matrix observed inside one simulation session');
} finally {
  publish();
  request({ op: 'stop' });
  request({ op: 'update', position: before.config.position });
  request({ op: 'set_scope', scope: before.config.scope });
  shell(`am force-stop ${app}`);
}
