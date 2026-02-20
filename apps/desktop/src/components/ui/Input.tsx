import { forwardRef, useId } from "react";

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { label, error, className = "", id: idProp, ...rest },
  ref,
) {
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
      <input
        ref={ref}
        id={id}
        className={`rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] px-3 py-2 text-sm text-[var(--text-primary)] transition-all duration-200 outline-none placeholder:text-[var(--text-muted)] focus:border-[var(--bg-accent)] focus:shadow-[0_0_0_3px_var(--bg-accent-subtle)] ${error ? "border-red-500 focus:border-red-500 focus:shadow-[0_0_0_3px_rgba(239,68,68,0.1)]" : ""} ${className}`}
        {...rest}
      />
      {error && (
        <p role="alert" className="text-xs text-red-400">
          {error}
        </p>
      )}
    </div>
  );
});
