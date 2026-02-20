import { renderHook, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { useResponsiveCollapse } from "../../hooks/useResponsiveCollapse";
import { useAppStore } from "../../store";

describe("Responsive behavior", () => {
  let originalInnerWidth: number;

  beforeEach(() => {
    originalInnerWidth = window.innerWidth;
    useAppStore.setState(useAppStore.getInitialState(), true);
    useAppStore.setState({ memberListVisible: true });
  });

  afterEach(() => {
    Object.defineProperty(window, "innerWidth", {
      value: originalInnerWidth,
      writable: true,
      configurable: true,
    });
  });

  function setWindowWidth(width: number) {
    Object.defineProperty(window, "innerWidth", {
      value: width,
      writable: true,
      configurable: true,
    });
    window.dispatchEvent(new Event("resize"));
  }

  it("collapses member list below 800px window width", async () => {
    vi.useFakeTimers();
    renderHook(() => useResponsiveCollapse());

    act(() => {
      setWindowWidth(750);
    });
    act(() => {
      vi.advanceTimersByTime(200);
    });

    expect(useAppStore.getState().memberListVisible).toBe(false);
    vi.useRealTimers();
  });

  it("keeps member list visible at 800px or above", async () => {
    vi.useFakeTimers();
    renderHook(() => useResponsiveCollapse());

    act(() => {
      setWindowWidth(800);
    });
    act(() => {
      vi.advanceTimersByTime(200);
    });

    expect(useAppStore.getState().memberListVisible).toBe(true);
    vi.useRealTimers();
  });

  it("sidebar collapse toggle still works after responsive collapse", async () => {
    vi.useFakeTimers();
    renderHook(() => useResponsiveCollapse());

    // Collapse via responsive
    act(() => {
      setWindowWidth(750);
    });
    act(() => {
      vi.advanceTimersByTime(200);
    });
    expect(useAppStore.getState().memberListVisible).toBe(false);

    // Resize back to wide
    act(() => {
      setWindowWidth(1200);
    });
    act(() => {
      vi.advanceTimersByTime(200);
    });

    // Manual toggle should still work
    act(() => {
      useAppStore.getState().toggleMemberList();
    });
    expect(useAppStore.getState().memberListVisible).toBe(true);

    vi.useRealTimers();
  });
});
