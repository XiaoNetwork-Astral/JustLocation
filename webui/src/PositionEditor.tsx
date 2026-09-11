import { useEffect, useRef, useState } from 'react';
import { MapPin, X } from 'lucide-react';
import { parsePosition, type Position } from './control';
import { convertCoordinates, type CoordinateSystem } from './coordinates';
import { MapPicker } from './MapPicker';

export function PositionEditor({ initial, name, onClose, onSave }: { initial: Position | null; name: string; onClose: () => void; onSave: (p: Position, name: string) => Promise<void> }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [error, setError] = useState(''), [saving, setSaving] = useState(false);
  const [label, setLabel] = useState(name);
  const [latitude, setLatitude] = useState(String(initial?.latitude ?? ''));
  const [longitude, setLongitude] = useState(String(initial?.longitude ?? ''));
  const [altitude, setAltitude] = useState(String(initial?.altitude ?? 0));
  const [system, setSystem] = useState<CoordinateSystem>('wgs84');
  const [map, setMap] = useState<{ initial: Position | null } | null>(null);
  useEffect(() => { dialog.current?.showModal?.(); }, []);
  const read = () => convertCoordinates(parsePosition(latitude, longitude, altitude), system, 'wgs84');
  function openMap() {
    try {
      const point = !latitude.trim() && !longitude.trim() ? null : read();
      window.history.pushState({ justlocationMap: true }, '');
      setMap({ initial: point }); setError('');
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  return <><dialog ref={dialog} className="editor" open={typeof HTMLDialogElement.prototype.showModal !== 'function'} onCancel={onClose} aria-labelledby="editor-title">
    <form onSubmit={async e => {
      e.preventDefault();
      try { const p = read(); setSaving(true); await onSave(p, label.trim() || '自定义位置'); }
      catch (error) { setError(error instanceof Error ? error.message : String(error)); }
      finally { setSaving(false); }
    }}><div className="dialog-heading"><h2 id="editor-title">添加位置</h2><button type="button" className="icon-button" aria-label="关闭位置编辑" disabled={saving} onClick={onClose}><X /></button></div>
      <button type="button" className="map-entry text-button" disabled={saving} onClick={openMap}><MapPin size={20} />地图选点</button>
      <label className="field">位置名称<input name="name" value={label} onChange={e => setLabel(e.target.value)} placeholder="给这个地方起个名字" autoFocus /></label>
      <label className="field">坐标系<select value={system} onChange={e => setSystem(e.target.value as CoordinateSystem)}>
        <option value="wgs84">WGS84</option><option value="gcj02">GCJ-02</option><option value="bd09">BD-09</option></select></label>
      <p>{system === 'wgs84' ? '填写 WGS84 坐标，或从地图上选择。' : '请选择输入坐标本身的类型，保存时会转换为 WGS84。'}</p>
      <div className="coordinate-fields"><label className="field">纬度<input name="latitude" inputMode="decimal" value={latitude} onChange={e => setLatitude(e.target.value)} placeholder="−90 ～ 90" /></label>
        <label className="field">经度<input name="longitude" inputMode="decimal" value={longitude} onChange={e => setLongitude(e.target.value)} placeholder="−180 ～ 180" /></label></div>
      <label className="field">海拔（米）<input name="altitude" inputMode="decimal" value={altitude} onChange={e => setAltitude(e.target.value)} /></label>
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="dialog-actions"><button className="text-button" type="button" disabled={saving} onClick={onClose}>取消</button><button className="primary" disabled={saving}>保存位置</button></div>
    </form>
  </dialog>{map && <MapPicker initial={map.initial} onClose={() => setMap(null)} onChoose={p => {
    setLatitude(String(p.latitude)); setLongitude(String(p.longitude)); setSystem('wgs84'); setError('');
  }} />}</>;
}
