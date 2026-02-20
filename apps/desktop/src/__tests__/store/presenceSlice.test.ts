import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

describe("PresenceSlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => { store = createAppStore(); });

  it("updatePresence sets status for a user", () => {
    store.getState().updatePresence("user-1", "online");
    expect(store.getState().presenceByUserId["user-1"]).toBe("online");
  });

  it("bulkUpdatePresence updates multiple users at once", () => {
    store.getState().bulkUpdatePresence({
      "user-1": "online",
      "user-2": "idle",
      "user-3": "dnd",
    });
    const s = store.getState();
    expect(s.presenceByUserId["user-1"]).toBe("online");
    expect(s.presenceByUserId["user-2"]).toBe("idle");
    expect(s.presenceByUserId["user-3"]).toBe("dnd");
  });
});
