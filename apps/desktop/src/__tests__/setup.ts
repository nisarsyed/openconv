import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// Polyfill ResizeObserver for react-resizable-panels in jsdom
if (typeof window !== "undefined" && !window.ResizeObserver) {
  window.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof window.ResizeObserver;
}

// Mock Tauri OS plugin for all tests
vi.mock("@tauri-apps/plugin-os", () => ({
  platform: vi.fn(() => "linux"),
  arch: vi.fn(() => "x86_64"),
  version: vi.fn(() => ""),
  type: vi.fn(() => "Linux"),
  locale: vi.fn(() => "en-US"),
}));
