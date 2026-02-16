import type { PresenceStatus } from "../types";
import type { SliceCreator } from "./index";

export interface PresenceSlice {
  presenceByUserId: Record<string, PresenceStatus>;
  updatePresence: (userId: string, status: PresenceStatus) => void;
  bulkUpdatePresence: (updates: Record<string, PresenceStatus>) => void;
}

export const createPresenceSlice: SliceCreator<PresenceSlice> = (set) => ({
  presenceByUserId: {},

  updatePresence: (userId, status) =>
    set((draft) => {
      draft.presenceByUserId[userId] = status;
    }),

  bulkUpdatePresence: (updates) =>
    set((draft) => {
      Object.assign(draft.presenceByUserId, updates);
    }),
});
