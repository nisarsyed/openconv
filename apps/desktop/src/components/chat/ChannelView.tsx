import { useCallback } from "react";
import { useParams } from "react-router";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../../store";
import { useTypingIndicator } from "../../hooks/useTypingIndicator";
import { MessageView } from "./MessageView";
import { MessageInput } from "./MessageInput";
import { TypingIndicator } from "./TypingIndicator";
import { ConnectionBanner } from "./ConnectionBanner";

export function ChannelView() {
  const { channelId, guildId } = useParams<{
    channelId: string;
    guildId: string;
  }>();
  const channel = useAppStore((s) =>
    channelId ? s.channelsById[channelId] : undefined,
  );

  const { typingNames, onKeyPress } = useTypingIndicator(channelId ?? "");

  const handleSend = useCallback(
    async (content: string, filePaths: string[]) => {
      if (!channelId) return;
      try {
        for (const filePath of filePaths) {
          await invoke("send_file", { channelId, filePath });
        }
        if (content) {
          await invoke("send_message", {
            channelId,
            guildId,
            plaintext: content,
          });
        }
      } catch (err) {
        // The backend keeps the optimistic row and queues a retry, but the
        // error still has to reach somewhere — swallowing it silently made a
        // failing send indistinguishable from a working one.
        const message =
          err instanceof Error
            ? err.message
            : typeof err === "object" && err !== null && "message" in err
              ? String((err as { message: unknown }).message)
              : String(err);
        console.error("send failed:", message, err);
        useAppStore.getState().addNotification({
          id: crypto.randomUUID(),
          type: "error",
          message: `Couldn't send: ${message}`,
          dismissAfterMs: 8000,
        });
      }
    },
    [channelId, guildId],
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
      <ConnectionBanner />
      <MessageView />
      <MessageInput
        onSend={handleSend}
        channelName={channel.name}
        onKeyPress={onKeyPress}
      />
      <TypingIndicator userNames={typingNames} />
    </div>
  );
}
