// How the two panes of the diff scroll together, without filler lines.
//
// Both panes move through one shared "aligned" space in which each block
// (a run of unchanged lines, or one change) is as tall as its taller side.
// An anchor slides from the top of the view (scrolled to the top) to the
// bottom (scrolled to the end); each pane is placed so that the line it shows
// at the anchor's aligned row sits at the anchor. Both documents therefore
// start and end together and only drift apart around blocks of different
// height, which is what bends the ribbons between them. As in VS Code, the
// end is past the last line: scrolled all the way, both last lines are at
// the top, so the end of a long document can be read away from the edge.

/** `[left start, left end, right start, right end]`, 0-based, ends exclusive. */
export type Segment = [number, number, number, number];

export class AlignedScroll {
  /** First aligned row of each segment. */
  private readonly starts: number[];
  /** Rows in aligned space: each segment counts its taller side. */
  readonly rows: number;

  constructor(
    private readonly segments: Segment[],
    private readonly lineHeight: number,
  ) {
    let row = 0;
    this.starts = segments.map((segment) => {
      const start = row;
      row += height(segment);
      return start;
    });
    this.rows = row;
  }

  /** The furthest the shared position can go: where both last lines reach
   *  the top. Panes need Monaco's `scrollBeyondLastLine` to get there. */
  maxScroll() {
    return Math.max(0, (this.rows - 1) * this.lineHeight);
  }

  /** The (fractional, 0-based) line each side shows at aligned row `row`.
   *  Inside a block, each side advances in proportion to its own length. */
  lineAt(row: number): [number, number] {
    const at = Math.max(0, row);
    let low = 0;
    let high = this.segments.length - 1;
    if (high < 0) return [at, at];
    while (low < high) {
      const middle = (low + high + 1) >> 1;
      if (this.starts[middle] <= at) low = middle;
      else high = middle - 1;
    }
    const [leftStart, leftEnd, rightStart, rightEnd] = this.segments[low];
    const rows = Math.max(1, height(this.segments[low]));
    const t = Math.min(1, Math.max(0, (at - this.starts[low]) / rows));
    // Past the end, both sides keep moving one line per row.
    const overflow = Math.max(0, at - (this.starts[low] + height(this.segments[low])));
    return [
      leftStart + t * (leftEnd - leftStart) + overflow,
      rightStart + t * (rightEnd - rightStart) + overflow,
    ];
  }

  /** Each pane's scrollTop for shared position `position`. */
  scrollTops(position: number, viewport: number): [number, number] {
    const max = this.maxScroll();
    const fraction = max > 0 ? position / max : 0;
    const anchor = fraction * Math.max(0, viewport - this.lineHeight);
    const [left, right] = this.lineAt((position + anchor) / this.lineHeight);
    return [left * this.lineHeight - anchor, right * this.lineHeight - anchor];
  }

  /** The shared position at which pane `side` (0 left, 1 right) has
   *  scrollTop `scrollTop`: used when a pane scrolled itself (keys, its
   *  scrollbar). */
  positionFor(side: 0 | 1, scrollTop: number, viewport: number) {
    let low = 0;
    let high = this.maxScroll();
    for (let step = 0; step < 40; step++) {
      const middle = (low + high) / 2;
      if (this.scrollTops(middle, viewport)[side] < scrollTop) low = middle;
      else high = middle;
    }
    return high;
  }

  /** The aligned row where the block of a change starting at these lines
   *  begins, or null. */
  rowOf(leftStart: number, rightStart: number) {
    const index = this.segments.findIndex(
      ([left, leftEnd, right, rightEnd]) =>
        left === leftStart && right === rightStart && (leftEnd > left || rightEnd > right),
    );
    return index < 0 ? null : this.starts[index];
  }
}

function height([leftStart, leftEnd, rightStart, rightEnd]: Segment) {
  return Math.max(leftEnd - leftStart, rightEnd - rightStart);
}
