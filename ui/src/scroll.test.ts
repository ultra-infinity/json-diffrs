import { describe, expect, it } from "vitest";
import { AlignedScroll, type Segment } from "./scroll";

const LINE = 20;
// 3 equal lines, then 4 lines inserted on the right, then 3 equal lines.
const segments: Segment[] = [
  [0, 3, 0, 3],
  [3, 3, 3, 7],
  [3, 6, 7, 10],
];

describe("AlignedScroll", () => {
  const scroll = new AlignedScroll(segments, LINE);

  it("counts each block's taller side", () => {
    expect(scroll.rows).toBe(10);
  });

  it("holds the side without lines still across an insertion", () => {
    expect(scroll.lineAt(1)).toEqual([1, 1]);
    expect(scroll.lineAt(5)).toEqual([3, 5]);
    expect(scroll.lineAt(8)).toEqual([4, 8]);
  });

  it("starts and ends both panes together", () => {
    const viewport = 5 * LINE;
    expect(scroll.scrollTops(0, viewport)).toEqual([0, 0]);
    const [left, right] = scroll.scrollTops(scroll.maxScroll(), viewport);
    // At the end each pane shows its own last line at the top.
    expect(left).toBeCloseTo(5 * LINE);
    expect(right).toBeCloseTo(9 * LINE);
  });

  it("brings both last lines to the bottom together on the way", () => {
    const viewport = 5 * LINE;
    // The shared position at which the right pane's last line is at the bottom.
    const position = scroll.positionFor(1, 10 * LINE - viewport, viewport);
    const [left] = scroll.scrollTops(position, viewport);
    expect(left).toBeCloseTo(6 * LINE - viewport, 3);
  });

  it("finds the shared position back from either pane", () => {
    const viewport = 5 * LINE;
    for (const position of [0, 37, 80, scroll.maxScroll()]) {
      const [, right] = scroll.scrollTops(position, viewport);
      expect(scroll.positionFor(1, right, viewport)).toBeCloseTo(position, 3);
    }
  });

  it("locates a change's block", () => {
    expect(scroll.rowOf(3, 3)).toBe(3);
    expect(scroll.rowOf(1, 1)).toBeNull();
  });
});
