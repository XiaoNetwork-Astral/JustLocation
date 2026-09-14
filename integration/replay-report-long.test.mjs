import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { report } from './replay-report.mjs';

/**
 * A ten-minute synthetic timeline with an intermittent subscription: the consumer subscribes,
 * unsubscribes, and subscribes again, while a provider reports the whole time.
 *
 * The point is to decide, before any device run, how the report is expected to describe a gap that
 * belongs to the consumer rather than to the simulated output.
 */
const directory = mkdtempSync(join(tmpdir(), 'justlocation-replay-long-'));
const SECONDS = 600;
const START = 1_000_000;

function rows() {
  const result = [];
  // The app states when it began collecting.
  result.push({ event: 'collection_start', received_ms: START });
  for (let second = 0; second <= SECONDS; second += 1) {
    const time = START + second * 1000;
    // gps every second; network every second as well, so a real gap can be told from a slow source.
    for (const provider of ['gps', 'network']) {
      result.push({
        event: 'callback',
        provider,
        received_ms: time,
        present: true,
        fix_provider: provider,
        fix_time_ms: 1_789_000_000_000 + second * 1000,
        fix_elapsed_ms: time - 5,
        fix_age_ms: 5,
        latitude: 31.0 + second * 1e-6,
        longitude: 121.0,
        accuracy: provider === 'gps' ? 5 : 30,
        speed: 0,
        bearing: 0,
        mock: false,
        extras_satellites: provider === 'gps' ? 8 : -1,
      });
    }
  }
  // The consumer unsubscribes between minute two and minute three: no rows exist for either
  // provider there, which is exactly the intermittent case the acceptance wording asks about.
  return result.filter((row) => {
    const second = (row.received_ms - START) / 1000;
    return !(second > 120 && second < 180);
  });
}

const capture = join(directory, 'ten-minute.jsonl');
writeFileSync(capture, rows().map((row) => JSON.stringify(row)).join('\n') + '\n');

const result = report([{ name: 'ten-minute', label: 'synthetic-fixture', path: capture }], {
  gapMs: 10000,
});
const summary = result.captures[0];

// Ten minutes of one-second samples from two providers, minus the minute the consumer was away:
// 601 samples per provider, of which 59 fall inside the unsubscribed window.
assert.equal(summary.fixes, 2 * 542, 'the fixture holds ten minutes of samples');
assert.equal(summary.errors, 0);
assert.deepEqual(summary.providers, ['gps', 'network']);
// The interruption appears as one gap per provider, in the minute where no rows were written: the
// last sample before the unsubscribed window and the first one after it are sixty seconds apart.
const gaps = summary.gaps.filter((gap) => gap.ms > 10_000);
assert.equal(gaps.length, 2, `expected one interruption per provider, saw ${summary.gaps.length}`);
for (const gap of gaps)
  assert.ok(
    gap.ms >= 60_000 && gap.ms <= 61_000,
    `the interruption should be about a minute, saw ${gap.ms}ms`,
  );
// Ages stay inside the heartbeat window for the whole run, which is what a ten-minute check is for.
assert.equal(summary.ages.negative, 0);
assert.ok(summary.ages.max <= 10, `fix age should stay small, saw ${summary.ages.max}ms`);
assert.deepEqual(summary.satellites.sort((a, b) => a - b), [-1, 8]);

// The same timeline through the SDK capture shape, to pin what a mixture looks like in one report.
const sdk = join(directory, 'ten-minute-sdk.jsonl');
writeFileSync(
  sdk,
  [
    { event: 'start', received_ms: START, mode: 'high', cache: false, sdk_version: '11.2.100' },
    ...Array.from({ length: 61 }, (_, index) => ({
      event: 'fix',
      received_ms: START + index * 10_000,
      error_code: 0,
      location_type: 1,
      provider: 'gps',
      latitude: 31.0 + index * 1e-5,
      longitude: 121.0,
      accuracy: 5,
      fix_time_ms: 1_789_000_000_000 + index * 10_000,
    })),
  ]
    .map((row) => JSON.stringify(row))
    .join('\n') + '\n',
);
const mixed = report(
  [
    { name: 'system', label: 'synthetic-fixture', path: capture },
    { name: 'sdk', label: 'synthetic-fixture', path: sdk },
  ],
  { gapMs: 10000 },
);
assert.equal(mixed.fix_count, summary.fixes + 61);
assert.equal(mixed.comparisons.length, 1);
assert.ok(
  mixed.comparisons[0].pairs > 50,
  `a ten-minute pair should match many samples, saw ${mixed.comparisons[0].pairs}`,
);
// The fixture is synthetic and must say so, whatever it is compared with.
assert.ok(mixed.captures.every((source) => source.label === 'synthetic-fixture'));

console.log(
  `PASS: ten-minute fixture reports ${summary.fixes} fixes, ${gaps.length} interruptions, ` +
    `ages up to ${summary.ages.max}ms`,
);
