import type { Notification } from "../types";
import type { WsConnectionState } from "../types/ws";
import type { SliceCreator } from "./index";
import { commands } from "../bindings";

export interface NotificationSettingsState {
  notificationsEnabled: boolean;
  notificationPreviews: boolean;
  dndEnabled: boolean;
  mutedGuilds: string[];
  mutedChannels: string[];
}

export interface UISlice {
  theme: "dark" | "light";
  channelSidebarVisible: boolean;
  memberListVisible: boolean;
  activeModal: { type: string; props?: Record<string, unknown> } | null;
  typingUsers: Record<string, string[]>;
  notifications: Notification[];
  scrollPositionByChannel: Record<string, number>;
  connectionState: WsConnectionState;
  notificationSettings: NotificationSettingsState;
  toggleTheme: () => void;
  toggleChannelSidebar: () => void;
  toggleMemberList: () => void;
  setMemberListVisible: (visible: boolean) => void;
  openModal: (type: string, props?: Record<string, unknown>) => void;
  closeModal: () => void;
  setTypingUsers: (channelId: string, userIds: string[]) => void;
  addNotification: (notification: Notification) => void;
  dismissNotification: (id: string) => void;
  saveScrollPosition: (channelId: string, position: number) => void;
  getScrollPosition: (channelId: string) => number;
  setConnectionState: (state: WsConnectionState) => void;
  setNotificationsEnabled: (enabled: boolean) => void;
  setNotificationPreviews: (enabled: boolean) => void;
  setDndEnabled: (enabled: boolean) => void;
  toggleGuildMute: (guildId: string) => void;
  toggleChannelMute: (channelId: string) => void;
  loadNotificationSettings: () => Promise<void>;
}

export const createUISlice: SliceCreator<UISlice> = (set, get) => ({
  theme: "dark",
  channelSidebarVisible: true,
  memberListVisible: true,
  activeModal: null,
  typingUsers: {},
  notifications: [],
  scrollPositionByChannel: {},
  connectionState: { status: "Disconnected" },
  notificationSettings: {
    notificationsEnabled: true,
    notificationPreviews: false,
    dndEnabled: false,
    mutedGuilds: [],
    mutedChannels: [],
  },

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

  setMemberListVisible: (visible) =>
    set((draft) => {
      draft.memberListVisible = visible;
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

  getScrollPosition: (channelId) =>
    get().scrollPositionByChannel[channelId] ?? 0,

  setConnectionState: (state) =>
    set((draft) => {
      draft.connectionState = state;
    }),

  setNotificationsEnabled: (enabled) => {
    set((draft) => {
      draft.notificationSettings.notificationsEnabled = enabled;
    });
    commands
      .updateNotificationSetting("notifications_enabled", String(enabled))
      .then((result) => {
        if (result.status === "error") {
          set((draft) => {
            draft.notificationSettings.notificationsEnabled = !enabled;
          });
        }
      });
  },

  setNotificationPreviews: (enabled) => {
    set((draft) => {
      draft.notificationSettings.notificationPreviews = enabled;
    });
    commands
      .updateNotificationSetting("notification_previews", String(enabled))
      .then((result) => {
        if (result.status === "error") {
          set((draft) => {
            draft.notificationSettings.notificationPreviews = !enabled;
          });
        }
      });
  },

  setDndEnabled: (enabled) => {
    set((draft) => {
      draft.notificationSettings.dndEnabled = enabled;
    });
    commands
      .updateNotificationSetting("dnd_enabled", String(enabled))
      .then((result) => {
        if (result.status === "error") {
          set((draft) => {
            draft.notificationSettings.dndEnabled = !enabled;
          });
        }
      });
  },

  toggleGuildMute: (guildId) => {
    const current = get().notificationSettings.mutedGuilds;
    const isMuted = current.includes(guildId);
    const updated = isMuted
      ? current.filter((id) => id !== guildId)
      : [...current, guildId];
    set((draft) => {
      draft.notificationSettings.mutedGuilds = updated;
    });
    commands
      .updateNotificationSetting("muted_guilds", JSON.stringify(updated))
      .then((result) => {
        if (result.status === "error") {
          set((draft) => {
            draft.notificationSettings.mutedGuilds = current;
          });
        }
      });
  },

  toggleChannelMute: (channelId) => {
    const current = get().notificationSettings.mutedChannels;
    const isMuted = current.includes(channelId);
    const updated = isMuted
      ? current.filter((id) => id !== channelId)
      : [...current, channelId];
    set((draft) => {
      draft.notificationSettings.mutedChannels = updated;
    });
    commands
      .updateNotificationSetting("muted_channels", JSON.stringify(updated))
      .then((result) => {
        if (result.status === "error") {
          set((draft) => {
            draft.notificationSettings.mutedChannels = current;
          });
        }
      });
  },

  loadNotificationSettings: async () => {
    const result = await commands.getNotificationSettings();
    if (result.status === "ok") {
      set((draft) => {
        draft.notificationSettings = {
          notificationsEnabled: result.data.notificationsEnabled,
          notificationPreviews: result.data.notificationPreviews,
          dndEnabled: result.data.dndEnabled,
          mutedGuilds: result.data.mutedGuilds,
          mutedChannels: result.data.mutedChannels,
        };
      });
    }
  },
});
