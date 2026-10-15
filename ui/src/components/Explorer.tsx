// The left panel: original folder above, modified folder below. With both
// open, files are paired by relative path and badged M / D / A.

import { forwardRef, useMemo, useState, type ReactNode } from "react";
import { WEB, pickFile, pickFolder, sideName, type FileView, type FolderView, type Run, type Side, type View } from "../api";
import { badgeOf, buildTree, type Directory } from "../tree";
import { ContextMenu, type MenuItem } from "./Menu";

/** Folders with more files than this start closed. */
const OPEN_BELOW_FILES = 200;

export const Explorer = forwardRef<HTMLDivElement, { view: View; run: Run }>(function Explorer(
  { view, run },
  ref,
) {
  return (
    <div className="explorer" ref={ref}>
      <Half view={view} run={run} side="left" />
      <Half view={view} run={run} side="right" />
    </div>
  );
});

function Half({ view, run, side }: { view: View; run: Run; side: Side }) {
  const folder = view.folders[side === "left" ? 0 : 1];
  const [typed, setTyped] = useState("");
  return (
    <section className="explorer-half" data-side={side}>
      <header className="explorer-header">
        <span className="section-title">{sideName(side)}</span>
        <span className="pane-hint" title={`Files from this folder open in the ${side} pane`}>
          → {side} pane
        </span>
        <span className="spacer" />
        <button className="icon" title="Open a folder…" onClick={() => pickFolder(run, side)}>
          <span className="codicon codicon-folder-opened" />
        </button>
        <button className="icon" title="Open a single file…" onClick={() => pickFile(run, side)}>
          <span className="codicon codicon-file" />
        </button>
        {folder && (
          <>
            {/* A web page cannot rescan a folder: pick it again instead. */}
            {!WEB && (
              <button className="icon" title="Rescan folders (F5)" onClick={() => run("refresh")}>
                <span className="codicon codicon-refresh" />
              </button>
            )}
            <button className="icon" title="Close folder" onClick={() => run("close_folder", { side })}>
              <span className="codicon codicon-close" />
            </button>
          </>
        )}
      </header>
      {folder ? (
        <Tree folder={folder} side={side} run={run} />
      ) : (
        <div className="explorer-empty">
          <p className="weak">No folder opened</p>
          <button onClick={() => pickFolder(run, side)}>Open Folder</button>
          {WEB ? (
            <button onClick={() => pickFile(run, side)}>Open File</button>
          ) : (
            <input
              placeholder="or type a path and press Enter"
              value={typed}
              onChange={(event) => setTyped(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && typed.trim()) {
                  void run("open_folder", { side, path: typed.trim() });
                }
              }}
            />
          )}
          <p className="weak small-text">You can also drop a {WEB ? "folder or file" : "folder"} here.</p>
        </div>
      )}
    </section>
  );
}

function Tree({ folder, side, run }: { folder: FolderView; side: Side; run: Run }) {
  const tree = useMemo(() => buildTree(folder.files), [folder.files]);
  const [closed, setClosed] = useState<Set<string>>(() => new Set());
  const [opened, setOpened] = useState<Set<string>>(() => new Set());
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const openByDefault = folder.files.length < OPEN_BELOW_FILES;
  const isOpen = (path: string, depth: number) =>
    opened.has(path) || (!closed.has(path) && openByDefault && depth === 0);
  const toggle = (path: string, depth: number) => {
    const wasOpen = isOpen(path, depth);
    const update = (set: Set<string>, add: boolean) => {
      const next = new Set(set);
      if (add) next.add(path);
      else next.delete(path);
      return next;
    };
    setOpened((set) => update(set, !wasOpen));
    setClosed((set) => update(set, wasOpen));
  };
  // A click fills this half's pane only, so any original file can be
  // compared with any modified one; same-named files are a right-click away.
  const openFile = (file: FileView) => run("select_file", { side, path: file.path });
  const hasTwin = (file: FileView) => file.status === "modified" || file.status === "identical";

  const renderDirectory = (directory: Directory, depth: number): ReactNode => (
    <>
      {directory.directories.map((child) => (
        <div key={child.path}>
          <div
            className={`tree-row${child.changed ? " changed" : ""}`}
            style={{ paddingLeft: 8 + depth * 12 }}
            onClick={() => toggle(child.path, depth)}
          >
            <span className={`codicon codicon-chevron-${isOpen(child.path, depth) ? "down" : "right"}`} />
            <span className="codicon codicon-folder" />
            {child.name}
          </div>
          {isOpen(child.path, depth) && renderDirectory(child, depth + 1)}
        </div>
      ))}
      {directory.files.map((file) => {
        const name = file.path.split("/").pop();
        const badge = badgeOf(file.status);
        return (
          <div
            key={file.path}
            className={`tree-row file${file.selected ? " selected" : ""}${badge ? ` status-${badge.toLowerCase()}` : ""}`}
            style={{ paddingLeft: 22 + depth * 12 }}
            title={`${file.path}\nClick: open in the ${side} pane`}
            onClick={() => void openFile(file)}
            onContextMenu={(event) => {
              event.preventDefault();
              const items: MenuItem[] = [
                { label: `Open in the ${side} pane`, action: () => run("select_file", { side, path: file.path }) },
              ];
              if (hasTwin(file)) {
                items.push({
                  label: "Compare with the same file in the other folder",
                  action: () => run("open_pair", { path: file.path }),
                });
              }
              setMenu({ x: event.clientX, y: event.clientY, items });
            }}
          >
            <span className="codicon codicon-json" />
            <span className="tree-name">{name}</span>
            {badge && <span className="badge">{badge}</span>}
          </div>
        );
      })}
    </>
  );

  return (
    <div className="tree">
      <div className="tree-root" title={folder.root}>
        {folder.name}
      </div>
      {folder.files.length === 0 ? (
        <p className="weak padded">No .json files in this folder</p>
      ) : (
        renderDirectory(tree, 0)
      )}
      {menu && <ContextMenu {...menu} onClose={() => setMenu(null)} />}
    </div>
  );
}
