// @vitest-environment jsdom
import { expect, it } from 'vitest';
import * as L from 'leaflet';
import { mapCrs, mapTiles } from './mapProjection';
import { findMapProvider } from './mapProviders';

it('projects Baidu coordinates with real Leaflet and preserves its standard projections', () => {
  const crs = mapCrs(findMapProvider('baidu'));
  expect(mapCrs(findMapProvider('osm'))).toBe(L.CRS.EPSG3857);
  expect(L.CRS.EPSG3395.code).toBe('EPSG:3395');
  for (const source of [
    L.latLng(0, 0),
    L.latLng(39.915, 116.404),
    L.latLng(-30, -100),
    L.latLng(74, 179),
  ]) {
    const projected = crs.project(source);
    const pixels = crs.latLngToPoint(source, 18);
    expect(pixels.x).toBeCloseTo(projected.x, 6);
    expect(pixels.y).toBeCloseTo(-projected.y, 6);
    const restored = crs.pointToLatLng(pixels, 18);
    expect(restored.lat).toBeCloseTo(source.lat, 6);
    expect(restored.lng).toBeCloseTo(source.lng, 6);
  }
  expect(crs.project(L.latLng(39.915, 116.404)).x).toBeCloseTo(12958175, 0);
  expect(crs.project(L.latLng(39.915, 116.404)).y).toBeCloseTo(4825924, -1);
});

it('maps Leaflet rows to signed Baidu tile indices without changing longitude indices', () => {
  const layer = mapTiles(findMapProvider('baidu'), 'https://tile{s}.example/{z}/{x}/{y}');
  const north = Object.assign(L.point(12654, -4713), { z: 16 });
  const south = Object.assign(L.point(-2, 3), { z: 4 });
  expect(layer.getTileUrl(north)).toMatch(/^https:\/\/tile[0-3]\.example\/16\/12654\/4712$/);
  expect(layer.getTileUrl(south)).toMatch(/^https:\/\/tile[0-3]\.example\/4\/-2\/-4$/);
});
