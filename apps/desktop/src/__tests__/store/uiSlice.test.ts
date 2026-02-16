import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

describe("UISlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => { store = createAppStore(); });

  it("toggleTheme switches between dark and light", () => {
    expect(store.getState().theme).toBe("dark");
    store.getState().toggleTheme();
    expect(store.getState().theme).toBe("light");
    store.getState().toggleTheme();
    expect(store.getState().theme).toBe("dark");
  });

  it("toggleChannelSidebar flips channelSidebarVisible", () => {
    expect(store.getState().channelSidebarVisible).toBe(true);
    store.getState().toggleChannelSidebar();
    expect(store.getState().channelSidebarVisible).toBe(false);
  });

  it("toggleMemberList flips memberListVisible", () => {
    expect(store.getState().memberListVisible).toBe(true);
    store.getState().toggleMemberList();
    expect(store.getState().memberListVisible).toBe(false);
  });

  it("openModal sets activeModal, closeModal clears it", () => {
    store.getState().openModal("createGuild");
    expect(store.getState().activeModal).toEqual({ type: "createGuild" });
    store.getState().closeModal();
    expect(store.getState().activeModal).toBeNull();
  });

  it("addNotification adds to notifications array", () => {
    store.getState().addNotification({
      id: "n1",
      type: "success",
      message: "Done",
      dismissAfterMs: null,
    });
    expect(store.getState().notifications).toHaveLength(1);
  });

  it("dismissNotification removes by id", () => {
    store.getState().addNotification({
      id: "n1",
      type: "success",
      message: "Done",
      dismissAfterMs: null,
    });
    store.getState().dismissNotification("n1");
    expect(store.getState().notifications).toHaveLength(0);
  });

  it("saveScrollPosition stores position for channel", () => {
    store.getState().saveScrollPosition("ch-1", 500);
    expect(store.getState().scrollPositionByChannel["ch-1"]).toBe(500);
  });

  it("setTypingUsers updates typing state for channel", () => {
    store.getState().setTypingUsers("ch-1", ["user-1", "user-2"]);
    expect(store.getState().typingUsers["ch-1"]).toEqual(["user-1", "user-2"]);
  });
});
