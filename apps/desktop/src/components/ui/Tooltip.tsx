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
    top: "bottom-full left-1/2 -translate-x-1/2 mb-2",
    bottom: "top-full left-1/2 -translate-x-1/2 mt-2",
    left: "right-full top-1/2 -translate-y-1/2 mr-2",
    right: "left-full top-1/2 -translate-y-1/2 ml-2",
  };

  return (
    <span className="relative inline-block" onMouseEnter={show} onMouseLeave={hide}>
      {cloneElement(children, { "aria-describedby": visible ? tooltipId : undefined })}
      {visible && (
        <span
          id={tooltipId}
          role="tooltip"
          className={`absolute z-50 whitespace-nowrap rounded-md px-2.5 py-1.5 text-xs font-medium text-[var(--text-primary)] bg-[var(--surface-popover)] border border-[var(--border-subtle)] shadow-[var(--shadow-md)] pointer-events-none animate-fade-in ${positionClasses[position]}`}
        >
          {content}
        </span>
      )}
    </span>
  );
}
