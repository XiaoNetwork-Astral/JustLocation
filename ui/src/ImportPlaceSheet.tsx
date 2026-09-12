import { useState } from 'react';
import { X } from 'lucide-react';
import { convertCoordinates, type CoordinateSystem } from './coordinates';
import { parseImportText, toPosition, type InlineLink } from './placeImport';
import type { Position } from './protocol';

type Preview = { label: string; note: string; position: Position; system: CoordinateSystem };

/* Try map links before coordinate text. */
export function ImportPlaceSheet({
  onClose,
  onSave,
}: {
  onClose(): void;
  onSave(p: Position, name: string): Promise<void> | void;
}) {
  const [text, setText] = useState('');
  const [preview, setPreview] = useState<Preview | null>(null);
  const [system, setSystem] = useState<CoordinateSystem>('wgs84');
  const [name, setName] = useState('');
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const altitude = 0;

  function analyse(value: string) {
    setText(value);
    setError('');
    setPreview(null);
    const trimmed = value.trim();
    if (!trimmed) return;
    try {
      const result = parseImportText(trimmed);
      if (result.kind === 'link') {
        const link: InlineLink = result.link;
        setSystem(link.coordinateSystem);
        setPreview({
          label: link.label,
          note: `来自${link.label}，按其坐标系 ${link.coordinateSystem === 'gcj02' ? 'GCJ-02' : link.coordinateSystem === 'bd09' ? 'BD-09' : 'WGS84'} 读入`,
          position: toPosition(link.position, altitude),
          system: link.coordinateSystem,
        });
        setName((previous) => previous || `来自${link.label}`);
      } else {
        setPreview({
          label: '经纬度',
          note: '按纬度在前、经度在后读入',
          position: toPosition(result.position, altitude),
          system: 'wgs84',
        });
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  /* Save WGS84 coordinates and show the selected input coordinate system. */
  const wgs84 = preview ? convertCoordinates(preview.position, preview.system, 'wgs84') : null;
  const shown = preview ? convertCoordinates(preview.position, preview.system, system) : null;

  async function save() {
    if (!wgs84 || !preview) {
      setError('请先粘贴可以直接识别的内容');
      return;
    }
    setSaving(true);
    setError('');
    try {
      await onSave(wgs84, name.trim() || preview.label);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <dialog
      className="sheet"
      open
      aria-labelledby="import-title"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <div className="dialog-heading">
        <h2 id="import-title">导入位置</h2>
        <button
          type="button"
          className="icon-button"
          aria-label="关闭导入位置"
          disabled={saving}
          onClick={onClose}
        >
          <X />
        </button>
      </div>
      <label className="field">
        粘贴地图链接或经纬度
        <textarea
          rows={3}
          value={text}
          disabled={saving}
          placeholder="https://… 或 31.2, 121.5"
          onChange={(e) => analyse(e.target.value)}
        />
      </label>
      {preview && wgs84 && shown && (
        <>
          <p role="status">{preview.note}</p>
          <label className="field">
            名称
            <input
              value={name}
              disabled={saving}
              maxLength={80}
              placeholder={preview.label}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label className="field">
            坐标类型
            <select
              value={system}
              disabled={saving}
              onChange={(e) => setSystem(e.target.value as CoordinateSystem)}
            >
              <option value="wgs84">WGS84</option>
              <option value="gcj02">GCJ-02</option>
              <option value="bd09">BD-09</option>
            </select>
          </label>
          <p>
            将保存为 WGS84：{wgs84.latitude.toFixed(6)}, {wgs84.longitude.toFixed(6)}
          </p>
          {system !== preview.system && (
            <p>
              按你选择的坐标系读入为 {shown.latitude.toFixed(6)}, {shown.longitude.toFixed(6)}
            </p>
          )}
        </>
      )}
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      <div className="dialog-actions">
        <button type="button" className="text-button" disabled={saving} onClick={onClose}>
          取消
        </button>
        <button
          type="button"
          className="primary"
          disabled={saving || !preview}
          onClick={() => void save()}
        >
          导入到历史位置
        </button>
      </div>
    </dialog>
  );
}
