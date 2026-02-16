import { useState } from "react";
import { useParams } from "react-router";
import { ChannelItem } from "./ChannelItem";
import { useAppStore } from "../../store";
import type { Channel } from "../../types";

interface ChannelCategoryProps {
  name: string;
  channels: Channel[];
  selectedChannelId: string | undefined;
  unreadChannelIds: Set<string>;
}

export function ChannelCategory({
  name,
  channels,
  selectedChannelId,
  unreadChannelIds,
}: ChannelCategoryProps) {
  const [collapsed, setCollapsed] = useState(false);
  const { guildId } = useParams<{ guildId: string }>();
  const openModal = useAppStore((s) => s.openModal);

  return (
    <div className="mt-4 first:mt-0">
      <div className="group flex items-center px-2">
        <button
          onClick={() => setCollapsed((c) => !c)}
          className="flex flex-1 items-center gap-0.5 text-[11px] font-semibold uppercase tracking-wide text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
        >
          <svg
            className={`h-3 w-3 transition-transform ${collapsed ? "-rotate-90" : ""}`}
            viewBox="0 0 12 12"
            fill="currentColor"
          >
            <path d="M3 4l3 3 3-3" />
          </svg>
          {name}
        </button>
        <button
          aria-label={`Create channel in ${name}`}
          onClick={() => openModal("createChannel", { guildId })}
          className="hidden rounded p-0.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] group-hover:block"
        >
          <svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        </button>
      </div>

      {!collapsed && (
        <ul role="list" className="mt-1">
          {channels.map((channel) => (
            <ChannelItem
              key={channel.id}
              channel={channel}
              isSelected={channel.id === selectedChannelId}
              isUnread={unreadChannelIds.has(channel.id)}
            />
          ))}
        </ul>
      )}
    </div>
  );
}
