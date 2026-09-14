#!/usr/bin/env node
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';

/**
 * Aligns location captures on one timeline and reports what they show.
 *
 * Captures come from the probe app, from device scripts or from fixtures: a JSONL file of rows, or
 * a JSON report whose `callbacks`, `observations` or `configurations` fields hold those rows. Real
 * captures, synthetic fixtures and device runs are labelled by the caller instead of being guessed,
 * so a report can never present one as the other.
 */

const LABELS = ['real-capture', 'synthetic-fixture', 'device-run'];

function rowsOf(path) {
  const text = readFileSync(path, 'utf8');
  if (!/\.jsonl$/i.test(path)) {
    const document = JSON.parse(text);
    if (Array.isArray(document.callbacks)) return document.callbacks;
    if (Array.isArray(document.observations)) return document.observations;
    // An SDK matrix report holds configurations, not a timeline.
    return [];
  }
  return text
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

/** One row from either capture kind, in one shape. */
function normalise(row, source) {
  if (row.event === 'collection_start') return { source, event: 'start', time: row.received_ms };
  if (row.event === 'start')
    return {
      source,
      event: 'start',
      time: row.received_ms,
      detail: row.sdk_version ?? null,
      identity: row.app_version ?? null,
    };
  const fix = row.event === 'fix' ? row : row.present ? row : null;
  if (!fix) return null;
  const time = fix.received_ms;
  const elapsed = fix.fix_elapsed_ms;
  const age = Number.isFinite(fix.fix_age_ms)
    ? fix.fix_age_ms
    : Number.isFinite(elapsed) && elapsed > 0
      ? time - elapsed
      : null;
  return {
    source,
    event: 'fix',
    time,
    provider: fix.provider,
    location_type: fix.location_type ?? null,
    latitude: fix.latitude,
    longitude: fix.longitude,
    accuracy: fix.accuracy ?? null,
    satellites: fix.extras_satellites ?? fix.satellites ?? null,
    age_ms: age,
    error_code: fix.error_code ?? null,
  };
}

function metres(a, b) {
  const radians = Math.PI / 180;
  const lat = (a.latitude - b.latitude) * radians;
  const lon = (a.longitude - b.longitude) * radians;
  const h =
    Math.sin(lat / 2) ** 2 +
    Math.cos(a.latitude * radians) * Math.cos(b.latitude * radians) * Math.sin(lon / 2) ** 2;
  return 6371000 * 2 * Math.asin(Math.min(1, Math.sqrt(h)));
}

/** Source sequence changes, ignoring repeated values. */
function switches(rows, field) {
  const seen = [];
  let previous = null;
  for (const row of rows) {
    const value = row[field];
    if (value === null || value === undefined) continue;
    if (value !== previous) {
      seen.push({ time: row.time, source: row.source, value });
      previous = value;
    }
  }
  return seen;
}

export function report(captures, options = {}) {
  const gapMs = options.gapMs ?? 10000;
  const sources = [];
  const rows = [];
  for (const capture of captures) {
    assert.ok(LABELS.includes(capture.label), `unknown capture label ${capture.label}`);
    const normalised = rowsOf(capture.path)
      .map((row) => normalise(row, capture.name))
      .filter(Boolean)
      .sort((a, b) => a.time - b.time);
    const fixes = normalised.filter((row) => row.event === 'fix');
    const summary = {
      name: capture.name,
      label: capture.label,
      path: capture.path,
      rows: normalised.length,
      fixes: fixes.length,
      errors: fixes.filter((row) => (row.error_code ?? 0) !== 0).length,
      providers: [...new Set(fixes.map((row) => row.provider).filter(Boolean))],
      location_types: [...new Set(fixes.map((row) => row.location_type).filter((value) => value !== null))],
      gaps: [],
      ages: null,
      satellites: [...new Set(fixes.map((row) => row.satellites).filter((value) => value !== null))],
    };
    if (fixes.length) {
      const ages = fixes.map((row) => row.age_ms).filter((age) => Number.isFinite(age));
      summary.ages =
        ages.length > 0
          ? { min: Math.min(...ages), max: Math.max(...ages), negative: ages.filter((age) => age < 0).length }
          : null;
      for (let index = 1; index < fixes.length; index += 1) {
        const gap = fixes[index].time - fixes[index - 1].time;
        if (gap > gapMs)
          summary.gaps.push({
            ms: gap,
            after: fixes[index - 1].time,
            source: capture.name,
            provider: fixes[index].provider,
          });
      }
    }
    sources.push(summary);
    rows.push(...normalised);
  }
  rows.sort((a, b) => a.time - b.time);
  const fixes = rows.filter((row) => row.event === 'fix');
  // Agreement between two capture kinds, measured at the same monotonic instant: both sources
  // describe the same simulated position, so a large divergence points at a source-specific path.
  const comparisons = [];
  const byName = new Map(sources.map((source) => [source.name, []]));
  // Only real fixes can be compared: an error row carries an error code and no usable position.
  for (const row of fixes)
    if (
      (row.error_code ?? 0) === 0 &&
      Number.isFinite(row.latitude) &&
      Number.isFinite(row.longitude) &&
      (row.latitude !== 0 || row.longitude !== 0)
    )
      byName.get(row.source)?.push(row);
  const names = [...byName.keys()];
  for (let first = 0; first < names.length; first += 1)
    for (let second = first + 1; second < names.length; second += 1) {
      const a = byName.get(names[first]);
      const b = byName.get(names[second]);
      // An SDK matrix report summarises configurations instead of holding a timeline, so a source
      // with no comparable fix contributes nothing rather than failing the report.
      if (!a.length || !b.length) continue;
      let worst = 0;
      let pairs = 0;
      for (const left of a) {
        const right = b.reduce((best, row) =>
          Math.abs(row.time - left.time) < Math.abs(best.time - left.time) ? row : best,
        );
        if (Math.abs(right.time - left.time) > 2000) continue;
        worst = Math.max(worst, metres(left, right));
        pairs += 1;
      }
      if (pairs)
        comparisons.push({
          first: names[first],
          second: names[second],
          pairs,
          worst_divergence_m: Number(worst.toFixed(2)),
        });
    }
  return {
    gap_ms: gapMs,
    captures: sources,
    fix_count: fixes.length,
    provider_switches: switches(fixes, 'provider'),
    location_type_switches: switches(fixes, 'location_type'),
    comparisons,
  };
}

function main() {
  const [output, ...rest] = process.argv.slice(2);
  if (!output || rest.length === 0)
    throw new Error(
      'Usage: node replay-report.mjs OUTPUT [NAME:LABEL:PATH ...]\n' +
        'LABEL is real-capture, synthetic-fixture or device-run.',
    );
  const captures = rest.map((argument) => {
    const separator = argument.indexOf(':');
    const labelSeparator = argument.indexOf(':', separator + 1);
    if (separator < 0 || labelSeparator < 0)
      throw new Error(`expected NAME:LABEL:PATH, got ${argument}`);
    return {
      name: argument.slice(0, separator),
      label: argument.slice(separator + 1, labelSeparator),
      path: argument.slice(labelSeparator + 1),
    };
  });
  const result = report(captures, { gapMs: 10000 });
  writeFileSync(output, JSON.stringify(result, null, 2));
  for (const source of result.captures)
    console.log(
      `${source.name.padEnd(16)} ${source.label.padEnd(18)} fixes=${String(source.fixes).padEnd(5)} ` +
        `errors=${String(source.errors).padEnd(3)} providers=${source.providers.join('/') || '-'} ` +
        `types=${source.location_types.join('/') || '-'} gaps=${source.gaps.length} ` +
        `ages=${source.ages ? `${source.ages.min}..${source.ages.max}ms` : '-'} ` +
        `satellites=${source.satellites.join('/') || '-'}`,
    );
  for (const comparison of result.comparisons)
    console.log(
      `${comparison.first} vs ${comparison.second}: ${comparison.pairs} paired samples, ` +
        `worst divergence ${comparison.worst_divergence_m}m`,
    );
  console.log(
    `provider switches ${result.provider_switches.length}, location-type switches ${result.location_type_switches.length}`,
  );
  console.log(`report written to ${output}`);
}

if (process.argv[1] && /replay-report\.mjs$/.test(process.argv[1])) main();
