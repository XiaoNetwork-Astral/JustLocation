import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';

// Fixture heartbeats use a separate daemon/socket and never enter installed phone hooks.
const [adbPath, serial, hostBinary] = process.argv.slice(2);
if (!adbPath || !serial || !hostBinary)
  throw new Error('Usage: node run-virtual-sim.mjs ADB SERIAL ARM64_BINARY');
const adb = (...args) =>
  execFileSync(adbPath, ['-s', serial, ...args], {
    encoding: 'utf8',
    timeout: 30_000,
    maxBuffer: 2 * 1024 * 1024,
  }).trim();
const quote = (value) => "'" + String(value).replaceAll("'", "'\\''") + "'";
const root = (command) => adb('shell', 'su -c ' + quote(command));
const directory = '/data/local/tmp/justlocation-virtual-sim-' + Date.now();
assert.match(directory, /^\/data\/local\/tmp\/justlocation-virtual-sim-\d+$/);
const binary = directory + '/justlocationd';
const cli = (...args) =>
  JSON.parse(root([binary, '--data-dir', directory, '--json', ...args].map(quote).join(' ')));
const request = (command) => {
  const reply = cli(
    'request',
    Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64'),
  );
  assert.equal(reply.ok, true, reply.error);
  return reply.state;
};
const operators = () =>
  [
    'gsm.operator.alpha',
    'gsm.sim.operator.alpha',
    'gsm.operator.numeric',
    'gsm.sim.operator.numeric',
  ].map((name) => adb('shell', 'getprop', name));
const before = operators();
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
const upsert = (slot, carrier, enabled = true) =>
  cli(
    'sim',
    'virtual',
    'upsert',
    '--slot',
    String(slot),
    '--mcc',
    '460',
    '--mnc',
    '01',
    '--carrier',
    carrier,
    '--enabled',
    String(enabled),
  );
const heartbeat = (subscriptions = [], extra = {}) =>
  request({
    op: 'telephony_hook_status',
    cells: true,
    sim: true,
    virtual_sim_queries: true,
    active_modem_count: 2,
    subscriptions,
    ...extra,
  });
const real = (slot) => ({
  id: 7 + slot,
  slot,
  mcc: '460',
  mnc: '11',
  country: 'cn',
  carrier: 'Fixture real',
});
adb('shell', 'mkdir -m 700 ' + quote(directory));
try {
  adb('push', hostBinary, binary);
  root('chmod 700 ' + quote(binary));
  await start();
  // A selected SIM scope prevents the test daemon from writing global operator properties.
  cli('scope', 'set', '--feature', 'sim', '--app', 'example.virtual');
  upsert(0, 'Virtual A');
  const configured = upsert(1, 'Virtual B').telephony.virtual_sim;
  const ids = configured.subscriptions.map((sub) => sub.id);
  assert.equal(new Set(ids).size, 2);
  assert.deepEqual(
    upsert(0, 'Renamed A').telephony.virtual_sim.subscriptions.map((sub) => sub.id),
    ids,
  );
  cli('sim', 'virtual', 'default', '1');
  cli('sim', 'operator', 'true');
  cli('start', '--lat', '0', '--lon', '0', '--app', 'example.virtual');
  let state = heartbeat();
  assert.deepEqual(state.telephony_output.virtual_ids, ids);
  assert.equal(state.telephony_output.virtual_default_id, ids[1]);
  const token = state.virtual_sim_version;
  state = heartbeat([], { virtual_sim_applied: token });
  assert.equal(state.virtual_sim_version, token);
  assert.equal(state.virtual_sim_applied, token);
  state = heartbeat([real(0)]);
  assert.deepEqual(state.telephony_output.virtual_ids, [ids[1]]);
  assert.equal(state.telephony_output.virtual_blocked[0].reason, 'slot_occupied');
  state = heartbeat([real(0), real(1)]);
  assert.deepEqual(state.telephony_output.virtual_ids, []);
  assert.equal(state.virtual_sim_version, 'off');
  state = heartbeat([], { active_modem_count: 1 });
  assert.deepEqual(state.telephony_output.virtual_ids, [ids[0]]);
  state = heartbeat([], { active_modem_count: null });
  assert.deepEqual(state.telephony_output.virtual_ids, []);
  const saved = cli('status').telephony.virtual_sim;
  stop();
  state = await start();
  assert.equal(state.requested_active, false);
  assert.deepEqual(state.telephony.virtual_sim, saved);
  assert.deepEqual(
    state.telephony.virtual_sim.subscriptions.map((sub) => sub.id),
    ids,
  );
  upsert(0, 'Renamed A', false);
  state = cli('sim', 'virtual', 'remove', '1');
  assert.equal(state.telephony.sim_enabled, false);
  assert.equal(state.telephony.virtual_sim.default_slot, null);
  assert.equal(state.telephony.virtual_sim.subscriptions[0].enabled, false);
  console.log(
    'PASS: virtual SIM CLI stable IDs, defaults, no/one/two-card fixtures, modem limits, acknowledgement, restart persistence and last-card disable',
  );
} finally {
  stop();
  root('rm -r -- ' + quote(directory));
  assert.deepEqual(operators(), before, 'real operator properties changed during isolated test');
  console.log('PASS: isolated daemon and files removed; real operator properties unchanged');
}
