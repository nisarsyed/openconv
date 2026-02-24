import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import App from "../App";
import { useAppStore } from "../store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onCloseRequested: vi.fn().mockResolvedValue(() => {}),
    onFocusChanged: vi.fn().mockResolvedValue(() => {}),
  })),
}));

describe("App", () => {
  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState());
  });

  afterEach(() => {
    document.documentElement.classList.remove("dark", "light");
  });

  it("renders without crashing", () => {
    render(<App />);
    expect(screen.getByText("OpenConv")).toBeInTheDocument();
  });

  it("unauthenticated users see the login page by default", () => {
    render(<App />);
    expect(screen.getByRole("button", { name: /log in/i })).toBeInTheDocument();
  });

  it("applies dark class to html element on mount", async () => {
    render(<App />);
    await waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });
  });

  it("authenticated users can reach /app routes", async () => {
    useAppStore.setState({
      isAuthenticated: true,
      currentUser: {
        id: "u1",
        displayName: "Test",
        email: "test@example.com",
        avatarUrl: null,
      },
    });
    render(<App />);
    // AppLayout seeds mock data and redirects to a guild/channel route
    await waitFor(() => {
      expect(screen.getByTestId("guild-sidebar")).toBeInTheDocument();
    });
  });
});
