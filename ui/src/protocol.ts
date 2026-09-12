export interface Position {
  latitude: number;
  longitude: number;
  altitude: number;
  accuracy: number;
  speed: number;
  bearing: number;
}
export type Scope = { mode: 'all' } | { mode: 'apps'; packages: string[] };
export interface Config {
  position: Position;
  scope: Scope;
}
export interface RoutePlan {
  points: Position[];
  speed: number;
  repeat_count?: number;
  repeat_delay?: number;
}
export interface RouteState {
  plan: RoutePlan;
  distance: number;
  total_distance: number;
  paused: boolean;
  completed: boolean;
  lap?: number;
  waiting_seconds?: number;
}
export interface DetectedSubscription {
  id: number;
  slot: number;
  mcc: string;
  mnc: string;
  country: string;
  carrier: string;
}
export interface Subscription extends DetectedSubscription {
  enabled: boolean;
  cdma_sid?: number;
}
export interface TelephonyConfig {
  cells_enabled: boolean;
  sim_enabled: boolean;
  radius_m: number;
  subscriptions: Subscription[];
}

export interface GnssConfig {
  gnss_enabled: boolean;
  nmea_enabled: boolean;
}
/* Signal fields use model defaults, not measured values. */
export interface WifiTarget {
  id: string;
  ssid: string;
  bssid: string;
  rssi: number;
  link_speed: number;
  frequency: number;
}
export interface WifiConfig {
  enabled: boolean;
  targets: WifiTarget[];
}
export interface State {
  requested_active: boolean;
  config: Config | null;
  hook_connected?: boolean;
  location_hook_ready?: boolean;
  route?: RouteState | null;
  phone_connected?: boolean;
  detected_subscriptions?: DetectedSubscription[] | null;
  telephony?: TelephonyConfig;
  cell_hook_ready?: boolean;
  sim_hook_ready?: boolean;
  gnss?: GnssConfig;
  wifi?: WifiConfig;

  gnss_hook_ready?: boolean;
  nmea_hook_ready?: boolean;
  cell_query_hook_ready?: boolean;
  cell_callback_hook_ready?: boolean;

  operator_hook_ready?: boolean;
  /* Scan and connection hooks report readiness separately. */
  wifi_scan_hook_ready?: boolean;
  wifi_connection_hook_ready?: boolean;
  telephony_output?: {
    availability: 'disabled' | 'ready' | 'missing_region' | 'outside_region';
    groups: { cells: unknown[] }[];
  } | null;
}
export type Command =
  | { op: 'status' | 'stop' | 'pause_route' | 'resume_route' }
  | { op: 'start'; config: Config }
  | { op: 'update'; position: Position }
  | { op: 'start_route'; route: RoutePlan; scope: Scope }
  | { op: 'set_telephony'; config: TelephonyConfig }
  | { op: 'set_gnss'; config: GnssConfig }
  | { op: 'set_wifi'; config: WifiConfig }
  | { op: 'set_cell_region'; region: CellRegion | null };

export type ProviderKind = 'open_cell_id' | 'custom';
export interface Coordinate {
  latitude: number;
  longitude: number;
}
export interface Cell {
  identity: { radio: string; [key: string]: string | number };
  position: Coordinate;
  range_m: number;
}
export interface CellRegion {
  center: Coordinate;
  radius_m: number;
  source: string;
  fetched_at_ms: number;
  cells: Cell[];
}
export interface CellDataset {
  provider: ProviderKind;
  origin: string;
  region: CellRegion;
  attribution: { text: string; source: string; license: string | null; changes: string | null };
  incomplete: boolean;
  skipped: number;
  failures: { provider: ProviderKind; error: string }[];
}
export interface CellSettings {
  primary: ProviderKind;
  fallback: ProviderKind | null;
  opencellid_configured: boolean;
  custom_endpoint: string;
  custom_token_configured: boolean;
}
export interface CellSettingsUpdate {
  primary: ProviderKind;
  fallback: ProviderKind | null;
  custom_endpoint: string;
  opencellid_key?: string;
  custom_token?: string;
}
export type CellCommand =
  | { op: 'settings' | 'clear_cache' }
  | { op: 'configure'; settings: CellSettingsUpdate }
  | {
      op: 'query';
      area: { target: Coordinate; radius_m: number };
      mode: 'prefer_cache' | 'refresh' | 'offline';
    }
  | { op: 'import'; dataset: CellDataset };
export interface CellResponse {
  settings?: CellSettings;
  dataset?: CellDataset;
  cached?: boolean;
  stale?: boolean;
  warning?: string;
}
