import { forwardRef, useId } from "react";

export interface TextAreaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
  error?: string;
}

export const TextArea = forwardRef<HTMLTextAreaElement, TextAreaProps>(
  function TextArea(
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
        <textarea
          ref={ref}
          id={id}
          className={`resize-y rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] px-3 py-2 text-sm text-[var(--text-primary)] transition-all duration-200 outline-none focus:border-[var(--bg-accent)] focus:shadow-[0_0_0_3px_var(--bg-accent-subtle)] ${error ? "border-red-500" : ""} ${className}`}
          {...rest}
        />
        {error && (
          <p role="alert" className="text-xs text-red-400">
            {error}
          </p>
        )}
      </div>
    );
  },
);
