import { useCallback, useMemo, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import { useParams } from "react-router";
import { useAppStore } from "../../store";
import { groupMessages, type DisplayItem } from "./groupMessages";
import { DateSeparator } from "./DateSeparator";
import { MessageGroup } from "./MessageGroup";
import { NewMessagesBar } from "./NewMessagesBar";
import { Spinner } from "../ui/Spinner";
import { mockFetchMessages } from "../../mock/api";

const EMPTY_IDS: string[] = [];

export function MessageView() {
  const { channelId } = useParams<{ channelId: string }>();
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [hasNewMessages, setHasNewMessages] = useState(false);

  const messageIds = useAppStore((s) =>
    channelId ? (s.messageIdsByChannel[channelId] ?? EMPTY_IDS) : EMPTY_IDS,
  );

  // Scope messagesById to only the IDs we need, with shallow comparison for stability
  const channelMessages = useAppStore(
    useShallow((s) => {
      if (!channelId) return EMPTY_IDS;
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

  const displayItems = useMemo(() => groupMessages(channelMessages), [channelMessages]);

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
      const olderMessages = await mockFetchMessages(channelId, oldest.createdAt);
      if (olderMessages.length > 0) {
        useAppStore.getState().prependMessages(channelId, [...olderMessages].reverse());
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

  const renderItem = useCallback(
    (_index: number, item: DisplayItem) => {
      if (item.type === "date-separator") {
        return <DateSeparator date={item.date} />;
      }
      return (
        <MessageGroup senderId={item.senderId} messages={item.messages} />
      );
    },
    [],
  );

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
        <div className="absolute left-1/2 top-2 z-10 -translate-x-1/2">
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
