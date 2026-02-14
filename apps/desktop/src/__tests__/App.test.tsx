import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, afterEach, vi } from "vitest";
import App from "../App";

vi.mock("../bindings", () => ({
  commands: {
    healthCheck: vi
      .fn()
      .mockResolvedValue({ version: "0.1.0", db_status: "ok" }),
  },
}));

import { commands } from "../bindings";

describe("App", () => {
  afterEach(() => {
    document.documentElement.classList.remove("dark");
    vi.mocked(commands.healthCheck).mockResolvedValue({
      version: "0.1.0",
      db_status: "ok",
    });
  });

  it("renders without crashing", async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByTestId("status-indicator")).toBeInTheDocument();
    });
  });

  it("renders the OpenConv title text", async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText("OpenConv")).toBeInTheDocument();
    });
  });

  it("mounts in dark mode (has 'dark' class on root html)", async () => {
    render(<App />);
    await waitFor(() => {
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });
  });

  it("displays a status indicator element", async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByTestId("status-indicator")).toBeInTheDocument();
    });
  });

  it("shows error state when health check fails", async () => {
    vi.mocked(commands.healthCheck).mockRejectedValueOnce(
      new Error("IPC unavailable"),
    );
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText("IPC Error")).toBeInTheDocument();
    });
  });
});
