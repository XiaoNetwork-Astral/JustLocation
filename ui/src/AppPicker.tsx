import { useEffect, useState } from 'react';
import { Search, RefreshCw } from 'lucide-react';
import { listPackages, getPackagesInfo } from 'kernelsu';

export interface InstalledApp { packageName: string; appLabel: string; isSystem: boolean }
export type LoadApps = () => Promise<InstalledApp[]>;
type Props = { selected: string[]; onChange: (packages: string[]) => void; disabled: boolean; loadApps: LoadApps };

export const loadInstalledApps: LoadApps = async () => {
  if (!('ksu' in window)) throw new Error('请从 KernelSU 管理器打开面板');
  const packages = [...new Set(listPackages('all'))];
  const info = new Map(getPackagesInfo(packages).map(app => [app.packageName, app]));
  return packages.map(packageName => ({ packageName, appLabel: info.get(packageName)?.appLabel || packageName,
    isSystem: info.get(packageName)?.isSystem === true }));
};

export function AppPicker({ selected, onChange, disabled, loadApps }: Props) {
  const [apps, setApps] = useState<InstalledApp[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const [query, setQuery] = useState('');
  const [showSystem, setShowSystem] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setLoading(true); setError('');
    void loadApps().then(result => { if (!cancelled) setApps(result); })
      .catch(error => { if (!cancelled) setError(error instanceof Error ? error.message : '应用列表读取失败'); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [loadApps, attempt]);
  const known = new Set(apps.map(app => app.packageName));
  const available = [...apps, ...selected.filter(name => !known.has(name)).map(packageName => ({
    packageName, appLabel: packageName, isSystem: false,
  }))];
  const needle = query.trim().toLocaleLowerCase();
  const visible = available.filter(app => (showSystem || !app.isSystem || selected.includes(app.packageName)) &&
    `${app.appLabel} ${app.packageName}`.toLocaleLowerCase().includes(needle))
    .sort((a, b) => a.appLabel.localeCompare(b.appLabel, 'zh-CN'));
  return <div className="app-picker">
    <label className="search-field"><Search size={20} /><input type="search" aria-label="搜索应用" placeholder="搜索应用名称或包名"
      value={query} onChange={event => setQuery(event.target.value)} /></label>
    <div className="picker-toolbar"><span>已选 {selected.length} 项</span>
      <label className="choice"><input type="checkbox" checked={showSystem} onChange={event => setShowSystem(event.target.checked)} />系统应用</label>
      <button className="icon-button" aria-label="刷新列表" disabled={loading} onClick={() => setAttempt(value => value + 1)}><RefreshCw size={18} /></button>
    </div>
    {loading && <p role="status">正在读取应用…</p>}
    {error && <div role="alert" className="notice error">{error}<button className="text-button" onClick={() => setAttempt(value => value + 1)}>重试</button></div>}
    <div className="app-list">{visible.map(app => <label className="choice app-choice" key={app.packageName}>
      <input type="checkbox" checked={selected.includes(app.packageName)} disabled={disabled} onChange={event => {
        if (!disabled) onChange(event.target.checked ? [...selected, app.packageName] : selected.filter(name => name !== app.packageName));
      }} /><span><strong>{app.appLabel}</strong><small>{app.packageName}{app.isSystem ? ' · 系统应用' : ''}{!known.has(app.packageName) ? ' · 未在列表中找到' : ''}</small></span>
    </label>)}</div>
    {!loading && !error && !visible.length && <p className="picker-empty">{query ? '没有找到应用' : '暂无应用，试试刷新列表'}</p>}
  </div>;
}
