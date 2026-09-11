import { useState } from 'react';
import { Download, Pencil, Save, Trash2 } from 'lucide-react';
import type { RoutePlan } from './control';
import { parseDraft, planDraft } from './routeDraft';
import { exportGpx } from './routeImport';
import { saveFile } from './fileExport';
import { routesKey as key, type SavedRoute } from './backup';

function readRoutes(): SavedRoute[] {
  try {
    const entries = JSON.parse(localStorage.getItem(key) || '[]');
    if (!Array.isArray(entries)) return [];
    return entries.flatMap(entry => {
      try {
        if (typeof entry?.id !== 'string' || typeof entry.name !== 'string' || !entry.name.trim()) return [];
        return [{ id: entry.id, name: entry.name, plan: parseDraft(planDraft(entry.plan)) }];
      } catch { return []; }
    });
  } catch { return []; }
}

export function RouteLibrary({ disabled, getPlan, onUse, onEdit }: {
  disabled: boolean; getPlan: () => RoutePlan; onUse: (plan: RoutePlan) => void; onEdit?: (plan: RoutePlan, name: string) => void;
}) {
  const [routes, setRoutes] = useState(readRoutes);
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState('');
  const [error, setError] = useState('');
  const [note, setNote] = useState('');
  const [exporting, setExporting] = useState(false);
  const [open, setOpen] = useState(false);
  const [deleted, setDeleted] = useState<{ route: SavedRoute; index: number } | null>(null);
  function write(next: SavedRoute[]) {
    try { localStorage.setItem(key, JSON.stringify(next)); }
    catch { throw new Error('路线保存失败，请检查面板的存储空间'); }
    setRoutes(next);
    setError('');
  }
  function save() {
    try {
      const title = name.trim();
      if (!title) throw new Error('请输入路线名称');
      if (routes.some(route => route.name === title)) throw new Error('已有同名路线，请换个名称');
      write([...routes, { id: crypto.randomUUID(), name: title, plan: getPlan() }]);
      setNaming(false); setName(''); setOpen(true); setDeleted(null);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  return <div className="route-library">
    <button className="text-button" disabled={disabled} onClick={() => { setNaming(true); setError(''); }}><Save size={18} />保存路线</button>
    {naming && <form className="route-name" onSubmit={e => { e.preventDefault(); save(); }}>
      <label className="field">路线名称<input autoFocus maxLength={80} value={name} disabled={disabled} onChange={e => setName(e.target.value)} /></label>
      <div className="route-actions"><button className="primary" disabled={disabled}>保存</button><button type="button" className="text-button" onClick={() => { setNaming(false); setError(''); }}>取消</button></div>
    </form>}
    {error && <p role="alert" className="form-error">{error}</p>}
    {note && <p role="status">{note}</p>}
    {!!routes.length && <details open={open} onToggle={e => setOpen(e.currentTarget.open)}>
      <summary>已保存的路线（{routes.length}）</summary>
      <div className="saved-routes">{routes.map((route, index) => <div className="saved-route" key={route.id}>
        <button className="saved-route-use" aria-label={`使用${route.name}`} disabled={disabled} onClick={() => { onUse(route.plan); setError(''); }}>
          <strong>{route.name}</strong><span>{route.plan.points.length} 个点 · {Number((route.plan.speed * 3.6).toFixed(2))} km/h · {route.plan.repeat_count ?? 1} 次</span>
        </button>
        <button className="icon-button" aria-label={`导出${route.name}为 GPX`} title="导出 GPX" disabled={exporting} onClick={async () => {
          setExporting(true); setError(''); setNote('');
          try { setNote(await saveFile(exportGpx(route.name, route.plan), 'gpx')); }
          catch (e) { setError(e instanceof Error ? e.message : String(e)); }
          finally { setExporting(false); }
        }}><Download size={18} /></button>
        {onEdit && <button className="icon-button" aria-label={`编辑${route.name}`} title="编辑路线点" disabled={disabled}
          onClick={() => onEdit(route.plan, route.name)}><Pencil size={18} /></button>}
        <button className="icon-button" aria-label={`删除${route.name}`} disabled={disabled} onClick={() => {
          try { write(routes.filter(item => item.id !== route.id)); setDeleted({ route, index }); }
          catch (e) { setError(e instanceof Error ? e.message : String(e)); }
        }}><Trash2 size={18} /></button>
      </div>)}</div>
    </details>}
    {deleted && <div className="route-undo" role="status"><span>已删除“{deleted.route.name}”</span><button className="text-button" disabled={disabled} onClick={() => {
      try { const next = [...routes]; next.splice(deleted.index, 0, deleted.route); write(next); setDeleted(null); setOpen(true); }
      catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    }}>撤销</button></div>}
  </div>;
}
