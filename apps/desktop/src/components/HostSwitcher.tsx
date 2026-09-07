import type { HostMode } from "../lib/types";

export interface HostSwitcherProps {
  mode: HostMode;
  onChange: (mode: HostMode) => void;
}

export function HostSwitcher({ mode, onChange }: HostSwitcherProps) {
  return (
    <div className="tk-host-switcher" role="group" aria-label="Host mode">
      <button
        type="button"
        className={mode === "local" ? "is-active" : undefined}
        aria-pressed={mode === "local"}
        onClick={() => onChange("local")}
      >
        Local
      </button>
      <button
        type="button"
        className={mode === "cloud" ? "is-active" : undefined}
        aria-pressed={mode === "cloud"}
        onClick={() => onChange("cloud")}
      >
        Cloud
      </button>
    </div>
  );
}
