import { useEffect, useRef, useState } from 'react';
import { ArrowLeft, Check, ChevronRight, Compass, Crosshair, Download, MapPin, Menu, Monitor, Moon, Pencil, Pin, Play, Plus, RefreshCw, Route, Search, Settings, Square, Sun, Trash2, Wifi, X } from 'lucide-react';
import type { Client, Command, GnssConfig, Position, Scope, State } from './control';
import { PositionEditor } from './PositionEditor';
import { AppPicker, loadInstalledApps, type LoadApps } from './AppPicker';
import { RoutePanel } from './RoutePanel';
import { joystickControl, type Joystick } from './joystick';
import { BackupPanel } from './BackupPanel';
import { CellPanel } from './CellPanel';
import { ImportPlaceSheet } from './ImportPlaceSheet';
import { TargetCard } from './TargetCard';
import { SectionHeader } from './SectionHeader';
import { RowCard } from './RowCard';
import { EmptyBox } from './EmptyBox';
import { SatellitePanel } from './SatellitePanel';
import { ScopePage } from './ScopePage';
import { WifiPanel } from './WifiPanel';
import { Segmented, SwitchRow } from './Controls';
import { colorModes, readColorMode, readStyle, saveTheme, styleFamilies, type ColorMode, type StyleFamily } from './theme';
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
/** 免责声明：与下方 docs 里那句一致，原版放在主界面底部居中。 */
const DISCLAIMER = '声明：本程序仅供开发人员调试使用，严禁用于一切侵权、侵害他人利益、违法违禁等不当行为和目的。';
/** 把"系统定位"那一项的状态说清楚：区分后台未连接、接口未接、等待接入与已就绪。 */
const readyText = (ready?: boolean, connected?: boolean) => ready ? '已就绪' : connected ? '等待接口接入' : '等待系统连接';

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
  // 作用范围与卫星都是独立页面/面板，各带自己的返回目标；用页面状态而不是 history，
  // 这样"进入选应用、完成返回原页"的行为不依赖浏览器历史，面板里点按更直接。
  const [scopePage, setScopePage] = useState(false);
  const [satelliteOpen, setSatelliteOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [places, setPlaces] = useState(readPlaces);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState(false);
  const [importing, setImporting] = useState(false);
  const [editingPlace, setEditingPlace] = useState<Place | null>(null);
  const [cellsOpen, setCellsOpen] = useState(false);
  const [joystickSpeed, setJoystickSpeed] = useState(() => localStorage.getItem('justlocation.joystick.speed') || '5.4');
  // 摇杆是否真的开着，之前前端完全不记录，所以"关闭摇杆"永远可点、图标也没有状态。
  const [joystickOpen, setJoystickOpen] = useState(false);
  // 是否已经从系统查到过真实状态。查不到时不做判断，避免把开着的摇杆显示成关着。
  const [joystickKnown, setJoystickKnown] = useState(false);
  const [joystickNote, setJoystickNote] = useState('拖到屏幕边缘可收起，点边缘把手展开。');
  const [style, setStyle] = useState<StyleFamily>(readStyle);
  const [mode, setMode] = useState<ColorMode>(readColorMode);
  const initialized = useRef(false);
  /** 区块头的"查看全部"箭头要滚到列表并聚焦搜索框，所以需要这两个节点。 */
  const historyRef = useRef<HTMLElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  function focusHistory() {
    historyRef.current?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    searchRef.current?.focus();
  }
  // 轮询回调要读最新值，但不希望这些值变化就重建定时器，所以用 ref 传递。
  const stateRef = useRef<State | null>(null);
  const cellsOpenRef = useRef(false);
  const busyRef = useRef(false);
  stateRef.current = state;
  cellsOpenRef.current = cellsOpen;
  busyRef.current = busy;
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
      // 页面刷新后本地不记得摇杆状态，向系统确认一次，免得重复打开或显示成关着。
      // 读不到状态时保持原样：这只是附加信息，绝不能因为它失败而让整次刷新中断。
      if (next.requested_active) {
        let running: boolean | null = null;
        try { running = await joystick.read(); } catch { running = null; }
        if (running !== null) {
          setJoystickKnown(true);
          setJoystickOpen(running);
          if (running) setJoystickNote('摇杆正在运行。');
        }
      } else {
        setJoystickOpen(false);
      }
    } catch (e) { setState(null); setError(String(e instanceof Error ? e.message : e)); }
    finally { setBusy(false); }
  }
  useEffect(() => { void refresh(); }, [client]);
  // 空闲时也轮询，只是慢一些：桥接是否连上、各通道接口是否就绪都来自后台，
  // 只在模拟进行中才查会让设置页永远停在打开面板那一刻。后台不可用时放慢到 15 秒，
  // 避免在没装模块的环境里持续打扰，同时仍然能自动恢复。
  useEffect(() => {
    let cancelled = false;
    let pending = false;
    let timer = 0;
    let delay = 0;
    const tick = async () => {
      if (!cancelled && !pending) {
        pending = true;
        try {
          const next = await client({ op: 'status' });
          if (!cancelled) {
            setState(next);
            // 只有模拟进行中才用后台坐标覆盖草稿，空闲时不要动用户正在编辑的内容。
            if (next.requested_active && next.config) setPosition(next.config.position);
            setError('');
          }
        } catch (e) { if (!cancelled) setError(e instanceof Error ? e.message : String(e)); }
        finally { pending = false; }
      }
      if (cancelled) return;
      const active = !!stateRef.current?.requested_active || cellsOpenRef.current;
      const reachable = !!stateRef.current;
      const wanted = active ? 2000 : reachable ? 5000 : 15000;
      delay = busyRef.current ? Math.min(delay || wanted, 2000) : wanted;
      timer = window.setTimeout(tick, delay);
    };
    timer = window.setTimeout(tick, 5000);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [client]);
  useEffect(() => {
    document.documentElement.dataset.style = style;
    document.documentElement.dataset.theme = mode;
    saveTheme(style, mode);
  }, [style, mode]);

  function navigate(next: Page) {
    setPage(next); setDrawer(false);
  }
  function openScope() {
    // 入口在目标卡上：进入作用范围页，并把模式切到"只对指定应用生效"。
    // 一个应用都没勾选时**必须仍然能进来**，否则取消勾选后就再也进不去选应用了。
    if (scopeDraft.mode !== 'apps') editScope({ ...scopeDraft, mode: 'apps' });
    setScopePage(true);
  }
  /** 清掉范围限制，回到"所有应用"。勾选本身在作用范围页里完成。 */
  function clearScope() {
    if (locked) return;
    editScope({ mode: 'all', packages: scopeDraft.packages });
  }
  useEffect(() => {
    // 浏览器返回键只负责关掉基站页（作用范围与卫星由各自的返回按钮关闭）。
    const back = () => setCellsOpen(false);
    window.addEventListener('popstate', back);
    return () => window.removeEventListener('popstate', back);
  }, []);
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
  /** 基站开关：打开时同时进入基站页，因为有效基站数据必须先从那里取得。 */
  async function toggleCells(next: boolean) {
    const config = state?.telephony;
    if (!config) { openCells(); return; }
    try {
      await environmentCommand({ op: 'set_telephony', config: { ...config, cells_enabled: next } });
      if (!next) return;
      if (!config.subscriptions.some(sub => sub.enabled)) {
        setError('还没有可用的基站数据，请先查询并应用。');
        openCells();
      }
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  /** 卫星通道开关：直接写后台配置，与基站开关保持同样的交互。 */
  async function toggleGnss(patch: Partial<GnssConfig>) {
    const config: GnssConfig = state?.gnss ?? { gnss_enabled: false, nmea_enabled: false };
    try { await environmentCommand({ op: 'set_gnss', config: { ...config, ...patch } }); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  function savePlaces(next: Place[]) {
    try { localStorage.setItem('justlocation.places', JSON.stringify(next)); setPlaces(next); }
    catch { setError('历史记录保存失败，浏览器存储可能已满'); }
  }
  async function stopSimulation() {
    // 停止模拟时摇杆必须一起关掉，否则悬浮窗会留在屏幕上；即使前端不知道它开着，
    // 这里也无条件请求一次关闭（服务本来就没运行时这条命令是安全的）。
    let closed = true;
    try { await joystick.close(); }
    catch { closed = joystickOpen; /* 确实开着的才需要报错，本来就没开就忽略 */ }
    setJoystickOpen(false);
    setJoystickNote(closed ? '位置模拟已停止，摇杆同时关闭。' : '位置模拟已停止，但摇杆没能关闭，请再点一次摇杆开关。');
    if (!closed) setError('摇杆没有关闭成功，请重试。');
  }
  async function toggle() {
    if (!state || busy) return;
    setBusy(true); setError('');
    try {
      const next = await client(state.requested_active ? { op: 'stop' } : { op: 'start', config: { position: position!, scope } });
      setState(next);
      if (state.requested_active) await stopSimulation();
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
        setJoystickOpen(true);
        setJoystickNote('摇杆已打开，拖动即可移动位置。');
      } else {
        await joystick.close();
        setJoystickOpen(false);
        setJoystickNote('摇杆已关闭，位置模拟会保持当前状态。');
      }
    } catch (e) {
      let message = e instanceof Error ? e.message : String(e);
      if (started) {
        try { setState(await client({ op: 'stop' })); }
        catch { message += '；停止模拟失败，请手动停止'; }
      }
      // 摇杆已经关掉、只是服务停止返回异常时，不要把它继续显示成打开。
      if (!open) { setJoystickOpen(false); }
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
  const scopeLimited = scopeDraft.mode === 'apps';

  // 作用范围与卫星都是独立页面，首页只剩 目标卡 / 区块头 / 点位列表 三段。
  if (scopePage) return <ScopePage limited={scopeLimited} packages={scopeDraft.packages} locked={locked} error={error}
    loadApps={loadApps} onChange={packages => editScope({ mode: 'apps', packages })}
    onClearScope={clearScope} canClear={scopeLimited} onClose={() => setScopePage(false)} />;

  return <div className="app-shell">
    {drawer && <button className="scrim" aria-label="关闭导航" onClick={() => setDrawer(false)} />}
    <aside className={`sidebar ${drawer ? 'open' : ''}`} aria-label="主导航">
      <div className="brand"><Compass size={32} /><strong>JustLocation</strong><span>位置与路线模拟</span></div>
      <nav>{pages.map(({ id, label, icon: Icon }) => <button key={id} aria-current={page === id ? 'page' : undefined}
        onClick={() => navigate(id)}><Icon size={22} /><span>{label}</span>{page === id && <span className="nav-dot" />}</button>)}</nav>
      <div className="sidebar-footer"><span className={`dot ${state ? 'connected' : ''}`} />{state ? '后台已连接' : '后台未连接'}<small>Android 15 · KernelSU</small></div>
    </aside>
    <main>
      <header className="topbar"><button className="icon-button nav-toggle" aria-label="打开导航" onClick={() => setDrawer(true)}><Menu /></button>
        <span>JustLocation</span><button className="icon-button" aria-label="刷新后台状态" disabled={busy} onClick={() => void refresh()}><RefreshCw size={20} /></button>
      </header>
      <div className="page-body">
      <div className="page-content">
        <div className="page-heading"><h1>{selectedPage.label}</h1></div>
        {error && <div role="alert" className="notice error">{error}<button className="icon-button" aria-label="关闭提示" onClick={() => setError('')}><X size={18} /></button></div>}
        {page === 'location' && <>
          {/* 目标卡按原版 ci.xml：小标题、主值、副值、坐标行，操作区在同一行内联。 */}
          <TargetCard position={position} name={name} summary="" active={!!state?.requested_active} busy={busy} canStart={canStart}
            joystickOpen={joystickOpen} joystickDisabled={state?.route ? true : !joystickOpen && (!state || !(state.requested_active || canStart))}
            cellsEnabled={!!state?.telephony?.cells_enabled} scopeLimited={scopeLimited} scopeCount={scopeDraft.packages.length}
            satelliteOn={!!state?.gnss?.gnss_enabled || !!state?.gnss?.nmea_enabled}
            joystickNote={state?.route ? '请先停止路线播放，再使用摇杆。' : joystickNote}
            joystickSpeed={joystickSpeed} setJoystickSpeed={setJoystickSpeed}
            onToggle={() => void toggle()} onEdit={() => setEditing(true)} onCells={openCells}
            onScope={openScope} onSatellite={() => setSatelliteOpen(true)}
            onToggleJoystick={open => void controlJoystick(open)} />
          <p className="session-note">{state?.requested_active ? (state.location_hook_ready ? '模拟已开启，关闭面板后仍会继续。' : '模拟已开启，正在连接系统定位服务。') : !position ? '先选择位置，再决定要模拟哪些应用。' : !canStart ? '还没有选择应用，请打开“作用范围”并选择应用。' : '准备好了，点击“开始模拟”即可。'}</p>
          <section className="history" ref={historyRef}>
            <SectionHeader title="历史位置" count={places.length} actionLabel="查看全部" onAction={focusHistory} />
            <label className="search-field"><Search size={20} /><input ref={searchRef} aria-label="搜索历史位置" placeholder="搜索名称或坐标" value={query} onChange={e => setQuery(e.target.value)} /></label>
            {visiblePlaces.length ? <div className="place-list">{visiblePlaces.map(p => <RowCard key={p.id}
              icon={<MapPin size={20} />} title={p.name} detail={coordinates(p.position)} label={`${p.name} ${coordinates(p.position)}`}
              onClick={busy || state?.route ? undefined : () => void select(p.position, p.name)}
              actions={<>
                <button className={`icon-button ${p.pinned ? 'pinned' : ''}`} aria-label={`${p.pinned ? '取消置顶' : '置顶'} ${p.name}`} onClick={() => savePlaces(places.map(item => item.id === p.id ? { ...item, pinned: !item.pinned } : item))}><Pin size={18} /></button>
                <button className="icon-button" aria-label={`编辑 ${p.name}`} onClick={() => setEditingPlace(p)}><Pencil size={18} /></button>
                <button className="icon-button" aria-label={`删除 ${p.name}`} onClick={() => savePlaces(places.filter(item => item.id !== p.id))}><Trash2 size={18} /></button>
              </>} />)}</div>
              : <EmptyBox text={query ? '没有找到匹配的位置' : undefined}
                hint={query ? '换个名称或坐标再试一次。' : '导入地图链接或直接输入经纬度也可以。'} />}
          </section>
        </>}
        {page === 'settings' && <>
          <section className="settings-card appearance-card"><h2>外观</h2><p>界面风格和明暗可以分开选，随时切换。</p>
            <span className="appearance-label">界面风格</span>
            <Segmented label="界面风格" value={style}
              options={styleFamilies.map(family => ({ id: family.id, label: family.label }))}
              onChange={next => setStyle(next)} />
            <span className="appearance-label">明暗</span>
            <div className="theme-options">{colorModes.map(({ id, label }) => {
              const Icon = id === 'system' ? Monitor : id === 'light' ? Sun : Moon;
              return <button key={id} aria-pressed={mode === id} onClick={() => setMode(id)}><Icon size={20} />{label}{mode === id && <Check size={16} />}</button>;
            })}</div>
          </section>
          <BackupPanel onImported={() => setPlaces(readPlaces())} />
          <section className="settings-card"><h2>系统通道</h2><p>这些开关决定应用能不能读到系统返回的模拟数据。</p>
            <dl>
              <div><dt>系统定位</dt><dd>{readyText(state?.location_hook_ready, !!state?.hook_connected)}</dd></div>
              <div><dt>GNSS 状态</dt><dd>{state?.gnss_hook_ready ? '已就绪' : state ? '尚未接入' : '后台未连接'}</dd></div>
              <div><dt>NMEA 报文</dt><dd>{state?.nmea_hook_ready ? '已就绪' : state ? '尚未接入' : '后台未连接'}</dd></div>
              <div><dt>基站查询</dt><dd>{state?.cell_query_hook_ready ? '已就绪' : state ? '尚未接入' : '后台未连接'}</dd></div>
              <div><dt>基站回调</dt><dd>{state?.cell_callback_hook_ready ? '已就绪' : state ? '尚未接入' : '后台未连接'}</dd></div>
              <div><dt>电话服务</dt><dd>{state?.phone_connected ? '已连接' : state ? '等待连接' : '后台未连接'}</dd></div>
            </dl></section>
          <section className="settings-card"><h2>运行环境</h2><dl><div><dt>模块</dt><dd>JustLocation 0.1.0</dd></div><div><dt>控制入口</dt><dd>KernelSU WebUI</dd></div><div><dt>后台</dt><dd>{state ? '已连接' : '未连接'}</dd></div></dl></section>
        </>}
        {page === 'routes' && <RoutePanel state={state} busy={busy} scope={scope} onCommand={routeCommand} onScope={openScope} />}
        {page === 'wifi' && <WifiPanel state={state} onConfigure={async config => { await environmentCommand({ op: 'set_wifi', config }); }} />}
        <p className="page-footer">{DISCLAIMER}</p>
      </div>
      {page === 'location' && <div className="fab-stack">
        <button className="fab small" aria-label="导入位置" title="导入位置" disabled={busy} onClick={() => setImporting(true)}><Download size={20} /></button>
        <button className="fab" aria-label="添加位置" title="添加位置" disabled={busy || !!state?.route} onClick={() => setEditing(true)}><Plus size={22} /></button>
      </div>}
      </div>
    </main>
    {cellsOpen && <CellPanel target={position} state={state} controlBusy={busy}
      onConfigure={config => environmentCommand({ op: 'set_telephony', config })}
      onApply={region => environmentCommand({ op: 'set_cell_region', region })} onClose={() => window.history.back()} />}
    {satelliteOpen && <SatellitePanel state={state} busy={busy} onToggle={patch => void toggleGnss(patch)} onClose={() => setSatelliteOpen(false)} />}
    {editing && <PositionEditor initial={position} name={name} onClose={() => setEditing(false)} onSave={async (p, label) => {
      if (!await select(p, label)) throw new Error('位置更新失败，请重试');
      savePlaces([{ id: crypto.randomUUID(), name: label, position: p, pinned: false }, ...places]);
      setEditing(false);
    }} />}
    {importing && <ImportPlaceSheet onClose={() => setImporting(false)} onSave={async (p, label) => {
      // 导入只进历史列表；要立即用于模拟，用户再点这一条即可。
      savePlaces([{ id: crypto.randomUUID(), name: label, position: p, pinned: false }, ...places]);
    }} />}
    {editingPlace && <PositionEditor title="编辑位置" initial={editingPlace.position} name={editingPlace.name} onClose={() => setEditingPlace(null)} onSave={async (p, label) => {
      savePlaces(places.map(item => item.id === editingPlace.id ? { ...item, name: label, position: p } : item));
      if (position && name === editingPlace.name) setName(label);
      setEditingPlace(null);
    }} />}
  </div>;
}
