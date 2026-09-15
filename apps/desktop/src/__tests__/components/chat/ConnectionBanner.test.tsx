import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { ConnectionBanner } from "../../../components/chat/ConnectionBanner";
import { useAppStore } from "../../../store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("ConnectionBanner", () => {
  beforeEach(() => {
    useAppStore.setState({
      connectionState: { status: "Disconnected" },
    });
  });

  it("is hidden when state is Authenticated", () => {
    useAppStore.setState({
      connectionState: { status: "Authenticated" },
    });
    const { container } = render(<ConnectionBanner />);
    expect(container.firstChild).toBeNull();
  });

  it("is hidden when state is Connected", () => {
    useAppStore.setState({
      connectionState: { status: "Connected" },
    });
    const { container } = render(<ConnectionBanner />);
    expect(container.firstChild).toBeNull();
  });

  it("is hidden when state is Disconnected", () => {
    useAppStore.setState({
      connectionState: { status: "Disconnected" },
    });
    const { container } = render(<ConnectionBanner />);
    expect(container.firstChild).toBeNull();
  });

  it("shows Connecting... when state is Connecting", () => {
    useAppStore.setState({
      connectionState: { status: "Connecting", attempt: 0 },
    });
    render(<ConnectionBanner />);
    expect(screen.getByText("Connecting...")).toBeInTheDocument();
  });

  it("shows Reconnecting with attempt number when state is Reconnecting", () => {
    useAppStore.setState({
      connectionState: {
        status: "Reconnecting",
        attempt: 2,
        next_retry_ms: 2000,
      },
    });
    render(<ConnectionBanner />);
    expect(screen.getByText("Reconnecting... (attempt 3)")).toBeInTheDocument();
  });

  it("shows Connection lost when state is Failed", () => {
    useAppStore.setState({
      connectionState: { status: "Failed", reason: "timeout" },
    });
    render(<ConnectionBanner />);
    expect(
      screen.getByText(
        "Connection lost. Please check your internet connection.",
      ),
    ).toBeInTheDocument();
  });

  it("shows a Retry button when state is Failed", () => {
    useAppStore.setState({
      connectionState: { status: "Failed", reason: "timeout" },
    });
    render(<ConnectionBanner />);
    expect(screen.getByText("Retry")).toBeInTheDocument();
  });
});
