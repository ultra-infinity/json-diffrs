// The side-by-side diff: two read-only Monaco editors showing what the Rust
// structural diff decided. Rust supplies each line's colour, the changed
// characters, the fold markers and the blank gaps that keep both sides
// aligned; this file only draws them, keeps the scroll positions in step,
// and paints the ribbons between the panes.

import { useEffect, useRef } from "react";
import type { HunkView, LineStatus, Run, Side, SideView, View } from "../api";
import { FONT_FAMILY, FONT_SIZE, LINE_HEIGHT, monaco, monacoTheme } from "../monaco";

type Editor = monaco.editor.IStandaloneCodeEditor;

/** Width of the ribbon gutter between the panes. */
const CONNECTOR_WIDTH = 48;

interface Pane {
  editor: Editor;
  decorations: monaco.editor.IEditorDecorationsCollection;
  zones: string[];
}

export function DiffView({ view, run }: { view: View; run: Run }) {
  const leftHost = useRef<HTMLDivElement>(null);
  const rightHost = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const panes = useRef<[Pane, Pane] | null>(null);
  // The handlers below are registered once; they read the latest view here.
  const latest = useRef({ view, run });
  latest.current = { view, run };

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
        scrollBeyondLastLine: false,
        renderLineHighlight: "none",
        occurrencesHighlight: "off",
        selectionHighlight: false,
        matchBrackets: "never",
        contextmenu: false,
        overviewRulerLanes: 1,
        overviewRulerBorder: false,
        lineNumbersMinChars: 4,
        lineDecorationsWidth: 14,
        fontFamily: FONT_FAMILY,
        fontSize: FONT_SIZE,
        lineHeight: LINE_HEIGHT,
        scrollbar: { useShadows: false, alwaysConsumeMouseWheel: false },
        stickyScroll: { enabled: false },
        guides: { indentation: false },
      });
      return { editor, decorations: editor.createDecorationsCollection(), zones: [] };
    };
    const left = create(leftHost.current!);
    const right = create(rightHost.current!);
    panes.current = [left, right];

    // Both sides are padded to the same height, so their scroll positions
    // can simply be copied.
    let syncing = false;
    const follow = (from: Editor, to: Editor) =>
      from.onDidScrollChange((event) => {
        if (!syncing && (event.scrollTopChanged || event.scrollLeftChanged)) {
          syncing = true;
          to.setScrollTop(event.scrollTop);
          to.setScrollLeft(event.scrollLeft);
          syncing = false;
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

    const drawRibbons = () => {
      const element = canvas.current;
      if (!element || !panes.current) return;
      paintRibbons(element, panes.current, latest.current.view);
    };

    const subscriptions = [
      follow(left.editor, right.editor),
      follow(right.editor, left.editor),
      clicks(left, "left"),
      clicks(right, "right"),
      left.editor.onDidLayoutChange(drawRibbons),
      left.editor.onDidContentSizeChange(drawRibbons),
    ];
    const resize = new ResizeObserver(drawRibbons);
    resize.observe(canvas.current!);

    return () => {
      resize.disconnect();
      subscriptions.forEach((subscription) => subscription.dispose());
      left.editor.dispose();
      right.editor.dispose();
      panes.current = null;
    };
  }, []);

  useEffect(() => {
    if (!panes.current) return;
    const [left, right] = panes.current;
    const scrollTop = left.editor.getScrollTop();
    const scrollLeft = left.editor.getScrollLeft();
    const current = view.currentHunk === null ? undefined : view.diff.hunks[view.currentHunk];
    render(left, view.diff.left, current?.left);
    render(right, view.diff.right, current?.right);
    left.editor.setScrollTop(scrollTop);
    left.editor.setScrollLeft(scrollLeft);
    right.editor.setScrollTop(scrollTop);
    right.editor.setScrollLeft(scrollLeft);

    if (view.reveal !== null) {
      const hunk = view.diff.hunks[view.reveal];
      if (hunk) {
        // Reveal on the side that has lines; scrolling follows on the other.
        const [pane, [start, end]] =
          hunk.right[1] > hunk.right[0] ? [right, hunk.right] : [left, hunk.left];
        const first = Math.max(1, start + 1);
        pane.editor.revealRangeInCenterIfOutsideViewport(
          new monaco.Range(first, 1, Math.max(first, end), 1),
        );
      }
    }
    paintRibbons(canvas.current!, panes.current, view);
  }, [view]);

  useEffect(() => {
    monaco.editor.setTheme(monacoTheme(view.theme));
  }, [view.theme]);

  return (
    <div className="diff">
      <Header view={view} side="left" />
      <div />
      <Header view={view} side="right" />
      <div className="diff-pane" ref={leftHost} />
      <canvas className="connector" ref={canvas} style={{ width: CONNECTOR_WIDTH }} />
      <div className="diff-pane" ref={rightHost} />
    </div>
  );
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

/** Puts one side's text, gaps and decorations into its editor. */
function render(pane: Pane, side: SideView, current: [number, number] | undefined) {
  const { editor } = pane;
  if (editor.getValue() !== side.text) editor.setValue(side.text);

  editor.changeViewZones((accessor) => {
    pane.zones.forEach((id) => accessor.removeZone(id));
    pane.zones = side.padding.map((gap) => {
      const domNode = document.createElement("div");
      domNode.className = "diff-gap";
      return accessor.addZone({
        afterLineNumber: gap.afterLine,
        heightInLines: gap.lines,
        domNode,
      });
    });
  });

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
        overviewRuler: {
          color: kind === "add" ? "#73c991" : "#f14c4c",
          position: monaco.editor.OverviewRulerLane.Full,
        },
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

/** The ribbons joining each change's two sides across the gutter. */
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
    const [ly0, ly1] = span(left.editor, hunk.left, view.diff.left);
    const [ry0, ry1] = span(right.editor, hunk.right, view.diff.right);
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

/** A side of a ribbon takes its own colour, or the other side's when it has
 *  none (an insertion point); a change of formatting only is neutral. */
function ribbonColor(hunk: HunkView, left: boolean): [string, string] {
  const own = left ? hunk.leftStatus : hunk.rightStatus;
  const other = left ? hunk.rightStatus : hunk.leftStatus;
  const status = own !== "unchanged" ? own : other;
  return status === "unchanged" ? NEUTRAL : COLORS[status];
}

/** Top and bottom, in editor-viewport pixels, of a 0-based line range plus
 *  the gap that pads it. An empty range is the gap alone. */
function span(editor: Editor, [start, end]: [number, number], side: SideView): [number, number] {
  const scroll = editor.getScrollTop();
  const gap = side.padding.find((padding) => padding.afterLine === end)?.lines ?? 0;
  const top =
    end > start
      ? editor.getTopForLineNumber(start + 1)
      : start === 0
        ? 0
        : editor.getTopForLineNumber(start) + LINE_HEIGHT;
  const bottom = (end > start ? editor.getTopForLineNumber(end) + LINE_HEIGHT : top) + gap * LINE_HEIGHT;
  return [top - scroll, bottom - scroll];
}
