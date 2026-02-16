import type { Role } from "../../types";

export interface MemberGroupProps {
  role: Role;
  count: number;
  children: React.ReactNode;
}

export function MemberGroup({ role, count, children }: MemberGroupProps) {
  return (
    <div data-testid="member-group">
      <h3
        data-testid="role-header"
        className="px-2 pt-4 pb-1 text-[11px] font-semibold uppercase tracking-wide text-[var(--text-muted)]"
      >
        {role.name} — {count}
      </h3>
      {children}
    </div>
  );
}
