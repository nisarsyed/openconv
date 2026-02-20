import { useCallback, useEffect, useMemo } from "react";
import { useParams } from "react-router";
import { useAppStore } from "../../store";
import { MessageView } from "./MessageView";
import { MessageInput } from "./MessageInput";
import { TypingIndicator } from "./TypingIndicator";
import { mockSendMessage } from "../../mock/api";

const EMPTY_TYPING: string[] = [];

export function ChannelView() {
  const { channelId, guildId } = useParams<{
    channelId: string;
    guildId: string;
  }>();
  const channel = useAppStore(
    (s) => (channelId ? s.channelsById[channelId] : undefined),
  );
  const typingUserIds = useAppStore(
    (s) => (channelId ? (s.typingUsers[channelId] ?? EMPTY_TYPING) : EMPTY_TYPING),
  );

  const typingNames = useMemo(() => {
    const state = useAppStore.getState();
    return typingUserIds
      .map((uid) => state.usersById[uid]?.displayName)
      .filter(Boolean) as string[];
  }, [typingUserIds]);

  // Simulate typing indicators from other users
  useEffect(() => {
    if (!channelId || !guildId) return;

    const state = useAppStore.getState();
    const memberKeys = state.memberIdsByGuild[guildId] ?? [];
    const currentUserId = state.currentUser?.id;

    const otherUserIds = memberKeys
      .map((key) => state.membersById[key]?.userId)
      .filter((uid): uid is string => !!uid && uid !== currentUserId);

    if (otherUserIds.length === 0) return;

    let mainTimer: ReturnType<typeof setTimeout>;
    let clearTimer: ReturnType<typeof setTimeout>;

    const scheduleTyping = () => {
      mainTimer = setTimeout(() => {
        const count = Math.floor(Math.random() * 3) + 1;
        const shuffled = [...otherUserIds].sort(() => Math.random() - 0.5);
        const selected = shuffled.slice(0, count);
        useAppStore.getState().setTypingUsers(channelId, selected);

        clearTimer = setTimeout(() => {
          useAppStore.getState().setTypingUsers(channelId, []);
          scheduleTyping();
        }, 2000 + Math.random() * 2000);
      }, 5000 + Math.random() * 5000);
    };

    scheduleTyping();

    return () => {
      clearTimeout(mainTimer);
      clearTimeout(clearTimer);
      useAppStore.getState().setTypingUsers(channelId, []);
    };
  }, [channelId, guildId]);

  const handleSend = useCallback(
    async (content: string, _files: File[]) => {
      if (!channelId) return;
      try {
        await mockSendMessage(channelId, content);
      } catch {
        // Message was added optimistically by mockSendMessage before throwing
        // In a real app we'd mark it as failed
      }
    },
    [channelId],
  );

  if (!channel) {
    return (
      <div className="flex flex-1 items-center justify-center text-[var(--text-muted)]">
        Channel not found
      </div>
    );
  }

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <MessageView />
      <MessageInput onSend={handleSend} channelName={channel.name} />
      <TypingIndicator userNames={typingNames} />
    </div>
  );
}
