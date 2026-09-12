import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { decode, ephemeris, ecef, solve, rangeAt } from './gnss-decode.mjs';

test('LNAV parity, pages and independent position/velocity solution across week and ephemeris changes', () => {
  const out = 'build/gnss-model-test';
  mkdirSync(out, { recursive: true });
  const tool = (name) => (process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin', name) : name);
  const run = (name, args) => {
    const r = spawnSync(tool(name), args, { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
    assert.equal(r.status, 0, r.stderr || String(r.error));
    return r.stdout;
  };
  const src = 'zygisk/bridge/src/main/java/me/idk/justlocation/bridge';
  run('javac', [
    '--release',
    '17',
    '-d',
    out,
    ...['GpsOrbit', 'GpsEpoch', 'GpsLnav'].map((n) => `${src}/${n}.java`),
    'zygisk/tests/GnssModelProbe.java',
  ]);
  const lines = run('java', ['-cp', out, 'me.idk.justlocation.bridge.GnssModelProbe'])
    .trim()
    .split(/\r?\n/);
  const cases = new Map(),
    pages = new Set(),
    almanacs = new Set();
  for (const line of lines) {
    const [kind, ...v] = line.split(' ');
    if (kind === 'FIX')
      cases.set(v[0], {
        point: v.slice(1, 4).map(Number),
        speed: +v[4],
        bearing: +v[5],
        gpsMs: BigInt(v[6]),
        frames: new Map(),
        measurements: [],
      });
    if (kind === 'NAV') {
      const c = cases.get(v[0]),
        prn = +v[1],
        msg = decode(v[4]);
      assert.equal(msg.subframe, +v[2]);
      assert.equal(+v[3], -1);
      if (!c.frames.has(prn)) c.frames.set(prn, new Map());
      c.frames.get(prn).set(msg.subframe, msg);
    }
    if (kind === 'MEAS')
      cases.get(v[0]).measurements.push({
        prn: +v[1],
        rx: Number(BigInt(v[2]) % 604800000000000n) / 1e9,
        tx: Number(v[3]) / 1e9,
        rate: +v[4],
        elevation: +v[5],
      });
    if (kind === 'CYCLE') {
      const m = decode(v[2]);
      assert.equal(m.subframe, +v[0]);
      if (m.subframe >= 4) {
        assert.ok(+v[1] >= 1 && +v[1] <= 25);
        pages.add(`${m.subframe}/${v[1]}`);
        assert.equal(m.bits(60, 2), 1);
        const id = m.bits(62, 6);
        if (id >= 1 && id <= 32) {
          almanacs.add(id);
          assert.equal(m.bits(68, 16), 0); // circular orbit
          assert.equal(m.bits(136, 8), 0); // healthy
          assert.ok(Math.abs((0.3 + m.signed(98, 16) * 2 ** -19) * 180 - 55) < 0.001);
          assert.ok(Math.abs(m.bits(150, 24) * 2 ** -11 - Math.sqrt(26560000)) < 0.001);
          assert.ok(m.signed(120, 16) < 0); // nodal regression
        }
        if (id === 56) {
          assert.equal(m.signed(240, 8), 18);
          assert.equal(m.signed(270, 8), 18);
          assert.ok(m.bits(256, 8) >= 1 && m.bits(256, 8) <= 7);
        }
        if (id === 52) assert.ok(m.bits(68, 2) >= 2); // correction table unavailable
      }
      const bad = Buffer.from(v[2], 'hex');
      bad[7] ^= 4;
      assert.throws(() => decode(bad.toString('hex')), /parity/);
    }
  }
  assert.equal(pages.size, 50);
  assert.equal(almanacs.size, 32);
  let worst = 0,
    rateError = 0;
  for (const c of cases.values()) {
    const expected = ecef(...c.point),
      p = (c.point[0] * Math.PI) / 180,
      l = (c.point[1] * Math.PI) / 180,
      b = (c.bearing * Math.PI) / 180;
    const east = c.speed * Math.sin(b),
      north = c.speed * Math.cos(b);
    const velocity = [
      -Math.sin(l) * east - Math.sin(p) * Math.cos(l) * north,
      Math.cos(l) * east - Math.sin(p) * Math.sin(l) * north,
      Math.cos(p) * north,
    ];
    for (const m of c.measurements) {
      m.e = ephemeris(c.frames.get(m.prn));
      assert.ok(m.e);
      assert.equal(m.e.health, 0);
      assert.equal(m.e.week, Number(c.gpsMs / 604800000n) % 1024);
      assert.ok(m.e.sqrtA > 5000 && m.e.sqrtA < 6000);
      assert.ok(m.elevation > 0, `below horizon: ${m.elevation}`);
      const later = expected.map((x, i) => x + velocity[i] * 0.5),
        earlier = expected.map((x, i) => x - velocity[i] * 0.5);
      const rate = rangeAt(m.e, m.rx + 0.5, later) - rangeAt(m.e, m.rx - 0.5, earlier);
      rateError = Math.max(rateError, Math.abs(rate - m.rate));
    }
    const actual = solve(c.measurements);
    const error = Math.hypot(...expected.map((x, i) => x - actual[i]));
    worst = Math.max(worst, error);
    assert.ok(error < 1, `position error ${error}m at ${c.point}`);
    assert.ok(Math.abs(actual[3]) < 1, `clock error ${actual[3]}m`);
  }
  assert.equal(cases.size, 42);
  assert.ok(rateError < 0.02, `rate error ${rateError}`);
  console.log(
    `42 solutions: worst ECEF error ${worst.toFixed(4)}m; rate error ${rateError.toFixed(6)}m/s; 50 almanac/auxiliary pages`,
  );
});
