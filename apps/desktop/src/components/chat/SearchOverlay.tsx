import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";
import { useSearch } from "../../hooks/useSearch";
import { useAppStore } from "../../store";
import type { SearchScope } from "../../bindings";

interface SearchOverlayProps {
  channelId?: string;
  guildId?: string;
}

function sanitizeSnippet(html: string): string {
  return html
    .replace(/<b>/g, "\x00B")
    .replace(/<\/b>/g, "\x00/B")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\x00B/g, "<b>")
    .replace(/\x00\/B/g, "</b>");
}

const SCOPE_OPTIONS: { label: string; makeScope: (props: SearchOverlayProps) => SearchScope }[] = [
  {
    label: "This Channel",
    makeScope: (props) =>
      props.channelId ? { Channel: props.channelId } : "AllMessages",
  },
  {
    label: "This Server",
    makeScope: (props) =>
      props.guildId ? { Guild: props.guildId } : "AllMessages",
  },
  { label: "All Messages", makeScope: () => "AllMessages" },
];

function scopeLabel(scope: SearchScope): string {
  if (scope === "AllMessages") return "All Messages";
  if ("Channel" in scope) return "This Channel";
  if ("Guild" in scope) return "This Server";
  return "All Messages";
}

function formatTimestamp(ts: number): string {
  const date = new Date(ts);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffDays === 0) {
    return date.toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
    });
  }
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) {
    return date.toLocaleDateString(undefined, { weekday: "short" });
  }
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

export function SearchOverlay({ channelId, guildId }: SearchOverlayProps) {
  const navigate = useNavigate();
  const { query, setQuery, scope, setScope, results, isSearching, clear } =
    useSearch();
  const inputRef = useRef<HTMLInputElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);
  const setSearchOverlayVisible = useAppStore(
    (s) => s.setSearchOverlayVisible,
  );
  const channelsById = useAppStore((s) => s.channelsById);
  const usersById = useAppStore((s) => s.usersById);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        clear();
        setSearchOverlayVisible(false);
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [clear, setSearchOverlayVisible]);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      const target = e.target as Node;
      if (overlayRef.current && !overlayRef.current.contains(target)) {
        const searchBtn = document.querySelector('[aria-label="Search"]');
        if (searchBtn?.contains(target)) return;
        clear();
        setSearchOverlayVisible(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [clear, setSearchOverlayVisible]);

  const handleResultClick = (messageId: string, resultChannelId: string | null) => {
    clear();
    setSearchOverlayVisible(false);
    if (resultChannelId) {
      const channel = channelsById[resultChannelId];
      if (channel) {
        navigate(`/app/guild/${channel.guildId}/channel/${channel.id}`, {
          state: { scrollToMessageId: messageId },
        });
      }
    }
  };

  return (
    <div
      ref={overlayRef}
      className="absolute right-0 top-12 z-50 flex w-96 flex-col rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-primary)] shadow-xl"
    >
      <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] p-3">
        <svg
          className="h-4 w-4 shrink-0 text-[var(--text-muted)]"
          viewBox="0 0 20 20"
          fill="currentColor"
        >
          <path
            fillRule="evenodd"
            d="M9 3.5a5.5 5.5 0 100 11 5.5 5.5 0 000-11zM2 9a7 7 0 1112.452 4.391l3.328 3.329a.75.75 0 11-1.06 1.06l-3.329-3.328A7 7 0 012 9z"
            clipRule="evenodd"
          />
        </svg>
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search messages..."
          className="flex-1 bg-transparent text-sm text-[var(--text-primary)] placeholder-[var(--text-muted)] outline-none"
        />
        {query && (
          <button
            onClick={() => {
              clear();
              inputRef.current?.focus();
            }}
            className="text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
          >
            <svg className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
              <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
            </svg>
          </button>
        )}
      </div>

      <div className="flex gap-1 border-b border-[var(--border-subtle)] px-3 py-2">
        {SCOPE_OPTIONS.map((opt) => {
          const active = scopeLabel(scope) === opt.label;
          return (
            <button
              key={opt.label}
              onClick={() => setScope(opt.makeScope({ channelId, guildId }))}
              className={`rounded px-2 py-0.5 text-xs transition-colors ${
                active
                  ? "bg-[var(--interactive-active)] text-[var(--text-primary)]"
                  : "text-[var(--text-muted)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-secondary)]"
              }`}
            >
              {opt.label}
            </button>
          );
        })}
      </div>

      <div className="max-h-80 overflow-y-auto">
        {isSearching && (
          <div className="flex items-center justify-center py-8 text-sm text-[var(--text-muted)]">
            Searching...
          </div>
        )}

        {!isSearching && query && results.length === 0 && (
          <div className="flex flex-col items-center justify-center py-8 text-sm text-[var(--text-muted)]">
            <p>No results found</p>
            <p className="mt-1 text-xs">Try a different search term</p>
          </div>
        )}

        {!isSearching &&
          results.map((r) => {
            const channel = r.channelId ? channelsById[r.channelId] : null;
            const sender = usersById?.[r.senderId];
            return (
              <button
                key={r.messageId}
                onClick={() => handleResultClick(r.messageId, r.channelId)}
                className="flex w-full flex-col gap-1 border-b border-[var(--border-subtle)] px-3 py-2.5 text-left transition-colors last:border-0 hover:bg-[var(--interactive-hover)]"
              >
                <div className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
                  <span className="font-medium text-[var(--text-secondary)]">
                    {sender?.displayName ?? r.senderId}
                  </span>
                  {channel && (
                    <>
                      <span>in</span>
                      <span className="font-medium text-[var(--text-secondary)]">
                        #{channel.name}
                      </span>
                    </>
                  )}
                  <span className="ml-auto">
                    {formatTimestamp(r.createdAt)}
                  </span>
                </div>
                <p
                  className="line-clamp-2 text-sm text-[var(--text-primary)] [&>b]:font-semibold [&>b]:text-[var(--accent)]"
                  dangerouslySetInnerHTML={{ __html: sanitizeSnippet(r.snippet) }}
                />
              </button>
            );
          })}
      </div>
    </div>
  );
}
