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
        className="px-3 pt-5 pb-1.5 text-[11px] font-semibold tracking-wider text-[var(--text-muted)] uppercase"
      >
        {role.name} — {count}
      </h3>
      <div className="space-y-0.5 px-1.5">{children}</div>
    </div>
  );
}
