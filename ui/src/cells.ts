import { exec as ksuExec } from 'kernelsu';

export type ProviderKind = 'open_cell_id' | 'fake_location' | 'custom';
export interface Coordinate { latitude: number; longitude: number }
export interface Cell { identity: { radio: string; [key: string]: string | number }; position: Coordinate; range_m: number }
export interface CellRegion { center: Coordinate; radius_m: number; source: string; fetched_at_ms: number; cells: Cell[] }
export interface CellDataset {
  provider: ProviderKind; origin: string;
  region: CellRegion;
  attribution: { text: string; source: string; license: string | null; changes: string | null };
  incomplete: boolean; skipped: number; failures: { provider: ProviderKind; error: string }[];
}
export interface CellSettings {
  primary: ProviderKind; fallback: ProviderKind | null; opencellid_configured: boolean;
  custom_endpoint: string; custom_token_configured: boolean; fake_location_ready: boolean;
}
export interface CellSettingsUpdate {
  primary: ProviderKind; fallback: ProviderKind | null; custom_endpoint: string;
  opencellid_key?: string; custom_token?: string;
}
export type CellCommand = { op: 'settings' | 'clear_cache' } | { op: 'configure'; settings: CellSettingsUpdate }
  | { op: 'query'; area: { target: Coordinate; radius_m: number }; mode: 'prefer_cache' | 'refresh' | 'offline' }
  | { op: 'import'; dataset: CellDataset };
export interface CellResponse { settings?: CellSettings; dataset?: CellDataset; cached?: boolean; stale?: boolean; warning?: string }
export type CellClient = (command: CellCommand) => Promise<CellResponse>;
type Exec = (command: string) => Promise<{ errno: number; stdout: string; stderr: string }>;

export function createCellClient(exec: Exec): CellClient {
  return async command => {
    const bytes = new TextEncoder().encode(JSON.stringify({ version: 1, ...command }));
    if (bytes.length >= 65536) throw new Error('基站文件太大，请分成较小的区域');
    const encoded = btoa(Array.from(bytes, byte => String.fromCharCode(byte)).join(''));
    const result = await exec(`/data/adb/modules/justlocation/bin/justlocationd cells ${encoded}`);
    if (result.errno) throw new Error('基站查询服务未响应，请检查模块版本');
    const response = JSON.parse(result.stdout);
    if (response.version !== 1 || typeof response.ok !== 'boolean') throw new Error('基站服务版本不兼容');
    if (!response.ok) throw new Error(response.error || '基站操作失败');
    if ((command.op === 'settings' || command.op === 'configure') && !response.settings) throw new Error('供应商设置无法读取');
    if (command.op === 'query' && (!Array.isArray(response.dataset?.region?.cells) || !response.dataset.attribution)) throw new Error('基站数据格式不兼容');
    return response;
  };
}
export const cellClient = createCellClient(async command => {
  if (!('ksu' in window)) throw new Error('请从 KernelSU 管理器打开模块面板');
  return ksuExec(command);
});

export function safeCellLink(value: string | null) {
  try { const url = new URL(value || ''); return ['http:', 'https:'].includes(url.protocol) ? url.href : undefined; }
  catch { return undefined; }
}
