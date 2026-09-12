import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';
import { decode, ephemeris, ecef, solve } from '../gnss-decode.mjs';

const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw Error('Provide adb path and explicit serial.');
const app = 'me.idk.justlocation.probe';
function run(args) {
  const r = spawnSync(adb, ['-s', serial, ...args], {
    encoding: 'utf8',
    timeout: 30000,
    maxBuffer: 8 * 1024 * 1024,
  });
  assert.equal(r.status, 0, r.stderr || String(r.error));
  return r.stdout.replace(/\r\n/g, '\n').trim();
}
const shell = (command) => run(['shell', command]);
function request(command) {
  const encoded = Buffer.from(JSON.stringify({ version: 1, ...command })).toString('base64');
  const text = shell(`su -c '/data/adb/modules/justlocation/bin/justlocationd request ${encoded}'`);
  const result = JSON.parse(text.slice(text.indexOf('{')));
  assert.equal(result.ok, true, result.error);
  return result.state;
}
async function capture(name, duration) {
  shell(`am force-stop ${app}`);
  shell(
    `am start -W -n ${app}/.ChannelCheckActivity --ez autorun true --ez gnss_only true --es capture_id ${name} --el gnss_duration_ms ${duration}`,
  );
  let log = '';
  for (let n = 0; n < duration / 1000 + 25; n++) {
    await delay(1000);
    log = shell(
      `run-as ${app} sh -c 'if test -f files/gnss-report.txt; then cat files/gnss-report.txt; fi'`,
    );
    if (log.includes(`GNSS_CAPTURE_ID=${name}\n`) && log.includes('CHECK_REPORT_END')) break;
  }
  writeFileSync(`build/tmp/gnss-${name}.log`, log);
  assert.ok(
    log.includes(`GNSS_CAPTURE_ID=${name}\n`) && log.includes('CHECK_REPORT_END'),
    'capture timed out',
  );
  assert.ok(log.includes('GNSS_REGISTER a=true b=true measurements=true'), 'registration');
  assert.ok(log.includes('GNSS_UNREGISTER additional=0'), 'callbacks continued after unregister');
  return log
    .split('\n')
    .map((line) => line.match(/GNSS_(\w+) (.*)/))
    .filter(Boolean)
    .map((m) => ({
      kind: m[1],
      ...Object.fromEntries(
        m[2]
          .trim()
          .split(' ')
          .map((pair) => pair.split('=')),
      ),
    }));
}
function validate(rows, position) {
  const expected = ecef(position.latitude, position.longitude, position.altitude),
    frames = new Map(),
    clocks = new Map(),
    raw = new Map();
  const receiverA = new Map(),
    receiverB = new Map(),
    seenFrames = new Set();
  let bias, discontinuity;
  for (const row of rows) {
    if (row.kind === 'NAV') {
      assert.equal(+row.type, 257);
      assert.equal(+row.status, 1);
      const nav = decode(row.data);
      assert.equal(nav.subframe, +row.sf);
      assert.equal(
        +row.page,
        nav.subframe <= 3 ? -1 : (Math.floor(((nav.tow + 604800 - 6) % 604800) / 30) % 25) + 1,
      );
      seenFrames.add(nav.subframe);
      const key = `${row.prn}/${nav.tow}`;
      const receiver = row.receiver === 'A' ? receiverA : receiverB;
      assert.ok(!receiver.has(key), 'duplicate same subframe');
      receiver.set(key, row.data);
      if (row.receiver === 'A') {
        if (!frames.has(+row.prn)) frames.set(+row.prn, new Map());
        frames.get(+row.prn).set(nav.subframe, nav);
      }
    }
    if (row.kind === 'CLOCK') {
      if (bias === undefined) {
        bias = row.fullBias;
        discontinuity = row.discontinuities;
      }
      assert.equal(row.fullBias, bias, 'full bias changed on continuous clock');
      assert.equal(row.discontinuities, discontinuity, 'spurious clock reset');
      const gps = BigInt(row.time) - BigInt(row.fullBias);
      clocks.set(row.epoch, Number(gps % 604800000000000n) / 1e9 - Number(row.bias) / 1e9);
    }
    if (row.kind === 'RAW') {
      assert.equal(+row.constellation, 1);
      if (!raw.has(row.epoch)) raw.set(row.epoch, []);
      raw
        .get(row.epoch)
        .push({ prn: +row.prn, tx: Number(row.tx) / 1e9, offset: Number(row.offset) / 1e9 });
    }
  }
  assert.equal(seenFrames.size, 5);
  assert.ok(receiverA.size >= 64);
  assert.deepEqual(receiverA, receiverB, 'listeners received different subframes');
  let solved = 0,
    worst = 0;
  for (const [epoch, values] of raw) {
    const measurements = values
      .map((v) => ({
        ...v,
        e: ephemeris(frames.get(v.prn) ?? new Map()),
        rx: clocks.get(epoch) + v.offset,
      }))
      .filter((v) => v.e);
    if (measurements.length < 4) continue;
    const point = solve(measurements),
      error = Math.hypot(...expected.map((x, i) => x - point[i]));
    worst = Math.max(worst, error);
    assert.ok(error < 1, `app raw-data position error ${error}m`);
    solved++;
  }
  assert.ok(solved >= 30, `only ${solved} epochs could be solved`);
  console.log(
    `PASS app: ${receiverA.size} identical messages per listener; ${solved} independently solved epochs, max 3D error ${worst.toFixed(4)}m`,
  );
}
function syntheticCount(rows) {
  let count = 0;
  for (const row of rows.filter((r) => r.kind === 'NAV' && +r.type === 257)) {
    try {
      const n = decode(row.data);
      if (
        n.subframe === 2 &&
        Math.abs(n.split(226, false) * 2 ** -19 - Math.sqrt(26560000)) < 0.00001 &&
        n.split(166, false) === 0
      )
        count++;
    } catch {
      /* Real hardware can report incomplete or differently packed frames. */
    }
  }
  return count;
}
const before = request({ op: 'status' });
assert.equal(before.requested_active, false, 'Stop simulation before running GNSS acceptance.');
const position = {
  latitude: 31.2304,
  longitude: 121.4737,
  altitude: 42,
  accuracy: 5,
  speed: 0,
  bearing: 0,
};
let installed = false;
try {
  run(['install', '-r', '-g', 'dist/justlocation-probe-debug.apk']);
  installed = true;
  request({ op: 'set_realism', config: { ...before.realism, enabled: false } });
  request({ op: 'set_gnss', config: { gnss_enabled: true, nmea_enabled: true } });
  request({ op: 'start', config: { position, scope: { mode: 'apps', packages: [app] } } });
  await delay(2500);
  validate(await capture('selected', 65000), position);
  request({ op: 'stop' });
  await delay(2500);
  assert.equal(syntheticCount(await capture('stopped', 32000)), 0, 'synthetic frames after stop');
  console.log('PASS app: stopped simulation restored system output');
  request({
    op: 'start',
    config: { position, scope: { mode: 'apps', packages: ['example.unselected'] } },
  });
  await delay(2500);
  assert.equal(
    syntheticCount(await capture('unselected', 32000)),
    0,
    'synthetic frames outside scope',
  );
  console.log('PASS app: unselected package did not receive synthetic frames');
} finally {
  request({ op: 'stop' });
  request({ op: 'set_gnss', config: before.gnss });
  request({ op: 'set_realism', config: before.realism });
  if (before.config) {
    request({ op: 'start', config: before.config });
    request({ op: 'stop' });
  }
  if (installed) run(['uninstall', app]);
}
