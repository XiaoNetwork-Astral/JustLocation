import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';

// An isolated daemon exercises CLI persistence without feeding the installed module's hooks.
const [adbPath, serial, hostBinary] = process.argv.slice(2);
if (!adbPath || !serial || !hostBinary)
  throw new Error('Usage: node run-scopes.mjs ADB SERIAL ARM64_BINARY');
const adb = (...args) =>
  execFileSync(adbPath, ['-s', serial, ...args], {
    encoding: 'utf8',
    timeout: 30_000,
    maxBuffer: 2 * 1024 * 1024,
  }).trim();
const quote = (value) => "'" + String(value).replaceAll("'", "'\\''") + "'";
const root = (command) => adb('shell', 'su -c ' + quote(command));
const directory = '/data/local/tmp/justlocation-scopes-' + Date.now();
assert.match(directory, /^\/data\/local\/tmp\/justlocation-scopes-\d+$/);
const binary = directory + '/justlocationd';
const invoke = (...args) =>
  root([binary, '--data-dir', directory, '--json', ...args].map(quote).join(' '));
const cli = (...args) => JSON.parse(invoke(...args));
const apps = (name) => ({ mode: 'apps', packages: ['example.' + name] });
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

adb('shell', 'mkdir -m 700 ' + quote(directory));
try {
  adb('push', hostBinary, binary);
  root('chmod 700 ' + quote(binary));
  const initial = await start();
  assert.equal(initial.telephony.sim_enabled, false);
  assert.equal(initial.requested_active, false);
  assert.throws(() => cli('start', '--lat', '0', '--lon', '0'));
  for (const feature of ['position', 'route', 'wifi', 'sim']) {
    cli('scope', 'set', '--feature', feature, '--app', 'example.' + feature);
    assert.deepEqual(cli('scope', 'get', '--feature', feature), apps(feature));
  }
  const selected = cli('status').scopes;
  const position = cli('start', '--lat', '0', '--lon', '0');
  assert.deepEqual(position.config.scope, apps('position'));
  assert.equal(position.location_hook_ready, false);
  cli('stop');
  const routePath = directory + '/route.json';
  const point = (longitude) => ({
    latitude: 0,
    longitude,
    altitude: 0,
    accuracy: 5,
    speed: 0,
    bearing: 0,
  });
  root(
    'printf %s ' +
      quote(JSON.stringify({ points: [point(0), point(0.001)], speed: 1 })) +
      ' > ' +
      quote(routePath),
  );
  const route = cli('route', 'start', '--input', routePath);
  assert.deepEqual(route.config.scope, apps('route'));
  const changed = cli('scope', 'set', '--feature', 'wifi', '--app', 'example.other-wifi');
  assert.deepEqual(changed.config.scope, apps('route'));
  assert.deepEqual(changed.scopes.sim, selected.sim);
  const switched = cli('scope', 'set', '--feature', 'route', '--app', 'example.other-route');
  assert.deepEqual(switched.config.scope, apps('other-route'));
  const expected = switched.scopes;
  stop();
  const restored = await start();
  assert.equal(restored.requested_active, false);
  assert.deepEqual(restored.scopes, expected);
  assert.deepEqual(restored.config.scope, apps('position'));
  const legacy = cli('scope', 'set', '--app', 'example.shared');
  for (const scope of Object.values(legacy.scopes)) assert.deepEqual(scope, apps('shared'));
  console.log(
    'PASS: feature CLI defaults, live selection, legacy shared edit, restart persistence; isolated daemon only',
  );
} finally {
  stop();
  root('rm -r -- ' + quote(directory));
}
