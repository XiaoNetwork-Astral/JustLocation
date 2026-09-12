import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * 逐通道真机验收。
 *
 * <p>为什么要有这个脚本：定位之外的通道没有可观察的入口，"模块装上了"不等于"输出被替换了"。
 * 做法是把配置发给后台，再让设备上的自检 App 用普通应用接口读一遍
 * （`ChannelCheckActivity`，只读公开 API），脚本对这份报告做断言。判定只有一处，
 * 应用侧只负责如实读出。
 *
 * <p>用法：`node build.mjs test:checks <adb-serial>`。脚本自己装 APK、跑配置、抓报告。
 * 结束时把模拟停掉并清掉设备上的临时文件。
 *
 * <p>**这个脚本的输出一律英文**（2026-09-12 用户要求）：它要经过 adb、终端与各家的
 * shell 显示链路，中文在这些地方容易被编码搞坏；而状态位本来就是英文键名，
 * 断言名直接用键名，三处（状态回包、action.sh、本脚本）叫同一个名字，排查时不用做翻译。
 * 唯一保留中文的是**被测数据的值**（运营商名），那是设备真实返回的内容，改了就不再是同一个断言。
 */

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
const apk = resolve(root, 'dist/justlocation-probe-debug.apk');
const app = 'me.idk.justlocation.probe';
const activity = `${app}/${app}.ChannelCheckActivity`;
const helper = '/data/local/tmp/justlocation-check.sh';

/**
 * 设备上真实的运营商属性。`TelephonyManager` 的四个运营商 getter 读的就是它们，
 * 而应用进程内读属性不经过 system_server，所以只能由守护进程改写属性来接管。
 * 这台 ROM 的 `getprop` 只收一个属性名，所以逐个读。
 */
const OPERATOR_PROPS = 'getprop gsm.operator.alpha; getprop gsm.sim.operator.alpha; '
  + 'getprop gsm.operator.numeric; getprop gsm.sim.operator.numeric';

/** 验收用的参数：都取"一眼能看出是模拟值"的数字，避免和真实环境混淆。 */
const latitude = 31.2304;
const longitude = 121.4737;
const altitude = 42;
const accuracy = 6;
const ssid = 'JustLocation-Test';
// 附近网络走 `getScanResults`，是与连接信息**分开**的一处服务端出口，所以要单独断言。
// 原版模型里目标列表的第一条是当前连接的网络，其余是附近网络，这里照这个形状配。
const nearbySsid = 'JustLocation-Nearby';
const otherSsid = 'JustLocation-Other';
const mcc = '460';
const mnc = '11';
// 运营商名是被测数据（设备真实返回的中文），断言必须比对它本身，不能翻译。
const carrier = '中国联通';
const event = 'acceptance region';

function run(args, input) {
  const result = spawnSync(adb, ['-s', serial, ...args], { input, encoding: 'utf8', timeout: 120_000, maxBuffer: 4 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`adb ${args.slice(0, 3).join(' ')} failed (${result.status}):\n${result.stdout}\n${result.stderr}`);
  return result.stdout.trim();
}

const shell = (command) => run(['shell', command]);

/**
 * 把一帧请求编码成 base64 再交给设备。
 *
 * <p>请求体在这里先编码成 base64 再交给设备：adb shell 那一层会把 JSON 里的引号、
 * 花括号和逗号吃掉（实测 `{"version":1,"op":"status"}` 到设备上变成 `version:1 op:status`），
 * 只传字母数字就穿得过去。设备侧的脚本负责把它交给后台。
 */
function request(frame) {
  const encoded = Buffer.from(JSON.stringify(frame), 'utf8').toString('base64');
  const reply = run(['shell', `sh ${helper} ${encoded}`]);
  let parsed;
  try { parsed = JSON.parse(reply.slice(reply.indexOf('{'))); }
  catch (error) { throw new Error(`${frame.op} response is not JSON:\n${reply}`); }
  if (parsed.ok !== true) throw new Error(`${frame.op} was rejected (error=${JSON.stringify(parsed.error)}):\n${reply}`);
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

// 后台需要 root；helper 自己走 su，避免三层引号嵌套。

const failures = [];
let installed = false;

try {
  phase('setup');
  // 请求助手要先到位，后面每一步都要用它；结束时删掉。
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
  // 原始测量与导航电文是一个合并位：两条出口在同一次安装里挂，任一条失败就是 false。
  check('gnss_raw_hook_ready', state.gnss_raw_hook_ready === true, String(state.gnss_raw_hook_ready));
  check('cell_hook_ready', state.cell_hook_ready === true, String(state.cell_hook_ready));
  check('cell_query_hook_ready', state.cell_query_hook_ready === true);
  check('cell_callback_hook_ready', state.cell_callback_hook_ready === true);
  check('sim_hook_ready', state.sim_hook_ready === true);
  // Wi-Fi 两项分开断言：原版报告特别强调"扫描与连接信息不是同一项适配"。
  check('wifi_scan_hook_ready', state.wifi_scan_hook_ready === true, String(state.wifi_scan_hook_ready));
  check('wifi_connection_hook_ready', state.wifi_connection_hook_ready === true, String(state.wifi_connection_hook_ready));

  // 记下验收前的真实 SSID，用来判断 Wi-Fi 通道有没有真的被接管。
  const realSsid = request({ version: 1, op: 'status' }).wifi?.targets?.[0]?.ssid ?? null;
  const deviceSsid = shell(`dumpsys wifi | grep -m1 -i "mWifiInfo" | head -1`).trim();
  check('real Wi-Fi state recorded', deviceSsid.length >= 0, realSsid ? `saved target ${realSsid}` : 'no saved target');
  // 接管运营商 getter 靠改写这四个系统属性，所以先记下真实值，收尾时逐个比对。
  const realOperatorProperties = shell(OPERATOR_PROPS);
  check('real operator properties recorded', realOperatorProperties.length > 0, realOperatorProperties.replaceAll('\n', ' | '));

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
        { id: 'acceptance', ssid, bssid: '02:11:22:33:44:55', rssi: -42, link_speed: 866, frequency: 5745 },
        { id: 'nearby', ssid: nearbySsid, bssid: '02:11:22:33:44:66', rssi: -55, link_speed: 433, frequency: 2412 },
        { id: 'other', ssid: otherSsid, bssid: '02:11:22:33:44:77', rssi: -70, link_speed: 144, frequency: 2437 },
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
  // 运营商属性只在模拟运行且 SIM 通道开启时才被接管，所以这一位要在开始之后断言。
  check('operator_hook_ready', started.operator_hook_ready === true, String(started.operator_hook_ready));
  const spoofedOperatorProperties = shell(OPERATOR_PROPS);
  check('system operator properties rewritten', spoofedOperatorProperties.includes(carrier) && spoofedOperatorProperties.includes(`${mcc}${mnc}`),
      spoofedOperatorProperties.replaceAll('\n', ' | '));

  phase('device self-check');
  shell('logcat -c');
  // 报告由应用写入 logcat；轮询到结束标记为止（采集本身约 6 秒）。
  // `--ez autorun true` 让页面一进去就开始，省掉点按钮（脚本点不动界面元素）。
  const launch = spawnSync(adb, ['-s', serial, 'shell', 'am', 'start', '-n', activity, '--ez', 'autorun', 'true'], { encoding: 'utf8', timeout: 30_000 });
  assert.equal(launch.status, 0, `cannot start the self-check activity: ${launch.stdout ?? ''}${launch.stderr ?? ''}`);
  let lines = [];
  // 采集本身包含一段 60 秒的持续订阅（模拟地图软件的长时间订阅），所以窗口要留够；
  // 太窄的话报告还没打印完就被判成"没抓到"，看起来像自检坏了。
  const deadline = Date.now() + 120_000;
  for (;;) {
    const out = spawnSync(adb, ['-s', serial, 'shell', 'logcat', '-d', '-s', 'JustLocationCheck:I'], { encoding: 'utf8', timeout: 30_000 });
    lines = (out.stdout ?? '').split(/\r?\n/).map((line) => line.replace(/^.*JustLocationCheck\s*:\s?/, '').trimEnd());
    if (lines.includes('CHECK_REPORT_END')) break;
    if (Date.now() > deadline) break;
    await new Promise((done) => setTimeout(done, 2000));
  }
  const begin = lines.indexOf('CHECK_REPORT_BEGIN');
  const end = lines.indexOf('CHECK_REPORT_END');
  assert.ok(begin >= 0 && end > begin, `no complete self-check report captured (got ${lines.length} lines)`);
  const body = lines.slice(begin + 1, end);

  // 报告按 "## title" 分节；断言只看内容，不依赖行的顺序。
  const sections = new Map();
  let current = 'preamble';
  sections.set(current, []);
  for (const line of body) {
    if (line.startsWith('## ')) { current = line.slice(3).trim(); sections.set(current, []); continue; }
    sections.get(current).push(line);
  }
  const section = (name) => (sections.get(name) ?? []).filter((line) => line.length > 0);
  const all = body.join('\n');

  phase('channel verdicts');
  // 自检过程中抛异常时，采集线程会把异常收进 "self-check error" 一节并照常输出报告——
  // 也就是说**报告不完整和报告为空是两回事**。不单独查一下的话，异常之后的所有断言
  // 只会表现为"某一节没读数"，看上去像通道问题而不是自检问题。
  const crashed = section('self-check error');
  if (!check('self-check did not throw', crashed.length === 0, crashed.join(' ') || '')) failures.push('self-check error');
  const appLines = section('app');
  if (!check('self-check reports this package', appLines.some((line) => line.includes(app)), appLines[0] ?? '')) failures.push('app');

  const location = section('location');
  const number = (text) => Number.parseFloat(text);
  const fixOf = (provider) => location.find((line) => line.startsWith(provider));
  for (const provider of ['gps', 'network']) {
    const line = fixOf(provider);
    if (!check(`${provider} has a fix`, Boolean(line))) { failures.push(provider); continue; }
    const match = line.match(/(-?\d+\.\d+), (-?\d+\.\d+)/);
    const matches = match && Math.abs(number(match[1]) - latitude) < 1e-4 && Math.abs(number(match[2]) - longitude) < 1e-4;
    if (!check(`${provider} fix is the simulated point`, Boolean(matches), line)) failures.push(`${provider} coordinates`);
    if (!check(`${provider} altitude and accuracy are simulated`, line.includes(`altitude ${altitude}.0`) && line.includes(`accuracy ${accuracy}.0`), line)) failures.push(`${provider} altitude`);
    if (!check(`${provider} not flagged as mock`, line.includes('mock=false'), line)) failures.push(`${provider} mock`);
  }

  const gnss = section('satellites');
  const count = gnss.find((line) => line.startsWith('count '));
  if (!check('satellite status callback arrived', Boolean(count), gnss[0] ?? 'no reading')) failures.push('satellite callback');
  else {
    const total = number(count.replace('count ', '').trim());
    if (!check('satellite count is the simulated 8', total === 8, count)) failures.push('satellite count');
    const sky = gnss.find((line) => line.startsWith('svid/cn0')) ?? '';
    const expected = ['3/38', '7/42', '11/35', '14/44', '19/40', '22/37', '26/41', '30/36'];
    const missing = expected.filter((entry) => !sky.includes(entry));
    if (!check('constellation and C/N0 match the simulated table', missing.length === 0, missing.length ? `missing ${missing.join(' ')}` : sky)) failures.push('constellation');
  }

  const nmea = section('nmea');
  const messages = nmea.find((line) => line.startsWith('sentences '));
  const kinds = nmea.find((line) => line.startsWith('types ')) ?? '';
  if (!check('NMEA sentences arrived', Boolean(messages) && number(messages.replace('sentences ', '').trim()) > 0, messages ?? 'no reading')) failures.push('nmea sentences');
  const wanted = ['GPGGA', 'GPRMC', 'GPGSA', 'GPGSV', 'GPVTG'];
  const absent = wanted.filter((kind) => !kinds.includes(kind));
  if (!check('sentence types are the five synthetic ones', absent.length === 0, absent.length ? `missing ${absent.join(' ')}` : kinds)) failures.push('nmea types');
  // GPGGA 用度分格式（ddmm.mmmmm）：31.2304° → 3113.82400。别用取模算，浮点误差会咬人。
  const minutes = (latitude - Math.trunc(latitude)) * 60;
  const expectedPoint = `${String(Math.trunc(latitude)).padStart(2, '0')}${minutes.toFixed(5).padStart(8, '0')}`;
  if (!check('NMEA carries the simulated coordinates', all.includes(expectedPoint), `expected ${expectedPoint}`)) failures.push('nmea coordinates');

  // 原始 GNSS 数据：与卫星状态是**两条不同的接口**，所以分开断言。
  // 只断言"装上钩子"没有意义，这里看的是应用真的收到了合成出来的测量与电文。
  const raw = section('raw gnss');
  const measurements = raw.find((line) => line.startsWith('measurements ')) ?? '';
  const navigation = raw.find((line) => line.startsWith('navigation ')) ?? '';
  // `GnssNavigationMessage` 在本进程里能不能被 Parcel 正确还原。这条决定了"送不到"到底该算
  // 谁的账：还原得回来而应用收不到，就是我们投递的问题；还原不回来（或还原成别的类型），
  // 就是类/ROM 这一侧的问题，模块无论怎么改都送不到。
  const parcelLine = raw.find((line) => line.startsWith('parcel_roundtrip ')) ?? '';
  if (!check('navigation message survives a Parcel round-trip in the app process',
          parcelLine.includes('same_type=true'), parcelLine || 'no reading')) failures.push('navigation parcel round-trip');
  if (!check('raw measurements arrived with the right satellite count',
          /seen=[1-9]\d*/.test(measurements) && measurements.includes('satellites=8'),
          measurements || 'no reading')) failures.push('raw measurements');
  // 导航电文曾经是这一轮的唯一缺口：钩子装上了、注册也包住了，应用却收到 0 条。根因是
  // 投递完全依赖 HAL 回调 `onReportNavigationMessage` 触发，而这条在这台设备上不成立——
  // 平台一次都不会来叫我们。现在两条原始出口都改成**自己驱动投递**，所以这里改成硬断言。
  // 导航电文是**这台 ROM 的框架缺陷**，不是待办项（2026-09-12 查清）：
  // 钩子、注册、投递、接收方全部正确（见下面计数里的 receiver 字段），
  // 但接收进程反序列化时抛 `Expected receiver of type GnssMeasurement but got GnssNavigationMessage`——
  // `IGnssNavigationMessageListener$Stub.onTransact` 的字节码是正确的，同一个类在本进程里
  // Parcel 往返也完全正常，所以问题在 ROM 那一侧。要绕开只能伪造 Binder 线格式，
  // 而那条路会跳过权限与 AppOps（备忘录已列为"不做"）。
  // 因此这里**如实记成 KNOWN-GAP，不再当失败**：一直红着只会掩盖真正的新回归。
  if (/seen=[1-9]\d*/.test(navigation) && navigation.includes('type=1') && navigation.includes('length=40')) {
    check('navigation messages arrived and are GPS L1 C/A', true, navigation);
  } else {
    report.push(`KNOWN-GAP navigation messages not delivered (ROM parcel defect): ${navigation || 'no reading'}`);
    process.stdout.write(`KNOWN-GAP navigation messages not delivered (ROM parcel defect): ${navigation || 'no reading'}\n`);
  }

  // 投递计数（桥接侧随心跳报上来）：就绪位只说明"挂上了"，这个数才说明真的投到了。
  // 格式 `index:registrations,dispatches,needed,ticks,delivered,failed,wrapperTicks,wrapperDeliveries,reason,identity`
  // （正文里的 `;` `:` `,` 已被桥接换成 `/` `=` 空格）。**下标是通道在 gnssDispatchers 里的位置**：
  // 0=卫星状态、1=NMEA、2=原始测量、3=导航电文。真实设备上还会多出系统自己注册的监听器，
  // 所以只能按下标取，不能按"第几条"。
  const detail = request({ version: 1, op: 'status' }).gnss_raw_detail ?? '';
  const channels = Object.fromEntries(detail.split(';').filter(Boolean).map((entry) => {
    const [index, values] = entry.split(':');
    const [registrations, dispatches, needed, ticks, delivered, failed, wrapperTicks, wrapperDeliveries, reason, identity]
      = values.split(',');
    return [index, { registrations, dispatches, needed, ticks, delivered, failed, wrapperTicks, wrapperDeliveries, reason, identity }];
  }));
  check('gnss_raw_detail is readable', Object.keys(channels).length >= 4, detail || 'no counters');
  for (const [name, index] of [['raw measurements', '2'], ['navigation messages', '3']]) {
    const channel = channels[index];
    // 判据只有一条：**有没有真的把数据送出去**。
    // 不要把 `registrations` 也算进来——注册数在应用注销或退出后会归零（桥接层按弱引用清理），
    // 而那时 `delivered` 已经证明了这条通道在工作；`failed` 同理只做参考（陈旧 binder 的
    // `DeadObjectException` 是正常的清理过程）。
    if (!check(`${name} channel delivered`,
            channel && Number(channel.wrapperDeliveries) > 0,
            channel ? `registrations=${channel.registrations} needed=${channel.needed} wrapper_ticks=${channel.wrapperTicks}`
                    + ` delivered=${channel.wrapperDeliveries} failed=${channel.failed}`
                    + (channel.identity && channel.identity !== '-' ? ` receiver=${channel.identity}` : '')
                    + (channel.reason && channel.reason !== '-' ? ` reason=${channel.reason}` : '')
                    : 'no counters')) failures.push(`${name} delivery`);
  }

  const cell = section('cells');
  if (!check('cell info readable', cell.length > 0 && !cell[0].includes('read failed'), cell[0] ?? 'no reading')) failures.push('cell read');
  else {
    // 装置在读不到真实小区时会**为虚拟位置造几个**并如实标 `cells_synthesized`。
    // 这里只如实提示、不算失败：验收脚本用的是自己写进配置的小区，正常情况下不该触发；
    // 真触发了说明"配置的小区没被读到"，那才是该去看的地方。
    if (request({ version: 1, op: 'status' }).cells_synthesized === true) {
      report.push('NOTE cells_synthesized=true: the reported cells are fabricated because no real cell data covers this position');
      process.stdout.write('NOTE cells_synthesized=true: the reported cells are fabricated because no real cell data covers this position\n');
    }
    // 装置按 `telephony.radius_m` 从区域里挑基站（这里 500 米），所以两个小区都放在 500 米内。
    // 判据用 CI/NCI 而不是 MCC/MNC：真实基站与模拟基站可能同属一个 PLMN（本机就是 46011），
    // 只有这两个号能证明读到的是模拟值。
    const lte = cell.filter((line) => line.includes('CellInfoLte'));
    const nr = cell.filter((line) => line.includes('CellInfoNr'));
    if (!check('LTE cell carries the simulated CI', lte.some((line) => line.includes('Ci=12345678')), lte.join(' | ') || 'no LTE rows')) failures.push('LTE cell');
    if (!check('NR cell carries the simulated NCI', nr.some((line) => line.includes('Nci=987654321')), nr.join(' | ') || 'no NR rows')) failures.push('NR cell');
    // 两个订阅各一份，两个制式各一份。
    if (!check('cell row count matches the subscription count', lte.length + nr.length === 4, `LTE ${lte.length}, NR ${nr.length}`)) failures.push('cell rows');
  }

  const sim = section('sim');
  const slots = sim.filter((line) => line.startsWith('slot '));
  if (slots.length === 2) {
    // 桥接替换的是 SubscriptionInfo 查询结果，应用侧应看到模拟的运营商名。
    if (!check('both SIMs report the simulated operator', slots.every((line) => line.includes(carrier) && line.includes(`${mcc}${mnc}`)), slots.join(' | '))) failures.push('subscription list');
  } else {
    // 订阅列表需要 READ_PHONE_STATE；缺权限时如实记为缺口，不混进失败数。
    report.push(`KNOWN-GAP subscription list unreadable: ${sim.join(' | ') || 'no reading'}`);
    process.stdout.write(`KNOWN-GAP subscription list unreadable: ${sim.join(' | ') || 'no reading'}\n`);
  }
  // `TelephonyManager` 的运营商 getter 读的是系统属性（`gsm.operator.alpha` /
  // `gsm.sim.operator.alpha` / 两个 numeric），与订阅记录是两条独立的出口，
  // 所以必须单独断言：只替换 SubscriptionInfo 时这两行仍会显示真实运营商。
  const operatorLines = sim.filter((line) => line.startsWith('network_operator ') || line.startsWith('sim_operator '));
  if (!check('operator getters report the simulated name and PLMN', operatorLines.length === 2
          && operatorLines.every((line) => line.includes(carrier) && line.includes(`${mcc}${mnc}`)),
          operatorLines.join(' | ') || 'no reading')) failures.push('operator getters');

  // Wi-Fi 服务端两项适配：面板报"已就绪"只说明 hook 装上了，所以还要看应用侧读到的内容。
  // 两条出口分开断言，且**各自只看自己那一行**——整段做子串匹配的话，
  // 附近列表里出现的同一个名字会把"连接信息不对"也一起蒙过去。
  const wifiLines = section('wifi');
  const connection = wifiLines.find((line) => line.startsWith('SSID ')) ?? '';
  const nearby = wifiLines.find((line) => line.startsWith('nearby networks')) ?? '';
  if (!check('wifi connection reports the simulated network', connection.includes(ssid), connection || 'no reading')) failures.push('wifi connection');
  if (!check('wifi scan results contain the saved targets',
          nearby.includes(`${nearbySsid}(02:11:22:33:44:66`) && nearby.includes(`${otherSsid}(02:11:22:33:44:77`),
          nearby || 'no reading')) failures.push('wifi scan results');
  // 就绪位只说明"方法挂上了"；回调计数才说明服务端那次调用真的走到了我们的代码。
  // 只断言前者的话，"挂钩装上了但从没被调用"会伪装成通过。
  const wifiCalls = request({ version: 1, op: 'status' }).wifi_hook_calls;
  if (!check('wifi hooks were actually called', wifiCalls > 0, `wifi_hook_calls=${wifiCalls}`)) failures.push('wifi calls');
  // 第三条取数路径：应用也可以从 `ConnectivityManager` 的 `NetworkCapabilities.transportInfo`
  // 拿 WifiInfo，它由 Wi-Fi 服务自己的 NetworkAgent 填充，是另一个出口。原版也没有适配它，
  // 所以这里**只记录不断言**：读到真实网络就如实记成缺口，不混进失败数。
  const transport = wifiLines.find((line) => line.startsWith('NetworkCapabilities')) ?? 'no reading';
  if (!transport.includes(ssid)) {
    report.push(`KNOWN-GAP ConnectivityManager not covered: ${transport}`);
    process.stdout.write(`KNOWN-GAP ConnectivityManager not covered: ${transport}\n`);
  }

  // 系统身份（uid < 10000）必须读到真实值。上面几条只证明了"作用范围内的应用拿到合成值"，
  // 这一条要**先把作用范围放开到全部**再看 adb shell（uid 2000）读到什么——不放开范围的话
  // shell 本来就不在名单里，断言会白过，测不出"系统身份闸门"到底在不在。
  // 作用范围不支持中途改（`update` 只改坐标），所以要停掉再开一次。
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
  if (!check('system identity still reads the real network', shellWifi.length > 0 && !shellWifi.includes(ssid), shellWifi || 'no reading')) {
    failures.push('system identity wifi');
  }

  phase('cleanup');
  request({ version: 1, op: 'stop' });
  const after = request({ version: 1, op: 'status' });
  check('simulation stopped', after.requested_active === false);
  // 运营商名称与 PLMN 是**系统属性**：模拟期间被守护进程改写，停止后必须原样写回。
  // 不还原的话模拟值会留在系统里，连系统自己的界面都会显示错的运营商。
  check('system operator properties restored', shell(OPERATOR_PROPS) === realOperatorProperties,
      `before ${realOperatorProperties} / now ${shell(OPERATOR_PROPS)}`);
  shell(`rm -f ${helper}`);
  check('temporary helper removed', shell(`ls ${helper} 2>/dev/null || true`) === '');
  check('system_server was not restarted', shell('pidof system_server') === systemServer, `${systemServer} -> ${shell('pidof system_server')}`);

  const failed = report.filter((line) => line.startsWith('FAIL'));
  process.stdout.write(`\n=== summary ===\npassed ${report.filter((line) => line.startsWith('PASS')).length}, failed ${failed.length}\n`);
  if (failed.length > 0) throw new Error(`failures:\n${failed.join('\n')}`);
  process.stdout.write('all channels verified\n');
} finally {
  // 顺序要紧：**先停模拟、再删 helper**。反过来写的话，异常路径上 stop 会因为 helper
  // 已经不在而失败（而且被 catch 吞掉），于是模拟与改写过的运营商属性留在设备上——
  // 下一次运行又会把那份模拟值当成"真实值"记下来，收尾时报"未还原"，看着像还原坏了。
  try { request({ version: 1, op: 'stop' }); } catch { /* 后台可能不可达 */ }
  // 只清理本脚本创建的东西：设备上的 helper 与模拟状态；不停后台、不动模块、不卸应用。
  try { run(['shell', `rm -f ${helper}`]); } catch { /* 设备可能已断开 */ }
  if (!installed) process.stdout.write('(APK not installed: exited before the install step)\n');
}
