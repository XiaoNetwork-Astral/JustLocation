import { useState } from 'react';
import { Pencil, Plus, Trash2, Wifi } from 'lucide-react';
import { createWifi, looksLikeBssid, readWifis, saveWifis, type SavedWifi } from './wifis';

type Draft = { id: string | null; ssid: string; bssid: string };

/**
 * Wi-Fi 模拟页。
 *
 * 规格第 7.1 节：从附近列表选择或手工填写名称与接入点地址，保存后在历史列表
 * 切换，支持编辑、删除和撤销删除。附近列表需要系统扫描结果，目前还没接，
 * 所以这里先提供手工添加与管理；真正的输出（WifiInfo / ScanResult 替换）也尚未实现，
 * 页面里明确写清这一点，不让人以为已经在改变应用读到的网络。
 */
export function WifiPanel() {
  const [items, setItems] = useState<SavedWifi[]>(readWifis);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [deleted, setDeleted] = useState<{ wifi: SavedWifi; index: number } | null>(null);
  const [error, setError] = useState('');
  const [note, setNote] = useState('');

  function write(next: SavedWifi[]) {
    saveWifis(next);
    setItems(next);
    setError('');
  }
  function startAdd() { setDraft({ id: null, ssid: '', bssid: '' }); setError(''); setNote(''); }
  function startEdit(wifi: SavedWifi) { setDraft({ id: wifi.id, ssid: wifi.ssid, bssid: wifi.bssid }); setError(''); setNote(''); }
  function submit() {
    if (!draft) return;
    try {
      const existing = draft.id ? items.find(item => item.id === draft.id) : undefined;
      // 已保存的条目保留原有信号取值，编辑名称或地址不该把它们重置。
      const wifi = existing ? { ...existing, ssid: draft.ssid.trim(), bssid: draft.bssid.trim() } : createWifi(draft.ssid, draft.bssid);
      write(draft.id ? items.map(item => item.id === draft.id ? wifi : item) : [...items, wifi]);
      setNote(draft.id ? '已更新这条网络' : '已保存，之后可以在列表里切换');
      setDraft(null);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  function remove(wifi: SavedWifi) {
    const index = items.findIndex(item => item.id === wifi.id);
    try { write(items.filter(item => item.id !== wifi.id)); setDeleted({ wifi, index }); setNote(''); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  const hint = draft && draft.bssid && !looksLikeBssid(draft.bssid) ? '这串看起来不像接入点地址，仍然可以保存。' : '';

  return <>
    <section className="card" aria-label="Wi-Fi 模拟">
      <h2>Wi-Fi 模拟</h2>
      <p>按名称与接入点地址模拟应用读到的无线网络。附近列表需要系统扫描结果，目前还没接入，先用手工添加。</p>
      <p role="status">真正的输出尚未实现：这些网络目前只是保存下来，还不会改变应用读到的 Wi-Fi 信息。</p>
      {!draft && <div className="route-actions">
        <button className="primary" onClick={startAdd}><Plus size={18} />添加网络</button>
      </div>}
      {draft && <form className="wifi-form" onSubmit={event => { event.preventDefault(); submit(); }}>
        <label className="field">网络名称（SSID）<input autoFocus maxLength={64} value={draft.ssid}
          onChange={e => setDraft({ ...draft, ssid: e.target.value })} placeholder="例如 Home" /></label>
        <label className="field">接入点地址（BSSID，可留空）<input maxLength={64} value={draft.bssid}
          onChange={e => setDraft({ ...draft, bssid: e.target.value })} placeholder="aa:bb:cc:dd:ee:ff" /></label>
        {hint && <p role="status">{hint}</p>}
        <div className="route-actions">
          <button className="primary" type="submit">{draft.id ? '保存修改' : '保存网络'}</button>
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
        <button className="icon-button" aria-label={`编辑 ${wifi.ssid}`} onClick={() => startEdit(wifi)}><Pencil size={18} /></button>
        <button className="icon-button" aria-label={`删除 ${wifi.ssid}`} onClick={() => remove(wifi)}><Trash2 size={18} /></button>
      </li>)}</ul> : <div className="empty-state"><Wifi size={32} /><h3>还没有保存的网络</h3><p>添加一个名称，之后就能在这里切换。</p></div>}
      {deleted && <div className="route-undo" role="status"><span>已删除“{deleted.wifi.ssid}”</span>
        <button className="text-button" onClick={() => {
          try {
            const next = [...items];
            next.splice(Math.min(deleted.index, next.length), 0, deleted.wifi);
            write(next); setDeleted(null);
          } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
        }}>撤销</button></div>}
    </section>
  </>;
}
