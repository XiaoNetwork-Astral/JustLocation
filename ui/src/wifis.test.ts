import { expect, it } from 'vitest';
import { createWifi, DEFAULT_FREQUENCY, DEFAULT_LINK_SPEED, DEFAULT_RSSI, looksLikeBssid, readWifis, saveWifis, validateWifi } from './wifis';

function storage(initial: string | null = null) {
  let value = initial;
  return {
    getItem: () => value,
    setItem: (_key: string, next: string) => { value = next; },
    current: () => value,
  };
}

it('keeps the signal defaults of the original model', () => {
  // 200 不是常规 dBm 读数，只是原版模型的取值；界面不要据此说信号强弱。
  expect(DEFAULT_RSSI).toBe(200);
  expect(DEFAULT_LINK_SPEED).toBe(866);
  expect(DEFAULT_FREQUENCY).toBe(5745);
  const wifi = createWifi('Home', 'aa:bb:cc:dd:ee:ff', 'w1');
  expect(wifi).toEqual({ id: 'w1', ssid: 'Home', bssid: 'aa:bb:cc:dd:ee:ff', rssi: 200, linkSpeed: 866, frequency: 5745 });
});

it('requires a name and refuses characters that cannot be stored', () => {
  expect(() => createWifi('   ', 'aa:bb:cc:dd:ee:ff', 'w1')).toThrow(/网络名称/);
  expect(() => validateWifi({ ssid: 'Home\u0001', bssid: '' })).toThrow(/无法保存的字符/);
  // 地址可以为空（只按名称模拟），这时不应报错。
  expect(() => validateWifi({ ssid: 'Home', bssid: '' })).not.toThrow();
  expect(() => createWifi('x'.repeat(65), '', 'w1')).toThrow(/无法保存的字符/);
});

it('accepts an access point address inside a longer piece of text', () => {
  // 原版用 find() 而不是完整匹配，所以从别处整句复制也能用。
  expect(looksLikeBssid('aa:bb:cc:dd:ee:ff')).toBe(true);
  expect(looksLikeBssid('AA-BB-CC-DD-EE-FF')).toBe(true);
  expect(looksLikeBssid('网关 aa:bb:cc:dd:ee:ff 的地址')).toBe(true);
  expect(looksLikeBssid('aa:bb:cc:dd:ee')).toBe(false);
  expect(looksLikeBssid('zz:bb:cc:dd:ee:ff')).toBe(false);
});

it('skips broken entries instead of losing the whole list', () => {
  const broken = JSON.stringify([
    { id: 'ok', ssid: 'Home', bssid: 'aa:bb:cc:dd:ee:ff' },
    { id: '', ssid: 'NoId', bssid: '' },
    { id: 'x', ssid: 'MissingBssid' },
    { id: 'y', ssid: 'BadSignal', bssid: '', rssi: 'strong', linkSpeed: null },
    'not an object',
  ]);
  const items = readWifis(storage(broken));
  expect(items.map(item => item.id)).toEqual(['ok', 'y']);
  // 数值字段坏掉时回落到原版缺省值，而不是留下 NaN。
  expect(items[1].rssi).toBe(DEFAULT_RSSI);
  expect(readWifis(storage('{ not json'))).toEqual([]);
  expect(readWifis(storage(null))).toEqual([]);
});

it('round trips through storage and reports a full quota clearly', () => {
  const box = storage();
  saveWifis([createWifi('Home', 'aa:bb:cc:dd:ee:ff', 'w1')], box);
  expect(readWifis(box)).toHaveLength(1);
  expect(() => saveWifis([], { setItem: () => { throw new Error('full'); } })).toThrow(/存储空间/);
});
