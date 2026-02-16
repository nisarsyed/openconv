import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, afterEach, beforeEach } from "vitest";
import App from "../App";
import { useAppStore } from "../store";

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
    expect(screen.getByLabelText(/email/i)).toBeInTheDocument();
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
      currentUser: { id: "u1", displayName: "Test", email: "test@example.com", avatarUrl: null },
    });
    render(<App />);
    // AppLayout seeds mock data and redirects to a guild/channel route
    await waitFor(() => {
      expect(screen.getByTestId("guild-sidebar")).toBeInTheDocument();
    });
  });
});
