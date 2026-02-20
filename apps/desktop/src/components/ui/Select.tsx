import { useId } from "react";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
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
    <div className="flex flex-col gap-1.5">
      {label && (
        <label
          htmlFor={id}
          className="text-[11px] font-semibold tracking-wider text-[var(--text-secondary)] uppercase"
        >
          {label}
        </label>
      )}
      <select
        id={id}
        className={`rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] px-3 py-2 text-sm text-[var(--text-primary)] transition-all duration-200 outline-none focus:border-[var(--bg-accent)] focus:shadow-[0_0_0_3px_var(--bg-accent-subtle)] ${error ? "border-red-500" : ""} ${className}`}
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
