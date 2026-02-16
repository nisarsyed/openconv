import { useId } from "react";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps
  extends React.SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  options: SelectOption[];
  error?: string;
}

export function Select({
  label,
  options,
  error,
  className = "",
  id: idProp,
  ...rest
}: SelectProps) {
  const autoId = useId();
  const id = idProp ?? autoId;

  return (
    <div className="flex flex-col gap-1">
      {label && (
        <label
          htmlFor={id}
          className="text-xs font-semibold uppercase text-[var(--text-secondary)]"
        >
          {label}
        </label>
      )}
      <select
        id={id}
        className={`rounded bg-[var(--bg-tertiary)] text-[var(--text-primary)] border border-[var(--border-primary)] px-2.5 py-1.5 text-sm outline-none transition-colors focus:border-[var(--bg-accent)] ${error ? "border-red-500" : ""} ${className}`}
        {...rest}
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      {error && (
        <p role="alert" className="text-xs text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
