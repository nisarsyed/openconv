import type { User } from "../types";
import type { SliceCreator } from "./index";

export interface AuthSlice {
  currentUser: User | null;
  keyPair: { publicKey: string; privateKey: string } | null;
  token: string | null;
  isAuthenticated: boolean;
  login: (email: string) => void;
  logout: () => void;
  updateProfile: (updates: Partial<Pick<User, "displayName" | "avatarUrl">>) => void;
}

export const createAuthSlice: SliceCreator<AuthSlice> = (set) => ({
  currentUser: null,
  keyPair: null,
  token: null,
  isAuthenticated: false,

  login: (email) =>
    set((draft) => {
      const id = crypto.randomUUID();
      const name = email.split("@")[0];
      draft.currentUser = {
        id,
        displayName: name,
        avatarUrl: null,
        email,
      };
      draft.keyPair = {
        publicKey: `mock-pk-${id}`,
        privateKey: `mock-sk-${id}`,
      };
      draft.token = `mock-token-${id}`;
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
