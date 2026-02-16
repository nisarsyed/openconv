import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Avatar } from "../../components/ui/Avatar";

describe("Avatar", () => {
  it("renders image when src provided", () => {
    render(<Avatar src="https://example.com/avatar.png" name="John Doe" />);
    const img = screen.getByRole("img");
    expect(img).toHaveAttribute("src", "https://example.com/avatar.png");
  });

  it("renders initials fallback when no src", () => {
    render(<Avatar name="John Doe" />);
    expect(screen.getByText("JD")).toBeInTheDocument();
  });

  it("renders single initial for single-word name", () => {
    render(<Avatar name="Alice" />);
    expect(screen.getByText("A")).toBeInTheDocument();
  });

  it("renders with different sizes", () => {
    const { rerender } = render(<Avatar name="John" size="sm" />);
    expect(screen.getByText("J")).toBeInTheDocument();

    rerender(<Avatar name="John" size="md" />);
    expect(screen.getByText("J")).toBeInTheDocument();

    rerender(<Avatar name="John" size="lg" />);
    expect(screen.getByText("J")).toBeInTheDocument();
  });
});
