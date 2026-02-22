import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

describe("UISlice.connectionState", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => {
    store = createAppStore();
  });

  it("starts with Disconnected state", () => {
    expect(store.getState().connectionState).toEqual({
      status: "Disconnected",
    });
  });

  it("setConnectionState updates to Connecting", () => {
    store.getState().setConnectionState({ status: "Connecting", attempt: 0 });
    expect(store.getState().connectionState).toEqual({
      status: "Connecting",
      attempt: 0,
    });
  });

  it("setConnectionState updates to Authenticated", () => {
    store.getState().setConnectionState({ status: "Authenticated" });
    expect(store.getState().connectionState).toEqual({
      status: "Authenticated",
    });
  });

  it("setConnectionState updates to Reconnecting", () => {
    store.getState().setConnectionState({
      status: "Reconnecting",
      attempt: 3,
      next_retry_ms: 4000,
    });
    expect(store.getState().connectionState).toEqual({
      status: "Reconnecting",
      attempt: 3,
      next_retry_ms: 4000,
    });
  });

  it("setConnectionState updates to Failed", () => {
    store.getState().setConnectionState({
      status: "Failed",
      reason: "timeout",
    });
    expect(store.getState().connectionState).toEqual({
      status: "Failed",
      reason: "timeout",
    });
  });
});
