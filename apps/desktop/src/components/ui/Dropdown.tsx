import { useState, useRef, useEffect, cloneElement } from "react";

export interface DropdownItem {
  id: string;
  label: string;
  icon?: React.ReactNode;
  danger?: boolean;
}

export interface DropdownProps {
  trigger: React.ReactElement;
  items: DropdownItem[];
  onSelect: (itemId: string) => void;
}

export function Dropdown({ trigger, items, onSelect }: DropdownProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
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
        <ul
          role="menu"
          className="absolute left-0 top-full z-50 mt-1.5 min-w-[160px] rounded-lg bg-[var(--surface-popover)] border border-[var(--border-subtle)] py-1 shadow-[var(--shadow-lg)] animate-scale-in"
        >
          {items.map((item) => (
            <li
              key={item.id}
              role="menuitem"
              className={`flex items-center gap-2 px-3 py-1.5 text-sm cursor-pointer transition-colors mx-1 rounded-md hover:bg-[var(--interactive-hover)] ${item.danger ? "text-red-400 hover:text-red-300" : "text-[var(--text-primary)]"}`}
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
