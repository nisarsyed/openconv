import type { User } from "../types";
import type { SliceCreator } from "./index";

export interface AuthSlice {
  currentUser: User | null;
  keyPair: { publicKey: string; privateKey: string } | null;
  token: string | null;
  isAuthenticated: boolean;
  login: (user: User, keyPair: { publicKey: string; privateKey: string }, token: string) => void;
  logout: () => void;
  updateProfile: (updates: Partial<Pick<User, "displayName" | "avatarUrl">>) => void;
}

export const createAuthSlice: SliceCreator<AuthSlice> = (set) => ({
  currentUser: null,
  keyPair: null,
  token: null,
  isAuthenticated: false,

  login: (user, keyPair, token) =>
    set((draft) => {
      draft.currentUser = user;
      draft.keyPair = keyPair;
      draft.token = token;
      draft.isAuthenticated = true;
    }),

  logout: () =>
    set((draft) => {
      draft.currentUser = null;
      draft.keyPair = null;
      draft.token = null;
      draft.isAuthenticated = false;
    }),

  updateProfile: (updates) =>
    set((draft) => {
      if (draft.currentUser) {
        Object.assign(draft.currentUser, updates);
      }
    }),
});
