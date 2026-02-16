import { vi } from "vitest";

let currentPlatform = "linux";

export function setMockPlatform(platform: string) {
  currentPlatform = platform;
}

export function resetMockPlatform() {
  currentPlatform = "linux";
}

vi.mock("@tauri-apps/plugin-os", () => ({
  platform: () => Promise.resolve(currentPlatform),
  type: () => Promise.resolve("Linux"),
  arch: () => Promise.resolve("x86_64"),
  version: () => Promise.resolve("1.0.0"),
}));
