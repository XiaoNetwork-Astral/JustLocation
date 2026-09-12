import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';

const root = resolve(import.meta.dirname, '..');
const version = JSON.parse(readFileSync(join(root, 'project-config.json'), 'utf8')).version;
const temporary = join(root, 'build/tmp');
mkdirSync(temporary, { recursive: true });
const bash =
  process.env.BASH || (process.platform === 'win32' ? 'C:/Program Files/Git/bin/bash.exe' : 'bash');
const unix = (path) =>
  path.replaceAll('\\', '/').replace(/^([A-Za-z]):/, (_, drive) => `/${drive.toLowerCase()}`);

function install(scenario) {
  const directory = mkdtempSync(join(temporary, 'installer-'));
  mkdirSync(join(directory, 'module'));
  const script = String.raw`
    MODPATH="$FIXTURE/module"
    KSU=true; BOOTMODE=true; ARCH=arm64; API=35; KSU_VER=v3.3.0; KSU_VER_CODE=33000
    [ "$SCENARIO" != unsupported ] || API=34
    ui_print() { printf '%s\n' "$*"; }
    abort() { ui_print "$*"; exit 1; }
    set_perm_recursive() { :; }
    set_perm() { :; }
    sleep() { :; }
    pm() {
      printf '%s\n' "$*" >> "$FIXTURE/pm.log"
      case "$1" in
        path) return 1 ;;
        uninstall) echo Success ;;
        install)
          case "$SCENARIO" in
            storage) echo 'Failure [INSTALL_FAILED_INSUFFICIENT_STORAGE]' >&2; return 1 ;;
            signature)
              if [ ! -f "$FIXTURE/retried" ]; then
                touch "$FIXTURE/retried"
                echo 'Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE]' >&2; return 1
              fi ;;
          esac
          echo Success ;;
      esac
    }
    appops() {
      if [ "$SCENARIO" = overlay ]; then echo 'Error: operation not allowed' >&2; return 1; fi
      [ "$1" != get ] || echo 'SYSTEM_ALERT_WINDOW: allow'
    }
    unzip() {
      if [ "$SCENARIO" = missing ] && [ "$3" = verify.sh ]; then return 1; fi
      command unzip "$@" || return $?
      if [ "$SCENARIO" = checksum ] && [ "$3" = checksums ]; then
        sed -i '1s/^[^ ]*/0000/' "$MODPATH/checksums"
      fi
    }
    . "$SOURCE"
  `;
  try {
    const result = spawnSync(bash, ['-c', script], {
      encoding: 'utf8',
      timeout: 30_000,
      env: {
        ...process.env,
        SCENARIO: scenario,
        FIXTURE: unix(directory),
        SOURCE: unix(join(root, 'packaging/module/customize.sh')),
        ZIPFILE: unix(join(root, `dist/justlocation-${version}-arm64.zip`)),
      },
    });
    assert.ifError(result.error);
    let commands = '';
    try {
      commands = readFileSync(join(directory, 'pm.log'), 'utf8');
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
    }
    return { code: result.status, output: result.stdout + result.stderr, commands };
  } finally {
    assert.equal(dirname(resolve(directory)), resolve(temporary));
    rmSync(directory, { recursive: true });
  }
}

test('successful install reports the device, verified payload, permission and required Root grant', () => {
  const result = install('success');
  assert.equal(result.code, 0, result.output);
  for (const text of [
    `JustLocation version ${version}`,
    'Android API: 35',
    'Device platform: arm64',
    'Verified ',
    'Overlay permission: allowed',
    'KernelSU > Superuser',
    'Setting file permissions',
  ])
    assert.ok(result.output.includes(text), result.output);
  assert.ok(!result.commands.includes('uninstall'));
  console.log(result.output);
});
test('installation failure preserves the existing app and prints the Package Manager reason', () => {
  const result = install('storage');
  assert.equal(result.code, 0);
  assert.match(result.output, /INSTALL_FAILED_INSUFFICIENT_STORAGE/);
  assert.match(result.output, /recording and overlay controls are unavailable/);
  assert.ok(!result.commands.includes('uninstall'));
  assert.ok(!result.output.includes('Overlay permission: allowed'));
});
test('signature mismatch is reported before the targeted reinstall', () => {
  const result = install('signature');
  assert.equal(result.code, 0, result.output);
  assert.match(result.output, /signature mismatch/);
  assert.match(result.commands, /uninstall me.idk.justlocation.joystick/);
  assert.equal(result.commands.split('\n').filter((line) => line.startsWith('install ')).length, 2);
});
test('overlay denial never produces a false allowed message', () => {
  const result = install('overlay');
  assert.equal(result.code, 0, result.output);
  assert.match(result.output, /operation not allowed/);
  assert.match(result.output, /Display over other apps/);
  assert.ok(!result.output.includes('Overlay permission: allowed'));
});
for (const [scenario, reason] of [
  ['unsupported', 'Unsupported Android API: 34 (requires 35)'],
  ['missing', 'Cannot extract verify.sh from ZIP'],
  ['checksum', 'Verification failed:'],
]) {
  test(`${scenario} stops before installing the app and identifies the failure`, () => {
    const result = install(scenario);
    assert.equal(result.code, 1, result.output);
    assert.ok(result.output.includes(reason), result.output);
    assert.equal(result.commands, '');
  });
}
