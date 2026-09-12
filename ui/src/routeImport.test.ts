// @vitest-environment jsdom
import { expect, it } from 'vitest';
import { exportGpx, parseGpx } from './routeImport';

it('exports a portable GPX route with escaped names and exact coordinates and elevation', () => {
  const points = [
    { latitude: 0, longitude: 0, altitude: -3, accuracy: 5, speed: 0, bearing: 0 },
    {
      latitude: 31.23456789,
      longitude: 121.12345678,
      altitude: 12.25,
      accuracy: 5,
      speed: 0,
      bearing: 0,
    },
  ];
  const name = '河边 & <小桥> 🏠';
  const xml = exportGpx(name, { points, speed: 1.5 });
  expect(xml).toContain('xmlns="http://www.topografix.com/GPX/1/1"');
  expect(parseGpx(xml)).toEqual([{ name, points }]);
});

it('reads namespace-qualified tracks in order, including zero coordinates and elevation', () => {
  const routes =
    parseGpx(`<g:gpx xmlns:g="http://www.topografix.com/GPX/1/1"><g:trk><g:name>河边</g:name><g:trkseg>
    <g:trkpt lat="0" lon="0"><g:ele>-3</g:ele></g:trkpt><g:trkpt lat="0" lon="0"/>
    <g:trkpt lat="0" lon="0.001"/></g:trkseg></g:trk></g:gpx>`);
  expect(routes).toHaveLength(1);
  expect(routes[0].name).toBe('河边');
  expect(routes[0].points.map((p) => [p.latitude, p.longitude, p.altitude])).toEqual([
    [0, 0, -3],
    [0, 0.001, 0],
  ]);
});

it('keeps separate segments and routes separate instead of crossing recording gaps', () => {
  const points = '<trkpt lat="1" lon="2"/><trkpt lat="1" lon="3"/>';
  const routes =
    parseGpx(`<gpx><trk><name>A</name><trkseg>${points}</trkseg><trkseg>${points}</trkseg></trk>
    <rte><name>B</name><rtept lat="4" lon="5"/><rtept lat="4" lon="6"/></rte></gpx>`);
  expect(routes.map((r) => r.name)).toEqual(['A · 第 1 段', 'A · 第 2 段', 'B']);
  expect(routes.map((r) => r.points.length)).toEqual([2, 2, 2]);
});

it('rejects broken files, missing or invalid coordinates and oversized routes without truncation', () => {
  for (const xml of [
    '<gpx>',
    '<html/>',
    '<gpx><wpt lat="1" lon="2"/></gpx>',
    '<gpx><rte><rtept lat="" lon="2"/><rtept lat="1" lon="3"/></rte></gpx>',
    '<gpx><rte><rtept lat="91" lon="2"/><rtept lat="1" lon="3"/></rte></gpx>',
  ]) {
    expect(() => parseGpx(xml)).toThrow();
  }
  expect(() =>
    parseGpx(
      `<gpx><rte>${Array.from({ length: 129 }, (_, i) => `<rtept lat="1" lon="${i / 1000}"/>`).join('')}</rte></gpx>`,
    ),
  ).toThrow('128');
});
