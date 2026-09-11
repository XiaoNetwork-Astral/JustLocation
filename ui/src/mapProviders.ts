// 地图图源注册表。
//
// 每家底图的坐标系不同，而项目内部一律以 WGS84 存储坐标：
//   - OSM / OpenFreeMap：WGS84，可直接用
//   - 高德 / 腾讯：GCJ-02（国测局偏移坐标系），需要转换
//   - 百度：BD-09，并且瓦片是米制墨卡托（每级 2^(18-z) 米），
//     必须换一套 CRS 才不偏，不能拿 Web 墨卡托直接套
// 因此每家都声明自己的 crs，地图组件据此建图，并在显示/取点时转换坐标。

import type { CoordinateSystem } from './coordinates';

export type MapProviderId = 'osm' | 'openfreemap' | 'amap' | 'tencent' | 'baidu' | 'custom';

export type MapProvider = {
  id: MapProviderId;
  label: string;
  /** 该底图瓦片实际使用的坐标系 */
  crs: 'wgs84' | 'gcj02' | 'bd09';
  /** 底图名称的坐标系表示；内部存储始终是 WGS84 */
  coordinateSystem: CoordinateSystem;
  url: string;
  subdomains: string[];
  maxZoom: number;
  attribution: string;
  /** 使用该图源需要用户自备的密钥名称；为空表示免费直连 */
  keyLabel?: string;
  /** 是否需要在界面上提示"公开接口可能变动" */
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
    attribution: '&copy; <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">OpenStreetMap</a> contributors',
  },
  {
    id: 'openfreemap',
    label: 'OpenFreeMap',
    crs: 'wgs84',
    coordinateSystem: 'wgs84',
    url: 'https://tile.openfreemap.org/planet/20240729_001001_pt/{z}/{x}/{y}.png',
    subdomains: [],
    maxZoom: 19,
    attribution: '&copy; <a href="https://openfreemap.org/" target="_blank" rel="noopener noreferrer">OpenFreeMap</a> · &copy; OpenStreetMap contributors',
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
    attribution: '&copy; <a href="https://lbs.amap.com/" target="_blank" rel="noopener noreferrer">高德地图</a>',
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
    attribution: '&copy; <a href="https://lbs.qq.com/" target="_blank" rel="noopener noreferrer">腾讯地图</a>',
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
    attribution: '&copy; <a href="https://map.baidu.com/" target="_blank" rel="noopener noreferrer">百度地图</a>',
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

const byId = new Map(mapProviders.map(provider => [provider.id, provider]));

export function isMapProviderId(value: unknown): value is MapProviderId {
  return typeof value === 'string' && byId.has(value as MapProviderId);
}

export function findMapProvider(id: MapProviderId): MapProvider {
  return byId.get(id) ?? byId.get(defaultMapProvider)!;
}

export type MapPreferences = { provider: MapProviderId; customUrl: string };

export function readMapPreferences(storage: Pick<Storage, 'getItem'> = localStorage): MapPreferences {
  let provider: MapProviderId = defaultMapProvider;
  let customUrl = '';
  try {
    const saved = storage.getItem(mapProviderKey);
    if (isMapProviderId(saved)) provider = saved;
    customUrl = storage.getItem(customTileKey) || '';
  } catch { /* 存储不可用时用默认值 */ }
  return { provider, customUrl };
}

export function saveMapPreferences(preferences: MapPreferences, storage: Pick<Storage, 'setItem'> = localStorage) {
  try {
    storage.setItem(mapProviderKey, preferences.provider);
    storage.setItem(customTileKey, preferences.customUrl);
  } catch { /* 忽略存储失败 */ }
}

/** 自定义图源必须有地址才能建图；其余图源开箱可用。 */
export function resolveTileUrl(provider: MapProvider, customUrl: string): string {
  if (provider.id === 'custom') return customUrl.trim();
  return provider.url;
}

export function hasUsableSource(provider: MapProvider, customUrl: string): boolean {
  return /\{z\}/.test(resolveTileUrl(provider, customUrl));
}
