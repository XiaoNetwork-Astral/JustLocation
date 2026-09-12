// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createElement } from 'react';

const view = vi.hoisted(() => ({
  markers: [] as { html: string; at: number[] }[],
  setViews: [] as { at: number[]; zoom: number }[],
  center: { lat: 31.2, lng: 121.5 },
}));

vi.mock('leaflet', () => {
  const on = vi.fn();
  const map = {
    setView(at: number[], zoom: number) {
      view.setViews.push({ at, zoom });
      view.center = { lat: at[0], lng: at[1] };
      return map;
    },
    getCenter: () => view.center,
    on,
    panTo: vi.fn(),
    fitBounds: vi.fn(),
    remove: vi.fn(),
    invalidateSize: vi.fn(),
  };
  const layerGroup = { addTo: () => layerGroup, clearLayers: vi.fn() };
  return {
    CRS: { EPSG3857: { code: 'EPSG:3857' } },
    Proj: {
      CRS: class {
        constructor(
          public code: string,
          public options: unknown,
        ) {}
      },
    },
    point: (x: number, y: number) => ({ x, y }),
    latLng: (lat: number, lng: number) => ({ lat, lng }),
    latLngBounds: (points: number[][]) => points,
    map: () => map,
    layerGroup: () => layerGroup,
    tileLayer: () => ({ addTo: () => ({ on: vi.fn() }), on: vi.fn() }),
    control: { zoom: () => ({ addTo: vi.fn() }) },
    polyline: () => ({ addTo: () => layerGroup }),
    marker: (at: number[], options: { icon: { options: { html: string } } }) => {
      view.markers.push({ html: options.icon.options.html, at });
      return { on: () => ({ addTo: () => layerGroup }) };
    },
    divIcon: (options: { html: string }) => ({ options }),
  };
});
vi.mock('leaflet/dist/leaflet.css', () => ({}));

const { MapPicker } = await import('./MapPicker');
const { defaultMapProvider, findMapProvider, hasUsableSource, mapProviderKey, resolveTileUrl } =
  await import('./mapProviders');
const at = { latitude: 31.2, longitude: 121.5, altitude: 12, accuracy: 5, speed: 0, bearing: 0 };

beforeEach(() => {
  view.markers.length = 0;
  view.setViews.length = 0;
});
afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.restoreAllMocks();
});

it('registers the providers with the coordinate system their tiles actually use', () => {
  expect(findMapProvider('amap').coordinateSystem).toBe('gcj02');
  expect(findMapProvider('tencent').coordinateSystem).toBe('gcj02');
  expect(findMapProvider('baidu').coordinateSystem).toBe('bd09');
  expect(findMapProvider(defaultMapProvider).coordinateSystem).toBe('wgs84');
  expect(findMapProvider('amap').label).toBe('高德地图');

  expect(hasUsableSource(findMapProvider('custom'), '')).toBe(false);
  expect(hasUsableSource(findMapProvider('custom'), 'https://tiles.example/{z}/{x}/{y}.png')).toBe(
    true,
  );
  expect(resolveTileUrl(findMapProvider('osm'), '')).toContain('{z}');
});

it('switches the map source and remembers the choice', async () => {
  render(<MapPicker initial={at} onChoose={vi.fn()} onClose={vi.fn()} />);
  const user = userEvent.setup();
  const select = await screen.findByLabelText('地图图源');
  expect(select).toHaveProperty('value', 'osm');
  await user.selectOptions(select, 'amap');
  expect(select).toHaveProperty('value', 'amap');
  expect(localStorage.getItem(mapProviderKey)).toBe('amap');
  expect(await screen.findByText(/公开瓦片接口/)).toBeTruthy();
});

it('centers on a search result so the coordinates come from the selected place', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => ({
      ok: true,
      json: async () => [
        {
          name: '上海火车站',
          display_name: '上海火车站, 静安区, 上海市',
          lat: '31.2497',
          lon: '121.4553',
        },
      ],
    })),
  );
  render(<MapPicker initial={at} onChoose={vi.fn()} onClose={vi.fn()} />);
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText('搜索地点'), '上海火车站');
  await user.click(screen.getByRole('button', { name: '搜索' }));
  expect(await screen.findByText('上海火车站')).toBeTruthy();
  await user.click(screen.getByRole('listitem'));
  await waitFor(() => expect(view.setViews.at(-1)).toEqual({ at: [31.2497, 121.4553], zoom: 16 }));
  expect(screen.getByLabelText('所选坐标').textContent).toBe('31.249700, 121.455300');
});

it('accepts coordinates typed into the search field without calling the search service', async () => {
  const fetchMock = vi.fn();
  vi.stubGlobal('fetch', fetchMock);
  render(<MapPicker initial={at} onChoose={vi.fn()} onClose={vi.fn()} />);
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText('搜索地点'), '30.5, 120.5');
  await user.click(screen.getByRole('button', { name: '搜索' }));
  await waitFor(() => expect(view.setViews.at(-1)).toEqual({ at: [30.5, 120.5], zoom: 16 }));
  expect(fetchMock).not.toHaveBeenCalled();
});

it('moves to the current position and reports a denied permission in plain words', async () => {
  const locate = vi.fn(async () => ({ latitude: 31.2, longitude: 121.5 }));
  const { unmount } = render(
    <MapPicker initial={null} onChoose={vi.fn()} onClose={vi.fn()} locate={locate} />,
  );
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: '定位到当前位置' }));
  await waitFor(() => expect(view.setViews.at(-1)).toEqual({ at: [31.2, 121.5], zoom: 16 }));
  expect(locate).toHaveBeenCalledTimes(1);
  unmount();

  render(
    <MapPicker
      initial={null}
      onChoose={vi.fn()}
      onClose={vi.fn()}
      locate={async () => {
        throw new Error('没有定位权限，无法定位到当前位置');
      }}
    />,
  );
  await user.click(await screen.findByRole('button', { name: '定位到当前位置' }));
  expect((await screen.findByRole('alert')).textContent).toContain('没有定位权限');
});

it('converts stored WGS84 points before handing them to a GCJ-02 map', async () => {
  render(
    <MapPicker
      route={{ points: [at, { ...at, longitude: 121.51 }], onConfirm: vi.fn() }}
      onClose={vi.fn()}
    />,
  );
  const user = userEvent.setup();
  await user.selectOptions(await screen.findByLabelText('地图图源'), 'amap');

  await waitFor(() => expect(view.markers.length).toBeGreaterThanOrEqual(2));
  const drawn = view.markers.slice(-2);
  expect(drawn[0].html).toBe('1');

  expect(drawn[0].at[0]).not.toBeCloseTo(at.latitude, 5);
  expect(drawn[0].at[1]).not.toBeCloseTo(at.longitude, 5);
  expect(drawn[0].at[0]).toBeGreaterThan(31.19);
  expect(drawn[0].at[0]).toBeLessThan(31.21);
  expect(screen.getByText(/已换算/)).toBeTruthy();
});
