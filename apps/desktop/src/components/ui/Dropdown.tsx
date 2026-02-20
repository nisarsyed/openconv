import { useState, useRef, useEffect, cloneElement } from "react";

export interface DropdownItem {
  id: string;
  label: string;
  icon?: React.ReactNode;
  danger?: boolean;
}

export interface DropdownProps {
  trigger: React.ReactElement<{ onClick?: () => void }>;
  items: DropdownItem[];
  onSelect: (itemId: string) => void;
}

export function Dropdown({ trigger, items, onSelect }: DropdownProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  return (
    <div ref={containerRef} className="relative inline-block">
      {cloneElement(trigger, { onClick: () => setOpen((prev) => !prev) })}
      {open && (
        <ul className="animate-scale-in absolute top-full left-0 z-50 mt-1.5 min-w-[160px] rounded-lg border border-[var(--border-subtle)] bg-[var(--surface-popover)] py-1 shadow-[var(--shadow-lg)]">
          {items.map((item) => (
            <li
              key={item.id}
              className={`mx-1 flex cursor-pointer items-center gap-2 rounded-md px-3 py-1.5 text-sm transition-colors hover:bg-[var(--interactive-hover)] ${item.danger ? "text-red-400 hover:text-red-300" : "text-[var(--text-primary)]"}`}
              onClick={() => {
                onSelect(item.id);
                setOpen(false);
              }}
            >
              {item.icon}
              {item.label}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
