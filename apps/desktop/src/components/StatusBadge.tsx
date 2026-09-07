const KNOWN = new Set(["ready", "starting", "stopped", "error"]);

export function StatusBadge({ status }: { status: string }) {
  const key = status.toLowerCase();
  const variant = KNOWN.has(key) ? key : "unknown";
  return (
    <span className={`tk-badge tk-badge--${variant}`} title={status}>
      {status}
    </span>
  );
}
