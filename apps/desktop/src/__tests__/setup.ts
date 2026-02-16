import "@testing-library/jest-dom/vitest";

// Polyfill ResizeObserver for react-resizable-panels in jsdom
if (typeof window !== "undefined" && !window.ResizeObserver) {
  window.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof window.ResizeObserver;
}
