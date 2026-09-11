import { useEffect, useState } from 'react';
import { Pencil, Plus, Trash2, Wifi } from 'lucide-react';
import { SwitchRow } from './Controls';
import { createWifi, looksLikeBssid, type SavedWifi } from './wifis';
import type { State, WifiConfig } from './control';

type Draft = { id: string | null; ssid: string; bssid: string };

/**
 * Wi-Fi 模拟页。
 *
 * 规格第 7.1 节：从附近列表选择或手工填写名称与接入点地址，保存后在历史列表切换，
 * 支持编辑、删除和撤销删除。附近列表需要系统扫描结果，尚未接入。
 *
 * 保存的网络现在同时写进后台配置（`set_wifi`），这样系统侧才有数据可读——
 * 与卫星通道一样，"界面能改"和"后端能读"必须一致，否则开关只是摆设。
 * 真正的输出（WifiInfo / ScanResult 替换）仍未实现，页面里明确写清。
 */
export function WifiPanel({ state, onConfigure }: { state: State | null; onConfigure(config: WifiConfig): Promise<void> }) {
  const configured = state?.wifi;
  const [items, setItems] = useState<SavedWifi[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [deleted, setDeleted] = useState<{ wifi: SavedWifi; index: number } | null>(null);
  const [error, setError] = useState('');
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);

  // 后台返回的目标列表是权威来源：面板重开或换设备后都按它显示。
  useEffect(() => {
    if (loaded || !configured) return;
    setItems(configured.targets.map(target => ({
      id: target.id, ssid: target.ssid, bssid: target.bssid,
      rssi: target.rssi, linkSpeed: target.link_speed, frequency: target.frequency,
    })));
    setLoaded(true);
  }, [configured, loaded]);

  /** 本地列表与后台配置必须一起更新：先成功写入后台，再更新界面。 */
  async function write(next: SavedWifi[], patch: Partial<WifiConfig> = {}) {
    const config: WifiConfig = {
      enabled: patch.enabled ?? configured?.enabled ?? false,
      targets: next.map(item => ({ id: item.id, ssid: item.ssid, bssid: item.bssid, rssi: item.rssi, link_speed: item.linkSpeed, frequency: item.frequency })),
    };
    setBusy(true); setError('');
    try {
      await onConfigure(config);
      setItems(next);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); throw e; }
    finally { setBusy(false); }
  }
  async function toggle(next: boolean) {
    const config: WifiConfig = {
      enabled: next,
      targets: items.map(item => ({ id: item.id, ssid: item.ssid, bssid: item.bssid, rssi: item.rssi, link_speed: item.linkSpeed, frequency: item.frequency })),
    };
    setBusy(true); setError(''); setNote('');
    try {
      await onConfigure(config);
      if (next && !items.length) setNote('模拟已启用，但还没有保存任何网络；应用会读到系统当前结果。');
      else setNote(next ? 'Wi-Fi 模拟已启用' : 'Wi-Fi 模拟已停用');
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  function startAdd() { setDraft({ id: null, ssid: '', bssid: '' }); setError(''); setNote(''); }
  function startEdit(wifi: SavedWifi) { setDraft({ id: wifi.id, ssid: wifi.ssid, bssid: wifi.bssid }); setError(''); setNote(''); }
  async function submit() {
    if (!draft) return;
    try {
      const existing = draft.id ? items.find(item => item.id === draft.id) : undefined;
      // 已保存的条目保留原有信号取值，编辑名称或地址不该把它们重置。
      const wifi = existing ? { ...existing, ssid: draft.ssid.trim(), bssid: draft.bssid.trim() } : createWifi(draft.ssid, draft.bssid);
      await write(draft.id ? items.map(item => item.id === draft.id ? wifi : item) : [...items, wifi]);
      setNote(draft.id ? '已更新这条网络' : '已保存，之后可以在列表里切换');
      setDraft(null);
    } catch { /* 错误已经显示在 write 里 */ }
  }
  async function remove(wifi: SavedWifi) {
    const index = items.findIndex(item => item.id === wifi.id);
    try { await write(items.filter(item => item.id !== wifi.id)); setDeleted({ wifi, index }); setNote(''); }
    catch { /* 错误已经显示 */ }
  }
  const hint = draft && draft.bssid && !looksLikeBssid(draft.bssid) ? '这串看起来不像接入点地址，仍然可以保存。' : '';
  const enabled = !!configured?.enabled;

  return <>
    <section className="card" aria-label="Wi-Fi 模拟">
      <h2>Wi-Fi 模拟</h2>
      <div className="feature-list">
        <SwitchRow title="Wi-Fi 模拟" checked={enabled} disabled={busy || !state}
          summary={!state ? '读取当前状态中' : enabled ? '应用会读到保存的网络信息' : '关闭时应用读到系统原样结果'}
          onChange={next => void toggle(next)} />
      </div>
      <p>按名称与接入点地址模拟应用读到的无线网络。附近列表需要系统扫描结果，目前还没接入，先用手工添加。</p>
      <p role="status">真正的输出尚未实现：这些网络已经写入后台配置，但还不会改变应用读到的 Wi-Fi 信息。</p>
      {!draft && <div className="route-actions">
        <button className="primary" disabled={busy || !state} onClick={startAdd}><Plus size={18} />添加网络</button>
      </div>}
      {draft && <form className="wifi-form" onSubmit={event => { event.preventDefault(); void submit(); }}>
        <label className="field">网络名称（SSID）<input autoFocus maxLength={64} value={draft.ssid}
          onChange={e => setDraft({ ...draft, ssid: e.target.value })} placeholder="例如 Home" /></label>
        <label className="field">接入点地址（BSSID，可留空）<input maxLength={64} value={draft.bssid}
          onChange={e => setDraft({ ...draft, bssid: e.target.value })} placeholder="aa:bb:cc:dd:ee:ff" /></label>
        {hint && <p role="status">{hint}</p>}
        <div className="route-actions">
          <button className="primary" type="submit" disabled={busy}>{draft.id ? '保存修改' : '保存网络'}</button>
          <button className="text-button" type="button" onClick={() => { setDraft(null); setError(''); }}>取消</button>
        </div>
      </form>}
      {error && <p className="form-error" role="alert">{error}</p>}
      {note && <p role="status">{note}</p>}
    </section>

    <section className="card">
      <div className="section-heading"><h2>已保存的网络 <span>{items.length}</span></h2></div>
      {items.length ? <ul className="list">{items.map(wifi => <li className="list-item" key={wifi.id}>
        <span className="place-symbol"><Wifi size={20} /></span>
        <span className="list-item-text">
          <strong className="list-item-title">{wifi.ssid}</strong>
          <small className="list-item-summary">{wifi.bssid || '未填写接入点地址'}</small>
        </span>
        <button className="icon-button" aria-label={`编辑 ${wifi.ssid}`} disabled={busy} onClick={() => startEdit(wifi)}><Pencil size={18} /></button>
        <button className="icon-button" aria-label={`删除 ${wifi.ssid}`} disabled={busy} onClick={() => void remove(wifi)}><Trash2 size={18} /></button>
      </li>)}</ul> : <div className="empty-state"><Wifi size={32} /><h3>还没有保存的网络</h3><p>添加一个名称，之后就能在这里切换。</p></div>}
      {deleted && <div className="route-undo" role="status"><span>已删除“{deleted.wifi.ssid}”</span>
        <button className="text-button" disabled={busy} onClick={() => {
          const next = [...items];
          next.splice(Math.min(deleted.index, next.length), 0, deleted.wifi);
          void write(next).then(() => setDeleted(null)).catch(() => {});
        }}>撤销</button></div>}
    </section>
  </>;
}
