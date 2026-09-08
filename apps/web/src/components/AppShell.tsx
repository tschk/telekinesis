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
      <header className="tk-shell__header">
        <div className="tk-shell__brand">
          <span className="tk-shell__mark" aria-hidden />
          <h1 className="tk-shell__title">Telekinesis</h1>
          <span className="tk-shell__subtitle">web control plane</span>
        </div>
        <div className="tk-shell__meta">
          <StatusBadge tone={tone} label={label} />
          <a className="tk-link" href={billingUrl}>
            Billing (stub)
          </a>
        </div>
      </header>
      <main className="tk-shell__main">{children}</main>
    </div>
  );
}
