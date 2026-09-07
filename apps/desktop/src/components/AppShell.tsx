import type { ReactNode } from "react";

export interface AppShellProps {
  topBar: ReactNode;
  sidebar: ReactNode;
  children: ReactNode;
}

export function AppShell({ topBar, sidebar, children }: AppShellProps) {
  return (
    <div className="tk-shell">
      <header className="tk-shell__top">{topBar}</header>
      <div className="tk-shell__body">
        <aside className="tk-shell__nav">{sidebar}</aside>
        <main className="tk-shell__main">{children}</main>
      </div>
    </div>
  );
}
