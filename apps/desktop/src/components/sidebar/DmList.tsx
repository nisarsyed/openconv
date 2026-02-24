import { useCallback, useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../../store";
import type { DmChannel } from "../../types";

function formatRelativeTime(isoDate: string): string {
  const date = new Date(isoDate);
  const now = Date.now();
  const diffMs = now - date.getTime();
  const diffMinutes = Math.floor(diffMs / 60000);

  if (diffMinutes < 1) return "now";
  if (diffMinutes < 60) return `${diffMinutes}m`;
  const diffHours = Math.floor(diffMinutes / 60);
  if (diffHours < 24) return `${diffHours}h`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays}d`;
  return date.toLocaleDateString();
}

export function DmList() {
  const navigate = useNavigate();
  const { dmChannelId: selectedDmId } = useParams<{
    dmChannelId: string;
  }>();
  const [dmChannels, setDmChannels] = useState<DmChannel[]>([]);
  const [loading, setLoading] = useState(true);
  const openModal = useAppStore((s) => s.openModal);
  const unreadCountByChannel = useAppStore((s) => s.unreadCountByChannel);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const dms = await invoke<DmChannel[]>("list_dms");
        if (!cancelled) setDmChannels(dms);
      } catch {
        // Failed to load DMs
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, []);

  const handleDmClick = useCallback(
    (dmId: string) => {
      navigate(`/app/dm/${dmId}`);
    },
    [navigate],
  );

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex items-center justify-between px-3 pt-3 pb-2">
        <h2 className="text-sm font-semibold text-[var(--text-primary)]">
          Direct Messages
        </h2>
        <button
          onClick={() => openModal("newDm")}
          className="flex h-6 w-6 items-center justify-center rounded text-[var(--text-muted)] transition-colors hover:text-[var(--text-primary)]"
          aria-label="New Message"
        >
          <svg
            className="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
          >
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        </button>
      </div>

      <div className="scrollbar-thin flex-1 overflow-y-auto px-2">
        {loading && (
          <div className="flex justify-center py-4 text-sm text-[var(--text-muted)]">
            Loading...
          </div>
        )}

        {!loading && dmChannels.length === 0 && (
          <div className="px-2 py-4 text-center text-sm text-[var(--text-muted)]">
            No conversations yet
          </div>
        )}

        {dmChannels.map((dm) => {
          const unread = unreadCountByChannel[dm.id] ?? 0;
          const isSelected = dm.id === selectedDmId;

          return (
            <button
              key={dm.id}
              onClick={() => handleDmClick(dm.id)}
              className={`mb-0.5 flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors ${
                isSelected
                  ? "bg-[var(--bg-active)] text-[var(--text-primary)]"
                  : "text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]"
              }`}
            >
              <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--bg-tertiary)] text-xs font-medium text-[var(--text-muted)]">
                DM
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between">
                  <span className="truncate text-sm font-medium">
                    {dm.participantIds.join(", ")}
                  </span>
                  {dm.lastMessageAt && (
                    <span className="ml-1 shrink-0 text-xs text-[var(--text-muted)]">
                      {formatRelativeTime(dm.lastMessageAt)}
                    </span>
                  )}
                </div>
                {dm.lastMessagePreview && (
                  <p className="truncate text-xs text-[var(--text-muted)]">
                    {dm.lastMessagePreview}
                  </p>
                )}
              </div>
              {unread > 0 && (
                <span className="flex h-5 min-w-5 shrink-0 items-center justify-center rounded-full bg-[var(--bg-accent)] px-1 text-xs font-bold text-[var(--text-on-accent)]">
                  {unread > 99 ? "99+" : unread}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
