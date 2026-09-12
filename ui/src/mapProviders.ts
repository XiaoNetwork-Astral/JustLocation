import type { CoordinateSystem } from './coordinates';

export type MapProviderId = 'osm' | 'openfreemap' | 'amap' | 'tencent' | 'baidu' | 'custom';

export type MapProvider = {
  id: MapProviderId;
  label: string;
  /* Coordinate system used by the tile provider. */
  crs: 'wgs84' | 'gcj02' | 'bd09';
  /* Coordinate system label; stored positions remain WGS84. */
  coordinateSystem: CoordinateSystem;
  url: string;
  subdomains: string[];
  maxZoom: number;
  attribution: string;
  /* Required user-provided key, or null for direct access. */
  keyLabel?: string;
  /* Whether to flag the public endpoint as unverified. */
  unverified?: boolean;
};

export const mapProviders: MapProvider[] = [
  {
    id: 'osm',
    label: 'OpenStreetMap',
    crs: 'wgs84',
    coordinateSystem: 'wgs84',
    url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
    subdomains: [],
    maxZoom: 19,
    attribution:
      '&copy; <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">OpenStreetMap</a> contributors',
  },
  {
    id: 'openfreemap',
    label: 'OpenFreeMap',
    crs: 'wgs84',
    coordinateSystem: 'wgs84',
    url: 'https://tile.openfreemap.org/planet/20240729_001001_pt/{z}/{x}/{y}.png',
    subdomains: [],
    maxZoom: 19,
    attribution:
      '&copy; <a href="https://openfreemap.org/" target="_blank" rel="noopener noreferrer">OpenFreeMap</a> · &copy; OpenStreetMap contributors',
    unverified: true,
  },
  {
    id: 'amap',
    label: '高德地图',
    crs: 'gcj02',
    coordinateSystem: 'gcj02',
    url: 'https://webrd0{s}.is.autonavi.com/appmaptile?lang=zh_cn&size=1&scale=1&style=8&x={x}&y={y}&z={z}',
    subdomains: ['1', '2', '3', '4'],
    maxZoom: 18,
    attribution:
      '&copy; <a href="https://lbs.amap.com/" target="_blank" rel="noopener noreferrer">高德地图</a>',
    unverified: true,
  },
  {
    id: 'tencent',
    label: '腾讯地图',
    crs: 'gcj02',
    coordinateSystem: 'gcj02',
    url: 'https://rt{s}.map.gtimg.com/tile?z={z}&x={x}&y={-y}&type=vector&styleid=3',
    subdomains: ['0', '1', '2', '3'],
    maxZoom: 18,
    attribution:
      '&copy; <a href="https://lbs.qq.com/" target="_blank" rel="noopener noreferrer">腾讯地图</a>',
    unverified: true,
  },
  {
    id: 'baidu',
    label: '百度地图',
    crs: 'bd09',
    coordinateSystem: 'bd09',
    url: 'https://maponline{s}.bdimg.com/tile/?qt=vtile&x={x}&y={y}&z={z}&styles=pl&scaler=1&udt=20240101',
    subdomains: ['0', '1', '2', '3'],
    maxZoom: 18,
    attribution:
      '&copy; <a href="https://map.baidu.com/" target="_blank" rel="noopener noreferrer">百度地图</a>',
    keyLabel: 'AK',
    unverified: true,
  },
  {
    id: 'custom',
    label: '自定义图源',
    crs: 'wgs84',
    coordinateSystem: 'wgs84',
    url: '',
    subdomains: [],
    maxZoom: 19,
    attribution: '自定义图源',
    keyLabel: '地址',
  },
];

export const defaultMapProvider: MapProviderId = 'osm';
export const mapProviderKey = 'justlocation.map.provider';
export const customTileKey = 'justlocation.map.custom';

const byId = new Map(mapProviders.map((provider) => [provider.id, provider]));

export function isMapProviderId(value: unknown): value is MapProviderId {
  return typeof value === 'string' && byId.has(value as MapProviderId);
}

export function findMapProvider(id: MapProviderId): MapProvider {
  return byId.get(id) ?? byId.get(defaultMapProvider)!;
}

export type MapPreferences = { provider: MapProviderId; customUrl: string };

export function readMapPreferences(
  storage: Pick<Storage, 'getItem'> = localStorage,
): MapPreferences {
  let provider: MapProviderId = defaultMapProvider;
  let customUrl = '';
  try {
    const saved = storage.getItem(mapProviderKey);
    if (isMapProviderId(saved)) provider = saved;
    customUrl = storage.getItem(customTileKey) || '';
  } catch {
    /* Keep the current session usable when browser storage is unavailable. */
  }
  return { provider, customUrl };
}

export function saveMapPreferences(
  preferences: MapPreferences,
  storage: Pick<Storage, 'setItem'> = localStorage,
) {
  try {
    storage.setItem(mapProviderKey, preferences.provider);
    storage.setItem(customTileKey, preferences.customUrl);
  } catch {
    /* Keep the current session usable when browser storage is unavailable. */
  }
}

/* Custom providers require a tile URL. */
export function resolveTileUrl(provider: MapProvider, customUrl: string): string {
  if (provider.id === 'custom') return customUrl.trim();
  return provider.url;
}

export function hasUsableSource(provider: MapProvider, customUrl: string): boolean {
  return /\{z\}/.test(resolveTileUrl(provider, customUrl));
}
