import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';

// Observe start/stop boundaries without interpreting cached results as live callbacks.
// Requires an unlocked device, the installed probe, its private AMap key and SDK consent.
const [adb, serial, output, gnssText = 'false'] = process.argv.slice(2);
assert.ok(adb && serial && output && ['false', 'true'].includes(gnssText));
const app = 'me.idk.justlocation.probe';
const binary = '/data/adb/modules/justlocation/bin/justlocationd';
const quote = (value) => "'" + String(value).replaceAll("'", "'\\''") + "'";
function shell(command) {
  const result = spawnSync(adb, ['-s', serial, 'shell', command], {
    encoding: 'utf8',
    timeout: 30000,
    maxBuffer: 12 * 1024 * 1024,
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
const uptime = () => Number(shell('cat /proc/uptime').split(' ')[0]) * 1000;
function capture(name) {
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
const before = request({ op: 'status' });
assert.equal(before.requested_active, false, 'Preserve an existing simulation.');
assert.equal(before.recording, null, 'Preserve an existing recording.');
assert.ok(before.hook_connected && before.phone_connected, 'Both bridges must be connected.');
const saved = { position: before.config.position, scope: before.config.scope, gnss: before.gnss };
const position = {
  latitude: 39.907333,
  longitude: 116.391083,
  altitude: 44,
  accuracy: 5,
  speed: 0,
  bearing: 0,
};
const trace = [],
  transitions = [];
let sdk = [],
  platform = [],
  interrupted = false;
const launched = uptime();
process.on('SIGINT', () => {
  interrupted = true;
});
function transition(command) {
  const sent = uptime();
  const state = request(command);
  transitions.push({
    op: command.op,
    sent_ms: sent,
    acknowledged_ms: uptime(),
    active: state.requested_active,
  });
  return state;
}
try {
  shell(`am force-stop ${app}`);
  shell(
    `am start -W -n ${app}/.ChannelCheckActivity --ez autorun true --ez continuity_only true --ez location_queries true --es amap_mode high --ez amap_cache true --el duration_ms 190000`,
  );
  let ready = false;
  for (let attempt = 0; attempt < 20; attempt++) {
    await delay(1000);
    if (capture('location-continuity.jsonl').some((row) => row.received_ms >= launched)) {
      ready = true;
      break;
    }
  }
  assert.ok(ready, 'Probe has not started; unlock the device and check permissions.');
  transition({
    op: 'set_gnss',
    config: { gnss_enabled: gnssText === 'true', nmea_enabled: gnssText === 'true' },
  });
  const begin = performance.now();
  let started = false,
    stopped = false,
    reported = -1;
  while (performance.now() - begin < 140000) {
    assert.ok(!interrupted, 'Capture interrupted.');
    const elapsed = (performance.now() - begin) / 1000;
    if (!started && elapsed >= 15) {
      transition({ op: 'start', config: { position, scope: { mode: 'apps', packages: [app] } } });
      started = true;
    }
    if (!stopped && elapsed >= 105) {
      transition({ op: 'stop' });
      stopped = true;
    }
    const state =
      elapsed >= 45 && elapsed < 75
        ? request({ op: 'drive', speed: 1.5, bearing: 90 })
        : request({ op: 'status' });
    trace.push({
      time: uptime(),
      elapsed,
      position: state.config.position,
      active: state.requested_active,
      hook: state.hook_connected,
      phone: state.phone_connected,
    });
    const progress = Math.floor(elapsed / 15);
    if (progress !== reported) {
      reported = progress;
      console.log(
        `t=${elapsed.toFixed(0)}s active=${state.requested_active} system=${state.hook_connected} phone=${state.phone_connected}`,
      );
    }
    await delay(700);
  }
} finally {
  try {
    sdk = capture('amap-continuity.jsonl').filter((row) => row.received_ms >= launched);
    platform = capture('location-continuity.jsonl').filter((row) => row.received_ms >= launched);
  } finally {
    writeFileSync(
      output,
      JSON.stringify(
        { saved, gnss: gnssText === 'true', transitions, trace, sdk, platform },
        null,
        2,
      ),
    );
    try {
      request({ op: 'stop' });
      request({ op: 'set_gnss', config: saved.gnss });
      request({ op: 'update', position: saved.position });
      request({ op: 'set_scope', scope: saved.scope });
    } finally {
      shell(`am force-stop ${app}`);
    }
  }
  console.log('Capture saved; simulation stopped and original configuration restored.');
}
