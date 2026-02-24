import { useCallback } from "react";
import { useParams } from "react-router";
import { invoke } from "@tauri-apps/api/core";
import { useTypingIndicator } from "../../hooks/useTypingIndicator";
import { MessageView } from "./MessageView";
import { MessageInput } from "./MessageInput";
import { TypingIndicator } from "./TypingIndicator";
import { ConnectionBanner } from "./ConnectionBanner";

export function DmChannelView() {
  const { dmChannelId } = useParams<{ dmChannelId: string }>();

  const { typingNames, onKeyPress } = useTypingIndicator(dmChannelId ?? "");

  const handleSend = useCallback(
    async (content: string, filePaths: string[]) => {
      if (!dmChannelId) return;
      try {
        for (const filePath of filePaths) {
          await invoke("send_dm_file", { dmChannelId, filePath });
        }
        if (content) {
          await invoke("send_dm_message", {
            dmChannelId,
            plaintext: content,
          });
        }
      } catch {
        // Send failure - backend handles optimistic updates
      }
    },
    [dmChannelId],
  );

  if (!dmChannelId) {
    return (
      <div className="flex flex-1 items-center justify-center text-[var(--text-muted)]">
        Select a conversation
      </div>
    );
  }

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <ConnectionBanner />
      <MessageView channelKey={dmChannelId} />
      <MessageInput
        onSend={handleSend}
        channelName="Direct Message"
        onKeyPress={onKeyPress}
      />
      <TypingIndicator userNames={typingNames} />
    </div>
  );
}
