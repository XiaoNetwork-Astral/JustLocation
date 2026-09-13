import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';

// All files and the daemon belong to this disposable fixture. Installed hooks use a different socket.
const [adbPath, serial, hostBinary] = process.argv.slice(2);
if (!adbPath || !serial || !hostBinary)
  throw new Error('Usage: node run-backups.mjs ADB SERIAL ARM64_BINARY');
const adb = (...args) =>
  execFileSync(adbPath, ['-s', serial, ...args], {
    encoding: 'utf8',
    timeout: 30_000,
    maxBuffer: 2 * 1024 * 1024,
  }).trim();
const quote = (value) => "'" + String(value).replaceAll("'", "'\\''") + "'";
const root = (command) => adb('shell', 'su -c ' + quote(command));
const directory = '/data/local/tmp/justlocation-backup-' + Date.now();
assert.match(directory, /^\/data\/local\/tmp\/justlocation-backup-\d+$/);
const binary = directory + '/justlocationd';
const raw = (...args) =>
  root([binary, '--data-dir', directory, '--json', ...args].map(quote).join(' '));
const cli = (...args) => JSON.parse(raw(...args));
const write = (name, value) =>
  root(
    `printf %s ${quote(Buffer.from(JSON.stringify(value)).toString('base64'))} | base64 -d > ${quote(directory + '/' + name)}`,
  );
const exported = directory + '/backup.json.gz';
try {
  adb('shell', 'mkdir -m 700 ' + quote(directory));
  adb('push', hostBinary, binary);
  root('chmod 700 ' + quote(binary));
  const place = cli(
    'place',
    'save',
    'Fixture place',
    '--lat',
    '31.1234567890123',
    '--lon',
    '121.987654321098',
  );
  write('route-input.json', {
    points: [
      { latitude: 31, longitude: 121, altitude: 0, accuracy: 5, speed: 0, bearing: 0 },
      { latitude: 31.001, longitude: 121.001, altitude: 0, accuracy: 5, speed: 0, bearing: 0 },
    ],
    speed: 1.4,
  });
  const route = cli('route', 'import', 'Fixture route', '--input', directory + '/route-input.json');
  raw('backup', 'export', '--gzip', '--output', exported);
  const preview = cli('backup', 'import', '--input', exported, '--preview');
  assert.equal(preview.preview, true);
  assert.equal(preview.places, 1);
  assert.equal(cli('place', 'list').length, 1);
  cli('service', 'start');
  assert.throws(() => raw('backup', 'import', '--input', exported, '--replace'), /shut down/);
  assert.equal(cli('place', 'list')[0].id, place.id);
  cli('service', 'stop');
  const merged = cli('backup', 'import', '--input', exported, '--category', 'places');
  assert.equal(merged.places, 1);
  assert.equal(cli('place', 'list').length, 2);
  assert.equal(cli('route', 'list')[0].id, route.id);
  const restored = cli('backup', 'import', '--input', exported, '--replace');
  assert.equal(restored.replaced, true);
  assert.equal(cli('place', 'list').length, 1);
  assert.equal(cli('place', 'list')[0].latitude, place.latitude);
  assert.equal(cli('route', 'export', route.id).points.length, 2);
  // Recover a process interruption before the service reads configuration.
  const original = root('cat ' + quote(directory + '/config.json'));
  write('backup-restore.json', {
    version: 1,
    committed: false,
    originals: [['config.json', Buffer.from(original).toString('base64')]],
    created: [],
    obsolete: [],
  });
  write('config.json', { deliberately: 'incomplete restore' });
  cli('service', 'start');
  assert.equal(cli('status').requested_active, false);
  cli('service', 'stop');
  assert.equal(root('cat ' + quote(directory + '/config.json')), original);
  console.log(
    'PASS backup GZIP, preview, category merge, replacement, live-service lock and startup recovery',
  );
} finally {
  try {
    cli('service', 'stop');
  } catch {}
  root('rm -rf ' + quote(directory));
}
