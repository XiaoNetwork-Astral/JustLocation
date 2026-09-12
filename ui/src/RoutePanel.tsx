import { useEffect, useState } from 'react';
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  MapPin,
  Pause,
  Play,
  Plus,
  Route,
  Square,
  Trash2,
  Upload,
} from 'lucide-react';
import { parsePosition } from './coordinates';
import type { Command, Position, RoutePlan, Scope, State } from './protocol';
import { parseGpx, type ImportedRoute } from './routeImport';
import {
  blankPoint,
  draftKey,
  inputPoint,
  parseDraft,
  planDraft,
  readDraft,
  readSavedDraft,
  type RouteDraft,
} from './routeDraft';
import { RouteLibrary } from './RouteLibrary';
import { MapPicker } from './MapPicker';

type View = { kind: 'manage' } | { kind: 'edit'; from: string };

export function RoutePanel({
  state,
  busy,
  scope,
  onCommand,
  onScope,
}: {
  state: State | null;
  busy: boolean;
  scope: Scope;
  onCommand: (command: Command) => Promise<void>;
  onScope: () => void;
}) {
  const [draft, setDraft] = useState(() => readDraft(state?.route?.plan ?? undefined));
  const [error, setError] = useState('');
  const [imports, setImports] = useState<ImportedRoute[]>([]);
  const [selectedImport, setSelectedImport] = useState(0);
  const [reading, setReading] = useState(false);
  const [mapPoint, setMapPoint] = useState<{ index: number; initial: Position | null } | null>(
    null,
  );
  const [mapRoute, setMapRoute] = useState<Position[] | null>(null);

  const [view, setView] = useState<View>({ kind: 'manage' });

  const [savedDraft, setSavedDraft] = useState(() => readSavedDraft());
  const locked = busy || !!state?.requested_active;
  const route = state?.route;
  useEffect(() => {
    if (route?.plan) setDraft(readDraft(route.plan));
  }, [!!route]);
  function save(next: RouteDraft) {
    setDraft(next);
    setSavedDraft(next);
    try {
      localStorage.setItem(draftKey, JSON.stringify(next));
      setError('');
    } catch {
      setError('路线草稿保存失败');
    }
  }

  function createNew() {
    try {
      localStorage.removeItem(draftKey);
    } catch {
      /* Keep the current session usable when browser storage is unavailable. */
    }
    setDraft({
      points: [blankPoint(), blankPoint()],
      speed: '5.4',
      repeatCount: '1',
      repeatDelay: '0',
    });
    setSavedDraft(null);
    setImports([]);
    setError('');
    setView({ kind: 'edit', from: '新建路线' });
  }

  function resumeDraft() {
    const saved = readSavedDraft();
    if (!saved) {
      setError('草稿已经不存在了');
      setSavedDraft(null);
      return;
    }
    setDraft(saved);
    setImports([]);
    setError('');
    setView({ kind: 'edit', from: '上次的草稿' });
  }
  function editSaved(plan: RoutePlan, name: string) {
    const next = planDraft(plan);
    setDraft(next);
    try {
      localStorage.setItem(draftKey, JSON.stringify(next));
    } catch {
      /* Keep the current session usable when browser storage is unavailable. */
    }
    setImports([]);
    setError('');
    setView({ kind: 'edit', from: name });
  }
  function move(index: number, offset: number) {
    const points = [...draft.points];
    [points[index], points[index + offset]] = [points[index + offset], points[index]];
    save({ ...draft, points });
  }
  async function readFile(file: File) {
    setReading(true);
    setImports([]);
    try {
      if (file.size > 2 * 1024 * 1024) throw new Error('文件过大，请选择 2 MB 以内的 GPX 文件');
      const text = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(new Error('文件读取失败，请重新选择'));
        reader.readAsText(file);
      });
      setImports(parseGpx(text));
      setSelectedImport(0);
      setError('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setReading(false);
    }
  }
  async function start() {
    try {
      const plan = parseDraft(draft);
      setError('');
      await onCommand({ op: 'start_route', route: plan, scope });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  if (view.kind === 'manage')
    return (
      <>
        <section className="settings-card route-status">
          <div className="section-heading">
            <h2>
              {route
                ? route.completed
                  ? '已到达终点'
                  : route.paused
                    ? '路线已暂停'
                    : (route.waiting_seconds ?? 0) > 0
                      ? '等待下一轮'
                      : '正在沿路线移动'
                : '路线管理'}
            </h2>
            <button
              className="icon-button"
              aria-label="作用范围"
              title="作用范围"
              onClick={onScope}
            >
              <Route size={20} />
            </button>
          </div>
          <p>新建一条路线，或者选一条已保存的直接开始。关闭面板后仍会继续运行。</p>
          {route && (
            <>
              <progress aria-label="路线进度" value={route.distance} max={route.total_distance} />
              <p>
                {(route.plan?.repeat_count ?? 1) > 1 && (
                  <>
                    第 {route.lap ?? 1} / {route.plan?.repeat_count} 次 ·{' '}
                  </>
                )}
                {Math.round(route.distance)} / {Math.round(route.total_distance)} m
              </p>
              {(route.waiting_seconds ?? 0) > 0 && (
                <p>
                  {route.paused
                    ? `剩余间隔 ${Math.ceil(route.waiting_seconds!)} 秒`
                    : `${Math.ceil(route.waiting_seconds!)} 秒后回到起点`}
                </p>
              )}
            </>
          )}
          <div className="route-actions">
            {route ? (
              <>
                {!route.completed && (
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={() =>
                      void onCommand({ op: route.paused ? 'resume_route' : 'pause_route' })
                    }
                  >
                    {route.paused ? <Play size={18} /> : <Pause size={18} />}
                    {route.paused ? '继续路线' : '暂停路线'}
                  </button>
                )}
                <button
                  className="primary stop"
                  disabled={busy}
                  onClick={() => void onCommand({ op: 'stop' })}
                >
                  <Square size={18} />
                  停止路线
                </button>
              </>
            ) : (
              <div className="route-actions">
                <button className="primary" disabled={locked || !state} onClick={createNew}>
                  <Plus size={18} />
                  新建路线
                </button>
                {savedDraft && (
                  <button
                    className="tonal-button"
                    disabled={locked || !state}
                    onClick={resumeDraft}
                  >
                    继续编辑草稿
                  </button>
                )}
              </div>
            )}
          </div>
          {state?.requested_active && !route && <p>先停止位置模拟，再开始路线。</p>}
          {error && (
            <p role="alert" className="form-error">
              {error}
            </p>
          )}
        </section>
        <section className="settings-card">
          <div className="section-heading">
            <h2>已保存的路线</h2>
          </div>
          <RouteLibrary
            disabled={locked}
            getPlan={() => parseDraft(draft)}
            onUse={(plan) => {
              save(planDraft(plan));
              setImports([]);
            }}
            onEdit={editSaved}
          />
        </section>
      </>
    );

  return (
    <>
      <section className="settings-card">
        <div className="screen-topbar">
          <button
            className="icon-button"
            aria-label="返回路线管理"
            onClick={() => setView({ kind: 'manage' })}
          >
            <ArrowLeft size={20} />
          </button>
          <h1 style={{ fontSize: 20 }}>编辑路线点</h1>
          <span className="row-value">{view.from}</span>
        </div>
        <div className="section-heading">
          <h2>路线点</h2>
          <label className={`text-button import-button${locked || reading ? ' disabled' : ''}`}>
            <Upload size={18} />
            {reading ? '正在读取' : '导入 GPX'}
            <input
              type="file"
              aria-label="导入 GPX"
              accept=".gpx,application/gpx+xml,application/xml,text/xml"
              disabled={locked || reading}
              onChange={(e) => {
                const file = e.target.files?.[0];
                e.target.value = '';
                if (file) void readFile(file);
              }}
            />
          </label>
        </div>
        <p>使用 WGS84 坐标，按列表顺序移动。草稿保存在当前面板中。</p>
        {error && (
          <p role="alert" className="form-error">
            {error}
          </p>
        )}
        {!!imports.length && (
          <div className="import-preview">
            <label className="field">
              选择路线
              <select
                value={selectedImport}
                disabled={locked}
                onChange={(e) => setSelectedImport(Number(e.target.value))}
              >
                {imports.map((item, index) => (
                  <option value={index} key={index}>
                    {item.name} · {item.points.length} 个点
                  </option>
                ))}
              </select>
            </label>
            <p>将替换当前路线点，速度保持不变。导入后不会自动开始。</p>
            <div className="route-actions">
              <button
                className="primary"
                disabled={locked}
                onClick={() => {
                  save({ ...draft, points: imports[selectedImport].points.map(inputPoint) });
                  setImports([]);
                }}
              >
                使用这条路线
              </button>
              <button className="text-button" onClick={() => setImports([])}>
                取消
              </button>
            </div>
          </div>
        )}
        <button
          className="text-button"
          disabled={locked}
          onClick={() => {
            try {
              const points = draft.points
                .filter((p) => p.latitude.trim() || p.longitude.trim())
                .map((p) => parsePosition(p.latitude, p.longitude, p.altitude));
              if (
                points.some(
                  (p, i) =>
                    i > 0 &&
                    p.latitude === points[i - 1].latitude &&
                    p.longitude === points[i - 1].longitude,
                )
              )
                throw new Error('相邻路线点不能相同');
              window.history.pushState({ justlocationMap: true }, '');
              setMapRoute(points);
              setError('');
            } catch (e) {
              setError(e instanceof Error ? e.message : String(e));
            }
          }}
        >
          <MapPin size={18} />
          地图规划
        </button>
        <label className="field">
          速度（km/h）
          <input
            inputMode="decimal"
            value={draft.speed}
            disabled={locked}
            onChange={(e) => save({ ...draft, speed: e.target.value })}
          />
        </label>
        <div className="coordinate-fields">
          <label className="field">
            播放次数
            <input
              inputMode="numeric"
              value={draft.repeatCount}
              disabled={locked}
              onChange={(e) => save({ ...draft, repeatCount: e.target.value })}
            />
          </label>
          {Number(draft.repeatCount) !== 1 && (
            <label className="field">
              每次间隔（秒）
              <input
                inputMode="decimal"
                value={draft.repeatDelay}
                disabled={locked}
                onChange={(e) => save({ ...draft, repeatDelay: e.target.value })}
              />
            </label>
          )}
        </div>
        {Number(draft.repeatCount) > 1 && <p>每轮结束后停在终点，间隔结束后回到起点重新出发。</p>}
        <div className="route-points">
          {draft.points.map((point, index) => (
            <div className="route-point" key={index}>
              <div className="section-heading">
                <h3>点 {index + 1}</h3>
                <div className="point-actions">
                  <button
                    className="icon-button"
                    aria-label={`在地图上选择点 ${index + 1}`}
                    title="地图选点"
                    disabled={locked}
                    onClick={() => {
                      try {
                        const initial =
                          !point.latitude.trim() && !point.longitude.trim()
                            ? null
                            : parsePosition(point.latitude, point.longitude, point.altitude);
                        window.history.pushState({ justlocationMap: true }, '');
                        setMapPoint({ index, initial });
                        setError('');
                      } catch (e) {
                        setError(e instanceof Error ? e.message : String(e));
                      }
                    }}
                  >
                    <MapPin size={18} />
                  </button>
                  <button
                    className="icon-button"
                    aria-label={`上移点 ${index + 1}`}
                    disabled={locked || index === 0}
                    onClick={() => move(index, -1)}
                  >
                    <ArrowUp size={18} />
                  </button>
                  <button
                    className="icon-button"
                    aria-label={`下移点 ${index + 1}`}
                    disabled={locked || index === draft.points.length - 1}
                    onClick={() => move(index, 1)}
                  >
                    <ArrowDown size={18} />
                  </button>
                  <button
                    className="icon-button"
                    aria-label={`删除点 ${index + 1}`}
                    disabled={locked || draft.points.length <= 2}
                    onClick={() =>
                      save({ ...draft, points: draft.points.filter((_, i) => i !== index) })
                    }
                  >
                    <Trash2 size={18} />
                  </button>
                </div>
              </div>
              <div className="coordinate-fields">
                {(['latitude', 'longitude', 'altitude'] as const).map((key, i) => (
                  <label className="field" key={key}>
                    {['纬度', '经度', '海拔（米）'][i]}
                    <input
                      aria-label={`点 ${index + 1} ${['纬度', '经度', '海拔'][i]}`}
                      inputMode="decimal"
                      disabled={locked}
                      value={point[key]}
                      onChange={(e) =>
                        save({
                          ...draft,
                          points: draft.points.map((p, n) =>
                            n === index ? { ...p, [key]: e.target.value } : p,
                          ),
                        })
                      }
                    />
                  </label>
                ))}
              </div>
            </div>
          ))}
        </div>
        <button
          className="text-button"
          disabled={locked || draft.points.length >= 128}
          onClick={() => save({ ...draft, points: [...draft.points, blankPoint()] })}
        >
          <Plus size={18} />
          添加路线点
        </button>
      </section>
      <section className="settings-card">
        <div className="route-actions">
          <button
            className="primary"
            disabled={locked || !state || (scope.mode === 'apps' && !scope.packages.length)}
            onClick={() => void start()}
          >
            <Play size={18} />
            开始路线
          </button>
        </div>
        <RouteLibrary
          disabled={locked}
          getPlan={() => parseDraft(draft)}
          onUse={(plan) => {
            save(planDraft(plan));
            setImports([]);
          }}
          onEdit={editSaved}
        />
      </section>
      {mapPoint && (
        <MapPicker
          initial={mapPoint.initial}
          onClose={() => setMapPoint(null)}
          onChoose={(p) => {
            if (!locked)
              save({
                ...draft,
                points: draft.points.map((point, i) =>
                  i === mapPoint.index ? { ...inputPoint(p), altitude: point.altitude } : point,
                ),
              });
          }}
        />
      )}
      {mapRoute && (
        <MapPicker
          onClose={() => setMapRoute(null)}
          route={{
            points: mapRoute,
            onConfirm: (points) => {
              if (!locked) {
                save({ ...draft, points: points.map(inputPoint) });
                setImports([]);
              }
            },
          }}
        />
      )}
    </>
  );
}
