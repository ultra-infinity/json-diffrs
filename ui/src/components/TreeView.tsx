// The tree view: both documents as one tree of keys, built in Rust
// (crates/json-diff-core/src/tree.rs). Added keys are green, removed ones
// red, a changed value reads old → new, and an object or array with changes
// inside shows how many. By default only the paths to changes are listed.

import { useState } from "react";
import { ruleArgs, type Run, type TreeRow, type View } from "../api";
import { ContextMenu, type MenuEntry } from "./Menu";

export function TreeView({ view, run }: { view: View; run: Run }) {
  const tree = view.tree;
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  if (!tree) return null;

  if (!tree.rows) {
    const errors = view.docs.filter((doc) => doc.error);
    return (
      <div className="key-tree-empty">
        <p>The tree view needs two valid JSON documents.</p>
        {errors.map((doc) => (
          <p key={doc.label} className="weak">
            {doc.label}: {doc.error!.message}
          </p>
        ))}
        <button onClick={() => run("set_view_mode", { mode: "diff" })}>Compare as text in the line diff</button>
      </div>
    );
  }

  if (tree.rows.length === 0) {
    return (
      <div className="key-tree-empty">
        <p>{view.ignoredChanges > 0 ? "No changes, apart from ignored keys." : "No changes: the documents are the same."}</p>
        {!tree.showUnchanged && (
          <button onClick={() => run("set_tree_options", { show_unchanged: true })}>Show the whole document</button>
        )}
      </div>
    );
  }

  const toggle = (row: TreeRow) =>
    run(row.expanded ? "collapse_tree_node" : "expand_tree_node", { id: row.id });

  /** Right-click: ignore the key here or by name, or count it again. */
  const contextMenu = (event: React.MouseEvent, row: TreeRow) => {
    event.preventDefault();
    const side = row.leftPointer !== null ? "left" : "right";
    const pointer = row.leftPointer ?? row.rightPointer;
    if (pointer === null || pointer === "") return;
    const key = row.label.startsWith("[") ? null : row.label;
    const items: MenuEntry[] = row.ignoredBy
      ? [{ label: `Stop ignoring ${ruleLabel(row)}`, action: () => run("stop_ignoring", ruleArgs(row.ignoredBy!)) }]
      : [
          { label: `Ignore changes to ${pointer}`, action: () => run("ignore_changes", { side, pointer }) },
          ...(key === null ? [] : [{ label: `Ignore "${key}" everywhere`, action: () => run("ignore_changes", { key }) }]),
        ];
    setMenu({ x: event.clientX, y: event.clientY, items });
  };

  return (
    <div className="key-tree" role="tree">
      {tree.rows.map((row) => (
        <div
          key={row.id}
          role="treeitem"
          aria-expanded={row.expanded ?? undefined}
          className={`key-tree-row status-${row.status}${row.ignoredBy ? " ignored" : ""}`}
          style={{ paddingLeft: 8 + row.depth * 18 }}
          title={row.rightPointer ?? row.leftPointer ?? undefined}
          onContextMenu={(event) => contextMenu(event, row)}
          onDoubleClick={() => row.expanded !== null && void toggle(row)}
        >
          <span className="key-tree-chevron">
            {row.expanded !== null && (
              <button className="icon" title={row.expanded ? "Collapse" : "Expand"} onClick={() => void toggle(row)}>
                <span className={`codicon codicon-chevron-${row.expanded ? "down" : "right"}`} />
              </button>
            )}
          </span>
          <span className="key-tree-sign">{SIGNS[row.status]}</span>
          <span className={row.label.startsWith("[") ? "key-tree-index" : "key-tree-key"}>{row.label}</span>
          <Values row={row} />
          {row.status === "changed" && (
            <span className="key-tree-count">
              {row.changes} {row.changes === 1 ? "change" : "changes"}
            </span>
          )}
          {row.ignoredBy && <span className="key-tree-tag">ignored</span>}
        </div>
      ))}
      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
    </div>
  );
}

const SIGNS: Record<TreeRow["status"], string> = {
  unchanged: "",
  added: "+",
  removed: "−",
  modified: "~",
  changed: "",
};

/** The value column: the one value, or old → new for a change. */
function Values({ row }: { row: TreeRow }) {
  switch (row.status) {
    case "added":
      return <span className="key-tree-value new">{row.new}</span>;
    case "removed":
      return <span className="key-tree-value old">{row.old}</span>;
    case "modified":
      return (
        <>
          <span className="key-tree-value old" title={row.old ?? undefined}>
            {row.old}
          </span>
          <span className="key-tree-arrow">→</span>
          <span className="key-tree-value new" title={row.new ?? undefined}>
            {row.new}
          </span>
        </>
      );
    default:
      return <span className="key-tree-value">{row.new ?? row.old}</span>;
  }
}

function ruleLabel(row: TreeRow) {
  const rule = row.ignoredBy!;
  return rule.kind === "pointer" ? rule.pointer : `"${rule.key}" everywhere`;
}
