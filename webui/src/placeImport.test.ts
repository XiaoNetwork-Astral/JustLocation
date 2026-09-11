import { expect, it } from 'vitest';
import { parseCoordinateText, parseImportText, parseMapLink, toPosition } from './placeImport';

it('reads plain coordinates with the separators the spec lists', () => {
  expect(parseCoordinateText('31.2, 121.5')).toEqual({ latitude: 31.2, longitude: 121.5 });
  expect(parseCoordinateText('31.2，121.5')).toEqual({ latitude: 31.2, longitude: 121.5 });
  expect(parseCoordinateText('31.2;121.5')).toEqual({ latitude: 31.2, longitude: 121.5 });
  expect(parseCoordinateText('31.2；121.5')).toEqual({ latitude: 31.2, longitude: 121.5 });
  expect(parseCoordinateText('  31.2   121.5 ')).toEqual({ latitude: 31.2, longitude: 121.5 });
  expect(parseCoordinateText('-31.2, -121.5')).toEqual({ latitude: -31.2, longitude: -121.5 });
});

it('honours labels and accepts them in either order', () => {
  expect(parseCoordinateText('纬度: 31.2, 经度: 121.5')).toEqual({ latitude: 31.2, longitude: 121.5 });
  expect(parseCoordinateText('经度 121.5 纬度 31.2')).toEqual({ latitude: 31.2, longitude: 121.5 });
  expect(parseCoordinateText('lng=121.5, lat=31.2')).toEqual({ latitude: 31.2, longitude: 121.5 });
  expect(parseCoordinateText('longitude: 121.5, latitude: 31.2')).toEqual({ latitude: 31.2, longitude: 121.5 });
  // 只有经度有标签时，另一个数字按纬度解释。
  expect(parseCoordinateText('121.5, lat 31.2')).toEqual({ latitude: 31.2, longitude: 121.5 });
});

it('rejects ranges, stray text, degree notation and three numbers', () => {
  expect(parseCoordinateText('91, 121.5')).toBeNull();
  expect(parseCoordinateText('31.2, 181')).toBeNull();
  expect(parseCoordinateText('31.2')).toBeNull();
  expect(parseCoordinateText('31.2, 121.5, 12')).toBeNull();
  // 标签本身允许，但夹带其他文字就不是一段纯坐标。
  expect(parseCoordinateText('大概在 31.2, 121.5 附近')).toBeNull();
  expect(parseCoordinateText("31°12'30\"N 121°30'E")).toBeNull();
  expect(parseCoordinateText('')).toBeNull();
});

it('reads the shared shapes of the four map providers', () => {
  // 分享链接里的坐标是"经度在前"，和坐标文本的默认顺序相反。
  expect(parseMapLink('https://uri.amap.com/marker?position=116.404,39.915&name=x')).toEqual({
    provider: 'amap', label: '高德地图', coordinateSystem: 'gcj02', position: { latitude: 39.915, longitude: 116.404 } });
  expect(parseMapLink('https://www.amap.com/?lng=116.404&lat=39.915')).toMatchObject({ provider: 'amap', coordinateSystem: 'gcj02' });
  expect(parseMapLink('https://map.qq.com/?what=marker&marker=coord:39.915,116.404')).toMatchObject({ provider: 'tencent', coordinateSystem: 'gcj02' });
  expect(parseMapLink('https://map.baidu.com/?latlng=39.915,116.404&title=x')).toMatchObject({ provider: 'baidu', coordinateSystem: 'bd09' });
  expect(parseMapLink('https://www.google.com/maps/place/x/@39.915,116.404,15z')).toMatchObject({ provider: 'google', coordinateSystem: 'wgs84' });
});

it('never mistakes projected map coordinates for latitude and longitude', () => {
  // 百度 /@ 后面是米制投影坐标；范围检查会拒掉它们，而不是给出一个错误的位置。
  expect(parseMapLink('https://map.baidu.com/@12947052.9,4825924.6,19z')).toBeNull();
  // 同一个链接里如果另有明确经纬度，仍然可以采信。
  expect(parseMapLink('https://map.baidu.com/@12947052.9,4825924.6,19z?latlng=39.915,116.404')).toMatchObject({ provider: 'baidu' });
  expect(parseCoordinateText('12947052, 4825924')).toBeNull();
});

it('refuses to guess when the host or the link carries no coordinates', () => {
  expect(parseMapLink('https://example.com/?x=1,2')).toBeNull();
  expect(parseMapLink('https://maps.app.goo.gl/abc123')).toBeNull();
  expect(parseMapLink('not a url')).toBeNull();
});

it('routes text the way the spec describes and explains what it cannot read', () => {
  expect(parseImportText('https://map.baidu.com/?latlng=39.915,116.404')).toMatchObject({ kind: 'link' });
  expect(parseImportText('31.2, 121.5')).toEqual({ kind: 'coordinate', position: { latitude: 31.2, longitude: 121.5 } });
  expect(() => parseImportText('https://maps.app.goo.gl/abc123')).toThrow(/认不出这个链接/);
  expect(() => parseImportText('S码：AbC123')).toThrow(/暂不支持 S 码/);
  expect(() => parseImportText('')).toThrow(/请粘贴/);
});

it('builds a full position with the requested altitude', () => {
  const position = toPosition({ latitude: 31.2, longitude: 121.5 }, 12);
  expect(position).toMatchObject({ latitude: 31.2, longitude: 121.5, altitude: 12, accuracy: 5 });
});
