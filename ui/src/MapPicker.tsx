import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ArrowLeft, Crosshair, MapPin, Plus, RotateCcw, Search, Undo2 } from 'lucide-react';
import * as L from 'leaflet';
import 'leaflet/dist/leaflet.css';
import { parsePosition, convertCoordinates } from './coordinates';
import type { Position } from './protocol';
import { mapCrs, mapTiles } from './mapProjection';

import {
  findMapProvider,
  hasUsableSource,
  mapProviders,
  readMapPreferences,
  resolveTileUrl,
  saveMapPreferences,
  type MapProvider,
  type MapProviderId,
} from './mapProviders';

export type LocateResult = { latitude: number; longitude: number };
export type Locate = () => Promise<LocateResult>;

const defaultLocate: Locate = () =>
  new Promise((resolve, reject) => {
    if (!('geolocation' in navigator)) {
      reject(new Error('这个环境不支持定位到当前位置'));
      return;
    }
    navigator.geolocation.getCurrentPosition(
      (fix) => resolve({ latitude: fix.coords.latitude, longitude: fix.coords.longitude }),
      (reason) =>
        reject(
          new Error(
            reason.code === reason.PERMISSION_DENIED
              ? '没有定位权限，无法定位到当前位置'
              : '暂时取不到当前位置，请稍后再试',
          ),
        ),
      { enableHighAccuracy: false, timeout: 10000, maximumAge: 60000 },
    );
  });

type Props = { onClose: () => void; locate?: Locate; searchEndpoint?: string } & (
  | { initial: Position | null; onChoose: (p: Position) => void; route?: never }
  | {
      initial?: never;
      onChoose?: never;
      route: { points: Position[]; onConfirm: (points: Position[]) => void };
    }
);

type SearchHit = { name: string; detail: string; position: Position };

/* Wrap only out-of-range longitudes to avoid introducing rounding error. */
const wrapLongitude = (longitude: number) =>
  longitude >= -180 && longitude <= 180
    ? longitude
    : ((((longitude + 180) % 360) + 360) % 360) - 180;
const clampLatitude = (latitude: number) => Math.max(-85, Math.min(85, latitude));
/* Round map coordinates to seven decimal places. */
const tidy = (value: number) => Math.round(value * 1e7) / 1e7;

/* Convert stored WGS84 positions to the tile provider coordinate system. */
function toProvider(position: Position, provider: MapProvider): Position {
  return provider.coordinateSystem === 'wgs84'
    ? position
    : convertCoordinates(position, 'wgs84', provider.coordinateSystem);
}
function fromProvider(position: Position, provider: MapProvider): Position {
  return provider.coordinateSystem === 'wgs84'
    ? position
    : convertCoordinates(position, provider.coordinateSystem, 'wgs84');
}

export function MapPicker({
  initial: point,
  onChoose,
  onClose,
  route,
  locate = defaultLocate,
  searchEndpoint = 'https://nominatim.openstreetmap.org/search',
}: Props) {
  const initial = route ? route.points.at(-1) || null : point;
  const dialog = useRef<HTMLDialogElement>(null),
    canvas = useRef<HTMLDivElement>(null);
  const mapRef = useRef<L.Map | null>(null),
    tilesRef = useRef<L.TileLayer | null>(null);
  const routeLayer = useRef<L.LayerGroup | null>(null);
  const [points, setPoints] = useState(() => route?.points || []);
  const [error, setError] = useState('');
  const [failed, setFailed] = useState(false);
  const [preferences, setPreferences] = useState(readMapPreferences);
  const provider = findMapProvider(preferences.provider);
  const tileUrl = resolveTileUrl(provider, preferences.customUrl);
  const usable = hasUsableSource(provider, preferences.customUrl);
  const [selected, setSelected] = useState(() => initial || parsePosition('35', '105', '0'));
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [note, setNote] = useState('');
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const providerRef = useRef(provider);
  providerRef.current = provider;
  // Keep altitude and other position fields current across map rebuilds.
  const baseRef = useRef(initial || parsePosition('35', '105', '0'));
  baseRef.current = initial || baseRef.current;
  const showPosition = useRef<Position>(initial || parsePosition('35', '105', '0'));
  const crs = mapCrs(provider);

  function applySelected(next: Position) {
    showPosition.current = next;
    setSelected(next);
  }

  useEffect(() => {
    dialog.current?.showModal?.();
    const back = () => closeRef.current();
    window.addEventListener('popstate', back);
    return () => window.removeEventListener('popstate', back);
  }, []);

  useEffect(() => {
    if (!usable || !canvas.current) return;
    const current = providerRef.current;
    // Rebuild from the selected position; the old map center may still be stale.

    const known = showPosition.current;
    const start = toProvider(known, current);
    const map = L.map(canvas.current, {
      zoomControl: false,
      minZoom: 2,
      maxZoom: current.maxZoom,
      worldCopyJump: true,
      crs,
      maxBounds:
        current.crs === 'bd09'
          ? undefined
          : [
              [-85, -Infinity],
              [85, Infinity],
            ],
      maxBoundsViscosity: 1,
    }).setView([clampLatitude(start.latitude), wrapLongitude(start.longitude)], initial ? 16 : 4);
    mapRef.current = map;
    routeLayer.current = L.layerGroup().addTo(map);
    const tiles = mapTiles(current, tileUrl).addTo(map);
    tilesRef.current = tiles;
    tiles.on('loading', () => setFailed(false));
    tiles.on('tileerror', () => setFailed(true));
    // Keep zoom controls clear of the search and provider controls.
    L.control
      .zoom({ position: 'bottomleft', zoomInTitle: '放大', zoomOutTitle: '缩小' })
      .addTo(map);
    const update = () => {
      const center = map.getCenter();
      applySelected(
        fromProvider(
          {
            ...baseRef.current,
            latitude: tidy(center.lat),
            longitude: tidy(wrapLongitude(center.lng)),
          },
          current,
        ),
      );
    };
    map.on('moveend', update);
    map.on('click', (event: L.LeafletMouseEvent) => map.panTo(event.latlng, { animate: false }));
    const observer = new ResizeObserver(() => map.invalidateSize({ animate: false }));
    observer.observe(canvas.current);
    return () => {
      observer.disconnect();
      map.remove();
      mapRef.current = null;
      tilesRef.current = null;
      routeLayer.current = null;
    };
  }, [crs, tileUrl, usable]);

  useEffect(() => {
    const layer = routeLayer.current;
    if (!layer || !route) return;
    const current = providerRef.current;
    const shown = points.map((p) => toProvider(p, current));
    layer.clearLayers();
    L.polyline(
      shown.map((p) => [p.latitude, p.longitude] as L.LatLngTuple),
      { color: '#526836', weight: 4 },
    ).addTo(layer);
    shown.forEach((p, index) =>
      L.marker([p.latitude, p.longitude], {
        icon: L.divIcon({
          className: 'route-map-marker',
          html: String(index + 1),
          iconSize: [26, 26],
          iconAnchor: [13, 13],
        }),
        title: `点 ${index + 1}`,
      })
        .on('click', () => mapRef.current?.panTo([p.latitude, p.longitude], { animate: false }))
        .addTo(layer),
    );
  }, [points, route, provider.id]);

  function close() {
    if (window.history.state?.justlocationMap) window.history.back();
    else onClose();
  }
  function centerOn(position: Position) {
    const native = toProvider(position, provider);
    mapRef.current?.setView([clampLatitude(native.latitude), wrapLongitude(native.longitude)], 16, {
      animate: false,
    });
    // Preserve explicit WGS84 input after moveend converts the displayed map center back.
    applySelected(position);
  }

  async function search() {
    const text = query.trim();
    if (!text) return;
    setSearching(true);
    setError('');
    setHits([]);
    setNote('');
    try {
      // Coordinate input does not need a network request.
      const direct = text.match(/^\s*(-?\d+(?:\.\d+)?)\s*[,，\s]\s*(-?\d+(?:\.\d+)?)\s*$/);
      if (direct) {
        centerOn(parsePosition(direct[1], direct[2], String(selected.altitude)));
        return;
      }
      const response = await fetch(
        `${searchEndpoint}?format=jsonv2&limit=8&q=${encodeURIComponent(text)}`,
        { headers: { Accept: 'application/json' } },
      );
      if (!response.ok) throw new Error('搜索服务暂时不可用，请稍后再试');
      const body: unknown = await response.json();
      if (!Array.isArray(body)) throw new Error('搜索返回的内容无法识别');
      const found: SearchHit[] = body.flatMap(
        (item: { display_name?: unknown; name?: unknown; lat?: unknown; lon?: unknown }) => {
          const latitude = Number(item.lat),
            longitude = Number(item.lon);
          if (!Number.isFinite(latitude) || !Number.isFinite(longitude)) return [];
          const name =
            typeof item.name === 'string' && item.name.trim()
              ? item.name
              : String(item.display_name ?? '').split(',')[0];
          const detail = String(item.display_name ?? '');
          const position = parsePosition(
            String(latitude),
            String(longitude),
            String(selected.altitude),
          );
          // Trim floating-point tails before storing search results.
          return [
            {
              name: name || '未命名地点',
              detail: detail === name ? '' : detail,
              position: {
                ...position,
                latitude: tidy(position.latitude),
                longitude: tidy(position.longitude),
              },
            },
          ];
        },
      );
      setHits(found);
      if (!found.length) setNote('没有找到匹配的地点，可以直接输入经纬度。');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSearching(false);
    }
  }
  async function locateHere() {
    setError('');
    setNote('');
    try {
      const fix = await locate();
      centerOn(
        parsePosition(String(fix.latitude), String(fix.longitude), String(selected.altitude)),
      );
      setNote('已定位到当前位置');
    } catch (e) {
      setError(e instanceof Error ? e.message : '暂时取不到当前位置');
    }
  }
  function changeProvider(id: MapProviderId) {
    const next = { ...preferences, provider: id };
    setPreferences(next);
    saveMapPreferences(next);
    setFailed(false);
    setHits([]);
    setNote('');
    if (id === 'custom' && !preferences.customUrl.trim())
      setNote('请填写自定义图源地址，地址里需要 {z}、{x}、{y} 三个占位符。');
    else if (findMapProvider(id).unverified)
      setNote('这类图源用的是公开瓦片接口，可能随时变动；显示不出来时换个图源即可。');
  }
  function changeCustomUrl(url: string) {
    const next = { ...preferences, customUrl: url };
    setPreferences(next);
    saveMapPreferences(next);
  }
  function addPoint() {
    const last = points.at(-1);
    if (last && last.latitude === selected.latitude && last.longitude === selected.longitude) {
      setError('相邻路线点不能相同');
      return;
    }
    setPoints([...points, { ...selected, altitude: last?.altitude ?? 0 }]);
    setError('');
  }

  return createPortal(
    <dialog
      ref={dialog}
      className="map-screen full-screen"
      open={typeof HTMLDialogElement.prototype.showModal !== 'function'}
      aria-labelledby="map-title"
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
    >
      <header className="scope-topbar screen-topbar">
        <button
          className="icon-button"
          aria-label={route ? '返回路线编辑' : '返回位置编辑'}
          onClick={close}
        >
          <ArrowLeft />
        </button>
        <h1 id="map-title">{route ? '地图规划' : '地图选点'}</h1>
        {route ? (
          <button
            className="text-button"
            aria-label="完成路线"
            disabled={points.length < 2}
            onClick={() => {
              route.onConfirm(points);
              close();
            }}
          >
            完成
          </button>
        ) : (
          initial && (
            <button
              className="icon-button"
              aria-label="回到原位置"
              onClick={() => centerOn(initial)}
            >
              <RotateCcw size={20} />
            </button>
          )
        )}
      </header>
      <div className="map-viewport">
        <div ref={canvas} className="map-canvas" aria-label="选点地图" />
        <div className="map-search">
          <label className="search-field">
            <Search size={18} />
            <input
              aria-label="搜索地点"
              placeholder="搜索地点，或直接输入经纬度"
              value={query}
              disabled={!usable}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  void search();
                }
              }}
            />
          </label>
          <button
            className="tonal-button"
            disabled={!usable || searching}
            onClick={() => void search()}
          >
            {searching ? '搜索中' : '搜索'}
          </button>
        </div>
        <div className="map-provider">
          <label className="map-provider-field">
            <span className="eyebrow">图源</span>
            <select
              aria-label="地图图源"
              value={preferences.provider}
              onChange={(e) => changeProvider(e.target.value as MapProviderId)}
            >
              {mapProviders.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </select>
          </label>
          {preferences.provider === 'custom' && (
            <label className="map-provider-url">
              <input
                aria-label="自定义图源地址"
                placeholder="https://…/{z}/{x}/{y}.png"
                value={preferences.customUrl}
                onChange={(e) => changeCustomUrl(e.target.value)}
              />
            </label>
          )}
        </div>
        {!!hits.length && (
          <div className="map-results" role="list" aria-label="搜索结果">
            {hits.map((hit, index) => (
              <button
                type="button"
                role="listitem"
                key={index}
                onClick={() => {
                  centerOn(hit.position);
                  setHits([]);
                  setNote(`已定位到${hit.name}`);
                }}
              >
                <strong>{hit.name}</strong>
                {hit.detail && <small>{hit.detail}</small>}
              </button>
            ))}
          </div>
        )}
        <MapPin className="map-center-pin" size={38} aria-hidden="true" />
        <div className="map-tools">
          <button
            className="icon-button"
            aria-label="定位到当前位置"
            title="定位到当前位置"
            disabled={!usable}
            onClick={() => void locateHere()}
          >
            <Crosshair size={20} />
          </button>
        </div>
        {!usable && (
          <div className="map-error" role="status">
            <span>这个图源还没有可用地址，请选择其他图源或填写自定义地址。</span>
          </div>
        )}
        {failed && usable && (
          <div className="map-error" role="status">
            <span>地图加载失败，可以换个图源、返回填写坐标，或重试。</span>
            <button className="text-button" onClick={() => tilesRef.current?.redraw()}>
              重试
            </button>
          </div>
        )}
        {error && (
          <div className="map-error" role="alert">
            <span>{error}</span>
          </div>
        )}
        {note && (
          <div className="map-note" role="status">
            {note}
          </div>
        )}
      </div>
      <footer className="map-selection">
        <p>
          {route ? (
            <>
              <output aria-label="路线点数">{points.length} 个点</output> · 按点直线连接
            </>
          ) : (
            '拖动地图，将图钉对准目标位置'
          )}
        </p>
        <span aria-label="所选坐标">
          {selected.latitude.toFixed(6)}, {selected.longitude.toFixed(6)}
        </span>
        <span>
          WGS84 · {provider.label}
          {provider.crs === 'wgs84'
            ? ''
            : ` · 底图为 ${provider.crs === 'bd09' ? 'BD-09' : 'GCJ-02'}，已换算`}
        </span>
        {route ? (
          <div className="route-actions">
            <button className="primary" disabled={points.length >= 128} onClick={addPoint}>
              <Plus size={18} />
              添加到路线
            </button>
            <button
              className="icon-button"
              aria-label="撤销最后一个点"
              disabled={!points.length}
              onClick={() => {
                setPoints(points.slice(0, -1));
                setError('');
              }}
            >
              <Undo2 size={20} />
            </button>
          </div>
        ) : (
          <button
            className="primary"
            onClick={() => {
              onChoose!(selected);
              close();
            }}
          >
            使用此位置
          </button>
        )}
      </footer>
    </dialog>,
    document.body,
  );
}
