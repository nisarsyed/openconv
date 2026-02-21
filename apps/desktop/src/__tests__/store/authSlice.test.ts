import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

const mockUser = {
  id: "user-1",
  displayName: "Test User",
  email: "user@example.com",
  avatarUrl: null,
};

describe("AuthSlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => {
    store = createAppStore();
  });

  it("has correct initial state", () => {
    const s = store.getState();
    expect(s.currentUser).toBeNull();
    expect(s.isAuthenticated).toBe(false);
    expect(s.keyPair).toBeNull();
    expect(s.isLoading).toBe(false);
    expect(s.error).toBeNull();
    expect(s.registrationStep).toBe("idle");
    expect(s.recoveryStep).toBe("idle");
    expect(s.registrationToken).toBeNull();
    expect(s.recoveryToken).toBeNull();
  });

  it("keyPair has no privateKey field", () => {
    store.setState({
      currentUser: mockUser,
      keyPair: { publicKey: "pk" },
      isAuthenticated: true,
    });
    const s = store.getState();
    expect(s.keyPair).toEqual({ publicKey: "pk" });
    expect(s.keyPair).not.toHaveProperty("privateKey");
  });

  it("logout clears all auth state and resets steps", () => {
    store.setState({
      currentUser: mockUser,
      keyPair: { publicKey: "pk" },
      isAuthenticated: true,
      registrationStep: "complete" as const,
      recoveryStep: "complete" as const,
      registrationToken: "tok",
      recoveryToken: "rtok",
    });
    // logout is async but resets state synchronously in the set() call
    store.getState().logout();
    const s = store.getState();
    expect(s.currentUser).toBeNull();
    expect(s.isAuthenticated).toBe(false);
    expect(s.keyPair).toBeNull();
    expect(s.registrationStep).toBe("idle");
    expect(s.recoveryStep).toBe("idle");
    expect(s.registrationToken).toBeNull();
    expect(s.recoveryToken).toBeNull();
  });

  it("updateProfile updates currentUser fields", () => {
    store.setState({
      currentUser: mockUser,
      keyPair: { publicKey: "pk" },
      isAuthenticated: true,
    });
    store.getState().updateProfile({ displayName: "New Name" });
    expect(store.getState().currentUser!.displayName).toBe("New Name");
  });

  it("setRegistrationStep transitions correctly", () => {
    expect(store.getState().registrationStep).toBe("idle");
    store.getState().setRegistrationStep("email_sent");
    expect(store.getState().registrationStep).toBe("email_sent");
    store.getState().setRegistrationStep("verified");
    expect(store.getState().registrationStep).toBe("verified");
    store.getState().setRegistrationStep("complete");
    expect(store.getState().registrationStep).toBe("complete");
  });

  it("setRecoveryStep transitions correctly", () => {
    expect(store.getState().recoveryStep).toBe("idle");
    store.getState().setRecoveryStep("email_sent");
    expect(store.getState().recoveryStep).toBe("email_sent");
    store.getState().setRecoveryStep("verified");
    expect(store.getState().recoveryStep).toBe("verified");
    store.getState().setRecoveryStep("complete");
    expect(store.getState().recoveryStep).toBe("complete");
  });

  it("clearError resets error to null", () => {
    store.setState({ error: "some error" });
    store.getState().clearError();
    expect(store.getState().error).toBeNull();
  });
});
