import type { ReactNode } from 'react';

type Props = {
  icon: ReactNode;

  title: string;

  detail?: string;
  /* Omit the handler to disable row selection. */
  onClick?(): void;

  actions?: ReactNode;

  label?: string;
};

export function RowCard({ icon, title, detail, onClick, actions, label }: Props) {
  const content = (
    <>
      <span className="row-symbol">{icon}</span>
      <span className="row-text-block">
        <strong>{title}</strong>
        {detail && <small>{detail}</small>}
      </span>
    </>
  );
  return (
    <div className="row-card">
      {onClick ? (
        <button
          type="button"
          className="row-card-main"
          aria-label={label ?? title}
          onClick={onClick}
        >
          {content}
        </button>
      ) : (
        <div className="row-card-main" aria-label={label ?? title}>
          {content}
        </div>
      )}
      {actions && <span className="row-card-actions">{actions}</span>}
    </div>
  );
}
