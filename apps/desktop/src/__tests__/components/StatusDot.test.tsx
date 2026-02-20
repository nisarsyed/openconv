import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { StatusDot } from "../../components/ui/StatusDot";

describe("StatusDot", () => {
  it("renders with Online aria-label for online status", () => {
    render(<StatusDot status="online" />);
    expect(screen.getByLabelText("Online")).toBeInTheDocument();
  });

  it("renders with Idle aria-label for idle status", () => {
    render(<StatusDot status="idle" />);
    expect(screen.getByLabelText("Idle")).toBeInTheDocument();
  });

  it("renders with Do Not Disturb aria-label for dnd status", () => {
    render(<StatusDot status="dnd" />);
    expect(screen.getByLabelText("Do Not Disturb")).toBeInTheDocument();
  });

  it("renders with Offline aria-label for offline status", () => {
    render(<StatusDot status="offline" />);
    expect(screen.getByLabelText("Offline")).toBeInTheDocument();
  });
});
