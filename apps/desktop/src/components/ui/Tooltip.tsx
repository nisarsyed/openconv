import { useState, useRef, useCallback, useEffect, cloneElement, useId } from "react";

export interface TooltipProps {
  content: string;
  children: React.ReactElement;
  position?: "top" | "bottom" | "left" | "right";
}

export function Tooltip({ content, children, position = "top" }: TooltipProps) {
  const [visible, setVisible] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(null);
  const tooltipId = useId();

  const show = useCallback(() => {
    timerRef.current = setTimeout(() => setVisible(true), 200);
  }, []);

  const hide = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setVisible(false);
  }, []);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const positionClasses: Record<string, string> = {
    top: "bottom-full left-1/2 -translate-x-1/2 mb-1",
    bottom: "top-full left-1/2 -translate-x-1/2 mt-1",
    left: "right-full top-1/2 -translate-y-1/2 mr-1",
    right: "left-full top-1/2 -translate-y-1/2 ml-1",
  };

  return (
    <span className="relative inline-block" onMouseEnter={show} onMouseLeave={hide}>
      {cloneElement(children, { "aria-describedby": visible ? tooltipId : undefined })}
      {visible && (
        <span
          id={tooltipId}
          role="tooltip"
          className={`absolute z-50 whitespace-nowrap rounded px-2 py-1 text-xs text-[var(--text-primary)] bg-[var(--surface-popover)] border border-[var(--border-subtle)] shadow-lg pointer-events-none ${positionClasses[position]}`}
        >
          {content}
        </span>
      )}
    </span>
  );
}
