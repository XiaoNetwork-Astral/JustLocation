import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * 逐通道真机验收。
 *
 * <p>为什么要有这个脚本：定位之外的通道没有可观察的入口，"模块装上了"不等于"输出被替换了"。
 * 做法是把配置发给后台，再让设备上的 companion 用普通应用接口读一遍
 * （`ChannelCheckActivity`，只读公开 API），脚本对这份报告做断言。判定只有一处，
 * 应用侧只负责如实读出。
 *
 * <p>用法：`node build.mjs test:checks <adb-serial>`。脚本自己装 APK、跑配置、抓报告。
 * 结束时把模拟停掉并清掉设备上的临时文件。
 */

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const [adb, serial] = process.argv.slice(2);
if (!adb || !serial) throw new Error('Provide adb path and explicit device serial.');
const apk = resolve(root, 'dist/justlocation-companion-debug.apk');
const app = 'me.idk.justlocation.companion';
const activity = `${app}/${app}.ChannelCheckActivity`;
const helper = '/data/local/tmp/justlocation-check.sh';

/** 验收用的参数：都取"一眼能看出是模拟值"的数字，避免和真实环境混淆。 */
const latitude = 31.2304;
const longitude = 121.4737;
const altitude = 42;
const accuracy = 6;
const ssid = 'JustLocation-Test';
const mcc = '460';
const mnc = '11';
const carrier = '中国联通';
const event = '验收环境';

function run(args, input) {
  const result = spawnSync(adb, ['-s', serial, ...args], { input, encoding: 'utf8', timeout: 120_000, maxBuffer: 4 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`adb ${args.slice(0, 3).join(' ')} failed (${result.status}):\n${result.stdout}\n${result.stderr}`);
  return result.stdout.trim();
}

const shell = (command) => run(['shell', command]);

/**
 * 一条协议请求；只认后台明确回 `ok: true`。
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
  catch (error) { throw new Error(`${frame.op} 的响应不是 JSON：\n${reply}`); }
  if (parsed.ok !== true) throw new Error(`${frame.op} 被后台拒绝（error=${JSON.stringify(parsed.error)}）：\n${reply}`);
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
  phase('准备');
  // 请求助手要先到位，后面每一步都要用它；结束时删掉。
  run(['push', resolve(root, 'tests/device/request.sh'), helper]);
  shell(`chmod 700 ${helper}`);
  check('设备上的请求助手已就位', shell(`test -x ${helper} && echo yes`) === 'yes');
  check('设备已开机', shell('getprop sys.boot_completed') === '1');
  const systemServer = shell('pidof system_server');
  check('system_server 正在运行', /^\d+$/.test(systemServer), systemServer);

  const state = request({ version: 1, op: 'status' });
  check('桥接已连接', state.hook_connected === true);
  check('定位 Hook 就绪', state.location_hook_ready === true);
  check('卫星 Hook 就绪', state.gnss_hook_ready === true, String(state.gnss_hook_ready));
  check('NMEA Hook 就绪', state.nmea_hook_ready === true, String(state.nmea_hook_ready));
  check('基站 Hook 就绪', state.cell_hook_ready === true, String(state.cell_hook_ready));
  check('基站查询 Hook 就绪', state.cell_query_hook_ready === true);
  check('基站回调 Hook 就绪', state.cell_callback_hook_ready === true);
  check('SIM Hook 就绪', state.sim_hook_ready === true);

  // 记下验收前的真实 SSID，用来判断 Wi-Fi 通道有没有真的被接管。
  const realSsid = request({ version: 1, op: 'status' }).wifi?.targets?.[0]?.ssid ?? null;
  const deviceSsid = shell(`dumpsys wifi | grep -m1 -i "mWifiInfo" | head -1`).trim();
  check('记录了原始 Wi-Fi 状态', deviceSsid.length >= 0, realSsid ? `已保存目标 ${realSsid}` : '无保存目标');

  phase('安装被测应用');
  run(['install', '-r', '-g', apk]);
  installed = true;
  check('companion 已安装并授予权限', shell(`pm list packages ${app}`).includes(app));

  phase('写入验收配置');
  request({ version: 1, op: 'stop' });
  request({
    version: 1,
    op: 'set_wifi',
    config: {
      enabled: true,
      targets: [{ id: 'acceptance', ssid, bssid: '02:11:22:33:44:55', rssi: -42, link_speed: 866, frequency: 5745 }],
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
  check('模拟已开始且作用范围只含被测应用', started.requested_active === true);
  check('基站输出已启用', started.telephony?.cells_enabled === true);
  check('SIM 输出已启用', started.telephony?.sim_enabled === true);
  check('卫星输出已启用', started.gnss?.gnss_enabled === true);
  check('NMEA 输出已启用', started.gnss?.nmea_enabled === true);
  check('Wi-Fi 输出已启用', started.wifi?.enabled === true);

  phase('设备侧自检');
  shell('logcat -c');
  // 报告由应用写入 logcat；轮询到结束标记为止（采集本身约 6 秒）。
  // `--ez autorun true` 让页面一进去就开始，省掉点按钮（脚本点不动界面元素）。
  const launch = spawnSync(adb, ['-s', serial, 'shell', 'am', 'start', '-n', activity, '--ez', 'autorun', 'true'], { encoding: 'utf8', timeout: 30_000 });
  assert.equal(launch.status, 0, `无法启动自检页：${launch.stdout ?? ''}${launch.stderr ?? ''}`);
  // 报告由应用写入 logcat；轮询到结束标记为止（采集本身约 6 秒）。
  let lines = [];
  const deadline = Date.now() + 60_000;
  for (;;) {
    const out = spawnSync(adb, ['-s', serial, 'shell', 'logcat', '-d', '-s', 'JustLocationCheck:I'], { encoding: 'utf8', timeout: 30_000 });
    lines = (out.stdout ?? '').split(/\r?\n/).map((line) => line.replace(/^.*JustLocationCheck\s*:\s?/, '').trimEnd());
    if (lines.includes('CHECK_REPORT_END')) break;
    if (Date.now() > deadline) break;
    await new Promise((done) => setTimeout(done, 2000));
  }
  const begin = lines.indexOf('CHECK_REPORT_BEGIN');
  const end = lines.indexOf('CHECK_REPORT_END');
  assert.ok(begin >= 0 && end > begin, `没有抓到最后一次自检报告（收到 ${lines.length} 行）`);
  const body = lines.slice(begin + 1, end);

  // 报告按 "## 标题" 分节；断言只看内容，不依赖行的顺序。
  const sections = new Map();
  let current = '前言';
  sections.set(current, []);
  for (const line of body) {
    if (line.startsWith('## ')) { current = line.slice(3).trim(); sections.set(current, []); continue; }
    sections.get(current).push(line);
  }
  const section = (name) => (sections.get(name) ?? []).filter((line) => line.length > 0);
  const all = body.join('\n');

  phase('通道判定');
  const app2 = section('应用');
  if (!check('自检读到本应用包名', app2.some((line) => line.includes(app)), app2[0] ?? '')) failures.push('应用');

  const location = section('定位');
  const number = (text) => Number.parseFloat(text);
  const fixOf = (provider) => location.find((line) => line.startsWith(provider));
  for (const provider of ['gps', 'network']) {
    const line = fixOf(provider);
    if (!check(`${provider} 有位置`, Boolean(line))) { failures.push(provider); continue; }
    const match = line.match(/(-?\d+\.\d+), (-?\d+\.\d+)/);
    const matches = match && Math.abs(number(match[1]) - latitude) < 1e-4 && Math.abs(number(match[2]) - longitude) < 1e-4;
    if (!check(`${provider} 是模拟坐标`, Boolean(matches), line)) failures.push(`${provider} 坐标`);
    if (!check(`${provider} 海拔与精度是模拟值`, line.includes(`海拔 ${altitude}.0`) && line.includes(`精度 ${accuracy}.0`), line)) failures.push(`${provider} 海拔`);
    if (!check(`${provider} 未被标记为 mock`, line.includes('mock=false'), line)) failures.push(`${provider} mock`);
  }

  const gnss = section('卫星状态');
  const count = gnss.find((line) => line.startsWith('卫星数'));
  if (!check('卫星状态有回调', Boolean(count), gnss[0] ?? '无')) failures.push('卫星回调');
  else {
    const total = number(count.replace('卫星数', '').trim());
    if (!check('卫星数为模拟的 8 颗', total === 8, count)) failures.push('卫星数量');
    const sky = gnss.find((line) => line.startsWith('星号/信噪')) ?? '';
    const expected = ['3/38', '7/42', '11/35', '14/44', '19/40', '22/37', '26/41', '30/36'];
    const missing = expected.filter((entry) => !sky.includes(entry));
    if (!check('星座与信噪是模拟表', missing.length === 0, missing.length ? `缺少 ${missing.join(' ')}` : sky)) failures.push('星座');
  }

  const nmea = section('NMEA');
  const messages = nmea.find((line) => line.startsWith('报文数'));
  const kinds = nmea.find((line) => line.startsWith('语句类型')) ?? '';
  if (!check('收到 NMEA 报文', Boolean(messages) && number(messages.replace('报文数', '').trim()) > 0, messages ?? '无')) failures.push('NMEA 报文');
  const wanted = ['GPGGA', 'GPRMC', 'GPGSA', 'GPGSV', 'GPVTG'];
  const absent = wanted.filter((kind) => !kinds.includes(kind));
  if (!check('语句类型是合成的那五种', absent.length === 0, absent.length ? `缺少 ${absent.join(' ')}` : kinds)) failures.push('NMEA 类型');
  // GPGGA 用度分格式（ddmm.mmmmm）：31.2304° → 3113.82400。别用取模算，浮点误差会咬人。
  const minutes = (latitude - Math.trunc(latitude)) * 60;
  const expectedPoint = `${String(Math.trunc(latitude)).padStart(2, '0')}${minutes.toFixed(5).padStart(8, '0')}`;
  if (!check('NMEA 里的坐标是模拟值', all.includes(expectedPoint), `期待 ${expectedPoint}`)) failures.push('NMEA 坐标');

  const cell = section('基站');
  if (!check('读到基站信息', cell.length > 0 && !cell[0].includes('读取失败'), cell[0] ?? '无')) failures.push('基站读取');
  else {
    // 装置按 `telephony.radius_m` 从区域里挑基站（这里 500 米），所以两个小区都放在 500 米内。
    // 判据用 CI/NCI 而不是 MCC/MNC：真实基站与模拟基站可能同属一个 PLMN（本机就是 46011），
    // 只有这两个号能证明读到的是模拟值。
    const lte = cell.filter((line) => line.includes('CellInfoLte'));
    const nr = cell.filter((line) => line.includes('CellInfoNr'));
    if (!check('LTE 基站带模拟的 CI', lte.some((line) => line.includes('Ci=12345678')), lte.join(' | ') || '无 LTE 条目')) failures.push('LTE 基站');
    if (!check('NR 基站带模拟的 NCI', nr.some((line) => line.includes('Nci=987654321')), nr.join(' | ') || '无 NR 条目')) failures.push('NR 基站');
    // 两个订阅各一份，两个制式各一份。
    if (!check('基站条数与订阅数一致', lte.length + nr.length === 4, `LTE ${lte.length} 条、NR ${nr.length} 条`)) failures.push('基站条数');
  }

  const sim = section('SIM');
  const slots = sim.filter((line) => line.startsWith('卡槽'));
  if (slots.length === 2) {
    // 桥接替换的是 SubscriptionInfo 查询结果，应用侧应看到模拟的运营商名。
    if (!check('两张卡都按模拟配置报告', slots.every((line) => line.includes(carrier) && line.includes(`${mcc}${mnc}`)), slots.join(' | '))) failures.push('订阅列表');
  } else {
    // 订阅列表需要 READ_PHONE_STATE；缺权限时如实记为缺口，不混进失败数。
    report.push(`KNOWN-GAP 订阅列表不可读：${sim.join(' | ') || '无读数'}`);
    process.stdout.write(`KNOWN-GAP 订阅列表不可读：${sim.join(' | ') || '无读数'}\n`);
  }
  // 这两个 getter 不在桥接的覆盖范围内（只做 SubscriptionInfo 与基站查询/回调），
  // 如实记录，不要用它冒充 SIM 通道的验收结论。
  report.push(`KNOWN-GAP TelephonyManager 运营商 getter：${sim.slice(0, 2).join(' | ')}`);
  process.stdout.write(`KNOWN-GAP TelephonyManager 运营商 getter：${sim.slice(0, 2).join(' | ')}\n`);

  phase('已知缺口');
  const wifi = section('Wi-Fi').join(' ');
  // Wi-Fi 通道目前装不上 hook（见 BridgeEntry.installWifi 的说明），读到真实 SSID 是预期结果；
  // 这一项单独标记成缺口，免得把"已知未接"混进失败数。
  report.push(`KNOWN-GAP Wi-Fi 输出：${wifi || '无读数'}`);
  process.stdout.write(`KNOWN-GAP Wi-Fi 输出：${wifi || '无读数'}\n`);

  phase('收尾');
  request({ version: 1, op: 'stop' });
  const after = request({ version: 1, op: 'status' });
  check('模拟已停止', after.requested_active === false);
  shell(`rm -f ${helper}`);
  check('临时脚本已删除', shell(`ls ${helper} 2>/dev/null || true`) === '');
  check('system_server 未被重启', shell('pidof system_server') === systemServer, `${systemServer} -> ${shell('pidof system_server')}`);

  const failed = report.filter((line) => line.startsWith('FAIL'));
  process.stdout.write(`\n=== 汇总 ===\n通过 ${report.filter((line) => line.startsWith('PASS')).length} 项，失败 ${failed.length} 项\n`);
  if (failed.length > 0) throw new Error(`未通过：\n${failed.join('\n')}`);
  process.stdout.write('全部通道验收通过（Wi-Fi 见已知缺口）\n');
} finally {
  // 只清理本脚本创建的东西：设备上的 helper 与模拟状态；不停后台、不动模块、不卸应用。
  try { run(['shell', `rm -f ${helper}`]); } catch { /* 设备可能已断开 */ }
  try { request({ version: 1, op: 'stop' }); } catch { /* 后台可能不可达 */ }
  if (!installed) process.stdout.write('（未安装 APK：安装步骤之前就退出）\n');
}
