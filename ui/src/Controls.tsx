import { useId, type ReactNode } from 'react';

type SwitchRowProps = {
  icon?: ReactNode;
  title: string;
  summary?: string;
  checked: boolean;
  disabled?: boolean;

  value?: string;
  onChange(next: boolean): void;
};

export function SwitchRow({
  icon,
  title,
  summary,
  checked,
  disabled,
  value,
  onChange,
}: SwitchRowProps) {
  // Use a unique ID because the same label can appear in multiple panels.
  const id = `switch-${useId()}`;
  return (
    <div className={`row switch-row${disabled ? ' disabled' : ''}`}>
      {icon && <span className="row-icon">{icon}</span>}
      <label className="row-text" htmlFor={id}>
        <span className="row-title">{title}</span>
        {summary && <span className="row-summary">{summary}</span>}
      </label>
      <span className="row-end">
        {value && <span className="row-value">{value}</span>}
        <input
          className="switch"
          id={id}
          type="checkbox"
          aria-label={title}
          checked={checked}
          disabled={disabled}
          onChange={(event) => onChange(event.target.checked)}
        />
      </span>
    </div>
  );
}

type SegmentedProps<T extends string> = {
  label: string;
  value: T;
  options: readonly { id: T; label: string }[];
  disabled?: boolean;
  onChange(next: T): void;
};

export function Segmented<T extends string>({
  label,
  value,
  options,
  disabled,
  onChange,
}: SegmentedProps<T>) {
  return (
    <div className="segmented" role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          aria-pressed={value === option.id}
          disabled={disabled}
          onClick={() => onChange(option.id)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
