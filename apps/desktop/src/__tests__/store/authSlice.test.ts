import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

const mockUser = {
  id: "user-1",
  displayName: "Test User",
  email: "user@example.com",
  avatarUrl: null,
};

const mockKeyPair = {
  publicKey: "mock-pk-user-1",
  privateKey: "mock-sk-user-1",
};

describe("AuthSlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => { store = createAppStore(); });

  it("has null user, null token, isAuthenticated false as initial state", () => {
    const s = store.getState();
    expect(s.currentUser).toBeNull();
    expect(s.token).toBeNull();
    expect(s.isAuthenticated).toBe(false);
    expect(s.keyPair).toBeNull();
  });

  it("login sets currentUser, keyPair, token, and isAuthenticated true", () => {
    store.getState().login(mockUser, mockKeyPair, "mock-token-1");
    const s = store.getState();
    expect(s.currentUser).not.toBeNull();
    expect(s.currentUser!.email).toBe("user@example.com");
    expect(s.keyPair).not.toBeNull();
    expect(s.keyPair!.publicKey).toBeTruthy();
    expect(s.keyPair!.privateKey).toBeTruthy();
    expect(s.token).toBe("mock-token-1");
    expect(s.isAuthenticated).toBe(true);
  });

  it("logout clears all auth state", () => {
    store.getState().login(mockUser, mockKeyPair, "mock-token-1");
    store.getState().logout();
    const s = store.getState();
    expect(s.currentUser).toBeNull();
    expect(s.token).toBeNull();
    expect(s.isAuthenticated).toBe(false);
    expect(s.keyPair).toBeNull();
  });

  it("updateProfile updates currentUser fields", () => {
    store.getState().login(mockUser, mockKeyPair, "mock-token-1");
    store.getState().updateProfile({ displayName: "New Name" });
    expect(store.getState().currentUser!.displayName).toBe("New Name");
  });
});
