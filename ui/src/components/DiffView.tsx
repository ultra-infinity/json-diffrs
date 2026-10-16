// The side-by-side diff: two read-only Monaco editors showing what the Rust
// structural diff decided. Rust supplies each line's colour, the changed
// characters and the fold markers; this file draws them, scrolls both panes
// together without filler lines (see `scroll.ts`), and paints the ribbons that
// join each change to where it belongs on the other side. An insertion's
// ribbon narrows to the line on the left where the new lines go. Dragging
// the ribbon gutter moves the split between the sides.

import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { keyAt, ruleArgs, type HunkView, type LineStatus, type Run, type Side, type SideView, type View } from "../api";
import { FONT_FAMILY, FONT_SIZE, LINE_HEIGHT, monaco, monacoTheme } from "../monaco";
import { AlignedScroll } from "../scroll";
import { uiSettings, updateUiSettings } from "../uiSettings";
import { ContextMenu, type MenuEntry } from "./Menu";

type Editor = monaco.editor.IStandaloneCodeEditor;

/** Width of the ribbon gutter between the panes. */
const CONNECTOR_WIDTH = 56;
/** How far the split between the sides can be dragged either way. */
const MIN_SPLIT = 0.1;

interface Pane {
  editor: Editor;
  decorations: monaco.editor.IEditorDecorationsCollection;
}

/** The shared scroll position, in aligned-space pixels, and its model. */
interface Scroll {
  position: number;
  model: AlignedScroll;
  /** Moves both panes to `position`, except the one that just scrolled. */
  apply: (skip?: 0 | 1) => void;
}

export function DiffView({ view, run }: { view: View; run: Run }) {
  const root = useRef<HTMLDivElement>(null);
  const leftHost = useRef<HTMLDivElement>(null);
  const rightHost = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const panes = useRef<[Pane, Pane] | null>(null);
  const scroll = useRef<Scroll>({
    position: 0,
    model: new AlignedScroll(view.diff.segments, LINE_HEIGHT),
    apply: () => {},
  });
  // The handlers below are registered once; they read the latest view here.
  const latest = useRef({ view, run });
  latest.current = { view, run };
  const [split, setSplit] = useState(() => clampSplit(uiSettings().diffSplit ?? 0.5));
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);

  /** Drags the split; the panes relayout and the ribbons follow. */
  const startDrag = (event: ReactPointerEvent) => {
    if (event.button !== 0) return;
    // A drag resizes; it must not also select text.
    event.preventDefault();
    const box = root.current!.getBoundingClientRect();
    let latestSplit = split;
    const move = (moved: PointerEvent) => {
      latestSplit = clampSplit((moved.clientX - box.left - CONNECTOR_WIDTH / 2) / (box.width - CONNECTOR_WIDTH));
      setSplit(latestSplit);
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      document.body.classList.remove("col-resizing");
      updateUiSettings({ diffSplit: Math.round(latestSplit * 1000) / 1000 });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    document.body.classList.add("col-resizing");
  };
  const resetSplit = () => {
    setSplit(0.5);
    updateUiSettings({ diffSplit: 0.5 });
  };

  useEffect(() => {
    const create = (host: HTMLDivElement): Pane => {
      const editor = monaco.editor.create(host, {
        value: "",
        language: "json",
        theme: monacoTheme(latest.current.view.theme),
        readOnly: true,
        domReadOnly: true,
        automaticLayout: true,
        glyphMargin: true,
        folding: false,
        minimap: { enabled: false },
        // The shared scroll ends with the last line at the top (scroll.ts).
        scrollBeyondLastLine: true,
        renderLineHighlight: "none",
        occurrencesHighlight: "off",
        selectionHighlight: false,
        matchBrackets: "never",
        contextmenu: false,
        // No change marks beside the scrollbar: the colours and ribbons show
        // the changes already (issue #2).
        overviewRulerLanes: 0,
        overviewRulerBorder: false,
        hideCursorInOverviewRuler: true,
        lineNumbersMinChars: 4,
        lineDecorationsWidth: 14,
        fontFamily: FONT_FAMILY,
        fontSize: FONT_SIZE,
        lineHeight: LINE_HEIGHT,
        // The wheel moves the shared position instead (below).
        scrollbar: { useShadows: false, handleMouseWheel: false, alwaysConsumeMouseWheel: false },
        stickyScroll: { enabled: false },
        guides: { indentation: false },
      });
      return { editor, decorations: editor.createDecorationsCollection() };
    };
    const left = create(leftHost.current!);
    const right = create(rightHost.current!);
    const editors = [left.editor, right.editor] as const;
    panes.current = [left, right];
    const state = scroll.current;
    const viewport = () => left.editor.getLayoutInfo().height;

    const drawRibbons = () => {
      if (canvas.current && panes.current) paintRibbons(canvas.current, panes.current, latest.current.view);
    };

    let applying = false;
    state.apply = (skip) => {
      const tops = state.model.scrollTops(state.position, viewport());
      applying = true;
      editors.forEach((editor, index) => index !== skip && editor.setScrollTop(tops[index]));
      applying = false;
      drawRibbons();
    };

    // Wheel and trackpad, anywhere over the diff; Shift scrolls sideways.
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      const unit = event.deltaMode === 1 ? LINE_HEIGHT : event.deltaMode === 2 ? viewport() : 1;
      let dx = event.deltaX * unit;
      let dy = event.deltaY * unit;
      if (event.shiftKey && dx === 0) [dx, dy] = [dy, 0];
      if (dy !== 0) {
        state.position = clamp(state.position + dy, 0, state.model.maxScroll());
        state.apply();
      }
      if (dx !== 0) {
        const x = left.editor.getScrollLeft() + dx;
        applying = true;
        editors.forEach((editor) => editor.setScrollLeft(x));
        applying = false;
      }
    };
    const element = root.current!;
    element.addEventListener("wheel", wheel, { passive: false });

    // A pane that scrolled itself (arrow keys, Page Up/Down, its scrollbar)
    // moves the shared position, and the other pane follows.
    const follow = (index: 0 | 1) =>
      editors[index].onDidScrollChange((event) => {
        if (applying) return;
        if (event.scrollTopChanged) {
          state.position = state.model.positionFor(index, event.scrollTop, viewport());
          state.apply(index);
        }
        if (event.scrollLeftChanged) {
          applying = true;
          editors[1 - index].setScrollLeft(event.scrollLeft);
          applying = false;
        }
        drawRibbons();
      });

    const clicks = (pane: Pane, side: Side) =>
      pane.editor.onMouseDown((event) => {
        const line = event.target.position?.lineNumber;
        if (line === undefined) return;
        const { view, run } = latest.current;
        const sideView = side === "left" ? view.diff.left : view.diff.right;
        if (event.target.type === monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) {
          const fold = sideView.folds.find((fold) => fold.line === line);
          if (fold) {
            void run(fold.collapsed ? "expand_node" : "collapse_node", {
              side,
              pointer: fold.pointer,
            });
          }
          return;
        }
        const index = view.diff.hunks.findIndex((hunk) => {
          const [start, end] = side === "left" ? hunk.left : hunk.right;
          return line - 1 >= start && line - 1 < end;
        });
        if (index >= 0 && index !== view.currentHunk) {
          void run("go_to_change", { index });
        }
      });

    // Right-click a key: ignore its changes, here or wherever the name occurs.
    const contextMenu = (pane: Pane, side: Side) =>
      pane.editor.onContextMenu((event) => {
        event.event.preventDefault();
        const line = event.target.position?.lineNumber;
        if (line === undefined) return;
        const { posx: x, posy: y } = event.event;
        void keyAt(side, line).then((key) => {
          if (!key) return;
          const { run } = latest.current;
          const items: MenuEntry[] = key.ignoredBy
            ? [
                {
                  label: `Stop ignoring ${key.ignoredBy.label}`,
                  action: () => run("stop_ignoring", ruleArgs(key.ignoredBy!.rule)),
                },
              ]
            : [
                {
                  label: `Ignore changes to ${key.pointer}`,
                  action: () => run("ignore_changes", { side, pointer: key.pointer }),
                },
                ...(key.key === null
                  ? []
                  : [{ label: `Ignore "${key.key}" everywhere`, action: () => run("ignore_changes", { key: key.key }) }]),
              ];
          setMenu({ x, y, items });
        });
      });

    const subscriptions = [
      follow(0),
      follow(1),
      clicks(left, "left"),
      clicks(right, "right"),
      contextMenu(left, "left"),
      contextMenu(right, "right"),
      // A new viewport height moves where the anchor sits.
      left.editor.onDidLayoutChange(() => state.apply()),
    ];
    const resize = new ResizeObserver(drawRibbons);
    resize.observe(canvas.current!);

    return () => {
      element.removeEventListener("wheel", wheel);
      resize.disconnect();
      subscriptions.forEach((subscription) => subscription.dispose());
      left.editor.dispose();
      right.editor.dispose();
      panes.current = null;
      state.apply = () => {};
    };
  }, []);

  useEffect(() => {
    if (!panes.current) return;
    const [left, right] = panes.current;
    const state = scroll.current;
    state.model = new AlignedScroll(view.diff.segments, LINE_HEIGHT);
    const current = view.currentHunk === null ? undefined : view.diff.hunks[view.currentHunk];
    render(left, view.diff.left, current?.left);
    render(right, view.diff.right, current?.right);

    const viewport = left.editor.getLayoutInfo().height;
    if (view.reveal !== null) {
      const hunk = view.diff.hunks[view.reveal];
      if (hunk && !isVisible(hunk, [left, right], viewport)) {
        const row = state.model.rowOf(hunk.left[0], hunk.right[0]);
        if (row !== null) state.position = row * LINE_HEIGHT - viewport / 3;
      }
    }
    // New text resets each editor's scroll; put both back.
    state.position = clamp(state.position, 0, state.model.maxScroll());
    state.apply();
  }, [view]);

  useEffect(() => {
    monaco.editor.setTheme(monacoTheme(view.theme));
  }, [view.theme]);

  return (
    <div
      className="diff"
      ref={root}
      style={{ gridTemplateColumns: `minmax(0, ${split}fr) auto minmax(0, ${1 - split}fr)` }}
    >
      <Header view={view} side="left" />
      <div className="splitter" onPointerDown={startDrag} onDoubleClick={resetSplit} />
      <Header view={view} side="right" />
      <div className="diff-pane" ref={leftHost} />
      <canvas
        className="connector splitter"
        ref={canvas}
        style={{ width: CONNECTOR_WIDTH }}
        title="Drag to resize the sides; double-click to split evenly"
        onPointerDown={startDrag}
        onDoubleClick={resetSplit}
      />
      <div className="diff-pane" ref={rightHost} />
      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
    </div>
  );
}

function clampSplit(split: number) {
  return Number.isFinite(split) ? clamp(split, MIN_SPLIT, 1 - MIN_SPLIT) : 0.5;
}

function clamp(value: number, low: number, high: number) {
  return Math.min(high, Math.max(low, value));
}

/** Whether a change already sits fully inside both panes. */
function isVisible(hunk: HunkView, [left, right]: [Pane, Pane], viewport: number) {
  return [
    span(left.editor, hunk.left),
    span(right.editor, hunk.right),
  ].every(([top, bottom]) => top >= 0 && bottom <= viewport);
}

function Header({ view, side }: { view: View; side: Side }) {
  const doc = view.docs[side === "left" ? 0 : 1];
  const count = side === "left" ? view.removedLines : view.addedLines;
  return (
    <div className="diff-header">
      <span className="diff-title" title={doc.path ?? undefined}>
        {doc.label}
      </span>
      {doc.error && (
        <span className="diff-error" title={doc.error.message}>
          ⚠ not valid JSON, compared as text
        </span>
      )}
      <span className={side === "left" ? "count-removed" : "count-added"}>
        {side === "left" ? "−" : "+"}
        {count}
      </span>
    </div>
  );
}

/** Puts one side's text and decorations into its editor. */
function render(pane: Pane, side: SideView, current: [number, number] | undefined) {
  const { editor } = pane;
  if (editor.getValue() !== side.text) editor.setValue(side.text);

  const decorations: monaco.editor.IModelDeltaDecoration[] = [];
  const status = side.status;
  for (let index = 0; index < status.length; index++) {
    const kind = lineKind(status[index]);
    if (!kind) continue;
    const top = index === 0 || status[index - 1] !== status[index];
    const bottom = index === status.length - 1 || status[index + 1] !== status[index];
    const isCurrent = current !== undefined && index >= current[0] && index < current[1];
    const classes = [`line-${kind}`];
    if (top) classes.push("box-top");
    if (bottom) classes.push("box-bottom");
    if (isCurrent) classes.push("current");
    decorations.push({
      range: new monaco.Range(index + 1, 1, index + 1, 1),
      options: {
        isWholeLine: true,
        className: classes.join(" "),
        linesDecorationsClassName: `sign sign-${kind}`,
        marginClassName: `margin-${kind}`,
      },
    });
  }
  for (const range of side.inline) {
    const kind = lineKind(status[range.line - 1]);
    if (!kind) continue;
    decorations.push({
      range: new monaco.Range(range.line, range.start, range.line, range.end),
      options: { inlineClassName: `char-${kind}` },
    });
  }
  const model = editor.getModel();
  for (const fold of side.folds) {
    decorations.push({
      range: new monaco.Range(fold.line, 1, fold.line, 1),
      options: {
        glyphMarginClassName: `codicon codicon-chevron-${fold.collapsed ? "right" : "down"} fold-glyph`,
        glyphMarginHoverMessage: { value: fold.collapsed ? "Expand" : "Collapse" },
      },
    });
    if (fold.collapsed && model) {
      const column = model.getLineMaxColumn(fold.line);
      const noun = fold.isArray ? "item" : "key";
      decorations.push({
        range: new monaco.Range(fold.line, column, fold.line, column),
        options: {
          // The range is empty: without this Monaco draws nothing.
          showIfCollapsed: true,
          after: {
            content: `  ${fold.children} ${noun}${fold.children === 1 ? "" : "s"} · ${fold.hiddenLines + 1} lines`,
            inlineClassName: "fold-hint",
          },
        },
      });
    }
  }
  pane.decorations.set(decorations);
}

function lineKind(status: string | undefined): "add" | "del" | null {
  return status === "+" ? "add" : status === "-" ? "del" : null;
}

const COLORS: Record<Exclude<LineStatus, "unchanged">, [string, string]> = {
  added: ["rgba(115, 201, 145, 0.22)", "rgba(115, 201, 145, 0.9)"],
  removed: ["rgba(241, 76, 76, 0.22)", "rgba(241, 76, 76, 0.9)"],
};
const NEUTRAL: [string, string] = ["rgba(128, 128, 128, 0.12)", "rgba(128, 128, 128, 0.5)"];

/** A side of a ribbon takes its own colour, or the other side's when it has
 *  none (an insertion point); a change of formatting only is neutral. */
function ribbonColor(hunk: HunkView, left: boolean): [string, string] {
  const own = left ? hunk.leftStatus : hunk.rightStatus;
  const other = left ? hunk.rightStatus : hunk.leftStatus;
  const status = own !== "unchanged" ? own : other;
  return status === "unchanged" ? NEUTRAL : COLORS[status];
}

/** The ribbons joining each change's two sides across the gutter. A side
 *  with no lines (an insertion point) narrows to the line boundary where
 *  the change belongs. */
function paintRibbons(element: HTMLCanvasElement, [left, right]: [Pane, Pane], view: View) {
  const ratio = window.devicePixelRatio || 1;
  const width = element.clientWidth;
  const height = element.clientHeight;
  if (element.width !== Math.round(width * ratio) || element.height !== Math.round(height * ratio)) {
    element.width = Math.round(width * ratio);
    element.height = Math.round(height * ratio);
  }
  const context = element.getContext("2d");
  if (!context) return;
  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  context.clearRect(0, 0, width, height);

  view.diff.hunks.forEach((hunk, index) => {
    const [ly0, ly1] = span(left.editor, hunk.left);
    const [ry0, ry1] = span(right.editor, hunk.right);
    if (Math.max(ly1, ry1) < 0 || Math.min(ly0, ry0) > height) return;
    const [leftFill, leftEdge] = ribbonColor(hunk, true);
    const [rightFill, rightEdge] = ribbonColor(hunk, false);
    const middle = width / 2;

    const fill = context.createLinearGradient(0, 0, width, 0);
    fill.addColorStop(0, leftFill);
    fill.addColorStop(1, rightFill);
    context.beginPath();
    context.moveTo(0, ly0);
    context.bezierCurveTo(middle, ly0, middle, ry0, width, ry0);
    context.lineTo(width, ry1);
    context.bezierCurveTo(middle, ry1, middle, ly1, 0, ly1);
    context.closePath();
    context.fillStyle = fill;
    context.fill();

    const edge = context.createLinearGradient(0, 0, width, 0);
    edge.addColorStop(0, leftEdge);
    edge.addColorStop(1, rightEdge);
    context.strokeStyle = edge;
    context.lineWidth = index === view.currentHunk ? 2 : 1;
    context.beginPath();
    context.moveTo(0, ly0);
    context.bezierCurveTo(middle, ly0, middle, ry0, width, ry0);
    context.moveTo(0, ly1);
    context.bezierCurveTo(middle, ly1, middle, ry1, width, ry1);
    context.stroke();
  });
}

/** Top and bottom, in viewport pixels, of a 0-based line range; an empty
 *  range is the boundary above its start line. */
function span(editor: Editor, [start, end]: [number, number]): [number, number] {
  const scroll = editor.getScrollTop();
  return [start * LINE_HEIGHT - scroll, end * LINE_HEIGHT - scroll];
}
