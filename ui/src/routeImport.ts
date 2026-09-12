import { parsePosition } from './coordinates';
import type { Position, RoutePlan } from './protocol';
import { parseDraft, planDraft } from './routeDraft';

export function exportGpx(name: string, plan: RoutePlan): string {
  parseDraft(planDraft(plan));
  const escape = (text: string) =>
    text
      .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  return `<?xml version="1.0" encoding="UTF-8"?>\n<gpx version="1.1" creator="JustLocation" xmlns="http://www.topografix.com/GPX/1/1">\n  <rte><name>${escape(name)}</name>\n${plan.points.map((p) => `    <rtept lat="${p.latitude}" lon="${p.longitude}"><ele>${p.altitude}</ele></rtept>`).join('\n')}\n  </rte>\n</gpx>\n`;
}

export interface ImportedRoute {
  name: string;
  points: Position[];
}
const children = (element: Element, name: string) =>
  Array.from(element.children).filter((child) => child.localName === name);

export function parseGpx(text: string): ImportedRoute[] {
  const document = new DOMParser().parseFromString(text, 'application/xml');
  const root = document.documentElement;
  if (document.querySelector('parsererror') || root.localName !== 'gpx')
    throw new Error('无法读取 GPX 文件');
  const routes: ImportedRoute[] = [];
  function add(element: Element, pointName: string, name: string) {
    const points: Position[] = [];
    for (const point of children(element, pointName)) {
      let position: Position;
      try {
        position = parsePosition(
          point.getAttribute('lat') ?? '',
          point.getAttribute('lon') ?? '',
          children(point, 'ele')[0]?.textContent ?? '0',
        );
      } catch {
        throw new Error(`${name} 中有无效的坐标或海拔`);
      }
      const previous = points.at(-1);
      if (
        !previous ||
        previous.latitude !== position.latitude ||
        previous.longitude !== position.longitude
      )
        points.push(position);
    }
    if (points.length < 2) throw new Error(`${name} 至少需要两个不同的路线点`);
    if (points.length > 128)
      throw new Error(`${name} 有 ${points.length} 个点，目前最多支持 128 个，请先简化轨迹`);
    routes.push({ name, points });
  }
  for (const element of Array.from(root.children)) {
    const name = children(element, 'name')[0]?.textContent?.trim() || `路线 ${routes.length + 1}`;
    if (element.localName === 'trk') {
      const segments = children(element, 'trkseg');
      segments.forEach((segment, i) =>
        add(segment, 'trkpt', segments.length > 1 ? `${name} · 第 ${i + 1} 段` : name),
      );
    } else if (element.localName === 'rte') add(element, 'rtept', name);
  }
  if (!routes.length) throw new Error('文件中没有轨迹或路线');
  return routes;
}
