import type { ReactNode } from 'react';

type Props = {
  title: string;

  count?: number;

  onAction?(): void;

  actionLabel?: string;

  action?: ReactNode;
};

export function SectionHeader({ title, count, onAction, actionLabel = '查看全部', action }: Props) {
  return (
    <div className="section-header">
      <h2>
        {title}
        {count !== undefined && <span className="section-count">{count}</span>}
      </h2>
      <span className="spacer" />
      {action ??
        (onAction && (
          <button
            type="button"
            className="section-arrow"
            aria-label={actionLabel}
            title={actionLabel}
            onClick={onAction}
          >
            <svg viewBox="0 0 24 24" width="32" height="32" aria-hidden="true" focusable="false">
              <path
                d="M9 6l6 6-6 6"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          </button>
        ))}
    </div>
  );
}
