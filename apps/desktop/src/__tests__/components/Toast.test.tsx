import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { Toast } from "../../components/ui/Toast";
import { NotificationContainer } from "../../components/ui/NotificationContainer";
import type { Notification } from "../../types";

describe("Toast", () => {
  it("renders notification with message and type", () => {
    const notification: Notification = {
      id: "1",
      type: "success",
      message: "Saved successfully",
      dismissAfterMs: null,
    };
    render(<Toast notification={notification} onDismiss={() => {}} />);
    expect(screen.getByText("Saved successfully")).toBeInTheDocument();
  });

  it("auto-dismisses after specified timeout", () => {
    vi.useFakeTimers();
    const onDismiss = vi.fn();
    const notification: Notification = {
      id: "1",
      type: "info",
      message: "Auto dismiss",
      dismissAfterMs: 3000,
    };
    render(<Toast notification={notification} onDismiss={onDismiss} />);
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(onDismiss).toHaveBeenCalledWith("1");
    vi.useRealTimers();
  });

  it("dismiss button removes notification", async () => {
    const onDismiss = vi.fn();
    const notification: Notification = {
      id: "1",
      type: "error",
      message: "Something went wrong",
      dismissAfterMs: null,
    };
    render(<Toast notification={notification} onDismiss={onDismiss} />);
    await userEvent.click(screen.getByLabelText("Dismiss"));
    expect(onDismiss).toHaveBeenCalledWith("1");
  });
});

describe("NotificationContainer", () => {
  it("renders multiple notifications", () => {
    const notifications: Notification[] = [
      { id: "1", type: "success", message: "First", dismissAfterMs: null },
      { id: "2", type: "error", message: "Second", dismissAfterMs: null },
    ];
    render(
      <NotificationContainer notifications={notifications} onDismiss={() => {}} />,
    );
    expect(screen.getByText("First")).toBeInTheDocument();
    expect(screen.getByText("Second")).toBeInTheDocument();
  });
});
