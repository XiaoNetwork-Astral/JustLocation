// Wi-Fi 模拟的保存对象（面板侧）。
//
// 依据重实现规格第 7.1 节：用户从附近列表选择或手工填写 SSID/BSSID，
// 保存后在历史列表里切换，并支持编辑、删除和撤销删除。
// 缺省值沿用原版模型（rssi 200、linkspeed 866、frequency 5745）；
// 200 不是常规 dBm 测量值，只是原版取值，界面不做"信号好坏"的解读。
//
// 存储位置与位置、路线一致：面板本地的 localStorage。真正的输出（WifiInfo /
// ScanResult 替换）尚未实现，届时由系统侧读取这份数据。

export const WIFI_KEY = 'justlocation.wifis';
export const DEFAULT_RSSI = 200;
export const DEFAULT_LINK_SPEED = 866;
export const DEFAULT_FREQUENCY = 5745;

export type SavedWifi = {
  id: string;
  ssid: string;
  bssid: string;
  rssi: number;
  linkSpeed: number;
  frequency: number;
};

/**
 * 判断文本里是否出现一个接入点地址。原版用 MAC 正则的 `find()` 而不是完整匹配，
 * 所以"复制了一整句带地址的话"也能通过；这里保持同样的宽松程度。
 */
export function looksLikeBssid(text: string): boolean {
  return /([0-9a-fA-F]{2}[:-]){5}[0-9a-fA-F]{2}/.test(text);
}

export function validateWifi(value: { ssid: string; bssid: string }): void {
  if (!value.ssid.trim()) throw new Error('请填写网络名称');
  if (value.ssid.length > 64 || /[\u0000-\u001f\u007f]/.test(value.ssid)) throw new Error('网络名称里有无法保存的字符');
  if (value.bssid.length > 64 || /[\u0000-\u001f\u007f]/.test(value.bssid)) throw new Error('接入点地址里有无法保存的字符');
}

function record(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

/** 读取已保存的网络；格式不对的条目直接跳过，不让一条坏数据毁掉整个列表。 */
export function readWifis(storage: Pick<Storage, 'getItem'> = localStorage): SavedWifi[] {
  try {
    const items: unknown = JSON.parse(storage.getItem(WIFI_KEY) || '[]');
    if (!Array.isArray(items)) return [];
    return items.flatMap(item => {
      const entry = record(item);
      if (!entry) return [];
      const { id, ssid, bssid } = entry;
      if (typeof id !== 'string' || !id.trim() || typeof ssid !== 'string' || typeof bssid !== 'string') return [];
      const number = (value: unknown, fallback: number) => (typeof value === 'number' && Number.isFinite(value) ? value : fallback);
      return [{
        id, ssid, bssid,
        rssi: number(entry.rssi, DEFAULT_RSSI),
        linkSpeed: number(entry.linkSpeed, DEFAULT_LINK_SPEED),
        frequency: number(entry.frequency, DEFAULT_FREQUENCY),
      }];
    });
  } catch { return []; }
}

export function saveWifis(items: SavedWifi[], storage: Pick<Storage, 'setItem'> = localStorage): void {
  try { storage.setItem(WIFI_KEY, JSON.stringify(items)); }
  catch { throw new Error('保存失败，请检查面板的存储空间'); }
}

/** 手工添加或从附近列表选入时用这个构造，只要求名称与地址。 */
export function createWifi(ssid: string, bssid: string, id: string = crypto.randomUUID()): SavedWifi {
  validateWifi({ ssid, bssid });
  return { id, ssid: ssid.trim(), bssid: bssid.trim(), rssi: DEFAULT_RSSI, linkSpeed: DEFAULT_LINK_SPEED, frequency: DEFAULT_FREQUENCY };
}
