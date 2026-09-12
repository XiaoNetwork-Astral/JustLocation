import * as L from 'leaflet';
import type { MapProvider } from './mapProviders';

// Baidu ellipsoid and tile conventions follow gumblex/leaflet.ChineseCRS (MIT).
// See ui/licenses/leaflet-chinese-crs.txt. Position conversion remains in coordinates.ts.
const baiduProjection = {
  ...L.Projection.Mercator,
  R: 6378206.4,
  R_MINOR: 6356583.8,
};
baiduProjection.bounds = L.bounds(
  baiduProjection.project(L.latLng(-85, -180)),
  baiduProjection.project(L.latLng(85, 180)),
);

const baiduCrs = {
  ...L.CRS.EPSG3395,
  code: 'BD09',
  projection: baiduProjection,
  // Leaflet's scale is 256 * 2^zoom; Baidu uses 2^(18-zoom) meters per pixel.
  transformation: L.transformation(2 ** -26, 0, -(2 ** -26), 0),
};

export function mapCrs(provider: MapProvider): L.CRS {
  return provider.crs === 'bd09' ? baiduCrs : L.CRS.EPSG3857;
}

export function mapTiles(provider: MapProvider, url: string): L.TileLayer {
  const layer = L.tileLayer(url, {
    maxZoom: provider.maxZoom,
    subdomains: provider.subdomains.length ? provider.subdomains : 'abc',
    attribution: provider.attribution,
    referrerPolicy: 'strict-origin-when-cross-origin',
  });
  if (provider.crs === 'bd09') {
    // Baidu's rows increase northward from the equator, with signed tile indices.
    layer.getTileUrl = (coords) =>
      L.Util.template(url, {
        x: coords.x,
        y: -coords.y - 1,
        z: coords.z,
        s: provider.subdomains[Math.abs(coords.x + coords.y) % provider.subdomains.length] ?? '',
      });
  }
  return layer;
}
