import { describe, expect, it } from "vitest";
import { parseNotes } from "./notes";

describe("parseNotes", () => {
  it("reads bullets, their continuation lines and paragraphs", () => {
    const blocks = parseNotes("Intro\ntext.\n\n- **Resizable:** drag the\n  gutter.\n- Use `F7`.");
    expect(blocks.map((block) => block.kind)).toEqual(["paragraph", "item", "item"]);
    expect(blocks[0].spans).toEqual([{ text: "Intro text.", style: "plain" }]);
    expect(blocks[1].spans).toEqual([
      { text: "Resizable:", style: "bold" },
      { text: " drag the gutter.", style: "plain" },
    ]);
    expect(blocks[2].spans).toEqual([
      { text: "Use ", style: "plain" },
      { text: "F7", style: "code" },
      { text: ".", style: "plain" },
    ]);
  });

  it("shows other Markdown as written", () => {
    expect(parseNotes("a * b ** c")[0].spans).toEqual([{ text: "a * b ** c", style: "plain" }]);
  });
});
