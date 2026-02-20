import { create, type StateCreator } from "zustand";
import { devtools, persist, subscribeWithSelector } from "zustand/middleware";
import { immer } from "zustand/middleware/immer";
import { createAuthSlice, type AuthSlice } from "./authSlice";
import { createGuildsSlice, type GuildsSlice } from "./guildsSlice";
import { createChannelsSlice, type ChannelsSlice } from "./channelsSlice";
import { createMessagesSlice, type MessagesSlice } from "./messagesSlice";
import { createMembersSlice, type MembersSlice } from "./membersSlice";
import { createPresenceSlice, type PresenceSlice } from "./presenceSlice";
import { createUnreadSlice, type UnreadSlice } from "./unreadSlice";
import { createUISlice, type UISlice } from "./uiSlice";

export type AppStore = AuthSlice &
  GuildsSlice &
  ChannelsSlice &
  MessagesSlice &
  MembersSlice &
  PresenceSlice &
  UnreadSlice &
  UISlice;

export type SliceCreator<T> = StateCreator<
  AppStore,
  [["zustand/immer", never]],
  [],
  T
>;

const storeSlices: StateCreator<
  AppStore,
  [["zustand/immer", never]],
  [],
  AppStore
> = (...args) => ({
  ...createAuthSlice(...args),
  ...createGuildsSlice(...args),
  ...createChannelsSlice(...args),
  ...createMessagesSlice(...args),
  ...createMembersSlice(...args),
  ...createPresenceSlice(...args),
  ...createUnreadSlice(...args),
  ...createUISlice(...args),
});

export const useAppStore = create<AppStore>()(
  devtools(
    persist(
      subscribeWithSelector(immer(storeSlices)),
      {
        name: "openconv-store",
        partialize: (state) => ({
          lastVisitedGuildId: state.lastVisitedGuildId,
          lastVisitedChannelByGuild: state.lastVisitedChannelByGuild,
          theme: state.theme,
          channelSidebarVisible: state.channelSidebarVisible,
          memberListVisible: state.memberListVisible,
        }),
      },
    ),
  ),
);

export const createAppStore = () =>
  create<AppStore>()(immer(storeSlices));
