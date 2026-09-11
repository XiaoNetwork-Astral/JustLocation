import { useEffect, useRef, useState } from 'react';
import { ArrowLeft, Download, Upload } from 'lucide-react';
import { cellClient, safeCellLink, type CellClient, type CellResponse, type CellSettings, type CellSettingsUpdate, type Coordinate, type ProviderKind } from './cells';
import { saveFile } from './fileExport';
import { TelephonySettings } from './TelephonySettings';
import type { State, TelephonyConfig } from './control';
import type { CellRegion } from './cells';

const providers: { value: ProviderKind; label: string }[] = [
  { value: 'open_cell_id', label: 'OpenCellID' }, { value: 'fake_location', label: 'Fake Location（待接入）' }, { value: 'custom', label: '自定义' },
];
export function CellPanel({ target, client = cellClient, onClose, state, controlBusy = false, onConfigure, onApply }: {
  target: Coordinate | null; client?: CellClient; onClose(): void; state?: State | null; controlBusy?: boolean;
  onConfigure?(config: TelephonyConfig): Promise<void>; onApply?(region: CellRegion): Promise<void>;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const generation = useRef(0);
  const [settings, setSettings] = useState<CellSettings | null>(null);
  const [savedSettings, setSavedSettings] = useState('');
  const [key, setKey] = useState<string | undefined>();
  const [token, setToken] = useState<string | undefined>();
  const [radius, setRadius] = useState('500');
  const [offline, setOffline] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<CellResponse | null>(null);
  const [error, setError] = useState('');
  const [note, setNote] = useState('');
  useEffect(() => { dialog.current?.showModal?.(); }, []);
  useEffect(() => {
    let alive = true;
    void client({ op: 'settings' }).then(r => { if (alive) { setSettings(r.settings!); setSavedSettings(JSON.stringify(r.settings)); } }).catch(e => { if (alive) setError(String(e.message || e)); });
    return () => { alive = false; };
  }, [client]);
  useEffect(() => {
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); onClose(); } };
    document.addEventListener('keydown', escape);
    return () => document.removeEventListener('keydown', escape);
  }, [onClose]);
  useEffect(() => {
    generation.current++; setResult(null); setBusy(false); setNote('');
    return () => { generation.current++; };
  }, [target?.latitude, target?.longitude]);
  async function work(action: () => Promise<void>) {
    setBusy(true); setError(''); setNote('');
    try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  async function query(refresh = false) {
    if (!target) return;
    const radius_m = Number(radius);
    if (!radius.trim() || !Number.isFinite(radius_m) || radius_m < 1 || radius_m > (offline || settings?.primary === 'custom' ? 200000 : 5000)) {
      setError('请填写有效的查询半径，OpenCellID 在线查询最多 5000 米'); return;
    }
    const current = ++generation.current;
    setBusy(true); setError(''); setNote(''); setResult(null);
    try {
      const response = await client({ op: 'query', area: { target: { ...target }, radius_m }, mode: offline ? 'offline' : refresh ? 'refresh' : 'prefer_cache' });
      if (generation.current === current) { setResult(response); setNote(response.warning || ''); }
    } catch (e) { if (generation.current === current) setError(e instanceof Error ? e.message : String(e)); }
    finally { if (generation.current === current) setBusy(false); }
  }
  async function saveSettings() {
    if (!settings) return;
    await work(async () => {
      const update: CellSettingsUpdate = { primary: settings.primary, fallback: settings.fallback, custom_endpoint: settings.custom_endpoint };
      if (key !== undefined) update.opencellid_key = key;
      if (token !== undefined) update.custom_token = token;
      const response = await client({ op: 'configure', settings: update });
      setSettings(response.settings!); setSavedSettings(JSON.stringify(response.settings)); setKey(undefined); setToken(undefined); setResult(null); setNote('供应商已保存');
    });
  }
  async function importFile(file: File) {
    await work(async () => {
      if (file.size > 64000) throw new Error('请选择 64 KB 以内的区域基站 JSON 文件');
      const text = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(new Error('文件读取失败')); reader.readAsText(file);
      });
      await client({ op: 'import', dataset: JSON.parse(text) }); setResult(null); setNote('离线数据已导入，可以查询了');
    });
  }
  const data = result?.dataset;
  const dirty = !!settings && (JSON.stringify(settings) !== savedSettings || key !== undefined || token !== undefined);
  return <dialog ref={dialog} className="cell-panel" open={typeof HTMLDialogElement.prototype.showModal !== 'function'} onCancel={onClose} aria-labelledby="cell-title">
    <header className="cell-heading"><button className="icon-button" aria-label="返回位置模拟" onClick={onClose}><ArrowLeft /></button><h2 id="cell-title">附近基站</h2></header>
    <div className="cell-content">
      <p>{target ? `${target.latitude.toFixed(6)}, ${target.longitude.toFixed(6)} · WGS84` : '先在位置模拟中选择一个位置。'}</p>
      {state && onConfigure && <TelephonySettings state={state} busy={controlBusy} onSave={onConfigure} />}
      {settings && <details className="cell-settings"><summary>供应商设置</summary>
        <label className="field">首选供应商<select disabled={busy} value={settings.primary} onChange={e => setSettings({ ...settings, primary: e.target.value as ProviderKind })}>{providers.map(p => <option key={p.value} value={p.value} disabled={p.value === 'fake_location'}>{p.label}</option>)}</select></label>
        <label className="field">备用供应商<select disabled={busy} value={settings.fallback || ''} onChange={e => setSettings({ ...settings, fallback: e.target.value as ProviderKind || null })}><option value="">不使用备用</option>{providers.map(p => <option key={p.value} value={p.value} disabled={p.value === 'fake_location' || p.value === settings.primary}>{p.label}</option>)}</select></label>
        <label className="field">OpenCellID API Key<input type="password" autoComplete="off" disabled={busy} value={key ?? ''} placeholder={key === '' ? '保存后清除' : settings.opencellid_configured ? '已保存，留空不修改' : '填写自己的 API Key'} onChange={e => setKey(e.target.value || undefined)} /></label>
        {settings.opencellid_configured && <button className="text-button" disabled={busy} onClick={() => setKey('')}>清除已保存的 API Key</button>}
        {(settings.primary === 'custom' || settings.fallback === 'custom') && <>
          <label className="field">接口地址<input type="url" disabled={busy} value={settings.custom_endpoint} placeholder="https://example.com/cells" onChange={e => { setSettings({ ...settings, custom_endpoint: e.target.value, custom_token_configured: false }); setToken(''); }} /></label>
          <label className="field">访问令牌（可选）<input type="password" autoComplete="off" disabled={busy} value={token ?? ''} placeholder={token === '' ? '保存后清除' : settings.custom_token_configured ? '已保存，留空不修改' : '填写访问令牌'} onChange={e => setToken(e.target.value || undefined)} /></label>
          {settings.custom_token_configured && <button className="text-button" disabled={busy} onClick={() => setToken('')}>清除已保存的访问令牌</button>}
        </>}
        {settings.fallback === 'fake_location' && <p>Fake Location 尚未接通，当前不会自动切换到它。</p>}
        <button className="text-button" disabled={busy} onClick={() => void saveSettings()}>保存供应商</button>
      </details>}
      {dirty && <p role="status">供应商设置已修改，保存后再查询。</p>}
      <div className="cell-query"><label className="field">查询半径（米）<input inputMode="numeric" value={radius} disabled={busy} onChange={e => { setRadius(e.target.value); setResult(null); }} /></label>
        <label className="choice"><input type="checkbox" checked={offline} disabled={busy} onChange={e => setOffline(e.target.checked)} />仅使用离线数据</label>
        <div className="route-actions"><button className="primary" disabled={busy || dirty || !target || !settings} onClick={() => void query()}>{busy ? '处理中…' : '查询附近基站'}</button>
          {!offline && <button className="text-button" disabled={busy || dirty || !target || !settings} onClick={() => void query(true)}>联网刷新</button>}</div>
      </div>
      {error && <p className="form-error" role="alert">{error}</p>}{note && <p role="status">{note}</p>}
      {data ? <section aria-label="基站查询结果">
        <p>{result?.cached ? '离线数据' : '在线查询'} · {new Date(data.region.fetched_at_ms).toLocaleString()}{result?.stale ? ' · 数据较旧' : ''}</p>
        {data.incomplete && <p role="status">结果未查全，部分数据可能缺失。可缩小范围后重新查询。</p>}
        {data.failures.length > 0 && <p>首选供应商未能完成查询，已使用备用供应商。</p>}
        {onApply && <button className="primary" disabled={busy || controlBusy || dirty} onClick={() => void work(async () => {
          await onApply(data.region); setNote('基站数据已应用');
        })}>应用这份基站数据</button>}
        <p className="cell-credit"><a href={safeCellLink(data.attribution.source)} target="_blank" rel="noreferrer">{data.attribution.text}</a>{data.attribution.license && <> · <a href={safeCellLink(data.attribution.license)} target="_blank" rel="noreferrer">数据许可</a></>}{data.attribution.changes && <small>{data.attribution.changes}</small>}</p>
        {data.region.cells.length ? <ul className="cell-list">{data.region.cells.map((cell, index) => <li key={index}><strong>{cell.identity.radio.toUpperCase()} · {cell.identity.mcc ? `${cell.identity.mcc}-${cell.identity.mnc}` : `SID ${cell.identity.sid}`}</strong><span>基站 {cell.identity.nci ?? cell.identity.ci ?? cell.identity.cid ?? cell.identity.bid}</span><small>{cell.position.latitude.toFixed(6)}, {cell.position.longitude.toFixed(6)}</small></li>)}</ul> : <p>{data.incomplete ? '未取得可用的基站记录' : '这个范围内没有基站数据'}</p>}
      </section> : <p>还没有基站数据</p>}
      <div className="route-actions"><label className={`text-button import-button${busy ? ' disabled' : ''}`}><Upload size={18} />导入离线数据<input type="file" accept=".json,application/json" disabled={busy} onChange={e => { const file = e.target.files?.[0]; if (file) void importFile(file); e.target.value = ''; }} /></label>
        <button className="text-button" disabled={busy || !data} onClick={() => void work(async () => setNote(await saveFile(JSON.stringify(data), 'json')))}><Download size={18} />导出当前数据</button>
      </div>
    </div>
  </dialog>;
}
