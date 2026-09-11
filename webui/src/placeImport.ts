// 位置导入的解析层。
//
// 依据重实现规格第 8.2 节：
//   - 坐标文本：两个十进制数，分隔符可为中英文逗号、分号或空白；
//     也支持 lat/latitude/纬度 与 lon/longitude/lng/经度 标签，标签顺序可以交换；
//     没有标签时按"纬度在前、经度在后"解释；只校验有限数值与取值范围，
//     不把度分秒、科学计数法或任意文本抽取当成正式格式。
//   - 地图链接：高德、腾讯、Google、百度。参数与路径形状各不同，
//     这里按各自常见形态取坐标；取不到就如实报错，不猜。

import { parsePosition, type Position } from './control';
import type { CoordinateSystem } from './coordinates';

export type ParsedCoordinates = { latitude: number; longitude: number };

/** 把"纬度在前"的值补成完整位置；海拔与其余字段由调用方决定。 */
export function toPosition(parsed: ParsedCoordinates, altitude = 0): Position {
  return parsePosition(String(parsed.latitude), String(parsed.longitude), String(altitude));
}

const numberToken = /-?\d+(?:\.\d+)?/g;
// 片段里允许出现的字符：各种分隔符，以及字段标签后常见的冒号/等号。
const plainSeparator = /^[\s,;，；、:=：]*$/;
/** 字段标签。长名必须写在短名前面，否则 "latitude" 只会被匹配掉 "lat"，剩下的 "itude" 会破坏校验。 */
const labelPattern = /latitude|longitude|lat|lat_|lng|lon|纬度|经度/gi;
const latitudeLabel = /latitude|lat|纬度/i;
const longitudeLabel = /longitude|lng|lon|经度/i;

/** 用等长空格盖掉字段标签，这样后续按下标切片的位置仍然对得上。 */
function maskLabels(text: string): string {
  return text.replace(labelPattern, match => ' '.repeat(match.length));
}

/** 解析一段坐标文本；无法作为坐标解释时返回 null。 */
export function parseCoordinateText(input: string, order: 'latFirst' | 'lngFirst' = 'latFirst'): ParsedCoordinates | null {
  const text = input.trim();
  if (!text) return null;
  const masked = maskLabels(text);
  const tokens: { value: number; start: number; end: number }[] = [];
  for (const match of masked.matchAll(numberToken)) {
    const value = Number(match[0]);
    if (!Number.isFinite(value)) return null;
    tokens.push({ value, start: match.index, end: match.index + match[0].length });
  }
  if (tokens.length !== 2) return null;
  const [first, second] = tokens;
  if (!plainSeparator.test(masked.slice(0, first.start)) || !plainSeparator.test(masked.slice(first.end, second.start))) return null;
  if (!plainSeparator.test(masked.slice(second.end))) return null;

  // 标签写在数字前面，决定这个数字是纬度还是经度；两个数字都没标签时按调用方给的顺序。
  const firstSegment = text.slice(0, first.start);
  const secondSegment = text.slice(first.end, second.start);
  const firstIsLongitude = longitudeLabel.test(firstSegment) && !latitudeLabel.test(firstSegment);
  const firstIsLatitude = latitudeLabel.test(firstSegment) && !longitudeLabel.test(firstSegment);
  const secondIsLatitude = latitudeLabel.test(secondSegment) && !longitudeLabel.test(secondSegment);
  // 没有标签时按调用方给的顺序读；这个顺序不合法时再试反过来的一种。
  // 地图链接常见 lng,lat（甚至单值参数），而腾讯的 marker=coord: 用的是 lat,lng，
  // 所以两种顺序都要能落对，而不是只认一种。
  const ordered = order === 'lngFirst' ? [second.value, first.value] : [first.value, second.value];
  const flipped = [ordered[1], ordered[0]];
  const valid = ([latitude, longitude]: number[]) => Math.abs(latitude) <= 90 && Math.abs(longitude) <= 180;
  let latitude: number, longitude: number;
  if (firstIsLongitude || secondIsLatitude) [latitude, longitude] = [second.value, first.value];
  else if (firstIsLatitude) [latitude, longitude] = [first.value, second.value];
  else if (valid(ordered)) [latitude, longitude] = ordered;
  else if (valid(flipped)) [latitude, longitude] = flipped;
  else return null;
  if (!valid([latitude, longitude])) return null;
  return { latitude, longitude };
}

export type InlineLink = { provider: string; label: string; coordinateSystem: CoordinateSystem; position: ParsedCoordinates };

const coordinatePattern = /(-?\d{1,3}(?:\.\d+)?)\s*,\s*(-?\d{1,3}(?:\.\d+)?)/g;
/**
 * 链接里必须明确是坐标才采信：要么带经纬度字段名，要么是 @纬度,经度 这种视图参数。
 * 不这样做的话，百度 /@12947052,4825924 这类**投影坐标**会被当成经纬度，
 * 范围检查过不去倒是小事，万一落进范围就会给出一个静默错误的位置。
 */
const coordinateListPattern = /@(-?\d{1,3}(?:\.\d+)?)\s*,\s*(-?\d{1,3}(?:\.\d+)?)/g;
const coordinateKeys = /(?:^|[?&#])(?:lat|lng|lon|latitude|longitude|latlng|ll|q|center|coord|coordinate|position|marker|point)=([\d.,;，；\s-]+)/gi;

/** 链接里的坐标一律按"经度在前"读，这是各家分享链接与 marker=coord: 写法的通行顺序。 */
function readPair(candidate: string): ParsedCoordinates | null {
  try { return parseCoordinateText(decodeURIComponent(candidate.replace(/[()（）]/g, ' ')), 'lngFirst'); }
  catch { return null; }
}

const knownHosts: { match: RegExp; provider: string; label: string; coordinateSystem: CoordinateSystem }[] = [
  { match: /(?:^|\.)amap\.com$|(?:^|\.)autonavi\.com$|(?:^|\.)gaode\.com$/, provider: 'amap', label: '高德地图', coordinateSystem: 'gcj02' },
  { match: /(?:^|\.)map\.qq\.com$|(?:^|\.)map\.gtimg\.com$|(?:^|\.)qq\.com$/, provider: 'tencent', label: '腾讯地图', coordinateSystem: 'gcj02' },
  { match: /(?:^|\.)map\.baidu\.com$|(?:^|\.)map\.baidu\.com\.cn$/i, provider: 'baidu', label: '百度地图', coordinateSystem: 'bd09' },
  { match: /(?:^|\.)google\.[a-z.]+$|(?:^|\.)goo\.gl$|(?:^|\.)maps\.app\.goo\.gl$/, provider: 'google', label: 'Google 地图', coordinateSystem: 'wgs84' },
];

function hostOf(url: URL) {
  return knownHosts.find(entry => entry.match.test(url.hostname));
}

/**
 * 解析地图链接里的坐标。hostname 必须能认出来，否则返回 null：
 * 拿一段陌生 URL 里的随机数字当地址，比直接报错更糟。
 */
export function parseMapLink(input: string): InlineLink | null {
  const text = input.trim();
  if (!/^https?:\/\//i.test(text)) return null;
  let url: URL;
  try { url = new URL(text); } catch { return null; }
  const host = hostOf(url);
  if (!host) return null;
  // 腾讯的 marker 参数带 "coord:" 前缀，属于该家特有的写法，先单独取出来。
  const marker = url.searchParams.get('marker');
  if (marker && /coord\s*:/i.test(marker)) {
    const parsed = readPair(marker.replace(/^[^:]*:/, ''));
    if (parsed) return { provider: host.provider, label: host.label, coordinateSystem: host.coordinateSystem, position: parsed };
  }
  // 再看有没有明确的 lat= 与 lng= 单值参数（高德等会这样给），
  // 拼成一对再解释会因为"经度在前"而把经度当成纬度，所以直接按字段名取值。
  const named = (keys: string[]) => keys.map(key => url.searchParams.get(key)).find(value => value !== null) ?? null;
  const latitudeValue = named(['lat', 'latitude']);
  const longitudeValue = named(['lng', 'lon', 'longitude']);
  if (latitudeValue !== null && longitudeValue !== null) {
    const latitude = Number(latitudeValue), longitude = Number(longitudeValue);
    if (Number.isFinite(latitude) && Number.isFinite(longitude) && Math.abs(latitude) <= 90 && Math.abs(longitude) <= 180) {
      return { provider: host.provider, label: host.label, coordinateSystem: host.coordinateSystem, position: { latitude, longitude } };
    }
  }
  const raw = `${url.search}&${url.hash}&${url.pathname}`;
  const decoded = decodeURIComponent(raw);
  const found: ParsedCoordinates[] = [];
  for (const source of [raw, decoded]) {
    for (const match of source.matchAll(coordinateKeys)) {
      const pair = coordinatePattern.exec(match[1]);
      coordinatePattern.lastIndex = 0;
      const parsed = pair && readPair(`${pair[1]},${pair[2]}`);
      if (parsed) found.push(parsed);
    }
    for (const match of source.matchAll(coordinateListPattern)) {
      const parsed = readPair(`${match[1]},${match[2]}`);
      if (parsed) found.push(parsed);
    }
  }
  const position = found[0];
  return position ? { provider: host.provider, label: host.label, coordinateSystem: host.coordinateSystem, position } : null;
}

export type ImportResult = { kind: 'link'; link: InlineLink } | { kind: 'coordinate'; position: ParsedCoordinates };

/** 按规格的分流顺序：先链接，再坐标文本；两者都不成立时报错。 */
export function parseImportText(input: string): ImportResult {
  const text = input.trim();
  if (!text) throw new Error('请粘贴地图链接、经纬度或 S 码');
  if (/^https?:\/\//i.test(text)) {
    const link = parseMapLink(text);
    if (link) return { kind: 'link', link };
    throw new Error('认不出这个链接里的坐标。可以在地图上打开后复制坐标，或直接粘贴经纬度');
  }
  if (/^s[\s-]?code|^s码/i.test(text)) throw new Error('暂不支持 S 码，请改用地图链接或直接粘贴经纬度');
  const coordinate = parseCoordinateText(text);
  if (coordinate) return { kind: 'coordinate', position: coordinate };
  throw new Error('认不出这段内容。请粘贴“纬度, 经度”或地图分享链接');
}
