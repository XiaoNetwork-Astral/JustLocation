import { RadioTower } from 'lucide-react';
import type { Position } from './control';
import { useTranslate } from './useTranslate';

type Props = {
  /** 当前目标位置；为空时按原版显示 NONE 占位 */
  position: Position | null;
  /** 位置名称与地址摘要；原版第二、三行分别是名称和"省 • 市" */
  name: string;
  summary: string;
  active: boolean;
  busy: boolean;
  canStart: boolean;
  /** 摇杆是否开着；原版这里是一个 SwitchCompat */
  joystickOpen: boolean;
  joystickDisabled: boolean;
  /** 摇杆状态提示；原版没有这一行，但搬走"模拟功能"卡片后需要有地方说明状态 */
  joystickNote: string;
  joystickSpeed: string;
  setJoystickSpeed(value: string): void;
  /** 基站入口：原版是操作行里的文字加圆形图标按钮 */
  cellsEnabled: boolean;
  onToggle(): void;
  onEdit(): void;
  onCells(): void;
  onToggleJoystick(next: boolean): void;
};

/**
 * 目标卡（原版 `ci.xml` 的核心区块）。
 *
 * 结构照原版：绿色小标题「目标位置」→ 加粗位置名 → 副值（地址摘要）→
 * 「经纬度:」加加粗绿色坐标 → 一行操作区：绿色药丸「启动模拟」在左，
 * 弹簧撑开，右侧依次是「基站」文字、圆形图标按钮与「摇杆」开关。
 *
 * 原版的摇杆与基站都在这一行里，而不是另开卡片；之前把它们挪到单独的
 * "模拟功能"卡片里，是偏离原版最明显的一处，这里改回内联。
 */
export function TargetCard(props: Props) {
  const t = useTranslate();
  const coordinates = props.position
    ? `${props.position.latitude.toFixed(6)}, ${props.position.longitude.toFixed(6)}`
    : 'NONE';
  return <section className="target-card card" aria-label={t('location.target')}>
    <h3 className="target-caption">{t('location.target')}</h3>
    <button className="target-name" disabled={props.busy} onClick={props.onEdit}>
      <strong>{props.position ? (props.name || t('location.custom')) : 'NONE'}</strong>
      <span className="target-summary">{props.summary || 'NONE'}</span>
      {props.position && <span className="target-altitude">{t('location.altitude', { value: props.position.altitude })}</span>}
    </button>
    <p className="target-coordinates">
      {/* 不在这里加 aria-label：位置编辑器已有"纬度""经度"字段，重名会让按标签查找产生歧义。 */}
      <span>{t('location.coordinatesLabel')}</span>
      <output data-testid="target-coordinates">{coordinates}</output>
    </p>
    <div className="target-actions">
      {/* 同时带 primary 类：既有浏览器用例按 .target-actions > .primary 定位主按钮。 */}
      <button className={`primary pill${props.active ? ' stop' : ''}`} disabled={props.busy || (!props.active && !props.canStart)} onClick={props.onToggle}>
        {props.busy ? t('action.processing') : props.active ? t('location.stop') : t('location.start')}
      </button>
      <span className="spacer" />
      <span className={`cells-label ${props.cellsEnabled ? 'on' : ''}`}>{t('location.cellsLabel')}</span>
      <button className="round-icon" aria-label={t('feature.cellsOpen')} title={t('feature.cellsOpen')} onClick={props.onCells}>
        <RadioTower size={18} />
      </button>
      <label className="joystick-toggle switch-row">
        <span>{t('feature.joystick')}</span>
        <input type="checkbox" className="switch" checked={props.joystickOpen} disabled={props.joystickDisabled}
          aria-label={t('feature.joystick')} onChange={event => props.onToggleJoystick(event.target.checked)} />
      </label>
    </div>
    {props.joystickNote && <p className="target-note" role="status">{props.joystickNote}</p>}
    {/* 摇杆打开后才需要设速度；原版的速度在锚定小面板里，这里收在目标卡下方。 */}
    {props.joystickOpen && <label className="field target-speed">{t('feature.joystickMaxSpeed')}
      <input inputMode="decimal" value={props.joystickSpeed} disabled={props.busy}
        onChange={event => props.setJoystickSpeed(event.target.value)} />
    </label>}
  </section>;
}
