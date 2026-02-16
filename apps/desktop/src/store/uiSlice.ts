import type { Notification } from "../types";
import type { SliceCreator } from "./index";

export interface UISlice {
  theme: "dark" | "light";
  channelSidebarVisible: boolean;
  memberListVisible: boolean;
  activeModal: { type: string; props?: Record<string, unknown> } | null;
  typingUsers: Record<string, string[]>;
  notifications: Notification[];
  scrollPositionByChannel: Record<string, number>;
  toggleTheme: () => void;
  toggleChannelSidebar: () => void;
  toggleMemberList: () => void;
  openModal: (type: string, props?: Record<string, unknown>) => void;
  closeModal: () => void;
  setTypingUsers: (channelId: string, userIds: string[]) => void;
  addNotification: (notification: Notification) => void;
  dismissNotification: (id: string) => void;
  saveScrollPosition: (channelId: string, position: number) => void;
  getScrollPosition: (channelId: string) => number;
}

export const createUISlice: SliceCreator<UISlice> = (set, get) => ({
  theme: "dark",
  channelSidebarVisible: true,
  memberListVisible: true,
  activeModal: null,
  typingUsers: {},
  notifications: [],
  scrollPositionByChannel: {},

  toggleTheme: () =>
    set((draft) => {
      draft.theme = draft.theme === "dark" ? "light" : "dark";
    }),

  toggleChannelSidebar: () =>
    set((draft) => {
      draft.channelSidebarVisible = !draft.channelSidebarVisible;
    }),

  toggleMemberList: () =>
    set((draft) => {
      draft.memberListVisible = !draft.memberListVisible;
    }),

  openModal: (type, props) =>
    set((draft) => {
      draft.activeModal = { type, ...(props ? { props } : {}) };
    }),

  closeModal: () =>
    set((draft) => {
      draft.activeModal = null;
    }),

  setTypingUsers: (channelId, userIds) =>
    set((draft) => {
      draft.typingUsers[channelId] = userIds;
    }),

  addNotification: (notification) =>
    set((draft) => {
      draft.notifications.push(notification);
    }),

  dismissNotification: (id) =>
    set((draft) => {
      draft.notifications = draft.notifications.filter((n) => n.id !== id);
    }),

  saveScrollPosition: (channelId, position) =>
    set((draft) => {
      draft.scrollPositionByChannel[channelId] = position;
    }),

  getScrollPosition: (channelId) => get().scrollPositionByChannel[channelId] ?? 0,
});
