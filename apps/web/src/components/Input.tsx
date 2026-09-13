import type { InputHTMLAttributes } from "react";

interface Props extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
}

export function Input({ label, id, className = "", ...rest }: Props) {
  const inputId = id ?? rest.name;
  return (
    <label className="tk-field" htmlFor={inputId}>
      {label ? <span className="tk-field__label">{label}</span> : null}
      <input id={inputId} className={`tk-input ${className}`.trim()} {...rest} />
    </label>
  );
}
