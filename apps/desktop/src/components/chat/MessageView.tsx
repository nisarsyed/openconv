import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useParams } from "react-router";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../../store";
import type { Message } from "../../types";
import { groupMessages, type DisplayItem } from "./groupMessages";
import { DateSeparator } from "./DateSeparator";
import { MessageGroup } from "./MessageGroup";
import { NewMessagesBar } from "./NewMessagesBar";
import { UnreadDivider } from "./UnreadDivider";
import { Spinner } from "../ui/Spinner";

const EMPTY_IDS: string[] = [];
const EMPTY_MESSAGES: Message[] = [];

interface MessageViewProps {
  channelKey?: string;
}

export function MessageView({ channelKey }: MessageViewProps = {}) {
  const { channelId: routeChannelId } = useParams<{ channelId: string }>();
  const channelId = channelKey ?? routeChannelId;
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [hasNewMessages, setHasNewMessages] = useState(false);
  const prevMessageCountRef = useRef(0);

  const messageIds = useAppStore((s) =>
    channelId ? (s.messageIdsByChannel[channelId] ?? EMPTY_IDS) : EMPTY_IDS,
  );

  // Scope messagesById to only the IDs we need, with shallow comparison for stability
  const channelMessages = useAppStore(
    useShallow((s) => {
      if (!channelId) return EMPTY_MESSAGES;
      const ids = s.messageIdsByChannel[channelId] ?? EMPTY_IDS;
      return ids.map((id) => s.messagesById[id]).filter(Boolean);
    }),
  );

  const loading = useAppStore((s) =>
    channelId ? (s.loadingMessages[channelId] ?? false) : false,
  );
  const hasMore = useAppStore((s) =>
    channelId ? (s.hasMore[channelId] ?? false) : false,
  );

  const lastReadMessageId = useAppStore((s) =>
    channelId ? s.lastReadByChannel[channelId] : undefined,
  );

  const displayItems = useMemo(
    () => groupMessages(channelMessages, lastReadMessageId),
    [channelMessages, lastReadMessageId],
  );

  useEffect(() => {
    if (messageIds.length > prevMessageCountRef.current && !isAtBottom) {
      setHasNewMessages(true);
    }
    prevMessageCountRef.current = messageIds.length;
  }, [messageIds.length, isAtBottom]);

  const handleStartReached = useCallback(async () => {
    if (!channelId || !hasMore) return;

    const state = useAppStore.getState();
    if (state.loadingMessages[channelId]) return;

    state.setLoadingMessages(channelId, true);

    const ids = state.messageIdsByChannel[channelId] ?? [];
    const oldest = ids.length > 0 ? state.messagesById[ids[0]] : undefined;
    if (!oldest) {
      state.setLoadingMessages(channelId, false);
      return;
    }

    try {
      const olderMessages = await invoke<Message[]>("fetch_message_history", {
        channelId,
        before: oldest.createdAt,
        limit: 20,
      });
      if (olderMessages.length > 0) {
        useAppStore
          .getState()
          .prependMessages(channelId, [...olderMessages].toReversed());
      }
      if (olderMessages.length < 20) {
        useAppStore.getState().setHasMore(channelId, false);
      }
    } finally {
      useAppStore.getState().setLoadingMessages(channelId, false);
    }
  }, [channelId, hasMore]);

  const scrollToBottom = useCallback(() => {
    virtuosoRef.current?.scrollToIndex({
      index: displayItems.length - 1,
      behavior: "smooth",
    });
    setHasNewMessages(false);
  }, [displayItems.length]);

  // Auto-mark-as-read when scrolled to bottom (debounced)
  useEffect(() => {
    if (!channelId || !isAtBottom) return;
    const timer = setTimeout(() => {
      const state = useAppStore.getState();
      const ids = state.messageIdsByChannel[channelId] ?? [];
      if (ids.length === 0) return;
      const lastMsgId = ids[ids.length - 1];
      const lastMsg = state.messagesById[lastMsgId];
      if (lastMsg && lastMsgId !== state.lastReadByChannel[channelId]) {
        const createdAtMs = new Date(lastMsg.createdAt).getTime();
        state.markChannelRead(channelId, lastMsgId, createdAtMs);
      }
    }, 500);
    return () => clearTimeout(timer);
  }, [channelId, isAtBottom, messageIds.length]);

  const renderItem = useCallback((_index: number, item: DisplayItem) => {
    if (item.type === "date-separator") {
      return <DateSeparator date={item.date} />;
    }
    if (item.type === "unread-divider") {
      return <UnreadDivider />;
    }
    return <MessageGroup senderId={item.senderId} messages={item.messages} />;
  }, []);

  if (!channelId) {
    return (
      <div className="flex flex-1 items-center justify-center text-[var(--text-muted)]">
        Select a channel
      </div>
    );
  }

  return (
    <div className="relative flex-1 overflow-hidden" data-testid="message-view">
      {loading && (
        <div className="absolute top-2 left-1/2 z-10 -translate-x-1/2">
          <Spinner size="sm" />
        </div>
      )}
      <Virtuoso
        ref={virtuosoRef}
        data={displayItems}
        itemContent={renderItem}
        followOutput="smooth"
        atBottomStateChange={(atBottom) => {
          setIsAtBottom(atBottom);
          if (atBottom) setHasNewMessages(false);
        }}
        startReached={hasMore ? handleStartReached : undefined}
        style={{ height: "100%" }}
        initialTopMostItemIndex={Math.max(0, displayItems.length - 1)}
      />
      <NewMessagesBar
        visible={!isAtBottom && hasNewMessages}
        onScrollToBottom={scrollToBottom}
      />
    </div>
  );
}
