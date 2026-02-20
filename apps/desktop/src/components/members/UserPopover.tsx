import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import type { User, Member, Role, PresenceStatus } from "../../types";
import { Avatar } from "../ui/Avatar";
import { StatusDot } from "../ui/StatusDot";

export interface UserPopoverProps {
  user: User;
  member: Member;
  roles: Role[];
  presence: PresenceStatus;
  onClose: () => void;
  anchorRect: DOMRect | null;
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

export function UserPopover({
  user,
  member,
  roles,
  presence,
  onClose,
  anchorRect,
}: UserPopoverProps) {
  const popoverRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleMouseDown(e: MouseEvent) {
      if (
        popoverRef.current &&
        !popoverRef.current.contains(e.target as Node)
      ) {
        onClose();
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        onClose();
      }
    }

    document.addEventListener("mousedown", handleMouseDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleMouseDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose]);

  const style: React.CSSProperties = { width: 300 };
  if (anchorRect) {
    const popoverWidth = 300;
    let left = anchorRect.left - popoverWidth - 8;
    if (left < 8) {
      left = anchorRect.right + 8;
    }
    let top = anchorRect.top;
    const maxTop = window.innerHeight - 400;
    if (top > maxTop) top = maxTop;
    if (top < 8) top = 8;

    style.position = "fixed";
    style.left = left;
    style.top = top;
    style.zIndex = 50;
  }

  const displayName = member.nickname ?? user.displayName;
  const highestRole = roles[0];
  const bannerColor = highestRole?.color ?? "var(--bg-accent)";

  const content = (
    <div
      ref={popoverRef}
      role="dialog"
      aria-label={`${member.nickname ?? user.displayName} profile`}
      className="animate-scale-in overflow-hidden rounded-xl border border-[var(--border-subtle)] shadow-[var(--shadow-lg)]"
      style={{ ...style, backgroundColor: "var(--surface-popover)" }}
    >
      {/* Banner */}
      <div
        className="h-16"
        style={{
          background: `linear-gradient(135deg, ${bannerColor}, ${bannerColor}88)`,
        }}
      />

      {/* Avatar + Name */}
      <div className="relative px-4 pb-3">
        <div className="-mt-8 mb-2">
          <div className="relative inline-block rounded-full ring-4 ring-[var(--surface-popover)]">
            <Avatar src={user.avatarUrl} name={displayName} size="lg" />
            <span className="absolute -right-0.5 -bottom-0.5">
              <StatusDot status={presence} size="md" />
            </span>
          </div>
        </div>

        <div className="text-lg font-bold tracking-[-0.01em] text-[var(--text-primary)]">
          {displayName}
        </div>
        <div className="text-sm text-[var(--text-muted)]">{user.email}</div>
      </div>

      <div className="divider-fade mx-4" />

      {/* Roles */}
      {roles.length > 0 && (
        <div className="px-4 py-3">
          <div className="mb-2 text-[11px] font-semibold tracking-wider text-[var(--text-muted)] uppercase">
            Roles
          </div>
          <div className="flex flex-wrap gap-1.5">
            {roles.map((role) => (
              <span
                key={role.id}
                className="inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-xs font-medium"
                style={{ borderColor: role.color + "40", color: role.color }}
              >
                <span
                  className="h-2 w-2 rounded-full"
                  style={{ backgroundColor: role.color }}
                />
                {role.name}
              </span>
            ))}
          </div>
        </div>
      )}

      <div className="divider-fade mx-4" />

      {/* Member since */}
      <div className="px-4 py-3">
        <div className="mb-1 text-[11px] font-semibold tracking-wider text-[var(--text-muted)] uppercase">
          Member Since
        </div>
        <div className="text-sm text-[var(--text-secondary)]">
          {formatDate(member.joinedAt)}
        </div>
      </div>

      <div className="divider-fade mx-4" />

      {/* Message button */}
      <div className="px-4 py-3">
        <button
          disabled
          className="w-full cursor-not-allowed rounded-lg bg-[var(--bg-tertiary)] px-3 py-2 text-sm font-medium text-[var(--text-muted)] opacity-40 transition-colors"
          title="DMs coming soon"
        >
          Message
        </button>
      </div>
    </div>
  );

  return createPortal(content, document.body);
}
