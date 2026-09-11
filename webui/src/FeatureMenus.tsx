import { AppWindow, Joystick, RadioTower } from 'lucide-react';
import { SwitchRow } from './Controls';

type Props = {
  /** 作用范围：开关代表"限定应用"，说明文字给出具体范围 */
  scopeLimited: boolean; scopeSummary: string; scopeLocked: boolean;
  toggleScope(): void; openScope(): void;
  /** 基站模拟 */
  cellsEnabled: boolean; cellNote: string; busy: boolean; toggleCells(next: boolean): void; openCells(): void;
  /** 摇杆 */
  joystickOpen: boolean; joystickKnown: boolean; joystickSpeed: string; setSpeed(value: string): void;
  canOpenJoystick: boolean; joystickNote: string; controlJoystick(open: boolean): void;
};

/**
 * 三个功能开关的列表。
 *
 * 之前这里是三个图标按钮加弹层：既看不出开关状态，又要点两次才能改，
 * 而且"高亮"表达的是别的东西（限定应用而非已启用）。现在改成整行开关：
 * 开关本身表达启用状态，开启时呈主题色；行的说明文字负责讲清当前取值，
 * 需要更多设置的（应用列表、基站数据、最高速度）在下方就地展开。
 */
export function FeatureMenus(props: Props) {
  return <div className="card feature-panel">
    <h2>模拟功能</h2>
    <div className="feature-list">
      <SwitchRow
        icon={<AppWindow size={20} />}
        title="作用范围"
        checked={props.scopeLimited}
        disabled={props.scopeLocked}
        summary={props.scopeSummary}
        onChange={() => props.toggleScope()}
      />
      {props.scopeLimited && <div className="feature-detail">
        <button type="button" className="text-button" disabled={props.scopeLocked} onClick={props.openScope}>选择应用</button>
      </div>}

      <SwitchRow
        icon={<RadioTower size={20} />}
        title="基站模拟"
        checked={props.cellsEnabled}
        disabled={props.busy}
        summary={props.cellNote}
        onChange={next => props.toggleCells(next)}
      />
      <div className="feature-detail">
        <button type="button" className="text-button" disabled={props.busy} onClick={props.openCells}>基站列表与运营商</button>
      </div>

      <SwitchRow
        icon={<Joystick size={20} />}
        title="摇杆"
        checked={props.joystickOpen}
        disabled={!props.joystickOpen && !props.canOpenJoystick}
        summary={props.joystickNote}
        onChange={next => props.controlJoystick(next)}
      />
      {/* 速度随时可改：关着摇杆时也能先把速度设好，不必先打开再关掉。 */}
      <div className="feature-detail">
        <label className="field">最高速度（km/h）
          <input inputMode="decimal" value={props.joystickSpeed} disabled={props.busy}
            onChange={event => props.setSpeed(event.target.value)} />
        </label>
      </div>
    </div>
  </div>;
}
