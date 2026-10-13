// Release notes are the small subset of Markdown CHANGELOG.md allows:
// paragraphs and `-` bullets (continued by indented lines), with **bold**
// and `code`. Anything else shows as written.

export interface Span {
  text: string;
  style: "plain" | "bold" | "code";
}

export interface Block {
  kind: "item" | "paragraph";
  spans: Span[];
}

export function parseNotes(notes: string): Block[] {
  const blocks: { kind: Block["kind"]; text: string }[] = [];
  let last: { kind: Block["kind"]; text: string } | null = null;
  for (const line of notes.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "") {
      last = null;
    } else if (/^[-*] /.test(trimmed)) {
      last = { kind: "item", text: trimmed.slice(2) };
      blocks.push(last);
    } else if (last) {
      last.text += ` ${trimmed}`;
    } else {
      last = { kind: "paragraph", text: trimmed };
      blocks.push(last);
    }
  }
  return blocks.map(({ kind, text }) => ({ kind, spans: spans(text) }));
}

function spans(text: string): Span[] {
  const result: Span[] = [];
  const pattern = /\*\*(.+?)\*\*|`([^`]+)`/g;
  let at = 0;
  for (const match of text.matchAll(pattern)) {
    if (match.index > at) result.push({ text: text.slice(at, match.index), style: "plain" });
    result.push(match[1] !== undefined ? { text: match[1], style: "bold" } : { text: match[2], style: "code" });
    at = match.index + match[0].length;
  }
  if (at < text.length) result.push({ text: text.slice(at), style: "plain" });
  return result;
}
