import { describe, it, expect } from "vitest";
import { parseMarkdown } from "../../../components/chat/markdownParser";

describe("parseMarkdown", () => {
  it("returns plain text unchanged", () => {
    const result = parseMarkdown("hello world");
    expect(result).toEqual([{ type: "text", content: "hello world" }]);
  });

  it("parses **bold** text", () => {
    const result = parseMarkdown("this is **bold** text");
    expect(result).toContainEqual({ type: "bold", content: "bold" });
    expect(result).toContainEqual({ type: "text", content: "this is " });
    expect(result).toContainEqual({ type: "text", content: " text" });
  });

  it("parses *italic* text", () => {
    const result = parseMarkdown("this is *italic* text");
    expect(result).toContainEqual({ type: "italic", content: "italic" });
  });

  it("parses `inline code`", () => {
    const result = parseMarkdown("use `console.log()` here");
    expect(result).toContainEqual({ type: "code", content: "console.log()" });
  });

  it("parses URLs into link segments", () => {
    const result = parseMarkdown("visit https://example.com today");
    expect(result).toContainEqual({ type: "link", url: "https://example.com" });
  });

  it("handles multiple formatting in one string", () => {
    const result = parseMarkdown("**bold** and *italic* and `code`");
    expect(result).toContainEqual({ type: "bold", content: "bold" });
    expect(result).toContainEqual({ type: "italic", content: "italic" });
    expect(result).toContainEqual({ type: "code", content: "code" });
  });

  it("does not crash on nested markdown", () => {
    // Should not throw
    const result = parseMarkdown("***bold italic***");
    expect(result.length).toBeGreaterThan(0);
  });
});
