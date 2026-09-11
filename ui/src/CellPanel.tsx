import { useEffect, useRef, useState } from 'react';
import { ArrowLeft, Download, RefreshCw, Upload } from 'lucide-react';
import { cellClient, safeCellLink, type CellClient, type CellResponse, type CellSettings, type CellSettingsUpdate, type Coordinate, type ProviderKind } from './cells';
import { saveFile } from './fileExport';
import { SwitchRow } from './Controls';
import { TelephonySettings } from './TelephonySettings';
import type { State, TelephonyConfig } from './control';
import type { CellRegion } from './cells';

const providers: { value: ProviderKind; label: string }[] = [
  { value: 'open_cell_id', label: 'OpenCellID' }, { value: 'custom', label: '自定义' },
];
const availabilityText: Record<string, string> = {
  disabled: '基站模拟未启用',
  missing_region: '还没有应用附近基站数据',
  outside_region: '当前位置超出已应用的数据范围',
  ready: '基站模拟已生效',
};

/**
 * 基站模拟面板。
 *
 * 以前这里是"一堆表单 + 一个保存按钮"：开关是普通复选框、要保存才生效，
 * 结果区和设置混在一起。现在按卡片分组：目标位置 / 总开关 / 运营商与 SIM /
 * 查询 / 数据，开关直接写后台配置，查到的数据仍然要点"应用"才用于模拟。
 */
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
  const [radius, setRadius] = useState(() => String(state?.telephony?.radius_m ?? 500));
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
    if (!target) { setError('先在位置模拟里选一个位置，再来查询基站'); return; }
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
      setSettings(response.settings!); setSavedSettings(JSON.stringify(response.settings)); setKey(undefined); setToken(undefined); setResult(null); setNote('数据来源已保存');
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
  const telephony = state?.telephony;
  const cellsEnabled = !!telephony?.cells_enabled;
  /** 总开关直接写后台配置，不再需要"保存设置"才生效。 */
  async function toggleCells(next: boolean) {
    if (!telephony || !onConfigure) return;
    await work(async () => {
      await onConfigure({ ...telephony, cells_enabled: next });
      setNote(next ? '基站模拟已启用' : '基站模拟已停用');
    });
  }
  return <dialog ref={dialog} className="cell-panel full-screen" open={typeof HTMLDialogElement.prototype.showModal !== 'function'} onCancel={onClose} aria-labelledby="cell-title">
    <header className="cell-heading screen-topbar"><button className="icon-button" aria-label="返回位置模拟" onClick={onClose}><ArrowLeft /></button><h2 id="cell-title">基站模拟</h2></header>
    <div className="cell-content">
      <section className="card" aria-label="目标位置">
        <h2>目标位置</h2>
        <p>{target ? `${target.latitude.toFixed(6)}, ${target.longitude.toFixed(6)} · WGS84` : '还没有选择位置，先回位置模拟选一个点。'}</p>
      </section>

      <section className="card" aria-label="基站模拟开关">
        <SwitchRow title="启用基站模拟" checked={cellsEnabled} disabled={controlBusy || busy || !onConfigure || !telephony}
          summary={!telephony ? '读取当前状态中' : cellsEnabled
            ? state?.requested_active ? (state.cell_hook_ready ? '已连接基站服务' : '等待基站服务连接') : '已启用，开始位置模拟后生效'
            : '使用目标位置附近的基站数据'}
          onChange={next => void toggleCells(next)} />
        {cellsEnabled && state?.requested_active && state.telephony_output &&
          <p className="cell-state" role="status">{availabilityText[state.telephony_output.availability] ?? '基站模拟状态未知'}</p>}
      </section>

      {state && onConfigure && <section className="card" aria-label="运营商设置">
        <h2>运营商与 SIM</h2>
        <TelephonySettings state={state} busy={controlBusy} onSave={onConfigure} />
      </section>}

      <section className="card" aria-label="查询附近基站">
        <h2>查询附近基站</h2>
        <p>按目标位置查这一带的基站，查到之后要点“应用这份基站数据”才会用于模拟。</p>
        <div className="cell-query">
          <label className="field">查询半径（米）<input inputMode="numeric" value={radius} disabled={busy} onChange={e => { setRadius(e.target.value); setResult(null); }} /></label>
          <label className="choice"><input type="checkbox" checked={offline} disabled={busy} onChange={e => setOffline(e.target.checked)} />只使用已导入的离线数据</label>
          <div className="route-actions">
            {/* 缺少位置或数据来源时按钮仍可点：点一下会说明到底缺什么，比灰着不给理由更有用。 */}
            <button className="primary" disabled={busy || dirty} onClick={() => void query()}>{busy ? '处理中…' : '查询附近基站'}</button>
            {!offline && <button className="tonal-button" disabled={busy || dirty || !target || !settings} onClick={() => void query(true)}><RefreshCw size={18} />联网刷新</button>}
          </div>
          {!target && <p role="status">还没有目标位置，先在位置模拟里选一个点。</p>}
        </div>
        {dirty && <p role="status">数据来源设置有修改，保存后再查询。</p>}
      </section>

      <section className="card" aria-label="基站数据">
        <h2>基站数据</h2>
        {data ? <>
          <p>{result?.cached ? '离线数据' : '在线查询'} · {new Date(data.region.fetched_at_ms).toLocaleString()}{result?.stale ? ' · 数据较旧' : ''}</p>
          {data.incomplete && <p role="status">结果未查全，部分数据可能缺失。可缩小范围后重新查询。</p>}
          {data.failures.length > 0 && <p>首选供应商未能完成查询，已使用备用供应商。</p>}
          {onApply && <button className="primary" disabled={busy || controlBusy || dirty} onClick={() => void work(async () => {
            await onApply(data.region); setNote('基站数据已应用');
          })}>应用这份基站数据</button>}
          {data.region.cells.length ? <ul className="cell-list">{data.region.cells.map((cell, index) => <li key={index}>
            <strong>{cell.identity.radio.toUpperCase()} · {cell.identity.mcc ? `${cell.identity.mcc}-${cell.identity.mnc}` : `SID ${cell.identity.sid}`}</strong>
            <span>基站 {cell.identity.nci ?? cell.identity.ci ?? cell.identity.cid ?? cell.identity.bid}</span>
            <small>{cell.position.latitude.toFixed(6)}, {cell.position.longitude.toFixed(6)}</small>
          </li>)}</ul> : <p>{data.incomplete ? '未取得可用的基站记录' : '这个范围内没有基站数据'}</p>}
          <p className="cell-credit"><a href={safeCellLink(data.attribution.source)} target="_blank" rel="noreferrer">{data.attribution.text}</a>{data.attribution.license && <> · <a href={safeCellLink(data.attribution.license)} target="_blank" rel="noreferrer">数据许可</a></>}{data.attribution.changes && <small>{data.attribution.changes}</small>}</p>
        </> : <p>还没有查询结果。上面的按钮会按目标位置取这一带的基站，也可以直接导入离线数据。</p>}
        <div className="route-actions">
          <label className={`text-button import-button${busy ? ' disabled' : ''}`}><Upload size={18} />导入离线数据<input type="file" aria-label="导入离线基站数据" accept=".json,application/json" disabled={busy} onChange={e => { const file = e.target.files?.[0]; if (file) void importFile(file); e.target.value = ''; }} /></label>
          <button className="text-button" disabled={busy || !data} onClick={() => void work(async () => setNote(await saveFile(JSON.stringify(data), 'json')))}><Download size={18} />导出当前数据</button>
        </div>
      </section>

      {settings && <details className="card cell-settings"><summary>数据来源设置</summary>
        <label className="field">首选供应商<select disabled={busy} value={settings.primary} onChange={e => setSettings({ ...settings, primary: e.target.value as ProviderKind })}>{providers.map(p => <option key={p.value} value={p.value}>{p.label}</option>)}</select></label>
        <label className="field">备用供应商<select disabled={busy} value={settings.fallback || ''} onChange={e => setSettings({ ...settings, fallback: e.target.value as ProviderKind || null })}><option value="">不使用备用</option>{providers.map(p => <option key={p.value} value={p.value} disabled={p.value === settings.primary}>{p.label}</option>)}</select></label>
        <label className="field">OpenCellID API Key<input type="password" autoComplete="off" disabled={busy} value={key ?? ''} placeholder={key === '' ? '保存后清除' : settings.opencellid_configured ? '已保存，留空不修改' : '填写自己的 API Key'} onChange={e => setKey(e.target.value || undefined)} /></label>
        {settings.opencellid_configured && <button className="text-button" disabled={busy} onClick={() => setKey('')}>清除已保存的 API Key</button>}
        {(settings.primary === 'custom' || settings.fallback === 'custom') && <>
          <label className="field">接口地址<input type="url" disabled={busy} value={settings.custom_endpoint} placeholder="https://example.com/cells" onChange={e => { setSettings({ ...settings, custom_endpoint: e.target.value, custom_token_configured: false }); setToken(''); }} /></label>
          <label className="field">访问令牌（可选）<input type="password" autoComplete="off" disabled={busy} value={token ?? ''} placeholder={token === '' ? '保存后清除' : settings.custom_token_configured ? '已保存，留空不修改' : '填写访问令牌'} onChange={e => setToken(e.target.value || undefined)} /></label>
          {settings.custom_token_configured && <button className="text-button" disabled={busy} onClick={() => setToken('')}>清除已保存的访问令牌</button>}
        </>}
        <button className="tonal-button" disabled={busy || !dirty} onClick={() => void saveSettings()}>保存数据来源</button>
      </details>}

      {error && <p className="form-error" role="alert">{error}</p>}
      {note && <p role="status">{note}</p>}
    </div>
  </dialog>;
}
