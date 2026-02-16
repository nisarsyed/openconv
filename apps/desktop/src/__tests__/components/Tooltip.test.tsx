import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { Tooltip } from "../../components/ui/Tooltip";

describe("Tooltip", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows tooltip text on hover after delay", async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(
      <Tooltip content="Helpful tip">
        <button>Hover me</button>
      </Tooltip>,
    );
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    await user.hover(screen.getByText("Hover me"));
    await act(async () => {
      vi.advanceTimersByTime(250);
    });
    expect(screen.getByRole("tooltip")).toHaveTextContent("Helpful tip");
  });

  it("hides tooltip on mouse leave", async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(
      <Tooltip content="Helpful tip">
        <button>Hover me</button>
      </Tooltip>,
    );
    await user.hover(screen.getByText("Hover me"));
    await act(async () => {
      vi.advanceTimersByTime(250);
    });
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
    await user.unhover(screen.getByText("Hover me"));
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });
});
