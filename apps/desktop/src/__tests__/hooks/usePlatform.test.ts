import { renderHook, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/plugin-os", () => ({
  platform: vi.fn(),
}));

import { platform } from "@tauri-apps/plugin-os";
import { usePlatform } from "../../hooks/usePlatform";
const mockPlatform = vi.mocked(platform);

describe("usePlatform", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns "macos" on macOS', async () => {
    mockPlatform.mockReturnValue("macos" as ReturnType<typeof platform>);
    const { result } = renderHook(() => usePlatform());
    await waitFor(() => {
      expect(result.current).toBe("macos");
    });
  });

  it('returns "windows" on Windows', async () => {
    mockPlatform.mockReturnValue("windows" as ReturnType<typeof platform>);
    const { result } = renderHook(() => usePlatform());
    await waitFor(() => {
      expect(result.current).toBe("windows");
    });
  });

  it('returns "linux" on Linux', async () => {
    mockPlatform.mockReturnValue("linux" as ReturnType<typeof platform>);
    const { result } = renderHook(() => usePlatform());
    await waitFor(() => {
      expect(result.current).toBe("linux");
    });
  });

  it("returns null when platform() throws", async () => {
    mockPlatform.mockImplementation(() => {
      throw new Error("Not running in Tauri");
    });
    const { result } = renderHook(() => usePlatform());
    await waitFor(() => {
      expect(result.current).toBeNull();
    });
  });
});
