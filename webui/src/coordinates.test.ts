import { expect, it } from 'vitest';
import { convertCoordinates } from './coordinates';
const p = { latitude: 39.915, longitude: 116.404, altitude: 12, accuracy: 5, speed: 0, bearing: 0 };

it('matches independently published coordtransform conversion examples', () => {
  // https://github.com/wandergis/coordtransform#示例用法exampleusage
  const gcj = convertCoordinates(p, 'wgs84', 'gcj02');
  expect(gcj.longitude).toBeCloseTo(116.41024449916938, 8);
  expect(gcj.latitude).toBeCloseTo(39.91640428150164, 8);
  const bd = convertCoordinates(p, 'gcj02', 'bd09');
  expect(bd.longitude).toBeCloseTo(116.41036949371029, 8);
  expect(bd.latitude).toBeCloseTo(39.92133699351021, 8);
  const back = convertCoordinates(p, 'bd09', 'gcj02');
  expect(back.longitude).toBeCloseTo(116.39762729119315, 8);
  expect(back.latitude).toBeCloseTo(39.90865673957631, 8);
});

it('uses an iterative inverse and preserves altitude and other position fields', () => {
  for (const latitude of [22.5, 31.2, 39.9, 45]) {
    const source = { ...p, latitude };
    for (const system of ['gcj02', 'bd09'] as const) {
      const result = convertCoordinates(convertCoordinates(source, 'wgs84', system), system, 'wgs84');
      expect(result.latitude).toBeCloseTo(source.latitude, system === 'gcj02' ? 8 : 5);
      expect(result.longitude).toBeCloseTo(source.longitude, system === 'gcj02' ? 8 : 5);
      expect(result.altitude).toBe(12);
    }
  }
});

it('does not apply GCJ offsets to overseas coordinates, including zero and the poles', () => {
  for (const [latitude, longitude] of [[0, 0], [51.5, -0.1], [-33.9, 151.2], [90, 0], [-90, 180]]) {
    const position = { ...p, latitude, longitude };
    expect(convertCoordinates(position, 'wgs84', 'gcj02')).toEqual(position);
    expect(convertCoordinates(position, 'gcj02', 'wgs84')).toEqual(position);
  }
});
