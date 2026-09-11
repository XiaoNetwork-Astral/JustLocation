import { AppWindow, RadioTower } from 'lucide-react';
import { SwitchRow } from './Controls';

type Props = {
  /** 作用范围：开关代表"限定应用"，说明文字给出具体范围 */
  scopeLimited: boolean; scopeSummary: string; scopeLocked: boolean;
  toggleScope(): void; openScope(): void;
  /** 基站模拟 */
  cellsEnabled: boolean; cellNote: string; busy: boolean; toggleCells(next: boolean): void; openCells(): void;
};

/**
 * 作用范围与基站的开关列表。
 *
 * 摇杆**不在这里**：原版把摇杆开关放在目标卡的操作行里（见 `TargetCard`），
 * 与「启动模拟」「基站」同一行；之前把它挪到这里，是偏离原版的一处。
 * 作用范围是原版的「独立模拟」页入口，保留在这一组里。
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
    </div>
  </div>;
}
