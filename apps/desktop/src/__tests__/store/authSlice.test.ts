import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

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
    store.getState().login("user@example.com");
    const s = store.getState();
    expect(s.currentUser).not.toBeNull();
    expect(s.currentUser!.email).toBe("user@example.com");
    expect(s.keyPair).not.toBeNull();
    expect(s.keyPair!.publicKey).toBeTruthy();
    expect(s.keyPair!.privateKey).toBeTruthy();
    expect(s.token).toBeTruthy();
    expect(s.isAuthenticated).toBe(true);
  });

  it("logout clears all auth state", () => {
    store.getState().login("user@example.com");
    store.getState().logout();
    const s = store.getState();
    expect(s.currentUser).toBeNull();
    expect(s.token).toBeNull();
    expect(s.isAuthenticated).toBe(false);
    expect(s.keyPair).toBeNull();
  });

  it("updateProfile updates currentUser fields", () => {
    store.getState().login("user@example.com");
    store.getState().updateProfile({ displayName: "New Name" });
    expect(store.getState().currentUser!.displayName).toBe("New Name");
  });
});
