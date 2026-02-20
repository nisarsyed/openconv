import type { Guild } from "../types";
import type { SliceCreator } from "./index";

export interface GuildsSlice {
  guildsById: Record<string, Guild>;
  guildIds: string[];
  lastVisitedGuildId: string | null;
  setLastVisitedGuild: (id: string) => void;
  createGuild: (name: string, iconUrl: string | null) => void;
  updateGuild: (id: string, updates: Partial<Pick<Guild, "name" | "iconUrl">>) => void;
  leaveGuild: (id: string) => void;
}

export const createGuildsSlice: SliceCreator<GuildsSlice> = (set, get) => ({
  guildsById: {},
  guildIds: [],
  lastVisitedGuildId: null,

  setLastVisitedGuild: (id) =>
    set((draft) => {
      draft.lastVisitedGuildId = id;
    }),

  createGuild: (name, iconUrl) => {
    const id = crypto.randomUUID();
    const ownerId = get().currentUser?.id ?? "";
    set((draft) => {
      draft.guildsById[id] = { id, name, ownerId, iconUrl };
      draft.guildIds.push(id);
    });
  },

  updateGuild: (id, updates) =>
    set((draft) => {
      if (draft.guildsById[id]) {
        Object.assign(draft.guildsById[id], updates);
      }
    }),

  leaveGuild: (id) =>
    set((draft) => {
      delete draft.guildsById[id];
      draft.guildIds = draft.guildIds.filter((gid) => gid !== id);
      if (draft.lastVisitedGuildId === id) {
        draft.lastVisitedGuildId = null;
      }
    }),
});
