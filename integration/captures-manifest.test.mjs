import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * The captures are a contract between the probe app (which writes them on the device) and the
 * integration scripts (which read them on the host). This checks the declared contract against the
 * probe sources in both directions, because a renamed field has already gone unnoticed twice.
 */
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(readFileSync(join(root, 'integration/captures.manifest.json'), 'utf8'));

/** Keys a probe source actually writes with `put("key", …)`. */
function writtenKeys(path) {
  const text = readFileSync(path, 'utf8');
  return new Set([...text.matchAll(/\.put\(\s*"([^"]+)"/g)].map((match) => match[1]));
}

assert.ok(Object.keys(manifest.captures).length >= 3, 'the manifest must describe every capture');

for (const [name, capture] of Object.entries(manifest.captures)) {
  const written = writtenKeys(join(root, capture.probe));
  assert.ok(written.size > 0, `${name}: ${capture.probe} writes no keys`);
  // A documented key the probe never writes is a contract that no longer exists.
  for (const key of capture.keys)
    assert.ok(
      written.has(key),
      `${name}: ${capture.probe} never writes the documented key "${key}"`,
    );
  // A key the probe writes but the manifest omits is an undocumented part of the format.
  for (const key of written)
    assert.ok(
      capture.keys.includes(key),
      `${name}: ${capture.probe} writes "${key}" but the manifest does not document it`,
    );
}

// The device scripts read these captures by name; a renamed capture would break them silently.
const scripts = [
  'integration/device/run-location-continuity.mjs',
  'integration/device/run-location-transitions.mjs',
  'integration/device/run-sdk-matrix.mjs',
  'integration/device/run-amap-continuity.mjs',
  'integration/replay-report.mjs',
];
let referenced = 0;
for (const script of scripts) {
  const text = readFileSync(join(root, script), 'utf8');
  for (const match of text.matchAll(/'([a-z-]+\.jsonl)'/g)) {
    referenced += 1;
    assert.ok(
      Object.hasOwn(manifest.captures, match[1]),
      `${script} reads "${match[1]}", which the manifest does not describe`,
    );
  }
}
assert.ok(referenced >= 4, `only ${referenced} capture references were found in the scripts`);

console.log('PASS: capture field contract matches the probe sources and the scripts that read them');
