import { AppWindow, RadioTower, Satellite } from 'lucide-react';
import type { Position } from './protocol';
import { useTranslate } from './useTranslate';

type Props = {
  position: Position | null;

  name: string;
  summary: string;
  active: boolean;
  busy: boolean;
  canStart: boolean;

  joystickOpen: boolean;
  joystickDisabled: boolean;

  joystickNote: string;
  joystickSpeed: string;
  setJoystickSpeed(value: string): void;

  cellsEnabled: boolean;

  scopeLimited: boolean;
  scopeCount: number;

  satelliteOn: boolean;
  onToggle(): void;
  onEdit(): void;
  onCells(): void;
  onScope(): void;
  onSatellite(): void;
  onToggleJoystick(next: boolean): void;
};

export function TargetCard(props: Props) {
  const t = useTranslate();
  const coordinates = props.position
    ? `${props.position.latitude.toFixed(6)}, ${props.position.longitude.toFixed(6)}`
    : 'NONE';
  return (
    <section className="target-card card" aria-label={t('location.target')}>
      <h3 className="target-caption">{t('location.target')}</h3>
      <button className="target-name" disabled={props.busy} onClick={props.onEdit}>
        <strong>{props.position ? props.name || t('location.custom') : 'NONE'}</strong>
        <span className="target-summary">{props.summary || 'NONE'}</span>
        {props.position && (
          <span className="target-altitude">
            {t('location.altitude', { value: props.position.altitude })}
          </span>
        )}
      </button>
      <p className="target-coordinates">
        {/* Coordinate editor inputs already own the latitude and longitude labels. */}
        <span>{t('location.coordinatesLabel')}</span>
        <output data-testid="target-coordinates">{coordinates}</output>
      </p>
      <div className="target-actions">
        <button
          className={`primary pill${props.active ? ' stop' : ''}`}
          disabled={props.busy || (!props.active && !props.canStart)}
          onClick={props.onToggle}
        >
          {props.busy
            ? t('action.processing')
            : props.active
              ? t('location.stop')
              : t('location.start')}
        </button>
        <span className="spacer" />

        <button
          className={`anchor-entry${props.scopeLimited ? ' on' : ''}`}
          onClick={props.onScope}
          disabled={props.busy}
          aria-label={props.scopeLimited ? `作用范围（已选 ${props.scopeCount} 个）` : '作用范围'}
        >
          <AppWindow size={18} />
          <span>{props.scopeLimited ? `已选 ${props.scopeCount} 个` : '作用范围'}</span>
        </button>
        <span className={`cells-label ${props.cellsEnabled ? 'on' : ''}`}>
          {t('location.cellsLabel')}
        </span>
        <button
          className="round-icon"
          aria-label="基站模拟设置"
          title="基站模拟设置"
          onClick={props.onCells}
        >
          <RadioTower size={18} />
        </button>
        <button
          className={`anchor-entry${props.satelliteOn ? ' on' : ''}`}
          onClick={props.onSatellite}
          disabled={props.busy}
        >
          <Satellite size={18} />
          <span>卫星</span>
        </button>
        <label className="joystick-toggle switch-row">
          <span>{t('feature.joystick')}</span>
          <input
            type="checkbox"
            className="switch"
            checked={props.joystickOpen}
            disabled={props.joystickDisabled}
            aria-label={t('feature.joystick')}
            onChange={(event) => props.onToggleJoystick(event.target.checked)}
          />
        </label>
      </div>
      {props.joystickNote && (
        <p className="target-note" role="status">
          {props.joystickNote}
        </p>
      )}

      {props.joystickOpen && (
        <label className="field target-speed">
          {t('feature.joystickMaxSpeed')}
          <input
            inputMode="decimal"
            value={props.joystickSpeed}
            disabled={props.busy}
            onChange={(event) => props.setJoystickSpeed(event.target.value)}
          />
        </label>
      )}
    </section>
  );
}
