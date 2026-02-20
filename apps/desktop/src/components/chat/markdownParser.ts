export type MarkdownSegment =
  | { type: "text"; content: string }
  | { type: "bold"; content: string }
  | { type: "italic"; content: string }
  | { type: "code"; content: string }
  | { type: "link"; url: string };

// Combined regex for all markdown tokens, applied in priority order
const TOKEN_RE = /(\*\*(.+?)\*\*)|(\*(.+?)\*)|(`(.+?)`)|(https?:\/\/[^\s]+)/g;

export function parseMarkdown(input: string): MarkdownSegment[] {
  const segments: MarkdownSegment[] = [];
  let lastIndex = 0;

  for (const match of input.matchAll(TOKEN_RE)) {
    const matchStart = match.index!;

    // Add preceding plain text
    if (matchStart > lastIndex) {
      segments.push({
        type: "text",
        content: input.slice(lastIndex, matchStart),
      });
    }

    if (match[1]) {
      // **bold**
      segments.push({ type: "bold", content: match[2] });
    } else if (match[3]) {
      // *italic*
      segments.push({ type: "italic", content: match[4] });
    } else if (match[5]) {
      // `code`
      segments.push({ type: "code", content: match[6] });
    } else if (match[7]) {
      // URL
      segments.push({ type: "link", url: match[7] });
    }

    lastIndex = matchStart + match[0].length;
  }

  // Trailing plain text
  if (lastIndex < input.length) {
    segments.push({ type: "text", content: input.slice(lastIndex) });
  }

  if (segments.length === 0) {
    segments.push({ type: "text", content: input });
  }

  return segments;
}
