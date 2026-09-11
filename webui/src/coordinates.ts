// Formulae adapted from wandergis/coordtransform (MIT).
// Copyright (c) 2015 记忆的残骸; see module/licenses/coordtransform.txt.
import { parsePosition, type Position } from './control';

export type CoordinateSystem = 'wgs84' | 'gcj02' | 'bd09';
type Point = { latitude: number; longitude: number };
const pi = Math.PI, xPi = pi * 3000 / 180, axis = 6378245, eccentricity = 0.00669342162296594323;
// Approximate conversion domain, not a geographic boundary dataset.
const inside = (p: Point) => p.longitude > 73.66 && p.longitude < 135.05 && p.latitude > 3.86 && p.latitude < 53.55;
function toGcj(p: Point): Point {
  if (!inside(p)) return p;
  const x = p.longitude - 105, y = p.latitude - 35;
  const waves = (20 * Math.sin(6 * x * pi) + 20 * Math.sin(2 * x * pi)) * 2 / 3;
  let lat = -100 + 2 * x + 3 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * Math.sqrt(Math.abs(x)) + waves;
  lat += (20 * Math.sin(y * pi) + 40 * Math.sin(y / 3 * pi)) * 2 / 3;
  lat += (160 * Math.sin(y / 12 * pi) + 320 * Math.sin(y * pi / 30)) * 2 / 3;
  let lon = 300 + x + 2 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * Math.sqrt(Math.abs(x)) + waves;
  lon += (20 * Math.sin(x * pi) + 40 * Math.sin(x / 3 * pi)) * 2 / 3;
  lon += (150 * Math.sin(x / 12 * pi) + 300 * Math.sin(x / 30 * pi)) * 2 / 3;
  const radians = p.latitude / 180 * pi;
  const magic = 1 - eccentricity * Math.sin(radians) ** 2, root = Math.sqrt(magic);
  return { latitude: p.latitude + lat * 180 / (axis * (1 - eccentricity) / (magic * root) * pi),
    longitude: p.longitude + lon * 180 / (axis / root * Math.cos(radians) * pi) };
}
function fromGcj(p: Point): Point {
  if (!inside(p)) return p;
  let result = { ...p };
  for (let i = 0; i < 20; i++) {
    const projected = toGcj(result);
    const latError = projected.latitude - p.latitude, lonError = projected.longitude - p.longitude;
    result = { latitude: result.latitude - latError, longitude: result.longitude - lonError };
    if (Math.max(Math.abs(latError), Math.abs(lonError)) < 1e-10) return result;
  }
  throw new Error('这个位置接近转换边界，请直接输入 WGS84 坐标');
}
function fromBd(p: Point): Point {
  const x = p.longitude - 0.0065, y = p.latitude - 0.006;
  const z = Math.hypot(x, y) - 0.00002 * Math.sin(y * xPi);
  const theta = Math.atan2(y, x) - 0.000003 * Math.cos(x * xPi);
  return { latitude: z * Math.sin(theta), longitude: z * Math.cos(theta) };
}
function toBd(p: Point): Point {
  const z = Math.hypot(p.longitude, p.latitude) + 0.00002 * Math.sin(p.latitude * xPi);
  const theta = Math.atan2(p.latitude, p.longitude) + 0.000003 * Math.cos(p.longitude * xPi);
  return { latitude: z * Math.sin(theta) + 0.006, longitude: z * Math.cos(theta) + 0.0065 };
}
export function convertCoordinates(position: Position, from: CoordinateSystem, to: CoordinateSystem): Position {
  parsePosition(String(position.latitude), String(position.longitude), String(position.altitude));
  if (from === to) return { ...position };
  const gcj = from === 'wgs84' ? toGcj(position) : from === 'bd09' ? fromBd(position) : position;
  const result = to === 'wgs84' ? fromGcj(gcj) : to === 'bd09' ? toBd(gcj) : gcj;
  parsePosition(String(result.latitude), String(result.longitude), String(position.altitude));
  return { ...position, ...result };
}
