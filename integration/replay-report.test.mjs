import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { report } from './replay-report.mjs';

/**
 * The offline report has to describe exactly what the captures contain: a synthetic fixture with
 * known gaps, providers and divergences must be reported as such, and a labelled capture must never
 * be presented as a different kind of evidence.
 */
const directory = mkdtempSync(join(tmpdir(), 'justlocation-replay-'));

function jsonl(name, rows) {
  const path = join(directory, name);
  writeFileSync(path, rows.map((row) => JSON.stringify(row)).join('\n') + '\n');
  return path;
}

// A standstill capture: two providers, one twelve-second gap in the network stream, one fix that
// carries satellites and one that does not.
const system = jsonl('system.jsonl', [
  { event: 'collection_start', received_ms: 1000 },
  {
    event: 'callback',
    provider: 'gps',
    received_ms: 2000,
    present: true,
    fix_provider: 'gps',
    fix_elapsed_ms: 1000,
    fix_age_ms: 1000,
    latitude: 31.0,
    longitude: 121.0,
    accuracy: 5,
    extras_satellites: 8,
  },
  {
    event: 'callback',
    provider: 'gps',
    received_ms: 3000,
    present: true,
    fix_provider: 'gps',
    fix_elapsed_ms: 2990,
    fix_age_ms: 10,
    latitude: 31.001,
    longitude: 121.0,
    accuracy: 5,
    extras_satellites: 8,
  },
  {
    event: 'callback',
    provider: 'network',
    received_ms: 4000,
    present: true,
    fix_provider: 'network',
    fix_elapsed_ms: 3990,
    fix_age_ms: 10,
    latitude: 31.0,
    longitude: 121.0,
    accuracy: 30,
    extras_satellites: -1,
  },
  // A gap in this provider's stream only.
  {
    event: 'callback',
    provider: 'network',
    received_ms: 20000,
    present: true,
    fix_provider: 'network',
    fix_elapsed_ms: 19990,
    fix_age_ms: 10,
    latitude: 31.0,
    longitude: 121.0,
    accuracy: 30,
    extras_satellites: -1,
  },
]);

// An SDK capture of the same instant, two metres away, with its own start row and one error.
const sdk = jsonl('sdk.jsonl', [
  {
    event: 'start',
    received_ms: 500,
    mode: 'high',
    cache: false,
    sdk_version: '11.2.100',
    app_version: '0.1.0-alpha.1+locenv.3 (6)',
  },
  {
    event: 'fix',
    received_ms: 2100,
    mode: 'high',
    cache: false,
    error_code: 0,
    location_type: 1,
    provider: 'gps',
    latitude: 31.000018,
    longitude: 121.0,
    accuracy: 5,
  },
  {
    event: 'fix',
    received_ms: 3100,
    mode: 'high',
    cache: false,
    error_code: 12,
    error_info: 'mock',
    location_type: 0,
    provider: null,
    latitude: 0,
    longitude: 0,
    accuracy: 0,
  },
]);

const result = report(
  [
    { name: 'system', label: 'synthetic-fixture', path: system },
    { name: 'sdk', label: 'synthetic-fixture', path: sdk },
  ],
  { gapMs: 10000 },
);

assert.equal(result.captures.length, 2);
const [systemSummary, sdkSummary] = result.captures;
assert.equal(systemSummary.name, 'system');
assert.equal(systemSummary.label, 'synthetic-fixture');
// The start row is a marker, not a fix.
assert.equal(systemSummary.fixes, 4);
assert.deepEqual(systemSummary.providers.sort(), ['gps', 'network']);
// The twelve-second network gap is reported; the three seconds between the streams are not.
assert.equal(systemSummary.gaps.length, 1);
assert.equal(systemSummary.gaps[0].ms, 16000);
assert.equal(systemSummary.gaps[0].provider, 'network');
assert.equal(systemSummary.ages.min, 10);
assert.equal(systemSummary.ages.max, 1000);
assert.equal(systemSummary.ages.negative, 0);
assert.deepEqual(systemSummary.satellites.sort((a, b) => a - b), [-1, 8]);
assert.equal(sdkSummary.fixes, 2);
assert.equal(sdkSummary.errors, 1, 'a non-zero error code is counted as an error');
assert.deepEqual(sdkSummary.location_types, [1, 0]);
assert.equal(sdkSummary.gaps.length, 0);
// The two captures describe the same position, but the fixture has the SDK a hundred metres away at
// one end, so the reported divergence is the worst pair inside the two-second pairing window, not
// just the two-metre offset at the start.
assert.equal(result.comparisons.length, 1);
assert.equal(result.comparisons[0].pairs > 0, true);
assert.ok(
  result.comparisons[0].worst_divergence_m > 2 &&
    result.comparisons[0].worst_divergence_m < 150,
  `divergence ${result.comparisons[0].worst_divergence_m}m is outside the fixture range`,
);
// A second SDK fix has no provider, so no provider switch is invented from it.
assert.deepEqual(result.provider_switches.map((entry) => entry.value), ['gps', 'network']);

// A capture kind that is not declared is refused instead of being guessed.
assert.throws(
  () => report([{ name: 'x', label: 'maybe-real', path: system }]),
  /unknown capture label/,
);
// A device report wrapped as JSON is accepted: its callback list is the timeline.
const wrapped = join(directory, 'wrapped.json');
writeFileSync(wrapped, JSON.stringify({ duration: 60, callbacks: [] }));
const empty = report([{ name: 'wrapped', label: 'device-run', path: wrapped }]);
assert.equal(empty.captures[0].fixes, 0);

console.log('PASS: offline replay report describes gaps, sources, ages and divergences');
