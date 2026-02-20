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
    // Match all :root { ... } blocks that are NOT :root.dark { ... }
    // and combine them (there may be multiple :root blocks)
    const rootBlocks: string[] = [];
    const regex = /:root\s*\{([^}]+)\}/gs;
    let match;
    while ((match = regex.exec(cssContent)) !== null) {
      // Exclude :root.dark by checking what's before the brace
      const before = cssContent.slice(Math.max(0, match.index - 5), match.index + 5);
      if (!before.includes(".dark")) {
        rootBlocks.push(match[1]);
      }
    }
    const lightBlock = rootBlocks.join("\n");
    expect(lightBlock).toBeTruthy();
    for (const token of DARK_TOKENS) {
      expect(lightBlock).toContain(token);
    }
  });

  it("dark and light themes have different values for background tokens", () => {
    const darkBlock = extractBlock(cssContent, ":root.dark");
    // Combine all non-dark :root blocks
    const rootBlocks: string[] = [];
    const regex = /:root\s*\{([^}]+)\}/gs;
    let match;
    while ((match = regex.exec(cssContent)) !== null) {
      const before = cssContent.slice(Math.max(0, match.index - 5), match.index + 5);
      if (!before.includes(".dark")) {
        rootBlocks.push(match[1]);
      }
    }
    const lightBlock = rootBlocks.join("\n");

    // Extract --bg-primary value from each
    const darkBg = darkBlock.match(/--bg-primary:\s*([^;]+)/)?.[1]?.trim();
    const lightBg = lightBlock.match(/--bg-primary:\s*([^;]+)/)?.[1]?.trim();

    expect(darkBg).toBeTruthy();
    expect(lightBg).toBeTruthy();
    expect(darkBg).not.toEqual(lightBg);
  });
});
