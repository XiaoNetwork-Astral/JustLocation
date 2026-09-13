// Offline CLI acceptance with synthetic candidates, not a provider service test.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';

const binary = resolve(process.argv[2] ?? 'backend/target/debug/justlocationd.exe');
const tmp = resolve('build/tmp');
mkdirSync(tmp, { recursive: true });
const directory = mkdtempSync(join(tmp, 'route-cli-'));
function run(args, success = true) {
  const result = spawnSync(binary, ['--data-dir', directory, '--json', ...args], {
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  assert.equal(result.status === 0, success, result.stderr);
  return success ? JSON.parse(result.stdout) : result.stderr;
}
const points = Array.from({ length: 10000 }, (_, i) => ({
  latitude: 31.23 + i / 1e6,
  longitude: 121.47,
  altitude: 0,
  accuracy: 5,
  speed: 0,
  bearing: 0,
}));
const plan = {
  points,
  speed: 1.4,
  repeat_count: 1,
  repeat_delay: 0,
  geometry: {
    provider: 'amap',
    mode: 'walking',
    distance_m: 1112,
    duration_s: 800,
    segments: [
      {
        first: 0,
        last: 9999,
        distance_m: 1112,
        duration_s: 800,
        road_name: 'Synthetic fixture',
        instruction: '',
        attributes: { navi: { walk_type: '22' } },
      },
    ],
  },
};
const candidates = join(directory, 'candidates.json');
writeFileSync(
  candidates,
  JSON.stringify({
    version: 1,
    ok: true,
    coordinate_system: 'wgs84',
    candidates: [plan, { ...plan, speed: 2 }],
  }),
);
const capabilities = run(['maps', 'capabilities']);
assert.equal(capabilities.providers.length, 3);
assert.ok(
  capabilities.providers.every((p) => p.map_matching === false && p.building_avoidance === false),
);
const summaries = run(['route', 'candidates', '-i', candidates]);
assert.equal(summaries.length, 2);
assert.equal(summaries[1].point_count, 10000);
const saved = run(['route', 'select', 'Chosen second', '--candidate', '1', '-i', candidates]);
const exported = run(['route', 'export', saved.id]);
assert.equal(exported.speed, 2);
assert.deepEqual(exported.points, points);
assert.deepEqual(exported.geometry, plan.geometry);
const before = readFileSync(join(directory, 'library.json'), 'utf8');
assert.match(
  run(['route', 'select', 'Invalid', '--candidate', '2', '-i', candidates], false),
  /candidate index/,
);
assert.match(
  run(['route', 'replan', saved.id, 'amap', 'walking', '--via', '10'], false),
  /unsupported waypoint/,
);
assert.match(
  run(['route', 'replan', saved.id, 'amap', 'driving'], false),
  /missing WebService key/,
);
assert.equal(readFileSync(join(directory, 'library.json'), 'utf8'), before);
const backup = run(['backup', 'export']);
assert.equal(backup.routes[0].plan.geometry.segments[0].attributes.navi.walk_type, '22');
console.log(
  JSON.stringify({ ok: true, candidates: 2, selected: 1, point_count: points.length, directory }),
);
