import type { ReactNode } from "react";
import { StatusBadge } from "./StatusBadge";

interface Props {
  healthOk: boolean | null;
  billingUrl: string;
  children: ReactNode;
}

export function AppShell({ healthOk, billingUrl, children }: Props) {
  const tone = healthOk === null ? "muted" : healthOk ? "ok" : "danger";
  const label =
    healthOk === null ? "API …" : healthOk ? "API healthy" : "API down";

  return (
    <div className="tk-shell">
      <div className="tk-shell__inner">
        <header className="tk-shell__header">
          <div className="tk-shell__brand">
            <h1 className="tk-shell__title">telekinesis</h1>
            <p className="tk-shell__subtitle">
              web control plane · ADE companion
            </p>
          </div>
          <nav className="tk-shell__meta" aria-label="Meta">
            <StatusBadge tone={tone} label={label} />
            <a className="tk-link" href={billingUrl}>
              Billing
            </a>
          </nav>
        </header>
        <main className="tk-shell__main">{children}</main>
        <footer className="tk-shell__footer">
          telekinesis ADE companion · API /v1 · not cloud.tk.tsc.hk
        </footer>
      </div>
    </div>
  );
}
