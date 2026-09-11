import { useEffect, useRef, useState } from 'react';
import { Crosshair, MapPin, Route, Wifi, SlidersHorizontal, Settings, Menu, X, Plus, Search, Play, Square, ArrowLeft, ChevronRight, Trash2, Pin, Compass, RefreshCw, Check, Monitor, Moon, Sun } from 'lucide-react';
import type { Client, Command, Position, Scope, State } from './control';
import { PositionEditor } from './PositionEditor';
import { AppPicker, loadInstalledApps, type LoadApps } from './AppPicker';
import { RoutePanel } from './RoutePanel';
import { joystickControl, type Joystick } from './joystick';
import { FeatureMenus } from './FeatureMenus';
import { BackupPanel } from './BackupPanel';
import { CellPanel } from './CellPanel';
import type { Place } from './backup';

type Page = 'location' | 'routes' | 'wifi' | 'scope' | 'settings';
type ScopeDraft = { mode: 'all' | 'apps'; packages: string[] };
function readScopeDraft(): ScopeDraft | null {
  try {
    const value = JSON.parse(localStorage.getItem('justlocation.scope') || 'null');
    if ((value?.mode === 'all' || value?.mode === 'apps') && Array.isArray(value.packages) && value.packages.every((p: unknown) => typeof p === 'string' && p.trim())) return value;
  } catch { /* Use the backend's saved scope. */ }
  return null;
}
const pages = [
  { id: 'location', label: '位置模拟', icon: MapPin }, { id: 'routes', label: '路线模拟', icon: Route },
  { id: 'wifi', label: 'Wi-Fi 模拟', icon: Wifi },
  { id: 'settings', label: '设置', icon: Settings },
] as const;
const coordinates = (p: Position) => `${p.latitude.toFixed(6)}, ${p.longitude.toFixed(6)}`;

function readPlaces(): Place[] {
  try {
    const items: unknown = JSON.parse(localStorage.getItem('justlocation.places') || '[]');
    if (!Array.isArray(items)) return [];
    return items.filter((p): p is Place => typeof p?.id === 'string' && typeof p?.name === 'string' &&
      typeof p?.position?.latitude === 'number' && typeof p?.position?.longitude === 'number');
  } catch { return []; }
}

export function App({ client, loadApps = loadInstalledApps, joystick = joystickControl }: { client: Client; loadApps?: LoadApps; joystick?: Joystick }) {
  const [page, setPage] = useState<Page>('location');
  const [drawer, setDrawer] = useState(false);
  const [state, setState] = useState<State | null>(null);
  const [position, setPosition] = useState<Position | null>(null);
  const [name, setName] = useState('');
  const [scopeDraft, setScopeDraft] = useState<ScopeDraft>(() => readScopeDraft() || { mode: 'apps', packages: [] });
  const scope: Scope = scopeDraft.mode === 'all' ? { mode: 'all' } : { mode: 'apps', packages: scopeDraft.packages };
  const [scopeReturn, setScopeReturn] = useState<Page>('location');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [places, setPlaces] = useState(readPlaces);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState(false);
  const [cellsOpen, setCellsOpen] = useState(false);
  const [joystickSpeed, setJoystickSpeed] = useState(() => localStorage.getItem('justlocation.joystick.speed') || '5.4');
  const [joystickNote, setJoystickNote] = useState('拖到屏幕边缘可收起，点边缘把手展开。');
  const [theme, setTheme] = useState(() => localStorage.getItem('justlocation.theme') || 'system');
  const initialized = useRef(false);
  const locked = busy || !!state?.requested_active;

  async function refresh() {
    setBusy(true);
    try {
      const next = await client({ op: 'status' });
      setState(next); setError('');
      if (!initialized.current && next.config) {
        setPosition(next.config.position);
        if (next.requested_active || !readScopeDraft()) {
          const savedScope = next.config.scope;
          setScopeDraft(previous => ({ mode: savedScope.mode, packages: savedScope.mode === 'apps' ? savedScope.packages : previous.packages }));
        }
      }
      initialized.current = true;
    } catch (e) { setState(null); setError(String(e instanceof Error ? e.message : e)); }
    finally { setBusy(false); }
  }
  useEffect(() => { void refresh(); }, [client]);
  useEffect(() => {
    if ((!state?.requested_active && !cellsOpen) || busy) return;
    let cancelled = false;
    let pending = false;
    const timer = window.setInterval(async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await client({ op: 'status' });
        if (!cancelled) {
          setState(next);
          if (next.requested_active && next.config) setPosition(next.config.position);
        }
      } catch (e) { if (!cancelled) setError(e instanceof Error ? e.message : String(e)); }
      finally { pending = false; }
    }, 2000);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [client, state?.requested_active, busy, cellsOpen]);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem('justlocation.theme', theme);
  }, [theme]);

  function navigate(next: Page) {
    if (next === 'scope' && page !== 'scope') {
      setScopeReturn(page === 'routes' ? 'routes' : 'location');
      window.history.pushState({ justlocationScope: true }, '');
    }
    setPage(next); setDrawer(false);
  }
  useEffect(() => {
    const back = () => { setPage(current => current === 'scope' ? scopeReturn : current); setCellsOpen(false); };
    window.addEventListener('popstate', back);
    return () => window.removeEventListener('popstate', back);
  }, [scopeReturn]);
  function closeScope() { window.history.back(); }
  function editScope(next: ScopeDraft) {
    setScopeDraft(next);
    try { localStorage.setItem('justlocation.scope', JSON.stringify(next)); }
    catch { setError('应用选择保存失败，关闭面板后可能丢失'); }
  }
  async function routeCommand(command: Command) {
    if (busy || !state) return;
    setBusy(true); setError('');
    try {
      const next = await client(command);
      setState(next);
      if (next.config) { setPosition(next.config.position); setName(''); }
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  function openCells() { window.history.pushState({ justlocationCells: true }, ''); setCellsOpen(true); }
  async function environmentCommand(command: Command) {
    if (busy || !state) throw new Error('后台尚未就绪，请稍后重试');
    setBusy(true);
    try { setState(await client(command)); }
    finally { setBusy(false); }
  }
  async function toggleCells() {
    const config = state?.telephony;
    if (!config || !config.subscriptions.some(sub => sub.enabled)) { openCells(); return; }
    try { await environmentCommand({ op: 'set_telephony', config: { ...config, cells_enabled: !config.cells_enabled } }); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  function savePlaces(next: Place[]) {
    try { localStorage.setItem('justlocation.places', JSON.stringify(next)); setPlaces(next); }
    catch { setError('历史记录保存失败，浏览器存储可能已满'); }
  }
  async function toggle() {
    if (!state || busy) return;
    setBusy(true); setError('');
    try {
      const next = await client(state.requested_active ? { op: 'stop' } : { op: 'start', config: { position: position!, scope } });
      setState(next);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  async function controlJoystick(open: boolean) {
    if (busy || !state) return;
    setBusy(true); setError('');
    let started = false;
    try {
      if (open) {
        const kmh = Number(joystickSpeed);
        if (!Number.isFinite(kmh) || kmh <= 0 || kmh > 3600) throw new Error('速度须大于 0、不超过 3600 km/h');
        await joystick.check();
        if (!state.requested_active) {
          const next = await client({ op: 'start', config: { position: position!, scope } });
          setState(next); started = true;
        }
        await joystick.open(kmh);
        localStorage.setItem('justlocation.joystick.speed', joystickSpeed);
        setJoystickNote('已请求打开摇杆；首次使用请在 App 中完成授权。');
      } else {
        await joystick.close();
        setJoystickNote('摇杆已关闭，位置模拟会保持当前状态。');
      }
    } catch (e) {
      let message = e instanceof Error ? e.message : String(e);
      if (started) {
        try { setState(await client({ op: 'stop' })); }
        catch { message += '；停止模拟失败，请手动停止'; }
      }
      setError(message);
    } finally { setBusy(false); }
  }
  async function select(p: Position, label: string) {
    if (busy) return false;
    if (state?.requested_active) {
      setBusy(true);
      try { setState(await client({ op: 'update', position: p })); }
      catch (e) { setError(e instanceof Error ? e.message : String(e)); return false; }
      finally { setBusy(false); }
    }
    setPosition(p); setName(label); setError('');
    return true;
  }

  const selectedPage = pages.find(p => p.id === page)!;
  const canStart = !!position && (scope.mode === 'all' || (scope.packages.length > 0 && scope.packages.every(p => p.trim())));
  const visiblePlaces = places.filter(p => `${p.name} ${coordinates(p.position)}`.toLowerCase().includes(query.toLowerCase()))
    .sort((a, b) => Number(b.pinned) - Number(a.pinned));

  if (page === 'scope') return <main className="scope-screen">
    <header className="scope-topbar">
      <button className="icon-button" aria-label="返回" onClick={closeScope}><ArrowLeft /></button>
      <h1>作用范围</h1><button className="text-button" onClick={closeScope}>完成</button>
    </header>
    <div className="scope-content">
      <div className="scope-modes">
        <label className="choice"><input type="radio" name="scope" checked={scope.mode === 'apps'} disabled={locked} onChange={() => editScope({ ...scopeDraft, mode: 'apps' })} /><strong>指定应用</strong></label>
        <label className="choice"><input type="radio" name="scope" checked={scope.mode === 'all'} disabled={locked} onChange={() => editScope({ ...scopeDraft, mode: 'all' })} /><strong>全部应用</strong></label>
      </div>
      <p className="scope-hint">{state?.requested_active ? '模拟中，停止后可修改' : '选择自动保存，下次开始模拟时生效'}</p>
      {error && <p className="form-error" role="alert">{error}</p>}
      {scope.mode === 'apps' ? <AppPicker selected={scope.packages} disabled={locked} loadApps={loadApps}
        onChange={packages => editScope({ mode: 'apps', packages })} /> : <p className="scope-all-note">所有应用都会使用模拟位置。之前勾选的应用已保留。</p>}
    </div>
  </main>;

  return <div className="app-shell">
    {drawer && <button className="scrim" aria-label="关闭导航" onClick={() => setDrawer(false)} />}
    <aside className={`sidebar ${drawer ? 'open' : ''}`} aria-label="主导航">
      <div className="brand"><Compass size={36} /><strong>JustLocation</strong><span>位置与路线模拟</span></div>
      <nav>{pages.map(({ id, label, icon: Icon }) => <button key={id} aria-current={page === id ? 'page' : undefined}
        onClick={() => navigate(id)}><Icon size={22} /><span>{label}</span>{page === id && <span className="nav-dot" />}</button>)}</nav>
      <div className="sidebar-footer"><span className={`dot ${state ? 'connected' : ''}`} />{state ? '后台已连接' : '后台未连接'}<small>Android 15 · KernelSU</small></div>
    </aside>
    <main>
      <header className="topbar"><button className="icon-button nav-toggle" aria-label="打开导航" onClick={() => setDrawer(true)}><Menu /></button>
        <span>JustLocation</span><button className="icon-button" aria-label="刷新后台状态" disabled={busy} onClick={() => void refresh()}><RefreshCw size={20} /></button>
      </header>
      <div className="page-content">
        <div className="page-heading"><h1>{selectedPage.label}</h1>
          {page === 'location' && <button className="add-button" aria-label="添加位置" disabled={busy || !!state?.route} onClick={() => setEditing(true)}><Plus /><span>添加位置</span></button>}
        </div>
        {error && <div role="alert" className="notice error">{error}<button className="icon-button" aria-label="关闭提示" onClick={() => setError('')}><X size={18} /></button></div>}
        {page === 'location' && <>
          <section className="target-card" aria-label="目标位置">
            <div className="target-top"><span className="target-symbol"><Crosshair size={25} /></span><span className="eyebrow">目标位置</span>
              <span className={`status-chip ${state?.requested_active ? 'active' : ''}`}>{state?.requested_active ? '会话已启动' : '未启动'}</span></div>
            <button className="target-detail" disabled={busy || !!state?.route} onClick={() => setEditing(true)}>
              <h2>{position ? name || '自定义位置' : '选择模拟位置'}</h2>
              <p>{position ? coordinates(position) : '点击这里填写坐标'}</p>
              {position && <span className="altitude">海拔 {position.altitude} m · WGS84</span>}
              {position && !state?.route && <span className="change-position">更改位置</span>}
              <ChevronRight className="target-chevron" />
            </button>
            <div className="target-actions"><button className={`primary ${state?.requested_active ? 'stop' : ''}`} disabled={busy || !state || (!state.requested_active && !canStart)} onClick={() => void toggle()}>
              {state?.requested_active ? <Square size={18} /> : <Play size={18} />} {busy ? '处理中…' : state?.requested_active ? '停止模拟' : '开始模拟'}</button>
              <FeatureMenus openCells={openCells} cellsEnabled={!!state?.telephony?.cells_enabled} toggleCells={() => void toggleCells()}
                cellNote={!state?.telephony?.cells_enabled ? '使用目标位置附近的基站数据。' : !state.requested_active ? '已启用，开始位置模拟后生效。' : state.cell_hook_ready ? '已连接基站服务' : '等待基站服务连接'} independent={scope.mode === 'apps'} scopeLocked={locked}
                toggleScope={() => editScope({ ...scopeDraft, mode: scope.mode === 'apps' ? 'all' : 'apps' })} openScope={() => navigate('scope')}
                speed={joystickSpeed} setSpeed={setJoystickSpeed} busy={busy}
                canOpen={!busy && !!state && !state.route && (state.requested_active || canStart)} canClose={!busy && !!state}
                controlJoystick={open => void controlJoystick(open)} note={state?.route ? '请先停止路线播放，再使用摇杆。' : joystickNote} /></div>
          </section>
          <p className="session-note">{state?.requested_active ? (state.location_hook_ready ? '模拟已开启，关闭面板后仍会继续。' : '模拟已开启，正在连接系统定位服务。') : !position ? '先选择位置，再选择要使用模拟位置的应用。' : !canStart ? '还没有选择应用，请打开独立模拟菜单，选择作用范围。' : '准备好了，点击“开始模拟”即可。'}</p>
          <section className="history"><div className="section-heading"><h2>历史位置 <span>{places.length}</span></h2></div>
            <label className="search-field"><Search size={20} /><input aria-label="搜索历史位置" placeholder="搜索名称或坐标" value={query} onChange={e => setQuery(e.target.value)} /></label>
            {visiblePlaces.length ? <div className="place-list">{visiblePlaces.map(p => <div className="place-row" key={p.id}>
              <button className="place-select" disabled={busy || !!state?.route} onClick={() => void select(p.position, p.name)}><span className="place-symbol"><MapPin size={22} /></span>
                <span><strong>{p.name}</strong><small>{coordinates(p.position)}</small></span></button>
              <button className={`icon-button ${p.pinned ? 'pinned' : ''}`} aria-label={`${p.pinned ? '取消置顶' : '置顶'} ${p.name}`} onClick={() => savePlaces(places.map(item => item.id === p.id ? { ...item, pinned: !item.pinned } : item))}><Pin size={18} /></button>
              <button className="icon-button" aria-label={`删除 ${p.name}`} onClick={() => savePlaces(places.filter(item => item.id !== p.id))}><Trash2 size={18} /></button>
            </div>)}</div> : <div className="empty-state"><MapPin size={32} /><h3>{query ? '没有找到位置' : '把常去的地方留在这里'}</h3><p>{query ? '试试其他名称或坐标' : '添加的位置会出现在这里，方便下次直接使用'}</p></div>}
          </section>
        </>}
        {page === 'settings' && <><BackupPanel onImported={() => setPlaces(readPlaces())} /><section className="settings-card"><h2>外观</h2><p>跟随你的使用习惯</p><div className="theme-options">{[
          { id: 'system', label: '跟随系统', icon: Monitor }, { id: 'light', label: '浅色', icon: Sun }, { id: 'dark', label: '深色', icon: Moon },
        ].map(({ id, label, icon: Icon }) => <button key={id} aria-pressed={theme === id} onClick={() => setTheme(id)}><Icon size={20} />{label}{theme === id && <Check size={16} />}</button>)}</div></section>
          <section className="settings-card"><h2>运行环境</h2><dl><div><dt>模块</dt><dd>JustLocation 0.1.0</dd></div><div><dt>控制入口</dt><dd>KernelSU WebUI</dd></div><div><dt>后台</dt><dd>{state ? '已连接' : '未连接'}</dd></div><div><dt>系统定位 Hook</dt><dd>{state?.location_hook_ready ? '已就绪' : state?.hook_connected ? '等待接口接入' : '等待系统连接'}</dd></div></dl></section></>}
        {page === 'routes' && <RoutePanel state={state} busy={busy} scope={scope} onCommand={routeCommand} onScope={() => navigate('scope')} />}
        {page === 'wifi' && <section className="empty-state feature-placeholder"><Wifi size={36} /><h2>Wi-Fi 功能正在接入</h2><p>已保存网络和模拟设置会在这里管理。</p></section>}
      </div>
    </main>
    {cellsOpen && <CellPanel target={position} state={state} controlBusy={busy}
      onConfigure={config => environmentCommand({ op: 'set_telephony', config })}
      onApply={region => environmentCommand({ op: 'set_cell_region', region })} onClose={() => window.history.back()} />}
    {editing && <PositionEditor initial={position} name={name} onClose={() => setEditing(false)} onSave={async (p, label) => {
      if (!await select(p, label)) throw new Error('位置更新失败，请重试');
      savePlaces([{ id: crypto.randomUUID(), name: label, position: p, pinned: false }, ...places]);
      setEditing(false);
    }} />}
  </div>;
}
