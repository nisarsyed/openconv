import { forwardRef, useId } from "react";

export interface TextAreaProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
  error?: string;
}

export const TextArea = forwardRef<HTMLTextAreaElement, TextAreaProps>(
  function TextArea({ label, error, className = "", id: idProp, ...rest }, ref) {
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
        <textarea
          ref={ref}
          id={id}
          className={`rounded bg-[var(--bg-tertiary)] text-[var(--text-primary)] border border-[var(--border-primary)] px-2.5 py-1.5 text-sm outline-none transition-colors focus:border-[var(--bg-accent)] resize-y ${error ? "border-red-500" : ""} ${className}`}
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
