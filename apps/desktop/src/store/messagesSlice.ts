import type { Message, FileAttachment } from "../types";
import type { SliceCreator } from "./index";

export interface MessagesSlice {
  messagesById: Record<string, Message>;
  messageIdsByChannel: Record<string, string[]>;
  hasMore: Record<string, boolean>;
  loadingMessages: Record<string, boolean>;
  addMessage: (channelId: string, message: Message) => void;
  prependMessages: (channelId: string, messages: Message[]) => void;
  sendMessage: (
    channelId: string,
    content: string,
    attachments: FileAttachment[],
  ) => void;
  deleteMessage: (id: string) => void;
  editMessage: (id: string, content: string) => void;
  setLoadingMessages: (channelId: string, loading: boolean) => void;
  setHasMore: (channelId: string, hasMore: boolean) => void;
}

export const createMessagesSlice: SliceCreator<MessagesSlice> = (set, get) => ({
  messagesById: {},
  messageIdsByChannel: {},
  hasMore: {},
  loadingMessages: {},

  addMessage: (channelId, message) =>
    set((draft) => {
      draft.messagesById[message.id] = message;
      if (!draft.messageIdsByChannel[channelId]) {
        draft.messageIdsByChannel[channelId] = [];
      }
      draft.messageIdsByChannel[channelId].push(message.id);
    }),

  prependMessages: (channelId, messages) =>
    set((draft) => {
      if (!draft.messageIdsByChannel[channelId]) {
        draft.messageIdsByChannel[channelId] = [];
      }
      const newIds: string[] = [];
      for (const msg of messages) {
        draft.messagesById[msg.id] = msg;
        newIds.push(msg.id);
      }
      draft.messageIdsByChannel[channelId] = [
        ...newIds,
        ...draft.messageIdsByChannel[channelId],
      ];
    }),

  sendMessage: (channelId, content, attachments) => {
    const id = crypto.randomUUID();
    const senderId = get().currentUser?.id ?? "";
    const nonce = `mock-nonce-${crypto.randomUUID()}`;
    const message: Message = {
      id,
      channelId,
      senderId,
      content,
      encryptedContent: content,
      nonce,
      createdAt: new Date().toISOString(),
      editedAt: null,
      attachments,
    };
    get().addMessage(channelId, message);
  },

  deleteMessage: (id) =>
    set((draft) => {
      const msg = draft.messagesById[id];
      if (!msg) return;
      const channelId = msg.channelId;
      delete draft.messagesById[id];
      if (draft.messageIdsByChannel[channelId]) {
        draft.messageIdsByChannel[channelId] = draft.messageIdsByChannel[
          channelId
        ].filter((mid) => mid !== id);
      }
    }),

  editMessage: (id, content) =>
    set((draft) => {
      const msg = draft.messagesById[id];
      if (!msg) return;
      msg.content = content;
      msg.encryptedContent = content;
      msg.editedAt = new Date().toISOString();
    }),

  setLoadingMessages: (channelId, loading) =>
    set((draft) => {
      draft.loadingMessages[channelId] = loading;
    }),

  setHasMore: (channelId, hasMore) =>
    set((draft) => {
      draft.hasMore[channelId] = hasMore;
    }),
});
