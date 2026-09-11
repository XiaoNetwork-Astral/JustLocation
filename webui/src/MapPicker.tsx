import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ArrowLeft, MapPin, Plus, RotateCcw, Undo2 } from 'lucide-react';
import * as L from 'leaflet';
import 'leaflet/dist/leaflet.css';
import { parsePosition, type Position } from './control';

type Props = { onClose: () => void } & (
  { initial: Position | null; onChoose: (p: Position) => void; route?: never } |
  { initial?: never; onChoose?: never; route: { points: Position[]; onConfirm: (points: Position[]) => void } }
);

export function MapPicker({ initial: point, onChoose, onClose, route }: Props) {
  const initial = route ? route.points.at(-1) || null : point;
  const dialog = useRef<HTMLDialogElement>(null), canvas = useRef<HTMLDivElement>(null);
  const mapRef = useRef<L.Map | null>(null), tilesRef = useRef<L.TileLayer | null>(null);
  const routeLayer = useRef<L.LayerGroup | null>(null);
  const [points, setPoints] = useState(() => route?.points || []);
  const [error, setError] = useState('');
  const [selected, setSelected] = useState(() => initial || parsePosition('35', '105', '0'));
  const [failed, setFailed] = useState(false);
  const closeRef = useRef(onClose); closeRef.current = onClose;
  useEffect(() => {
    dialog.current?.showModal?.();
    const back = () => closeRef.current();
    window.addEventListener('popstate', back);
    const start = initial || parsePosition('35', '105', '0');
    const map = L.map(canvas.current!, { zoomControl: false, minZoom: 2, maxZoom: 19, worldCopyJump: true,
      maxBounds: [[-85, -Infinity], [85, Infinity]], maxBoundsViscosity: 1,
    }).setView([Math.max(-85, Math.min(85, start.latitude)), start.longitude], initial ? 16 : 4);
    mapRef.current = map;
    routeLayer.current = L.layerGroup().addTo(map);
    if (route && route.points.length > 1) map.fitBounds(L.latLngBounds(route.points.map(p => [p.latitude, p.longitude])), { padding: [30, 30], maxZoom: 16 });
    const tiles = L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', { maxZoom: 19,
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">OpenStreetMap</a> contributors',
      referrerPolicy: 'strict-origin-when-cross-origin',
    }).addTo(map);
    tilesRef.current = tiles;
    tiles.on('loading', () => setFailed(false));
    tiles.on('tileerror', () => setFailed(true));
    L.control.zoom({ position: 'topright', zoomInTitle: '放大', zoomOutTitle: '缩小' }).addTo(map);
    const update = () => {
      const center = map.getCenter().wrap();
      setSelected({ ...start, latitude: center.lat, longitude: center.lng });
    };
    update();
    map.on('moveend', update);
    map.on('click', (event: L.LeafletMouseEvent) => map.panTo(event.latlng, { animate: false }));
    const observer = new ResizeObserver(() => map.invalidateSize({ animate: false })); observer.observe(canvas.current!);
    return () => { observer.disconnect(); window.removeEventListener('popstate', back); map.remove(); mapRef.current = null; tilesRef.current = null; routeLayer.current = null; };
  }, []);
  useEffect(() => {
    const layer = routeLayer.current;
    if (!layer || !route) return;
    layer.clearLayers();
    L.polyline(points.map(p => [p.latitude, p.longitude] as L.LatLngTuple), { color: '#526836', weight: 4 }).addTo(layer);
    points.forEach((p, index) => L.marker([p.latitude, p.longitude], {
      icon: L.divIcon({ className: 'route-map-marker', html: String(index + 1), iconSize: [26, 26], iconAnchor: [13, 13] }),
      title: `点 ${index + 1}`,
    }).on('click', () => mapRef.current?.panTo([p.latitude, p.longitude], { animate: false })).addTo(layer));
  }, [points]);
  function close() { if (window.history.state?.justlocationMap) window.history.back(); else onClose(); }
  function addPoint() {
    const last = points.at(-1);
    if (last && last.latitude === selected.latitude && last.longitude === selected.longitude) { setError('相邻路线点不能相同'); return; }
    setPoints([...points, { ...selected, altitude: last?.altitude ?? 0 }]); setError('');
  }
  return createPortal(<dialog ref={dialog} className="map-screen" open={typeof HTMLDialogElement.prototype.showModal !== 'function'} aria-labelledby="map-title" onCancel={e => { e.preventDefault(); close(); }}>
    <header className="scope-topbar"><button className="icon-button" aria-label={route ? '返回路线编辑' : '返回位置编辑'} onClick={close}><ArrowLeft /></button><h1 id="map-title">{route ? '地图规划' : '地图选点'}</h1>
      {route ? <button className="text-button" aria-label="完成路线" disabled={points.length < 2} onClick={() => { route.onConfirm(points); close(); }}>完成</button> : initial && <button className="icon-button" aria-label="回到原位置" onClick={() => mapRef.current?.setView([Math.max(-85, Math.min(85, initial.latitude)), initial.longitude], 16)}><RotateCcw size={20} /></button>}</header>
    <div className="map-viewport"><div ref={canvas} className="map-canvas" aria-label="选点地图" />
      <MapPin className="map-center-pin" size={38} aria-hidden="true" />
      {failed && <div className="map-error" role="status"><span>地图加载失败，可以返回填写坐标。</span><button className="text-button" onClick={() => tilesRef.current?.redraw()}>重试</button></div>}
    </div>
    <footer className="map-selection"><p>{route ? <><output aria-label="路线点数">{points.length} 个点</output> · 按点直线连接</> : '拖动地图，将图钉对准目标位置'}</p><output aria-label="所选坐标">{selected.latitude.toFixed(6)}, {selected.longitude.toFixed(6)}</output><span>{route ? 'WGS84 · 新增点沿用上一点海拔' : 'WGS84 · 海拔沿用输入值'}</span>
      {error && <p role="alert" className="form-error">{error}</p>}
      {route ? <div className="route-actions"><button className="primary" disabled={points.length >= 128} onClick={addPoint}><Plus size={18} />添加到路线</button><button className="icon-button" aria-label="撤销最后一个点" disabled={!points.length} onClick={() => { setPoints(points.slice(0, -1)); setError(''); }}><Undo2 size={20} /></button></div> : <button className="primary" onClick={() => { onChoose!(selected); close(); }}>使用此位置</button>}</footer>
  </dialog>, document.body);
}
