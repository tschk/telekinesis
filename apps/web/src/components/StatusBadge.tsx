type Tone = "ok" | "warn" | "danger" | "muted";

interface Props {
  tone?: Tone;
  label: string;
}

export function StatusBadge({ tone = "muted", label }: Props) {
  return <span className={`tk-badge tk-badge--${tone}`}>{label}</span>;
}
