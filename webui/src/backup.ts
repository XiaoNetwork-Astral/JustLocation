import type { Position, RoutePlan } from './control';
import { parseDraft, planDraft } from './routeDraft';

export type Place = { id: string; name: string; position: Position; pinned: boolean };
export type SavedRoute = { id: string; name: string; plan: RoutePlan };
export type Backup = { format: 'justlocation'; version: 1; places: Place[]; routes: SavedRoute[] };
export const placesKey = 'justlocation.places';
export const routesKey = 'justlocation.routes';
export const maxFileSize = 2 * 1024 * 1024;

function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('备份中的数据格式有误');
  return value as Record<string, unknown>;
}
function title(value: unknown): string {
  if (typeof value !== 'string' || !value.trim()) throw new Error('备份中有空名称或编号');
  return value;
}
function finite(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) throw new Error('备份中有无效的数值');
  return value;
}
function position(value: unknown): Position {
  const p = record(value);
  const result = { latitude: finite(p.latitude), longitude: finite(p.longitude), altitude: finite(p.altitude),
    accuracy: finite(p.accuracy), speed: finite(p.speed), bearing: finite(p.bearing) };
  if (Math.abs(result.latitude) > 90 || Math.abs(result.longitude) > 180 || result.accuracy < 0 || result.speed < 0 || result.bearing < 0 || result.bearing >= 360) throw new Error('备份中有超出范围的位置数据');
  return result;
}
function array<T>(value: unknown, parse: (entry: unknown) => T): T[] {
  if (!Array.isArray(value)) throw new Error('备份中缺少位置或路线列表');
  return value.map(parse);
}
function place(value: unknown): Place {
  const p = record(value);
  if (typeof p.pinned !== 'boolean') throw new Error('备份中的置顶状态有误');
  return { id: title(p.id), name: title(p.name), position: position(p.position), pinned: p.pinned };
}
function route(value: unknown): SavedRoute {
  const r = record(value), p = record(r.plan);
  const plan: RoutePlan = { points: array(p.points, position), speed: finite(p.speed),
    ...(p.repeat_count === undefined ? {} : { repeat_count: finite(p.repeat_count) }),
    ...(p.repeat_delay === undefined ? {} : { repeat_delay: finite(p.repeat_delay) }) };
  parseDraft(planDraft(plan));
  if (plan.repeat_delay !== undefined && (plan.repeat_delay < 0 || plan.repeat_delay > 86400)) throw new Error('备份中的路线间隔有误');
  return { id: title(r.id), name: title(r.name), plan };
}
export function parseBackup(text: string): Backup {
  if (new TextEncoder().encode(text).length > maxFileSize) throw new Error('请选择 2 MB 以内的备份文件');
  let value;
  try { value = record(JSON.parse(text)); } catch { throw new Error('无法读取备份文件'); }
  if (value.format !== 'justlocation' || value.version !== 1) throw new Error('不支持这个备份格式，请检查文件或更新模块');
  return { format: 'justlocation', version: 1, places: array(value.places, place), routes: array(value.routes, route) };
}
export function createBackup(storage: Storage): string {
  const value = { format: 'justlocation', version: 1,
    places: JSON.parse(storage.getItem(placesKey) || '[]'), routes: JSON.parse(storage.getItem(routesKey) || '[]') };
  const text = JSON.stringify(parseBackup(JSON.stringify(value)), null, 2);
  if (new TextEncoder().encode(text).length > maxFileSize) throw new Error('数据超过 2 MB，请先分开导出路线');
  return text;
}
function merge<T extends { id: string }>(current: T[], incoming: T[]): T[] {
  const result = [...current];
  const signature = ({ id: _id, ...content }: T) => JSON.stringify(content);
  const known = new Set(current.map(signature));
  const ids = new Set(current.map(item => item.id));
  for (const item of incoming) {
    const content = signature(item);
    if (known.has(content)) continue;
    const id = ids.has(item.id) ? crypto.randomUUID() : item.id;
    result.push({ ...item, id }); known.add(content); ids.add(id);
  }
  return result;
}
export function importBackup(backup: Backup, storage: Storage): { places: number; routes: number } {
  const incoming = parseBackup(JSON.stringify(backup));
  const current = parseBackup(createBackup(storage));
  const places = merge(current.places, incoming.places), routes = merge(current.routes, incoming.routes);
  const oldPlaces = storage.getItem(placesKey);
  let wrotePlaces = false;
  try {
    storage.setItem(placesKey, JSON.stringify(places)); wrotePlaces = true;
    storage.setItem(routesKey, JSON.stringify(routes));
  } catch {
    if (wrotePlaces) {
      try { if (oldPlaces === null) storage.removeItem(placesKey); else storage.setItem(placesKey, oldPlaces); }
      catch { throw new Error('保存失败，部分位置可能已导入，请检查列表后重试'); }
    }
    throw new Error('保存失败，请检查面板的存储空间');
  }
  return { places: places.length - current.places.length, routes: routes.length - current.routes.length };
}
