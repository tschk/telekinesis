import type { ButtonHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "ghost" | "danger";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  children: ReactNode;
}

export function Button({
  variant = "primary",
  children,
  className = "",
  ...rest
}: Props) {
  return (
    <button
      type="button"
      className={`tk-btn tk-btn--${variant} ${className}`.trim()}
      {...rest}
    >
      {children}
    </button>
  );
}
