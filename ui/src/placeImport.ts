import { parsePosition, type CoordinateSystem } from './coordinates';
import type { Position } from './protocol';

export type ParsedCoordinates = { latitude: number; longitude: number };

export function toPosition(parsed: ParsedCoordinates, altitude = 0): Position {
  return parsePosition(String(parsed.latitude), String(parsed.longitude), String(altitude));
}

const numberToken = /-?\d+(?:\.\d+)?/g;
// Separators and punctuation allowed around coordinate labels.
const plainSeparator = /^[\s,;，；、:=：]*$/;
/* Match long labels first so their prefixes do not leave unmatched suffixes. */
const labelPattern = /latitude|longitude|lat|lat_|lng|lon|纬度|经度/gi;
const latitudeLabel = /latitude|lat|纬度/i;
const longitudeLabel = /longitude|lng|lon|经度/i;

/* Mask labels with equal-length spaces to preserve match offsets. */
function maskLabels(text: string): string {
  return text.replace(labelPattern, (match) => ' '.repeat(match.length));
}

export function parseCoordinateText(
  input: string,
  order: 'latFirst' | 'lngFirst' = 'latFirst',
): ParsedCoordinates | null {
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
  if (
    !plainSeparator.test(masked.slice(0, first.start)) ||
    !plainSeparator.test(masked.slice(first.end, second.start))
  )
    return null;
  if (!plainSeparator.test(masked.slice(second.end))) return null;

  const firstSegment = text.slice(0, first.start);
  const secondSegment = text.slice(first.end, second.start);
  const firstIsLongitude = longitudeLabel.test(firstSegment) && !latitudeLabel.test(firstSegment);
  const firstIsLatitude = latitudeLabel.test(firstSegment) && !longitudeLabel.test(firstSegment);
  const secondIsLatitude = latitudeLabel.test(secondSegment) && !longitudeLabel.test(secondSegment);
  // Try the requested coordinate order first, then reverse it if out of range.

  const ordered = order === 'lngFirst' ? [second.value, first.value] : [first.value, second.value];
  const flipped = [ordered[1], ordered[0]];
  const valid = ([latitude, longitude]: number[]) =>
    Math.abs(latitude) <= 90 && Math.abs(longitude) <= 180;
  let latitude: number, longitude: number;
  if (firstIsLongitude || secondIsLatitude) [latitude, longitude] = [second.value, first.value];
  else if (firstIsLatitude) [latitude, longitude] = [first.value, second.value];
  else if (valid(ordered)) [latitude, longitude] = ordered;
  else if (valid(flipped)) [latitude, longitude] = flipped;
  else return null;
  if (!valid([latitude, longitude])) return null;
  return { latitude, longitude };
}

export type InlineLink = {
  provider: string;
  label: string;
  coordinateSystem: CoordinateSystem;
  position: ParsedCoordinates;
};

const coordinatePattern = /(-?\d{1,3}(?:\.\d+)?)\s*,\s*(-?\d{1,3}(?:\.\d+)?)/g;
/* Accept explicit coordinate fields and view paths, not arbitrary numbers in URLs. */
const coordinateListPattern = /@(-?\d{1,3}(?:\.\d+)?)\s*,\s*(-?\d{1,3}(?:\.\d+)?)/g;
const coordinateKeys =
  /(?:^|[?&#])(?:lat|lng|lon|latitude|longitude|latlng|ll|q|center|coord|coordinate|position|marker|point)=([\d.,;，；\s-]+)/gi;

function readPair(candidate: string): ParsedCoordinates | null {
  try {
    return parseCoordinateText(decodeURIComponent(candidate.replace(/[()（）]/g, ' ')), 'lngFirst');
  } catch {
    return null;
  }
}

const knownHosts: {
  match: RegExp;
  provider: string;
  label: string;
  coordinateSystem: CoordinateSystem;
}[] = [
  {
    match: /(?:^|\.)amap\.com$|(?:^|\.)autonavi\.com$|(?:^|\.)gaode\.com$/,
    provider: 'amap',
    label: '高德地图',
    coordinateSystem: 'gcj02',
  },
  {
    match: /(?:^|\.)map\.qq\.com$|(?:^|\.)map\.gtimg\.com$|(?:^|\.)qq\.com$/,
    provider: 'tencent',
    label: '腾讯地图',
    coordinateSystem: 'gcj02',
  },
  {
    match: /(?:^|\.)map\.baidu\.com$|(?:^|\.)map\.baidu\.com\.cn$/i,
    provider: 'baidu',
    label: '百度地图',
    coordinateSystem: 'bd09',
  },
  {
    match: /(?:^|\.)google\.[a-z.]+$|(?:^|\.)goo\.gl$|(?:^|\.)maps\.app\.goo\.gl$/,
    provider: 'google',
    label: 'Google 地图',
    coordinateSystem: 'wgs84',
  },
];

function hostOf(url: URL) {
  return knownHosts.find((entry) => entry.match.test(url.hostname));
}

/* Only extract coordinates from recognized map hosts. */
export function parseMapLink(input: string): InlineLink | null {
  const text = input.trim();
  if (!/^https?:\/\//i.test(text)) return null;
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return null;
  }
  const host = hostOf(url);
  if (!host) return null;
  // Tencent embeds its coordinate pair in the marker parameter.
  const marker = url.searchParams.get('marker');
  if (marker && /coord\s*:/i.test(marker)) {
    const parsed = readPair(marker.replace(/^[^:]*:/, ''));
    if (parsed)
      return {
        provider: host.provider,
        label: host.label,
        coordinateSystem: host.coordinateSystem,
        position: parsed,
      };
  }
  // Read explicitly named latitude and longitude fields without reordering them.

  const named = (keys: string[]) =>
    keys.map((key) => url.searchParams.get(key)).find((value) => value !== null) ?? null;
  const latitudeValue = named(['lat', 'latitude']);
  const longitudeValue = named(['lng', 'lon', 'longitude']);
  if (latitudeValue !== null && longitudeValue !== null) {
    const latitude = Number(latitudeValue),
      longitude = Number(longitudeValue);
    if (
      Number.isFinite(latitude) &&
      Number.isFinite(longitude) &&
      Math.abs(latitude) <= 90 &&
      Math.abs(longitude) <= 180
    ) {
      return {
        provider: host.provider,
        label: host.label,
        coordinateSystem: host.coordinateSystem,
        position: { latitude, longitude },
      };
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
  return position
    ? {
        provider: host.provider,
        label: host.label,
        coordinateSystem: host.coordinateSystem,
        position,
      }
    : null;
}

export type ImportResult =
  | { kind: 'link'; link: InlineLink }
  | { kind: 'coordinate'; position: ParsedCoordinates };

/* Try map links before coordinate text. */
export function parseImportText(input: string): ImportResult {
  const text = input.trim();
  if (!text) throw new Error('请粘贴地图链接、经纬度或 S 码');
  if (/^https?:\/\//i.test(text)) {
    const link = parseMapLink(text);
    if (link) return { kind: 'link', link };
    throw new Error('认不出这个链接里的坐标。可以在地图上打开后复制坐标，或直接粘贴经纬度');
  }
  if (/^s[\s-]?code|^s码/i.test(text))
    throw new Error('暂不支持 S 码，请改用地图链接或直接粘贴经纬度');
  const coordinate = parseCoordinateText(text);
  if (coordinate) return { kind: 'coordinate', position: coordinate };
  throw new Error('认不出这段内容。请粘贴“纬度, 经度”或地图分享链接');
}
