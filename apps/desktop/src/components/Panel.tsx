import type { ReactNode } from "react";

export interface PanelProps {
  title?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function Panel({ title, actions, children, className = "" }: PanelProps) {
  return (
    <section className={`tk-panel ${className}`.trim()}>
      {(title || actions) && (
        <header className="tk-panel__header">
          {title ? <h2 className="tk-panel__title">{title}</h2> : <span />}
          {actions ? <div className="tk-panel__actions">{actions}</div> : null}
        </header>
      )}
      <div className="tk-panel__body">{children}</div>
    </section>
  );
}
