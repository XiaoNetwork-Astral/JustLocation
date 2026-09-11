export interface Position { latitude: number; longitude: number; altitude: number; accuracy: number; speed: number; bearing: number }
export type Scope = { mode: 'all' } | { mode: 'apps'; packages: string[] };
export interface Config { position: Position; scope: Scope }
export interface RoutePlan { points: Position[]; speed: number; repeat_count?: number; repeat_delay?: number }
export interface RouteState { plan: RoutePlan; distance: number; total_distance: number; paused: boolean; completed: boolean; lap?: number; waiting_seconds?: number }
export interface DetectedSubscription { id: number; slot: number; mcc: string; mnc: string; country: string; carrier: string }
export interface Subscription extends DetectedSubscription { enabled: boolean; cdma_sid?: number }
export interface TelephonyConfig { cells_enabled: boolean; sim_enabled: boolean; radius_m: number; subscriptions: Subscription[] }
/** 卫星通道开关。规格第 7.4 节确认原版投递的是预置卫星状态数组，NMEA 只丢弃不合成。 */
export interface GnssConfig { gnss_enabled: boolean; nmea_enabled: boolean }
export interface State {
  requested_active: boolean; config: Config | null; hook_connected?: boolean; location_hook_ready?: boolean; route?: RouteState | null;
  phone_connected?: boolean; detected_subscriptions?: DetectedSubscription[] | null;
  telephony?: TelephonyConfig; cell_hook_ready?: boolean; sim_hook_ready?: boolean;
  gnss?: GnssConfig;
  /** 后端已经上报、前端此前没有声明的通道就绪位，用于在设置页展示。 */
  gnss_hook_ready?: boolean; nmea_hook_ready?: boolean;
  cell_query_hook_ready?: boolean; cell_callback_hook_ready?: boolean;
  telephony_output?: { availability: 'disabled' | 'ready' | 'missing_region' | 'outside_region'; groups: { cells: unknown[] }[] } | null;
}
export type Command = { op: 'status' | 'stop' | 'pause_route' | 'resume_route' } | { op: 'start'; config: Config } | { op: 'update'; position: Position }
  | { op: 'start_route'; route: RoutePlan; scope: Scope } | { op: 'set_telephony'; config: TelephonyConfig }
  | { op: 'set_gnss'; config: GnssConfig }
  | { op: 'set_cell_region'; region: import('./cells').CellRegion | null };
export type Client = (command: Command) => Promise<State>;
type Exec = (command: string) => Promise<{ errno: number; stdout: string; stderr: string }>;

export function createClient(exec: Exec): Client {
  return async command => {
    const bytes = new TextEncoder().encode(JSON.stringify({ version: 1, ...command }));
    const encoded = btoa(Array.from(bytes, byte => String.fromCharCode(byte)).join(''));
    const result = await exec(`/data/adb/modules/justlocation/bin/justlocationd request ${encoded}`);
    if (result.errno !== 0) throw new Error(result.stderr.trim() || '后台未响应，请检查模块是否启用');
    const response = JSON.parse(result.stdout);
    if (response.version !== 1 || typeof response.ok !== 'boolean' ||
      typeof response.state?.requested_active !== 'boolean' || !('config' in response.state)) {
      throw new Error('后台响应格式不兼容，请更新模块和面板');
    }
    if (!response.ok) throw new Error(response.error || '操作失败');
    return response.state;
  };
}

export function parsePosition(latitude: string, longitude: string, altitude: string): Position {
  if ([latitude, longitude, altitude].some(value => value.trim() === '' || !Number.isFinite(Number(value)))) {
    throw new Error('请填写有效的坐标和海拔');
  }
  const lat = Number(latitude), lon = Number(longitude);
  if (Math.abs(lat) > 90 || Math.abs(lon) > 180) throw new Error('纬度须在 −90～90，经度须在 −180～180');
  return { latitude: lat, longitude: lon, altitude: Number(altitude), accuracy: 5, speed: 0, bearing: 0 };
}
