import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";

const cssContent = readFileSync(
  resolve(__dirname, "../index.css"),
  "utf-8",
);

const DARK_TOKENS = [
  "--bg-primary",
  "--bg-secondary",
  "--bg-tertiary",
  "--bg-accent",
  "--text-primary",
  "--text-secondary",
  "--text-muted",
  "--text-link",
  "--border-primary",
  "--border-subtle",
  "--interactive-normal",
  "--interactive-hover",
  "--interactive-active",
  "--status-online",
  "--status-idle",
  "--status-dnd",
  "--status-offline",
  "--surface-overlay",
  "--surface-popover",
];

function extractBlock(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const regex = new RegExp(`${escaped}\\s*\\{([^}]+)\\}`, "s");
  const match = css.match(regex);
  return match ? match[1] : "";
}

describe("Theme system", () => {
  it("dark mode defines all color tokens under :root.dark", () => {
    const darkBlock = extractBlock(cssContent, ":root.dark");
    expect(darkBlock).toBeTruthy();
    for (const token of DARK_TOKENS) {
      expect(darkBlock).toContain(token);
    }
  });

  it("light mode defines all color tokens under :root", () => {
    // Match :root { ... } but NOT :root.dark { ... }
    const rootMatch = cssContent.match(/:root\s*\{([^}]+)\}/s);
    expect(rootMatch).toBeTruthy();
    const lightBlock = rootMatch![1];
    for (const token of DARK_TOKENS) {
      expect(lightBlock).toContain(token);
    }
  });

  it("dark and light themes have different values for background tokens", () => {
    const darkBlock = extractBlock(cssContent, ":root.dark");
    const rootMatch = cssContent.match(/:root\s*\{([^}]+)\}/s);
    const lightBlock = rootMatch![1];

    // Extract --bg-primary value from each
    const darkBg = darkBlock.match(/--bg-primary:\s*([^;]+)/)?.[1]?.trim();
    const lightBg = lightBlock.match(/--bg-primary:\s*([^;]+)/)?.[1]?.trim();

    expect(darkBg).toBeTruthy();
    expect(lightBg).toBeTruthy();
    expect(darkBg).not.toEqual(lightBg);
  });
});
