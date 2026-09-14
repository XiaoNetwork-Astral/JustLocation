import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';

// Installed module and probe required. Preserve configuration, finish stopped, no reboot/install.
const [adb, serial, output, durationText = '180'] = process.argv.slice(2);
const duration = Number(durationText);
if (!adb || !serial || !output || !Number.isFinite(duration) || duration < 90 || duration > 540)
  throw new Error('Provide adb, serial, local report path and duration in seconds (90..540).');
const app = 'me.idk.justlocation.probe';
const binary = '/data/adb/modules/justlocation/bin/justlocationd';
const quote = (value) => "'" + String(value).replaceAll("'", "'\\''") + "'";
function shell(command) {
  const result = spawnSync(adb, ['-s', serial, 'shell', command], {
    encoding: 'utf8',
    timeout: 30_000,
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result.stdout.replaceAll('\r\n', '\n').trim();
}
function request(command) {
  const encoded = Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64');
  const reply = JSON.parse(shell('su -c ' + quote(`${binary} request ${encoded}`)));
  assert.equal(reply.ok, true, reply.error);
  return reply.state;
}
const before = request({ op: 'status' });
assert.equal(before.requested_active, false, 'An existing simulation must be preserved.');
assert.equal(before.recording, null, 'An existing recording must be preserved.');
assert.equal(before.hook_connected, true, 'System bridge unavailable.');
const position = {
  latitude: 39.907333,
  longitude: 116.391083,
  altitude: 44,
  accuracy: 5,
  speed: 0,
  bearing: 0,
};
const observations = [];
let callbacks = [];
let stoppedAt;
try {
  request({ op: 'start', config: { position, scope: { mode: 'apps', packages: [app] } } });
  shell(`am force-stop ${app}`);
  const launchedAt = Number(shell('cat /proc/uptime').split(' ')[0]) * 1000;
  shell(
    `am start -W -n ${app}/.ChannelCheckActivity --ez autorun true ` +
      `--ez continuity_only true --el duration_ms ${(duration + 50) * 1000}`,
  );
  let ready = false;
  for (let attempt = 0; attempt < 15; attempt++) {
    await delay(1000);
    const first = shell(
      'su -c ' +
        quote(
          `if test -f /data/user/0/${app}/files/location-continuity.jsonl; then ` +
            `head -n 1 /data/user/0/${app}/files/location-continuity.jsonl; fi`,
        ),
    );
    if (first && JSON.parse(first).received_ms >= launchedAt) {
      ready = true;
      break;
    }
  }
  assert.ok(
    ready,
    'Probe did not receive a live callback; check unlock, foreground and permissions.',
  );
  console.log('Probe is receiving live callbacks; starting timed observation.');
  const start = performance.now();
  let lastProgress = -1;
  while (performance.now() - start < (duration + 28) * 1000) {
    const elapsed = (performance.now() - start) / 1000;
    let state;
    if (elapsed >= duration) {
      if (stoppedAt === undefined) {
        request({ op: 'stop' });
        stoppedAt = Number(shell('cat /proc/uptime').split(' ')[0]) * 1000;
      }
      state = request({ op: 'status' });
    } else if (elapsed >= duration / 3 && elapsed < (duration * 2) / 3) {
      state = request({ op: 'drive', speed: 1.5, bearing: 90 });
    } else {
      state = request({ op: 'status' });
    }
    const time = Number(shell('cat /proc/uptime').split(' ')[0]) * 1000;
    observations.push({
      time,
      elapsed,
      position: state.config.position,
      active: state.requested_active,
      hook: state.hook_connected,
      phone: state.phone_connected,
    });
    const progress = Math.floor(elapsed / 30);
    if (progress !== lastProgress) {
      console.log(
        `t=${elapsed.toFixed(0)}s active=${state.requested_active} ` +
          `system=${state.hook_connected} phone=${state.phone_connected}`,
      );
      lastProgress = progress;
    }
    await delay(700);
  }
  callbacks = shell('su -c ' + quote(`cat /data/user/0/${app}/files/location-continuity.jsonl`))
    .split('\n')
    .filter(Boolean)
    .map(JSON.parse);
  const distance = (a, b) => {
    const radians = Math.PI / 180;
    const lat = (a.latitude - b.latitude) * radians;
    const lon = (a.longitude - b.longitude) * radians;
    const h =
      Math.sin(lat / 2) ** 2 +
      Math.cos(a.latitude * radians) * Math.cos(b.latitude * radians) * Math.sin(lon / 2) ** 2;
    return 6371000 * 2 * Math.asin(Math.min(1, Math.sqrt(h)));
  };
  assert.ok(
    observations.every((s) => s.hook && s.phone),
    'A bridge heartbeat was lost.',
  );
  for (const provider of ['gps', 'network']) {
    const active = callbacks.filter(
      (s) =>
        s.provider === provider &&
        s.received_ms > observations[0].time + 3000 &&
        s.received_ms < stoppedAt,
    );
    assert.ok(active.length > duration / 3, `${provider}: too few live callbacks`);
    assert.ok(
      active[0].received_ms < observations[0].time + 10000,
      `${provider}: delivery began late`,
    );
    let maxError = 0;
    let maxGap = 0;
    for (let i = 0; i < active.length; i++) {
      const sample = active[i];
      const nearest = observations.reduce((a, b) =>
        Math.abs(a.time - sample.received_ms) < Math.abs(b.time - sample.received_ms) ? a : b,
      );
      maxError = Math.max(maxError, distance(sample, nearest.position));
      if (i) maxGap = Math.max(maxGap, sample.received_ms - active[i - 1].received_ms);
    }
    assert.ok(maxError < 10, `${provider}: diverged from backend by ${maxError.toFixed(2)}m`);
    assert.ok(maxGap < 10000, `${provider}: callback gap ${maxGap}ms`);
    assert.ok(active.at(-1).received_ms > stoppedAt - 10000, `${provider}: delivery ended early`);
    // A GPS fix must state how many satellites solved it; without the extra, a consumer reads -1
    // and treats the fix as simulated. A network fix is not a satellite solution, so it must never
    // claim one.
    const extras = active.map((s) => s.extras_satellites);
    // A delivered fix must state its sample age instead of looking freshly produced by the read.
    const ages = active.map((s) => s.fix_age_ms).filter((age) => Number.isFinite(age));
    assert.ok(ages.length > 0, `${provider}: no fix reported a sample age`);
    assert.ok(
      ages.every((age) => age >= 0 && age <= 20_000),
      `${provider}: sample age outside the heartbeat window (${Math.min(...ages)}..${Math.max(
        ...ages,
      )} ms)`,
    );
    if (provider === 'gps') {
      assert.ok(
        extras.some((count) => count > 0),
        `gps: no simulated fix carried Location.extras satellites (values: ${[
          ...new Set(extras),
        ].join(', ')})`,
      );
      assert.ok(
        extras.every((count) => count !== 0),
        'gps: a simulated fix reported zero satellites',
      );
      console.log(
        `gps satellites extra: ${[...new Set(extras)].sort((a, b) => a - b).join(', ')}`,
      );
    } else {
      assert.ok(
        extras.every((count) => count <= 0),
        `network: a fix claimed satellites (values: ${[...new Set(extras)].join(', ')})`,
      );
    }
    const after = callbacks.filter(
      (s) => s.provider === provider && s.received_ms > stoppedAt + 22000,
    );
    assert.ok(after.length > 0, `${provider}: no fresh callback to verify restoration after stop`);
    assert.ok(
      after.every((s) => distance(s, observations.at(-1).position) > 100),
      `${provider}: synthetic output continued after stop`,
    );
    console.log(
      `PASS ${provider}: ${active.length} live callbacks, max error ` +
        `${maxError.toFixed(2)}m, max gap ${maxGap}ms; ${after.length} post-stop callbacks`,
    );
  }
  assert.ok(
    distance(observations[0].position, observations.at(-1).position) > duration / 3,
    'Leased movement did not advance backend coordinates.',
  );
} finally {
  writeFileSync(output, JSON.stringify({ duration, stoppedAt, observations, callbacks }, null, 2));
  request({ op: 'stop' });
  request({ op: 'update', position: before.config.position });
  request({ op: 'set_scope', scope: before.config.scope });
  shell(`am force-stop ${app}`);
}
