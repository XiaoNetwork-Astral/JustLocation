import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

// Requires the current module and probe APK to be installed. Does not install or reboot.
const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
const app = 'me.idk.justlocation.probe';
const daemon = '/data/adb/modules/justlocation/bin/justlocationd';
function shell(command) {
  const result = spawnSync(adb, ['-s', serial, 'shell', command], {
    encoding: 'utf8',
    timeout: 30_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr || result.stdout);
  return result.stdout.trim();
}
function request(command) {
  const encoded = Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64');
  const output = shell(`su -c '${daemon} request ${encoded}'`);
  const reply = JSON.parse(output.slice(output.indexOf('{')));
  assert.equal(reply.ok, true, reply.error);
  return reply.state;
}
async function collect(label) {
  shell(`am force-stop ${app}`);
  shell(`am start -W -n ${app}/.ChannelCheckActivity --ez autorun true --ez steps_only true`);
  const pid = shell(`pidof ${app}`);
  assert.match(pid, /^\d+$/);
  let log = '';
  for (let attempt = 0; attempt < 20; attempt++) {
    await delay(1000);
    log = shell(`logcat -d --pid=${pid} -s JustLocationCheck:I '*:S'`);
    if (log.includes('CHECK_REPORT_END')) break;
  }
  assert.ok(log.includes('CHECK_REPORT_END'), `${label}: probe report timed out\n${log}`);
  assert.ok(log.includes('step_registered 18 true'), `${label}: detector registration failed`);
  assert.ok(log.includes('step_registered 19 true'), `${label}: counter registration failed`);
  const events = [...log.matchAll(/step_event (18|19) ([\d.E+-]+) (\d+)/g)].map((match) => ({
    type: Number(match[1]),
    value: Number(match[2]),
    time: BigInt(match[3]),
  }));
  process.stdout.write(
    `${label}: ${JSON.stringify(events, (_, value) =>
      typeof value === 'bigint' ? value.toString() : value,
    )}\n`,
  );
  return events;
}
const before = request({ op: 'status' });
assert.equal(
  before.requested_active,
  false,
  'Stop the current simulation before device acceptance.',
);
assert.ok(
  !before.telephony.cells_enabled && !before.telephony.sim_enabled && !before.wifi.enabled,
  'This step-only acceptance requires other environment channels to be disabled.',
);
const position = { latitude: 0, longitude: 0, altitude: 0, accuracy: 5, speed: 0, bearing: 0 };
const settings = {
  enabled: true,
  cadence: 2,
  movement_linked: false,
  stride_m: 0.75,
  daily_reset: true,
};
try {
  request({ op: 'set_steps', config: settings });
  request({ op: 'start', config: { position, scope: { mode: 'apps', packages: [app] } } });
  for (let retry = 0; retry < 10 && !request({ op: 'status' }).step_hook_ready; retry++)
    await delay(1000);
  assert.equal(request({ op: 'status' }).step_hook_ready, true, 'Native step hooks are not ready.');
  const active = await collect('active');
  const detector = active.filter((event) => event.type === 18);
  const counter = active.filter((event) => event.type === 19);
  assert.ok(
    detector.length >= 18 && detector.length <= 28,
    `2 steps/s: got ${detector.length} events`,
  );
  assert.ok(
    detector.every((event) => event.value === 1),
    'Detector values must be one.',
  );
  assert.ok(counter.length >= 2, 'No cumulative counter updates.');
  assert.ok(counter.at(-1).value - counter[0].value >= 18, 'Counter failed to advance.');
  for (let i = 1; i < counter.length; i++) {
    assert.ok(counter[i].value >= counter[i - 1].value, 'Counter regressed.');
    assert.ok(counter[i].time >= counter[i - 1].time, 'Counter timestamp regressed.');
  }
  process.stdout.write('PASS stationary phone receives detector and cumulative step events\n');

  request({ op: 'set_steps', config: { ...settings, movement_linked: true } });
  await delay(1500);
  const stationary = await collect('linked-stationary');
  assert.equal(stationary.filter((event) => event.type === 18).length, 0);
  assert.ok(
    stationary.some((event) => event.type === 19),
    'Stationary counter baseline missing.',
  );
  process.stdout.write('PASS linked mode stops steps at rest\n');

  request({ op: 'drive', speed: 0.75, bearing: 90 });
  const renew = setInterval(() => request({ op: 'drive', speed: 0.75, bearing: 90 }), 800);
  let moving;
  try {
    moving = await collect('linked-moving');
  } finally {
    clearInterval(renew);
    request({ op: 'drive', speed: 0, bearing: 90 });
  }
  const walking = moving.filter((event) => event.type === 18).length;
  assert.ok(walking >= 9 && walking <= 15, `Linked walking cadence: got ${walking} events`);
  process.stdout.write('PASS joystick movement produces linked step events\n');

  request({ op: 'stop' });
  const baseline = await collect('stopped');
  request({ op: 'set_steps', config: settings });
  request({
    op: 'start',
    config: { position, scope: { mode: 'apps', packages: ['example.unselected'] } },
  });
  await delay(1500);
  const excluded = await collect('excluded');
  const realCounter = baseline.filter((event) => event.type === 19).at(-1);
  const excludedCounter = excluded.filter((event) => event.type === 19).at(-1);
  assert.ok(realCounter && excludedCounter, 'Original counter unavailable.');
  assert.equal(
    excludedCounter.value,
    realCounter.value,
    'Excluded stationary app received altered steps.',
  );
  assert.equal(excluded.filter((event) => event.type === 18).length, 0);
  process.stdout.write('PASS excluded app keeps original sensor output\n');
} finally {
  request({ op: 'stop' });
  request({ op: 'set_steps', config: before.steps });
  shell(`am force-stop ${app}`);
}
