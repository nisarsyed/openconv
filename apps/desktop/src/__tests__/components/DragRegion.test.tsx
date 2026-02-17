import { render } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { DragRegion } from "../../components/layout/DragRegion";

vi.mock("../../hooks/usePlatform", () => ({
  usePlatform: vi.fn(),
}));

import { usePlatform } from "../../hooks/usePlatform";
const mockUsePlatform = vi.mocked(usePlatform);

describe("DragRegion", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders with data-tauri-drag-region on macOS", () => {
    mockUsePlatform.mockReturnValue("macos");
    const { container } = render(<DragRegion />);
    const dragDiv = container.querySelector("[data-tauri-drag-region]");
    expect(dragDiv).toBeInTheDocument();
  });

  it("does not render on Windows", () => {
    mockUsePlatform.mockReturnValue("windows");
    const { container } = render(<DragRegion />);
    expect(container.firstChild).toBeNull();
  });

  it("does not render on Linux", () => {
    mockUsePlatform.mockReturnValue("linux");
    const { container } = render(<DragRegion />);
    expect(container.firstChild).toBeNull();
  });

  it("does not render when platform is null", () => {
    mockUsePlatform.mockReturnValue(null);
    const { container } = render(<DragRegion />);
    expect(container.firstChild).toBeNull();
  });

  it("has appropriate height styling on macOS", () => {
    mockUsePlatform.mockReturnValue("macos");
    const { container } = render(<DragRegion />);
    const dragDiv = container.querySelector("[data-tauri-drag-region]");
    expect(dragDiv).toHaveClass("h-7");
  });
});
