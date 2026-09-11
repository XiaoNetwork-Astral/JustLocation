import type { ReactNode } from 'react';

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
 * 语义由一个原生 checkbox 承载（它能表达 checked/disabled，键盘操作也免费），
 * 名称直接写在 checkbox 的 aria-label 上；视觉由 .switch 呈现并标记 aria-hidden，
 * 避免辅助技术读到两个同名控件。整行只有勾选框本身可点，其余区域不响应，
 * 这样点击文字不会产生"点两次反而没反应"的困惑。
 */
export function SwitchRow({ icon, title, summary, checked, disabled, value, onChange }: SwitchRowProps) {
  return <div className={`row switch-row${disabled ? ' disabled' : ''}`}>
    <input className="switch-input" type="checkbox" aria-label={title} checked={checked} disabled={disabled}
      onChange={event => onChange(event.target.checked)} />
    {icon && <span className="row-icon">{icon}</span>}
    <span className="row-text">
      <span className="row-title">{title}</span>
      {summary && <span className="row-summary">{summary}</span>}
    </span>
    <span className="row-end">
      {value && <span className="row-value">{value}</span>}
      <span className="switch" aria-hidden="true" data-checked={checked ? 'true' : 'false'} />
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
