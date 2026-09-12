// The legacy RSSI default is not a measured dBm value.

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

/* Allow a BSSID embedded in pasted text, matching the existing input behavior. */
export function looksLikeBssid(text: string): boolean {
  return /([0-9a-fA-F]{2}[:-]){5}[0-9a-fA-F]{2}/.test(text);
}

export function validateWifi(value: { ssid: string; bssid: string }): void {
  if (!value.ssid.trim()) throw new Error('请填写网络名称');
  if (value.ssid.length > 64 || /[\u0000-\u001f\u007f]/.test(value.ssid))
    throw new Error('网络名称里有无法保存的字符');
  if (value.bssid.length > 64 || /[\u0000-\u001f\u007f]/.test(value.bssid))
    throw new Error('接入点地址里有无法保存的字符');
}

function record(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

/* Skip malformed entries without discarding the rest of the saved list. */
export function readWifis(storage: Pick<Storage, 'getItem'> = localStorage): SavedWifi[] {
  try {
    const items: unknown = JSON.parse(storage.getItem(WIFI_KEY) || '[]');
    if (!Array.isArray(items)) return [];
    return items.flatMap((item) => {
      const entry = record(item);
      if (!entry) return [];
      const { id, ssid, bssid } = entry;
      if (
        typeof id !== 'string' ||
        !id.trim() ||
        typeof ssid !== 'string' ||
        typeof bssid !== 'string'
      )
        return [];
      const number = (value: unknown, fallback: number) =>
        typeof value === 'number' && Number.isFinite(value) ? value : fallback;
      return [
        {
          id,
          ssid,
          bssid,
          rssi: number(entry.rssi, DEFAULT_RSSI),
          linkSpeed: number(entry.linkSpeed, DEFAULT_LINK_SPEED),
          frequency: number(entry.frequency, DEFAULT_FREQUENCY),
        },
      ];
    });
  } catch {
    return [];
  }
}

export function saveWifis(
  items: SavedWifi[],
  storage: Pick<Storage, 'setItem'> = localStorage,
): void {
  try {
    storage.setItem(WIFI_KEY, JSON.stringify(items));
  } catch {
    throw new Error('保存失败，请检查面板的存储空间');
  }
}

export function createWifi(
  ssid: string,
  bssid: string,
  id: string = crypto.randomUUID(),
): SavedWifi {
  validateWifi({ ssid, bssid });
  return {
    id,
    ssid: ssid.trim(),
    bssid: bssid.trim(),
    rssi: DEFAULT_RSSI,
    linkSpeed: DEFAULT_LINK_SPEED,
    frequency: DEFAULT_FREQUENCY,
  };
}
