import { useEffect, useRef, useState } from 'react';
import {
  Compass,
  Download,
  MapPin,
  Menu,
  Pencil,
  Pin,
  Plus,
  RefreshCw,
  Route,
  Search,
  Settings,
  Trash2,
  Wifi,
  X,
} from 'lucide-react';
import type { Client } from './control';
import type { GnssConfig, Position } from './protocol';
import { PositionEditor } from './PositionEditor';
import { loadInstalledApps, type LoadApps } from './installedApps';
import { RoutePanel } from './RoutePanel';
import { joystickControl, type Joystick } from './joystick';
import { CellPanel } from './CellPanel';
import { ImportPlaceSheet } from './ImportPlaceSheet';
import { TargetCard } from './TargetCard';
import { SectionHeader } from './SectionHeader';
import { RowCard } from './RowCard';
import { EmptyBox } from './EmptyBox';
import { SatellitePanel } from './SatellitePanel';
import { ScopePage } from './ScopePage';
import { WifiPanel } from './WifiPanel';
import { placesKey, type Place } from './backup';
import { readPlaces } from './panelStorage';
import { SettingsPage } from './SettingsPage';
import {
  applyTheme,
  readColorMode,
  readStyle,
  saveTheme,
  type ColorMode,
  type StyleFamily,
} from './theme';
import { useControlSession } from './useControlSession';

type Page = 'location' | 'routes' | 'wifi' | 'settings';
const pages = [
  { id: 'location', label: '位置模拟', icon: MapPin },
  { id: 'routes', label: '路线模拟', icon: Route },
  { id: 'wifi', label: 'Wi-Fi 模拟', icon: Wifi },
  { id: 'settings', label: '设置', icon: Settings },
] as const;
const coordinates = (p: Position) => `${p.latitude.toFixed(6)}, ${p.longitude.toFixed(6)}`;

const DISCLAIMER =
  '声明：本程序仅供开发人员调试使用，严禁用于一切侵权、侵害他人利益、违法违禁等不当行为和目的。';

export function App({
  client,
  loadApps = loadInstalledApps,
  joystick = joystickControl,
}: {
  client: Client;
  loadApps?: LoadApps;
  joystick?: Joystick;
}) {
  const [page, setPage] = useState<Page>('location');
  const [drawer, setDrawer] = useState(false);

  const [scopePage, setScopePage] = useState(false);
  const [satelliteOpen, setSatelliteOpen] = useState(false);
  const [places, setPlaces] = useState(readPlaces);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState(false);
  const [importing, setImporting] = useState(false);
  const [editingPlace, setEditingPlace] = useState<Place | null>(null);
  const [cellsOpen, setCellsOpen] = useState(false);
  const {
    state,
    position,
    name,
    setName,
    scopeDraft,
    scope,
    busy,
    error,
    setError,
    locked,
    joystickSpeed,
    setJoystickSpeed,
    joystickOpen,
    joystickNote,
    refresh,
    editScope,
    routeCommand,
    environmentCommand,
    toggle,
    controlJoystick,
    select,
  } = useControlSession(client, joystick, cellsOpen);

  const historyRef = useRef<HTMLElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  function focusHistory() {
    historyRef.current?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    searchRef.current?.focus();
  }
  const [style, setStyle] = useState<StyleFamily>(readStyle);
  const [mode, setMode] = useState<ColorMode>(readColorMode);
  useEffect(() => {
    applyTheme(style, mode);
    saveTheme(style, mode);
  }, [style, mode]);

  function navigate(next: Page) {
    setPage(next);
    setDrawer(false);
  }
  function openScope() {
    // Keep the picker accessible when the selection is empty.
    if (scopeDraft.mode !== 'apps') editScope({ ...scopeDraft, mode: 'apps' });
    setScopePage(true);
  }

  function clearScope() {
    if (locked) return;
    editScope({ mode: 'all', packages: scopeDraft.packages });
  }
  useEffect(() => {
    // Browser history closes cell settings; the other panels own their back actions.
    const back = () => setCellsOpen(false);
    window.addEventListener('popstate', back);
    return () => window.removeEventListener('popstate', back);
  }, []);
  function openCells() {
    window.history.pushState({ justlocationCells: true }, '');
    setCellsOpen(true);
  }

  async function toggleGnss(patch: Partial<GnssConfig>) {
    const config: GnssConfig = state?.gnss ?? { gnss_enabled: false, nmea_enabled: false };
    try {
      await environmentCommand({ op: 'set_gnss', config: { ...config, ...patch } });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  function savePlaces(next: Place[]) {
    try {
      localStorage.setItem(placesKey, JSON.stringify(next));
      setPlaces(next);
    } catch {
      setError('历史记录保存失败，浏览器存储可能已满');
    }
  }
  const selectedPage = pages.find((p) => p.id === page)!;
  const canStart =
    !!position &&
    (scope.mode === 'all' || (scope.packages.length > 0 && scope.packages.every((p) => p.trim())));
  const visiblePlaces = places
    .filter((p) =>
      `${p.name} ${coordinates(p.position)}`.toLowerCase().includes(query.toLowerCase()),
    )
    .sort((a, b) => Number(b.pinned) - Number(a.pinned));
  const scopeLimited = scopeDraft.mode === 'apps';

  if (scopePage)
    return (
      <ScopePage
        limited={scopeLimited}
        packages={scopeDraft.packages}
        locked={locked}
        error={error}
        loadApps={loadApps}
        onChange={(packages) => editScope({ mode: 'apps', packages })}
        onClearScope={clearScope}
        canClear={scopeLimited}
        onClose={() => setScopePage(false)}
      />
    );

  return (
    <div className="app-shell">
      {drawer && (
        <button className="scrim" aria-label="关闭导航" onClick={() => setDrawer(false)} />
      )}
      <aside className={`sidebar ${drawer ? 'open' : ''}`} aria-label="主导航">
        <div className="brand">
          <Compass size={32} />
          <strong>JustLocation</strong>
          <span>位置与路线模拟</span>
        </div>
        <nav>
          {pages.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              aria-current={page === id ? 'page' : undefined}
              onClick={() => navigate(id)}
            >
              <Icon size={22} />
              <span>{label}</span>
              {page === id && <span className="nav-dot" />}
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className={`dot ${state ? 'connected' : ''}`} />
          {state ? '后台已连接' : '后台未连接'}
          <small>Android 15 · KernelSU</small>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <button
            className="icon-button nav-toggle"
            aria-label="打开导航"
            onClick={() => setDrawer(true)}
          >
            <Menu />
          </button>
          <span>JustLocation</span>
          <button
            className="icon-button"
            aria-label="刷新后台状态"
            disabled={busy}
            onClick={() => void refresh()}
          >
            <RefreshCw size={20} />
          </button>
        </header>
        <div className="page-body">
          <div className="page-content">
            <div className="page-heading">
              <h1>{selectedPage.label}</h1>
            </div>
            {error && (
              <div role="alert" className="notice error">
                {error}
                <button className="icon-button" aria-label="关闭提示" onClick={() => setError('')}>
                  <X size={18} />
                </button>
              </div>
            )}
            {page === 'location' && (
              <>
                <TargetCard
                  position={position}
                  name={name}
                  summary=""
                  active={!!state?.requested_active}
                  busy={busy}
                  canStart={canStart}
                  joystickOpen={joystickOpen}
                  joystickDisabled={
                    state?.route
                      ? true
                      : !joystickOpen && (!state || !(state.requested_active || canStart))
                  }
                  cellsEnabled={!!state?.telephony?.cells_enabled}
                  scopeLimited={scopeLimited}
                  scopeCount={scopeDraft.packages.length}
                  satelliteOn={!!state?.gnss?.gnss_enabled || !!state?.gnss?.nmea_enabled}
                  joystickNote={state?.route ? '请先停止路线播放，再使用摇杆。' : joystickNote}
                  joystickSpeed={joystickSpeed}
                  setJoystickSpeed={setJoystickSpeed}
                  onToggle={() => void toggle()}
                  onEdit={() => setEditing(true)}
                  onCells={openCells}
                  onScope={openScope}
                  onSatellite={() => setSatelliteOpen(true)}
                  onToggleJoystick={(open) => void controlJoystick(open)}
                />
                <p className="session-note">
                  {state?.requested_active
                    ? state.location_hook_ready
                      ? '模拟已开启，关闭面板后仍会继续。'
                      : '模拟已开启，正在连接系统定位服务。'
                    : !position
                      ? '先选择位置，再决定要模拟哪些应用。'
                      : !canStart
                        ? '还没有选择应用，请打开“作用范围”并选择应用。'
                        : '准备好了，点击“开始模拟”即可。'}
                </p>
                <section className="history" ref={historyRef}>
                  <SectionHeader
                    title="历史位置"
                    count={places.length}
                    actionLabel="查看全部"
                    onAction={focusHistory}
                  />
                  <label className="search-field">
                    <Search size={20} />
                    <input
                      ref={searchRef}
                      aria-label="搜索历史位置"
                      placeholder="搜索名称或坐标"
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                    />
                  </label>
                  {visiblePlaces.length ? (
                    <div className="place-list">
                      {visiblePlaces.map((p) => (
                        <RowCard
                          key={p.id}
                          icon={<MapPin size={20} />}
                          title={p.name}
                          detail={coordinates(p.position)}
                          label={`${p.name} ${coordinates(p.position)}`}
                          onClick={
                            busy || state?.route ? undefined : () => void select(p.position, p.name)
                          }
                          actions={
                            <>
                              <button
                                className={`icon-button ${p.pinned ? 'pinned' : ''}`}
                                aria-label={`${p.pinned ? '取消置顶' : '置顶'} ${p.name}`}
                                onClick={() =>
                                  savePlaces(
                                    places.map((item) =>
                                      item.id === p.id ? { ...item, pinned: !item.pinned } : item,
                                    ),
                                  )
                                }
                              >
                                <Pin size={18} />
                              </button>
                              <button
                                className="icon-button"
                                aria-label={`编辑 ${p.name}`}
                                onClick={() => setEditingPlace(p)}
                              >
                                <Pencil size={18} />
                              </button>
                              <button
                                className="icon-button"
                                aria-label={`删除 ${p.name}`}
                                onClick={() =>
                                  savePlaces(places.filter((item) => item.id !== p.id))
                                }
                              >
                                <Trash2 size={18} />
                              </button>
                            </>
                          }
                        />
                      ))}
                    </div>
                  ) : (
                    <EmptyBox
                      text={query ? '没有找到匹配的位置' : undefined}
                      hint={
                        query ? '换个名称或坐标再试一次。' : '导入地图链接或直接输入经纬度也可以。'
                      }
                    />
                  )}
                </section>
              </>
            )}
            {page === 'settings' && (
              <SettingsPage
                style={style}
                setStyle={setStyle}
                mode={mode}
                setMode={setMode}
                state={state}
                onImported={() => setPlaces(readPlaces())}
              />
            )}
            {page === 'routes' && (
              <RoutePanel
                state={state}
                busy={busy}
                scope={scope}
                onCommand={routeCommand}
                onScope={openScope}
              />
            )}
            {page === 'wifi' && (
              <WifiPanel
                state={state}
                onConfigure={async (config) => {
                  await environmentCommand({ op: 'set_wifi', config });
                }}
              />
            )}
            <p className="page-footer">{DISCLAIMER}</p>
          </div>
          {page === 'location' && (
            <div className="fab-stack">
              <button
                className="fab small"
                aria-label="导入位置"
                title="导入位置"
                disabled={busy}
                onClick={() => setImporting(true)}
              >
                <Download size={20} />
              </button>
              <button
                className="fab"
                aria-label="添加位置"
                title="添加位置"
                disabled={busy || !!state?.route}
                onClick={() => setEditing(true)}
              >
                <Plus size={22} />
              </button>
            </div>
          )}
        </div>
      </main>
      {cellsOpen && (
        <CellPanel
          target={position}
          state={state}
          controlBusy={busy}
          onConfigure={(config) => environmentCommand({ op: 'set_telephony', config })}
          onApply={(region) => environmentCommand({ op: 'set_cell_region', region })}
          onClose={() => window.history.back()}
        />
      )}
      {satelliteOpen && (
        <SatellitePanel
          state={state}
          busy={busy}
          onToggle={(patch) => void toggleGnss(patch)}
          onClose={() => setSatelliteOpen(false)}
        />
      )}
      {editing && (
        <PositionEditor
          initial={position}
          name={name}
          onClose={() => setEditing(false)}
          onSave={async (p, label) => {
            if (!(await select(p, label))) throw new Error('位置更新失败，请重试');
            savePlaces([
              { id: crypto.randomUUID(), name: label, position: p, pinned: false },
              ...places,
            ]);
            setEditing(false);
          }}
        />
      )}
      {importing && (
        <ImportPlaceSheet
          onClose={() => setImporting(false)}
          onSave={async (p, label) => {
            // Import adds to history; selecting the saved entry applies it.
            savePlaces([
              { id: crypto.randomUUID(), name: label, position: p, pinned: false },
              ...places,
            ]);
          }}
        />
      )}
      {editingPlace && (
        <PositionEditor
          title="编辑位置"
          initial={editingPlace.position}
          name={editingPlace.name}
          onClose={() => setEditingPlace(null)}
          onSave={async (p, label) => {
            savePlaces(
              places.map((item) =>
                item.id === editingPlace.id ? { ...item, name: label, position: p } : item,
              ),
            );
            if (position && name === editingPlace.name) setName(label);
            setEditingPlace(null);
          }}
        />
      )}
    </div>
  );
}
