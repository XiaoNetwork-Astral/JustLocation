import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

// Run after a completed cold boot. Only reads natural bridge heartbeats; no install,
// process restart, configuration change or synthetic hook-status report is performed.
const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
function shell(command) {
  const result = spawnSync(adb, ['-s', serial, 'shell', command], {
    encoding: 'utf8',
    timeout: 30_000,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result.stdout.trim();
}
assert.equal(shell('getprop sys.boot_completed'), '1', 'Wait for boot completion first.');
const before = shell('pidof com.android.phone');
assert.match(before, /^\d+(?: \d+)*$/);
const encoded = Buffer.from(JSON.stringify({ version: 1, op: 'status' })).toString('base64');
for (let sample = 0; sample < 16; sample++) {
  const output = shell(
    `su -c '/data/adb/modules/justlocation/bin/justlocationd request ${encoded}'`,
  );
  const { ok, error, state } = JSON.parse(output.slice(output.indexOf('{')));
  assert.equal(ok, true, error);
  for (const field of [
    'hook_connected',
    'phone_connected',
    'cell_query_hook_ready',
    'sim_hook_ready',
  ])
    assert.equal(state[field], true, `Sample ${sample}: ${field} unavailable`);
  assert.ok(Array.isArray(state.detected_subscriptions), 'Subscription discovery unavailable');
  if (sample < 15) await delay(2000);
}
assert.equal(shell('pidof com.android.phone'), before, 'Phone process changed during acceptance');
console.log('PASS: 16 natural phone heartbeats across 30 seconds with stable process IDs');
