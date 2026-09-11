import { useState } from 'react';
import { Download, Upload } from 'lucide-react';
import { createBackup, importBackup, maxFileSize, parseBackup, type Backup } from './backup';
import { saveFile } from './fileExport';

export function BackupPanel({ onImported, save = saveFile }: { onImported: () => void; save?: typeof saveFile }) {
  const [preview, setPreview] = useState<Backup | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [note, setNote] = useState('');
  async function read(file: File) {
    setBusy(true); setPreview(null); setError(''); setNote('');
    try {
      if (file.size > maxFileSize) throw new Error('请选择 2 MB 以内的备份文件');
      const text = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(new Error('文件读取失败，请重新选择'));
        reader.readAsText(file);
      });
      setPreview(parseBackup(text));
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  async function download() {
    setBusy(true); setError(''); setNote('');
    try { setNote(await save(createBackup(localStorage), 'json')); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  return <section className="settings-card backup-panel"><h2>备份与恢复</h2>
    <p>备份历史位置、置顶状态和已保存的路线。换管理器或重装前，可以先留一份。</p>
    <div className="route-actions">
      <button className="text-button" disabled={busy} onClick={() => void download()}><Download size={18} />导出备份</button>
      <label className={`text-button import-button${busy ? ' disabled' : ''}`}><Upload size={18} />导入备份
        <input type="file" aria-label="导入备份" accept=".json,application/json" disabled={busy} onChange={e => {
          const file = e.target.files?.[0]; e.target.value = ''; if (file) void read(file);
        }} /></label>
    </div>
    {busy && <p role="status">正在处理…</p>}
    {error && <p role="alert" className="form-error">{error}</p>}
    {note && <p role="status">{note}</p>}
    {preview && <div className="import-preview"><strong>{preview.places.length} 个位置 · {preview.routes.length} 条路线</strong>
      <p>添加到现有列表，相同内容会跳过。当前的模拟和应用选择不受影响。</p>
      <div className="route-actions"><button className="primary" disabled={busy} onClick={() => {
        try {
          const count = importBackup(preview, localStorage);
          setNote(`已添加 ${count.places} 个位置、${count.routes} 条路线`); setError(''); setPreview(null); onImported();
        } catch (e) { setError(e instanceof Error ? e.message : String(e)); onImported(); }
      }}>合并导入</button><button className="text-button" disabled={busy} onClick={() => setPreview(null)}>取消</button></div>
    </div>}
  </section>;
}
