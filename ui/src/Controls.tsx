import { useId, type ReactNode } from 'react';

type SwitchRowProps = {
  icon?: ReactNode;
  title: string;
  summary?: string;
  checked: boolean;
  disabled?: boolean;
  /** 右侧开关之外还想显示的说明文字 */
  value?: string;
  onChange(next: boolean): void;
};

/**
 * 整行可点的开关行。
 *
 * 开关就是一个 `input[type=checkbox]`，用 CSS（`.switch`）把它画成开关样子：
 * 这样 checked / disabled / 键盘空格 / 点击全部由浏览器原生处理，不需要
 * 隐藏输入框再用 label 转发点击——那种结构在真机上出现过"点两次相互抵消"和
 * 被其他 CSS 覆盖成默认复选框的问题。标题用 label 关联，点文字也能切换。
 */
export function SwitchRow({ icon, title, summary, checked, disabled, value, onChange }: SwitchRowProps) {
  // id 必须唯一：同一标题可能在页面上出现两次（首页与基站面板都有"基站模拟"）。
  const id = `switch-${useId()}`;
  return <div className={`row switch-row${disabled ? ' disabled' : ''}`}>
    {icon && <span className="row-icon">{icon}</span>}
    <label className="row-text" htmlFor={id}>
      <span className="row-title">{title}</span>
      {summary && <span className="row-summary">{summary}</span>}
    </label>
    <span className="row-end">
      {value && <span className="row-value">{value}</span>}
      <input className="switch" id={id} type="checkbox" aria-label={title} checked={checked} disabled={disabled}
        onChange={event => onChange(event.target.checked)} />
    </span>
  </div>;
}

type SegmentedProps<T extends string> = {
  label: string;
  value: T;
  options: readonly { id: T; label: string }[];
  disabled?: boolean;
  onChange(next: T): void;
};

/** 分段控件：用于二选一/三选一的互斥设置。 */
export function Segmented<T extends string>({ label, value, options, disabled, onChange }: SegmentedProps<T>) {
  return <div className="segmented" role="group" aria-label={label}>
    {options.map(option => <button
      key={option.id}
      type="button"
      aria-pressed={value === option.id}
      disabled={disabled}
      onClick={() => onChange(option.id)}
    >{option.label}</button>)}
  </div>;
}
