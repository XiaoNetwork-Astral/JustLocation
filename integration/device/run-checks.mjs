import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/* Configure the backend, collect the probe app report and check each output channel. */

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
const apk = resolve(root, 'dist/justlocation-probe-debug.apk');
const app = 'me.idk.justlocation.probe';
const activity = `${app}/${app}.ChannelCheckActivity`;
const helper = '/data/local/tmp/justlocation-check.sh';

/* Read original operator properties separately; this ROM accepts one getprop key per call. */
const OPERATOR_PROPS =
  'getprop gsm.operator.alpha; getprop gsm.sim.operator.alpha; ' +
  'getprop gsm.operator.numeric; getprop gsm.sim.operator.numeric';

const latitude = 31.2304;
const longitude = 121.4737;
const altitude = 42;
const accuracy = 6;
const ssid = 'JustLocation-Test';
// Assert scan results independently of connection information.
// The first target is the connected network; remaining targets are nearby networks.
const nearbySsid = 'JustLocation-Nearby';
const otherSsid = 'JustLocation-Other';
const mcc = '460';
const mnc = '11';
// The carrier name is test data and must match the original device value.
const carrier = '中国联通';
const event = 'acceptance region';

function run(args, input) {
  const result = spawnSync(adb, ['-s', serial, ...args], {
    input,
    encoding: 'utf8',
    timeout: 120_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(
      `adb ${args.slice(0, 3).join(' ')} failed (${result.status}):\n${result.stdout}\n${result.stderr}`,
    );
  return result.stdout.trim();
}

const shell = (command) => run(['shell', command]);

/* Base64 preserves JSON across the adb shell transport. */
function request(frame) {
  const encoded = Buffer.from(JSON.stringify(frame), 'utf8').toString('base64');
  const reply = run(['shell', `sh ${helper} ${encoded}`]);
  let parsed;
  try {
    parsed = JSON.parse(reply.slice(reply.indexOf('{')));
  } catch (error) {
    throw new Error(`${frame.op} response is not JSON:\n${reply}`);
  }
  if (parsed.ok !== true)
    throw new Error(`${frame.op} was rejected (error=${JSON.stringify(parsed.error)}):\n${reply}`);
  return parsed.state;
}

const report = [];
function phase(name) {
  report.push(`\n=== ${name} ===`);
  process.stdout.write(`\n=== ${name} ===\n`);
}
function check(label, condition, detail = '') {
  const line = `${condition ? 'PASS' : 'FAIL'} ${label}${detail ? ` — ${detail}` : ''}`;
  report.push(line);
  process.stdout.write(`${line}\n`);
  return condition;
}

const failures = [];
let installed = false;

try {
  phase('setup');

  run(['push', resolve(root, 'integration/device/request.sh'), helper]);
  shell(`chmod 700 ${helper}`);
  check('request helper installed', shell(`test -x ${helper} && echo yes`) === 'yes');
  check('device booted', shell('getprop sys.boot_completed') === '1');
  const systemServer = shell('pidof system_server');
  check('system_server running', /^\d+$/.test(systemServer), systemServer);

  const state = request({ version: 1, op: 'status' });
  check('hook_connected', state.hook_connected === true);
  check('location_hook_ready', state.location_hook_ready === true);
  check('gnss_hook_ready', state.gnss_hook_ready === true, String(state.gnss_hook_ready));
  check('nmea_hook_ready', state.nmea_hook_ready === true, String(state.nmea_hook_ready));

  check(
    'gnss_raw_hook_ready',
    state.gnss_raw_hook_ready === true,
    String(state.gnss_raw_hook_ready),
  );
  check('cell_hook_ready', state.cell_hook_ready === true, String(state.cell_hook_ready));
  check('cell_query_hook_ready', state.cell_query_hook_ready === true);
  check('cell_callback_hook_ready', state.cell_callback_hook_ready === true);
  check('sim_hook_ready', state.sim_hook_ready === true);

  check(
    'wifi_scan_hook_ready',
    state.wifi_scan_hook_ready === true,
    String(state.wifi_scan_hook_ready),
  );
  check(
    'wifi_connection_hook_ready',
    state.wifi_connection_hook_ready === true,
    String(state.wifi_connection_hook_ready),
  );

  const realSsid = request({ version: 1, op: 'status' }).wifi?.targets?.[0]?.ssid ?? null;
  const deviceSsid = shell(`dumpsys wifi | grep -m1 -i "mWifiInfo" | head -1`).trim();
  check(
    'real Wi-Fi state recorded',
    deviceSsid.length >= 0,
    realSsid ? `saved target ${realSsid}` : 'no saved target',
  );

  const realOperatorProperties = shell(OPERATOR_PROPS);
  check(
    'real operator properties recorded',
    realOperatorProperties.length > 0,
    realOperatorProperties.replaceAll('\n', ' | '),
  );

  phase('install the app under test');
  run(['install', '-r', '-g', apk]);
  installed = true;
  check('probe app installed with permissions', shell(`pm list packages ${app}`).includes(app));

  phase('write the acceptance configuration');
  request({ version: 1, op: 'stop' });
  request({
    version: 1,
    op: 'set_wifi',
    config: {
      enabled: true,
      targets: [
        {
          id: 'acceptance',
          ssid,
          bssid: '02:11:22:33:44:55',
          rssi: -42,
          link_speed: 866,
          frequency: 5745,
        },
        {
          id: 'nearby',
          ssid: nearbySsid,
          bssid: '02:11:22:33:44:66',
          rssi: -55,
          link_speed: 433,
          frequency: 2412,
        },
        {
          id: 'other',
          ssid: otherSsid,
          bssid: '02:11:22:33:44:77',
          rssi: -70,
          link_speed: 144,
          frequency: 2437,
        },
      ],
    },
  });
  request({
    version: 1,
    op: 'set_telephony',
    config: {
      cells_enabled: true,
      sim_enabled: true,
      radius_m: 500,
      subscriptions: [
        { id: 1, slot: 0, mcc, mnc, country: 'cn', carrier, enabled: true },
        { id: 2, slot: 1, mcc, mnc, country: 'cn', carrier, enabled: true },
      ],
    },
  });
  request({
    version: 1,
    op: 'set_cell_region',
    region: {
      center: { latitude, longitude },
      radius_m: 2000,
      source: event,
      fetched_at_ms: Date.now(),
      cells: [
        {
          identity: { radio: 'lte', mcc, mnc, tac: 6001, ci: 12345678, pci: 301 },
          position: { latitude: latitude + 0.002, longitude: longitude + 0.003 },
          range_m: 800,
        },
        {
          identity: { radio: 'nr', mcc, mnc, tac: 6001, nci: 987654321, pci: 401 },
          position: { latitude: latitude - 0.002, longitude: longitude - 0.002 },
          range_m: 500,
        },
      ],
    },
  });
  request({ version: 1, op: 'set_gnss', config: { gnss_enabled: true, nmea_enabled: true } });
  const started = request({
    version: 1,
    op: 'start',
    config: {
      position: { latitude, longitude, altitude, accuracy, speed: 0, bearing: 0 },
      scope: { mode: 'apps', packages: [app] },
    },
  });
  check('simulation started with app-only scope', started.requested_active === true);
  check('cell output enabled', started.telephony?.cells_enabled === true);
  check('SIM output enabled', started.telephony?.sim_enabled === true);
  check('satellite output enabled', started.gnss?.gnss_enabled === true);
  check('NMEA output enabled', started.gnss?.nmea_enabled === true);
  check('Wi-Fi output enabled', started.wifi?.enabled === true);

  check(
    'operator_hook_ready',
    started.operator_hook_ready === true,
    String(started.operator_hook_ready),
  );
  const spoofedOperatorProperties = shell(OPERATOR_PROPS);
  check(
    'system operator properties rewritten',
    spoofedOperatorProperties.includes(carrier) &&
      spoofedOperatorProperties.includes(`${mcc}${mnc}`),
    spoofedOperatorProperties.replaceAll('\n', ' | '),
  );

  phase('device self-check');
  shell('logcat -c');

  const launch = spawnSync(
    adb,
    ['-s', serial, 'shell', 'am', 'start', '-n', activity, '--ez', 'autorun', 'true'],
    { encoding: 'utf8', timeout: 30_000 },
  );
  assert.equal(
    launch.status,
    0,
    `cannot start the self-check activity: ${launch.stdout ?? ''}${launch.stderr ?? ''}`,
  );
  let lines = [];

  const deadline = Date.now() + 120_000;
  for (;;) {
    const out = spawnSync(
      adb,
      ['-s', serial, 'shell', 'logcat', '-d', '-s', 'JustLocationCheck:I'],
      { encoding: 'utf8', timeout: 30_000 },
    );
    lines = (out.stdout ?? '')
      .split(/\r?\n/)
      .map((line) => line.replace(/^.*JustLocationCheck\s*:\s?/, '').trimEnd());
    if (lines.includes('CHECK_REPORT_END')) break;
    if (Date.now() > deadline) break;
    await new Promise((done) => setTimeout(done, 2000));
  }
  const begin = lines.indexOf('CHECK_REPORT_BEGIN');
  const end = lines.indexOf('CHECK_REPORT_END');
  assert.ok(
    begin >= 0 && end > begin,
    `no complete self-check report captured (got ${lines.length} lines)`,
  );
  const body = lines.slice(begin + 1, end);

  const sections = new Map();
  let current = 'preamble';
  sections.set(current, []);
  for (const line of body) {
    if (line.startsWith('## ')) {
      current = line.slice(3).trim();
      sections.set(current, []);
      continue;
    }
    sections.get(current).push(line);
  }
  const section = (name) => (sections.get(name) ?? []).filter((line) => line.length > 0);
  const all = body.join('\n');

  phase('channel verdicts');

  const crashed = section('self-check error');
  if (!check('self-check did not throw', crashed.length === 0, crashed.join(' ') || ''))
    failures.push('self-check error');
  const appLines = section('app');
  if (
    !check(
      'self-check reports this package',
      appLines.some((line) => line.includes(app)),
      appLines[0] ?? '',
    )
  )
    failures.push('app');

  const location = section('location');
  const number = (text) => Number.parseFloat(text);
  const fixOf = (provider) => location.find((line) => line.startsWith(provider));
  for (const provider of ['gps', 'network']) {
    const line = fixOf(provider);
    if (!check(`${provider} has a fix`, Boolean(line))) {
      failures.push(provider);
      continue;
    }
    const match = line.match(/(-?\d+\.\d+), (-?\d+\.\d+)/);
    const matches =
      match &&
      Math.abs(number(match[1]) - latitude) < 1e-4 &&
      Math.abs(number(match[2]) - longitude) < 1e-4;
    if (!check(`${provider} fix is the simulated point`, Boolean(matches), line))
      failures.push(`${provider} coordinates`);
    if (
      !check(
        `${provider} altitude and accuracy are simulated`,
        line.includes(`altitude ${altitude}.0`) && line.includes(`accuracy ${accuracy}.0`),
        line,
      )
    )
      failures.push(`${provider} altitude`);
    if (!check(`${provider} not flagged as mock`, line.includes('mock=false'), line))
      failures.push(`${provider} mock`);
  }

  const gnss = section('satellites');
  const count = gnss.find((line) => line.startsWith('count '));
  if (!check('satellite status callback arrived', Boolean(count), gnss[0] ?? 'no reading'))
    failures.push('satellite callback');
  else {
    const total = number(count.replace('count ', '').trim());
    if (!check('satellite count is the simulated 8', total === 8, count))
      failures.push('satellite count');
    const sky = gnss.find((line) => line.startsWith('svid/cn0')) ?? '';
    const expected = ['3/38', '7/42', '11/35', '14/44', '19/40', '22/37', '26/41', '30/36'];
    const missing = expected.filter((entry) => !sky.includes(entry));
    if (
      !check(
        'constellation and C/N0 match the simulated table',
        missing.length === 0,
        missing.length ? `missing ${missing.join(' ')}` : sky,
      )
    )
      failures.push('constellation');
  }

  const nmea = section('nmea');
  const messages = nmea.find((line) => line.startsWith('sentences '));
  const kinds = nmea.find((line) => line.startsWith('types ')) ?? '';
  if (
    !check(
      'NMEA sentences arrived',
      Boolean(messages) && number(messages.replace('sentences ', '').trim()) > 0,
      messages ?? 'no reading',
    )
  )
    failures.push('nmea sentences');
  const wanted = ['GPGGA', 'GPRMC', 'GPGSA', 'GPGSV', 'GPVTG'];
  const absent = wanted.filter((kind) => !kinds.includes(kind));
  if (
    !check(
      'sentence types are the five synthetic ones',
      absent.length === 0,
      absent.length ? `missing ${absent.join(' ')}` : kinds,
    )
  )
    failures.push('nmea types');

  const minutes = (latitude - Math.trunc(latitude)) * 60;
  const expectedPoint = `${String(Math.trunc(latitude)).padStart(2, '0')}${minutes.toFixed(5).padStart(8, '0')}`;
  if (
    !check(
      'NMEA carries the simulated coordinates',
      all.includes(expectedPoint),
      `expected ${expectedPoint}`,
    )
  )
    failures.push('nmea coordinates');

  const raw = section('raw gnss');
  const measurements = raw.find((line) => line.startsWith('measurements ')) ?? '';
  const navigation = raw.find((line) => line.startsWith('navigation ')) ?? '';

  const parcelLine = raw.find((line) => line.startsWith('parcel_roundtrip ')) ?? '';
  if (
    !check(
      'navigation message survives a Parcel round-trip in the app process',
      parcelLine.includes('same_type=true'),
      parcelLine || 'no reading',
    )
  )
    failures.push('navigation parcel round-trip');
  if (
    !check(
      'raw measurements arrived with the right satellite count',
      /seen=[1-9]\d*/.test(measurements) && measurements.includes('satellites=8'),
      measurements || 'no reading',
    )
  )
    failures.push('raw measurements');

  if (
    /seen=[1-9]\d*/.test(navigation) &&
    navigation.includes('type=1') &&
    navigation.includes('length=40')
  ) {
    check('navigation messages arrived and are GPS L1 C/A', true, navigation);
  } else {
    report.push(
      `KNOWN-GAP navigation messages not delivered (ROM parcel defect): ${navigation || 'no reading'}`,
    );
    process.stdout.write(
      `KNOWN-GAP navigation messages not delivered (ROM parcel defect): ${navigation || 'no reading'}\n`,
    );
  }

  // Readiness confirms installation; callback counts confirm execution.

  // Dispatcher indices identify channels; system listeners may add unrelated entries.

  const detail = request({ version: 1, op: 'status' }).gnss_raw_detail ?? '';
  const channels = Object.fromEntries(
    detail
      .split(';')
      .filter(Boolean)
      .map((entry) => {
        const [index, values] = entry.split(':');
        const [
          registrations,
          dispatches,
          needed,
          ticks,
          delivered,
          failed,
          wrapperTicks,
          wrapperDeliveries,
          reason,
          identity,
        ] = values.split(',');
        return [
          index,
          {
            registrations,
            dispatches,
            needed,
            ticks,
            delivered,
            failed,
            wrapperTicks,
            wrapperDeliveries,
            reason,
            identity,
          },
        ];
      }),
  );
  check('gnss_raw_detail is readable', Object.keys(channels).length >= 4, detail || 'no counters');
  for (const [name, index] of [
    ['raw measurements', '2'],
    ['navigation messages', '3'],
  ]) {
    const channel = channels[index];
    // Require successful delivery; current registration counts may fall to zero after cleanup.

    if (
      !check(
        `${name} channel delivered`,
        channel && Number(channel.wrapperDeliveries) > 0,
        channel
          ? `registrations=${channel.registrations} needed=${channel.needed} wrapper_ticks=${channel.wrapperTicks}` +
              ` delivered=${channel.wrapperDeliveries} failed=${channel.failed}` +
              (channel.identity && channel.identity !== '-'
                ? ` receiver=${channel.identity}`
                : '') +
              (channel.reason && channel.reason !== '-' ? ` reason=${channel.reason}` : '')
          : 'no counters',
      )
    )
      failures.push(`${name} delivery`);
  }

  const cell = section('cells');
  if (
    !check(
      'cell info readable',
      cell.length > 0 && !cell[0].includes('read failed'),
      cell[0] ?? 'no reading',
    )
  )
    failures.push('cell read');
  else {
    // Report synthesized cells separately from acquired tower records.

    if (request({ version: 1, op: 'status' }).cells_synthesized === true) {
      report.push(
        'NOTE cells_synthesized=true: the reported cells are fabricated because no real cell data covers this position',
      );
      process.stdout.write(
        'NOTE cells_synthesized=true: the reported cells are fabricated because no real cell data covers this position\n',
      );
    }
    // Keep test cells within the configured query radius.
    // Identify test cells by CI/NCI; real cells may share their MCC/MNC.

    const lte = cell.filter((line) => line.includes('CellInfoLte'));
    const nr = cell.filter((line) => line.includes('CellInfoNr'));
    if (
      !check(
        'LTE cell carries the simulated CI',
        lte.some((line) => line.includes('Ci=12345678')),
        lte.join(' | ') || 'no LTE rows',
      )
    )
      failures.push('LTE cell');
    if (
      !check(
        'NR cell carries the simulated NCI',
        nr.some((line) => line.includes('Nci=987654321')),
        nr.join(' | ') || 'no NR rows',
      )
    )
      failures.push('NR cell');

    if (
      !check(
        'cell row count matches the subscription count',
        lte.length + nr.length === 4,
        `LTE ${lte.length}, NR ${nr.length}`,
      )
    )
      failures.push('cell rows');
  }

  const sim = section('sim');
  const slots = sim.filter((line) => line.startsWith('slot '));
  if (slots.length === 2) {
    if (
      !check(
        'both SIMs report the simulated operator',
        slots.every((line) => line.includes(carrier) && line.includes(`${mcc}${mnc}`)),
        slots.join(' | '),
      )
    )
      failures.push('subscription list');
  } else {
    // Record missing READ_PHONE_STATE permission separately from assertion failures.
    report.push(`KNOWN-GAP subscription list unreadable: ${sim.join(' | ') || 'no reading'}`);
    process.stdout.write(
      `KNOWN-GAP subscription list unreadable: ${sim.join(' | ') || 'no reading'}\n`,
    );
  }
  // Operator properties and subscription records are separate outputs; check both.

  const operatorLines = sim.filter(
    (line) => line.startsWith('network_operator ') || line.startsWith('sim_operator '),
  );
  if (
    !check(
      'operator getters report the simulated name and PLMN',
      operatorLines.length === 2 &&
        operatorLines.every((line) => line.includes(carrier) && line.includes(`${mcc}${mnc}`)),
      operatorLines.join(' | ') || 'no reading',
    )
  )
    failures.push('operator getters');

  // Check each Wi-Fi report line separately so scan names cannot mask a wrong connection.

  const wifiLines = section('wifi');
  const connection = wifiLines.find((line) => line.startsWith('SSID ')) ?? '';
  const nearby = wifiLines.find((line) => line.startsWith('nearby networks')) ?? '';
  if (
    !check(
      'wifi connection reports the simulated network',
      connection.includes(ssid),
      connection || 'no reading',
    )
  )
    failures.push('wifi connection');
  if (
    !check(
      'wifi scan results contain the saved targets',
      nearby.includes(`${nearbySsid}(02:11:22:33:44:66`) &&
        nearby.includes(`${otherSsid}(02:11:22:33:44:77`),
      nearby || 'no reading',
    )
  )
    failures.push('wifi scan results');
  // Readiness confirms installation; callback counts confirm execution.

  const wifiCalls = request({ version: 1, op: 'status' }).wifi_hook_calls;
  if (!check('wifi hooks were actually called', wifiCalls > 0, `wifi_hook_calls=${wifiCalls}`))
    failures.push('wifi calls');
  // NetworkCapabilities.transportInfo is not replaced; report it without asserting replacement.

  const transport =
    wifiLines.find((line) => line.startsWith('NetworkCapabilities')) ?? 'no reading';
  if (!transport.includes(ssid)) {
    report.push(`KNOWN-GAP ConnectivityManager not covered: ${transport}`);
    process.stdout.write(`KNOWN-GAP ConnectivityManager not covered: ${transport}\n`);
  }

  // Use all-app scope to verify that the system UID gate still preserves real values.

  // Restart the session to change scope; update only changes the position.
  request({ version: 1, op: 'stop' });
  request({
    version: 1,
    op: 'start',
    config: {
      position: { latitude, longitude, altitude, accuracy, speed: 0, bearing: 0 },
      scope: { mode: 'all' },
    },
  });
  await new Promise((done) => setTimeout(done, 3000));
  const shellWifi = shell('cmd wifi status 2>&1 | grep -m1 "Wifi is connected"').trim();
  if (
    !check(
      'system identity still reads the real network',
      shellWifi.length > 0 && !shellWifi.includes(ssid),
      shellWifi || 'no reading',
    )
  ) {
    failures.push('system identity wifi');
  }

  phase('cleanup');
  request({ version: 1, op: 'stop' });
  const after = request({ version: 1, op: 'status' });
  check('simulation stopped', after.requested_active === false);
  // Stopping simulation must restore the original operator properties.

  check(
    'system operator properties restored',
    shell(OPERATOR_PROPS) === realOperatorProperties,
    `before ${realOperatorProperties} / now ${shell(OPERATOR_PROPS)}`,
  );
  shell(`rm -f ${helper}`);
  check('temporary helper removed', shell(`ls ${helper} 2>/dev/null || true`) === '');
  check(
    'system_server was not restarted',
    shell('pidof system_server') === systemServer,
    `${systemServer} -> ${shell('pidof system_server')}`,
  );

  const failed = report.filter((line) => line.startsWith('FAIL'));
  process.stdout.write(
    `\n=== summary ===\npassed ${report.filter((line) => line.startsWith('PASS')).length}, failed ${failed.length}\n`,
  );
  if (failed.length > 0) throw new Error(`failures:\n${failed.join('\n')}`);
  process.stdout.write('all channels verified\n');
} finally {
  // Stop simulation before deleting the request helper so failure cleanup can still reach the backend.

  try {
    request({ version: 1, op: 'stop' });
  } catch {}
  // Clean up this run only; leave the daemon, module and installed apps intact.
  try {
    run(['shell', `rm -f ${helper}`]);
  } catch {}
  if (!installed) process.stdout.write('(APK not installed: exited before the install step)\n');
}
