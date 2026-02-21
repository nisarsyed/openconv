import { commands } from "../bindings";
import type { User } from "../types";
import type { SliceCreator } from "./index";

type RegistrationStep = "idle" | "email_sent" | "verified" | "complete";
type RecoveryStep = "idle" | "email_sent" | "verified" | "complete";

export interface AuthSlice {
  currentUser: User | null;
  keyPair: { publicKey: string } | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  error: string | null;
  registrationStep: RegistrationStep;
  recoveryStep: RecoveryStep;
  registrationToken: string | null;
  recoveryToken: string | null;

  login: () => Promise<void>;
  registerStart: (email: string, displayName: string) => Promise<void>;
  registerVerify: (email: string, code: string) => Promise<void>;
  registerComplete: (displayName: string) => Promise<void>;
  recoverStart: (email: string) => Promise<void>;
  recoverVerify: (email: string, code: string) => Promise<void>;
  recoverComplete: () => Promise<void>;
  logout: () => Promise<void>;
  updateProfile: (
    updates: Partial<Pick<User, "displayName" | "avatarUrl">>,
  ) => void;
  setRegistrationStep: (step: RegistrationStep) => void;
  setRecoveryStep: (step: RecoveryStep) => void;
  clearError: () => void;
}

export const createAuthSlice: SliceCreator<AuthSlice> = (set, get) => ({
  currentUser: null,
  keyPair: null,
  isAuthenticated: false,
  isLoading: false,
  error: null,
  registrationStep: "idle",
  recoveryStep: "idle",
  registrationToken: null,
  recoveryToken: null,

  login: async () => {
    set((draft) => {
      draft.isLoading = true;
      draft.error = null;
    });
    try {
      const result = await commands.authLogin();
      if (result.status === "ok") {
        set((draft) => {
          draft.isAuthenticated = true;
          draft.keyPair = { publicKey: result.data.public_key };
          draft.isLoading = false;
        });
      } else {
        set((draft) => {
          draft.error = result.error.message;
          draft.isLoading = false;
        });
      }
    } catch (e) {
      set((draft) => {
        draft.error = String(e);
        draft.isLoading = false;
      });
    }
  },

  registerStart: async (email, displayName) => {
    set((draft) => {
      draft.isLoading = true;
      draft.error = null;
    });
    try {
      const result = await commands.authRegisterStart(email, displayName);
      if (result.status === "ok") {
        set((draft) => {
          draft.registrationStep = "email_sent";
          draft.isLoading = false;
        });
      } else {
        set((draft) => {
          draft.error = result.error.message;
          draft.isLoading = false;
        });
      }
    } catch (e) {
      set((draft) => {
        draft.error = String(e);
        draft.isLoading = false;
      });
    }
  },

  registerVerify: async (email, code) => {
    set((draft) => {
      draft.isLoading = true;
      draft.error = null;
    });
    try {
      const result = await commands.authVerifyEmail(email, code);
      if (result.status === "ok") {
        set((draft) => {
          draft.registrationToken = result.data;
          draft.registrationStep = "verified";
          draft.isLoading = false;
        });
      } else {
        set((draft) => {
          draft.error = result.error.message;
          draft.isLoading = false;
        });
      }
    } catch (e) {
      set((draft) => {
        draft.error = String(e);
        draft.isLoading = false;
      });
    }
  },

  registerComplete: async (displayName) => {
    const { registrationToken } = get();
    if (!registrationToken) {
      set((draft) => {
        draft.error = "Missing registration token";
      });
      return;
    }
    set((draft) => {
      draft.isLoading = true;
      draft.error = null;
    });
    try {
      const result = await commands.authRegisterComplete(
        registrationToken,
        displayName,
      );
      if (result.status === "ok") {
        set((draft) => {
          draft.isAuthenticated = true;
          draft.keyPair = { publicKey: result.data.public_key };
          draft.registrationStep = "complete";
          draft.isLoading = false;
        });
      } else {
        set((draft) => {
          draft.error = result.error.message;
          draft.isLoading = false;
        });
      }
    } catch (e) {
      set((draft) => {
        draft.error = String(e);
        draft.isLoading = false;
      });
    }
  },

  recoverStart: async (email) => {
    set((draft) => {
      draft.isLoading = true;
      draft.error = null;
    });
    try {
      const result = await commands.authRecoverStart(email);
      if (result.status === "ok") {
        set((draft) => {
          draft.recoveryStep = "email_sent";
          draft.isLoading = false;
        });
      } else {
        set((draft) => {
          draft.error = result.error.message;
          draft.isLoading = false;
        });
      }
    } catch (e) {
      set((draft) => {
        draft.error = String(e);
        draft.isLoading = false;
      });
    }
  },

  recoverVerify: async (email, code) => {
    set((draft) => {
      draft.isLoading = true;
      draft.error = null;
    });
    try {
      const result = await commands.authRecoverVerify(email, code);
      if (result.status === "ok") {
        set((draft) => {
          draft.recoveryToken = result.data;
          draft.recoveryStep = "verified";
          draft.isLoading = false;
        });
      } else {
        set((draft) => {
          draft.error = result.error.message;
          draft.isLoading = false;
        });
      }
    } catch (e) {
      set((draft) => {
        draft.error = String(e);
        draft.isLoading = false;
      });
    }
  },

  recoverComplete: async () => {
    const { recoveryToken } = get();
    if (!recoveryToken) {
      set((draft) => {
        draft.error = "Missing recovery token";
      });
      return;
    }
    set((draft) => {
      draft.isLoading = true;
      draft.error = null;
    });
    try {
      const result = await commands.authRecoverComplete(recoveryToken);
      if (result.status === "ok") {
        set((draft) => {
          draft.isAuthenticated = true;
          draft.keyPair = { publicKey: result.data.public_key };
          draft.recoveryStep = "complete";
          draft.isLoading = false;
        });
      } else {
        set((draft) => {
          draft.error = result.error.message;
          draft.isLoading = false;
        });
      }
    } catch (e) {
      set((draft) => {
        draft.error = String(e);
        draft.isLoading = false;
      });
    }
  },

  logout: async () => {
    try {
      await commands.authLogout();
    } catch {
      // Best-effort: clear local state even if server logout fails
    }
    set((draft) => {
      draft.currentUser = null;
      draft.keyPair = null;
      draft.isAuthenticated = false;
      draft.registrationStep = "idle";
      draft.recoveryStep = "idle";
      draft.registrationToken = null;
      draft.recoveryToken = null;
      draft.error = null;
    });
  },

  updateProfile: (updates) =>
    set((draft) => {
      if (draft.currentUser) {
        Object.assign(draft.currentUser, updates);
      }
    }),

  setRegistrationStep: (step) =>
    set((draft) => {
      draft.registrationStep = step;
    }),

  setRecoveryStep: (step) =>
    set((draft) => {
      draft.recoveryStep = step;
    }),

  clearError: () =>
    set((draft) => {
      draft.error = null;
    }),
});
