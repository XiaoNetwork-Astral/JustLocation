import { Check, Monitor, Moon, Sun } from 'lucide-react';
import type { State } from './protocol';
import { BackupPanel } from './BackupPanel';
import { Segmented } from './Controls';
import { colorModes, styleFamilies, type ColorMode, type StyleFamily } from './theme';

const channels = [
  ['gnss_hook_ready', 'GNSS 状态'],
  ['nmea_hook_ready', 'NMEA 报文'],
  ['cell_query_hook_ready', '基站查询'],
  ['cell_callback_hook_ready', '基站回调'],
  ['operator_hook_ready', '运营商名称与 PLMN'],
  ['wifi_connection_hook_ready', 'Wi-Fi 连接信息'],
  ['wifi_scan_hook_ready', 'Wi-Fi 扫描结果'],
] as const;

const readyText = (ready?: boolean, connected?: boolean) =>
  ready ? '已就绪' : connected ? '等待接口接入' : '等待系统连接';

export function SettingsPage({
  state,
  onImported,
  style,
  setStyle,
  mode,
  setMode,
}: {
  state: State | null;
  onImported(): void;
  style: StyleFamily;
  setStyle(style: StyleFamily): void;
  mode: ColorMode;
  setMode(mode: ColorMode): void;
}) {
  return (
    <>
      <section className="settings-card appearance-card">
        <h2>外观</h2>
        <p>界面风格和明暗可以分开选，随时切换。</p>
        <span className="appearance-label">界面风格</span>
        <Segmented
          label="界面风格"
          value={style}
          options={styleFamilies.map((family) => ({ id: family.id, label: family.label }))}
          onChange={(next) => setStyle(next)}
        />
        <span className="appearance-label">明暗</span>
        <div className="theme-options">
          {colorModes.map(({ id, label }) => {
            const Icon = id === 'system' ? Monitor : id === 'light' ? Sun : Moon;
            return (
              <button key={id} aria-pressed={mode === id} onClick={() => setMode(id)}>
                <Icon size={20} />
                {label}
                {mode === id && <Check size={16} />}
              </button>
            );
          })}
        </div>
      </section>
      <BackupPanel onImported={onImported} />
      <section className="settings-card">
        <h2>系统通道</h2>
        <p>这些开关决定应用能不能读到系统返回的模拟数据。</p>
        <dl>
          <div>
            <dt>系统定位</dt>
            <dd>{readyText(state?.location_hook_ready, !!state?.hook_connected)}</dd>
          </div>
          {channels.map(([key, label]) => (
            <div key={key}>
              <dt>{label}</dt>
              <dd>{state?.[key] ? '已就绪' : state ? '尚未接入' : '后台未连接'}</dd>
            </div>
          ))}
          <div>
            <dt>电话服务</dt>
            <dd>{state?.phone_connected ? '已连接' : state ? '等待连接' : '后台未连接'}</dd>
          </div>
        </dl>
      </section>
      <section className="settings-card">
        <h2>运行环境</h2>
        <dl>
          <div>
            <dt>模块</dt>
            <dd>JustLocation 0.1.0</dd>
          </div>
          <div>
            <dt>控制入口</dt>
            <dd>KernelSU WebUI</dd>
          </div>
          <div>
            <dt>后台</dt>
            <dd>{state ? '已连接' : '未连接'}</dd>
          </div>
        </dl>
      </section>
    </>
  );
}
