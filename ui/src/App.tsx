// The window: menu bar, toolbar, explorer, diff or editors, assistant and
// status bar. All state lives in Rust; this renders the latest `View` and
// turns clicks into commands.

import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode, type RefObject } from "react";
import {
  WEB,
  getView,
  onDrop,
  onView,
  openRelease,
  pickFile,
  pickFolder,
  releaseNotes,
  ruleArgs,
  runCommand,
  whatsNew,
  type Release,
  type Run,
  type Side,
  type UpdateInfo,
  type View,
} from "./api";
import { AssistantPanel, useAssistant, type Assistant } from "./components/AssistantPanel";
import { DiffView } from "./components/DiffView";
import { TreeView } from "./components/TreeView";
import { EditView } from "./components/EditView";
import { Explorer } from "./components/Explorer";
import { ExportDialog, ExportedBar } from "./components/ExportDialog";
import { LicensesDialog } from "./components/LicensesDialog";
import { ContextMenu, MenuBar, type MenuEntry } from "./components/Menu";
import { loadUiSettings, uiSettings, updateUiSettings } from "./uiSettings";
import { UpdateBanner, useUpdates, type Updates } from "./components/UpdateBanner";
import { WhatsNewDialog } from "./components/WhatsNewDialog";

export function App() {
  const [view, setView] = useState<View | null>(null);
  const assistant = useAssistant();
  const updates = useUpdates();
  const [licensesOpen, setLicensesOpen] = useState(false);
  const closeLicenses = useCallback(() => setLicensesOpen(false), []);
  const [exportOpen, setExportOpen] = useState(false);
  const closeExport = useCallback(() => setExportOpen(false), []);
  const [exported, setExported] = useState<{ path: string; pages: number } | null>(null);
  const [notes, setNotes] = useState<{ title: string; releases: Release[]; actions?: ReactNode } | null>(null);
  const closeNotes = useCallback(() => setNotes(null), []);
  const explorer = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Saved panel widths and model choice are needed for the first render.
    void Promise.all([getView(), loadUiSettings()]).then(([view]) => setView(view));
    const unlisten = onView(setView);
    // Once after an update: what changed since the version that last ran.
    void whatsNew().then((releases) => {
      if (releases.length) setNotes({ title: `What's new in json-diff ${releases[0].version}`, releases });
    });
    return unlisten;
  }, []);

  const showAllNotes = useCallback(() => {
    void releaseNotes().then((releases) => setNotes({ title: "What's new", releases }));
  }, []);

  // The update bar's "What's new": the coming version's notes, with its
  // install button.
  const showUpdateNotes = (info: UpdateInfo) =>
    setNotes({
        title: `What's new in json-diff ${info.version}`,
        releases: [{ version: info.version, date: null, notes: info.notes ?? "" }],
        actions: info.canInstall ? (
          <button
            className="primary"
            onClick={() => {
              setNotes(null);
              updates.install();
            }}
          >
            Update now
          </button>
        ) : (
          <button onClick={() => void openRelease(info.releaseUrl)}>Release page</button>
        ),
      });

  const run: Run = useCallback(async (name, args = {}) => {
    try {
      setView((await runCommand(name, args)).view);
    } catch (error) {
      console.error(name, error);
    }
  }, []);

  useShortcuts(run, view?.viewMode === "edit");
  useDrops(explorer, run, setView);

  useEffect(() => {
    if (view) document.documentElement.dataset.theme = view.theme;
  }, [view?.theme]);

  // The desktop app titles its window from Rust; a web page titles its tab.
  useEffect(() => {
    if (WEB && view) document.title = view.title;
  }, [view?.title]);

  if (!view) return null;
  return (
    <div className="app">
      <MenuBar
        menus={menus(view, run, updates, {
          showLicenses: () => setLicensesOpen(true),
          exportPdf: () => setExportOpen(true),
          whatsNew: showAllNotes,
        })}
      />
      <UpdateBanner updates={updates} onNotes={showUpdateNotes} />
      {exported && <ExportedBar exported={exported} onClose={() => setExported(null)} />}
      <div className="workspace">
        {view.showExplorer && (
          <Resizable side="left" setting="explorerWidth" initial={260} min={160} max={600}>
            <Explorer ref={explorer} view={view} run={run} />
          </Resizable>
        )}
        <main className="main">
          <Toolbar view={view} run={run} />
          {view.viewMode === "diff" && <DiffView view={view} run={run} />}
          {view.viewMode === "tree" && <TreeView view={view} run={run} />}
          {view.viewMode === "edit" && <EditView view={view} onView={setView} />}
        </main>
        {/* Hidden rather than unmounted, so the conversation survives. */}
        {!WEB && (
          <Resizable side="right" setting="assistantWidth" initial={340} min={240} max={700} hidden={!view.showAgent}>
            <AssistantPanel view={view} run={run} assistant={assistant} />
          </Resizable>
        )}
      </div>
      <StatusBar view={view} assistant={assistant} />
      {licensesOpen && <LicensesDialog onClose={closeLicenses} />}
      {notes && <WhatsNewDialog {...notes} onClose={closeNotes} />}
      {exportOpen && <ExportDialog onClose={closeExport} onView={setView} onDone={setExported} />}
    </div>
  );
}

function menus(
  view: View,
  run: Run,
  updates: Updates,
  dialogs: { showLicenses: () => void; exportPdf: () => void; whatsNew: () => void },
): { title: string; entries: MenuEntry[] }[] {
  const sides: Side[] = ["left", "right"];
  const title = (side: Side) => (side === "left" ? "Original" : "Modified");
  const option = (key: keyof View["options"]) => () => run("set_options", { [key]: !view.options[key] });
  /** Leaves out what the web app can't do: rescanning folders, the
   *  assistant, the window, zoom (the browser's own does it) and updates. */
  const desktop = (...entries: MenuEntry[]): MenuEntry[] => (WEB ? [] : entries);
  return [
    {
      title: "File",
      entries: [
        ...sides.map((side) => ({ label: `Open ${title(side)} Folder…`, action: () => pickFolder(run, side) })),
        ...sides.map((side) => ({ label: `Open ${title(side)} File…`, action: () => pickFile(run, side) })),
        "separator",
        { label: "Export as PDF…", action: dialogs.exportPdf },
        "separator",
        ...desktop({ label: "Refresh", shortcut: "F5", action: () => run("refresh") }),
        { label: "Swap Sides", action: () => run("swap_sides") },
        ...sides.map((side) => ({ label: `Clear ${title(side)}`, action: () => run("clear_side", { side }) })),
        ...desktop("separator", { label: "Quit", action: () => run("window", { action: "close" }) }),
      ],
    },
    {
      title: "View",
      entries: [
        { label: "Side-by-side Diff", checked: view.viewMode === "diff", action: () => run("set_view_mode", { mode: "diff" }) },
        { label: "Tree", checked: view.viewMode === "tree", action: () => run("set_view_mode", { mode: "tree" }) },
        { label: "Edit", checked: view.viewMode === "edit", action: () => run("set_view_mode", { mode: "edit" }) },
        "separator",
        { label: "Explorer", checked: view.showExplorer, action: () => run("set_panel_visible", { panel: "explorer" }) },
        ...desktop({ label: "Assistant", checked: view.showAgent, action: () => run("set_panel_visible", { panel: "agent" }) }),
        "separator",
        { label: "Expand All", disabled: !view.foldable, action: () => run("expand_all") },
        { label: "Collapse All", disabled: !view.foldable, action: () => run("collapse_all") },
        { label: "Reveal Changes", disabled: !view.foldable, action: () => run("reveal_changes") },
        "separator",
        { label: "Dark Theme", checked: view.theme === "dark", action: () => run("set_theme", { theme: "dark" }) },
        { label: "Light Theme", checked: view.theme === "light", action: () => run("set_theme", { theme: "light" }) },
        ...desktop(
          "separator",
          { label: "Zoom In", action: () => run("zoom_in") },
          { label: "Zoom Out", action: () => run("zoom_out") },
          { label: "Reset Zoom", action: () => run("set_zoom", { zoom: 1 }) },
        ),
      ],
    },
    {
      title: "Go",
      entries: [
        { label: "Next Change", shortcut: "F7", action: () => run("next_change") },
        { label: "Previous Change", shortcut: "Shift+F7", action: () => run("previous_change") },
      ],
    },
    {
      title: "Options",
      entries: [
        { label: "Format JSON", checked: view.options.format, action: option("format") },
        { label: "Sort Keys", checked: view.options.sort_keys, disabled: !view.options.format, action: option("sort_keys") },
        {
          label: "Ignore Whitespace",
          checked: view.options.ignore_whitespace,
          disabled: view.options.format,
          action: option("ignore_whitespace"),
        },
      ],
    },
    {
      title: "Help",
      entries: [
        { label: "What's New…", action: dialogs.whatsNew },
        ...desktop({ label: "Check for Updates…", action: () => updates.check(true) }),
        { label: "Licenses…", action: dialogs.showLicenses },
        // Where the code of the copy running in the page comes from.
        ...(WEB ? [{ label: "Source Code…", action: () => void openRelease(SOURCE_URL) }] : []),
        "separator",
        { label: `json-diff ${updates.version}`, disabled: true, action: () => {} },
        { label: "Authored by Omkar Mohanty", disabled: true, action: () => {} },
      ],
    },
  ];
}

const SOURCE_URL = "https://github.com/ultra-infinity/json-diffrs";

function Toolbar({ view, run }: { view: View; run: Run }) {
  const count = view.diff.hunks.length;
  const position =
    count === 0 ? "No changes" : view.currentHunk === null ? `${count} changes` : `Change ${view.currentHunk + 1} of ${count}`;
  const toggle = (key: keyof View["options"]) => () => run("set_options", { [key]: !view.options[key] });
  const foldTip = view.foldable ? undefined : "Folding needs Format JSON on and valid JSON";
  const tree = view.viewMode === "tree";
  const modes: { mode: View["viewMode"]; label: string; title: string }[] = [
    { mode: "diff", label: "Diff", title: "Both documents side by side, line by line" },
    { mode: "tree", label: "Tree", title: "An overview: one tree of keys, with what changed marked" },
    { mode: "edit", label: "Edit", title: "Edit or paste either document" },
  ];
  return (
    <div className="toolbar">
      <div className="segmented">
        {modes.map(({ mode, label, title }) => (
          <button key={mode} className={view.viewMode === mode ? "active" : ""} title={title} onClick={() => run("set_view_mode", { mode })}>
            {label}
          </button>
        ))}
      </div>
      <span className="divider" />
      {tree ? (
        <>
          <label title="List every key, not only the paths to changes">
            <input
              type="checkbox"
              checked={view.tree?.showUnchanged ?? false}
              onChange={() => run("set_tree_options", { show_unchanged: !view.tree?.showUnchanged })}
            />{" "}
            Show unchanged
          </label>
          <label title="List keys alphabetically">
            <input type="checkbox" checked={view.options.sort_keys} onChange={toggle("sort_keys")} /> Sort keys
          </label>
          <span className="divider" />
          <button title="Swap original and modified" onClick={() => run("swap_sides")}>
            Swap sides
          </button>
          <span className="divider" />
          <button title="Fold the tree to the top-level keys" onClick={() => run("set_tree_options", { open_changes: false })}>
            Collapse all
          </button>
          <button title="Open every path down to each change" onClick={() => run("set_tree_options", { open_changes: true })}>
            Reveal changes
          </button>
        </>
      ) : (
        <>
          <button disabled={count === 0} title="Previous change (Shift+F7)" onClick={() => run("previous_change")}>
            Previous
          </button>
          <button disabled={count === 0} title="Next change (F7)" onClick={() => run("next_change")}>
            Next
          </button>
          <span className="position">{position}</span>
          <span className="divider" />
          <label title="Pretty print both sides and align lines by JSON structure">
            <input type="checkbox" checked={view.options.format} onChange={toggle("format")} /> Format JSON
          </label>
          <label title="Ignore differences in key order" className={view.options.format ? "" : "disabled"}>
            <input type="checkbox" checked={view.options.sort_keys} disabled={!view.options.format} onChange={toggle("sort_keys")} />{" "}
            Sort keys
          </label>
          <label title="Only applies when Format JSON is off" className={view.options.format ? "disabled" : ""}>
            <input
              type="checkbox"
              checked={view.options.ignore_whitespace}
              disabled={view.options.format}
              onChange={toggle("ignore_whitespace")}
            />{" "}
            Ignore whitespace
          </label>
          <span className="divider" />
          <button title="Swap original and modified" onClick={() => run("swap_sides")}>
            Swap sides
          </button>
          <span className="divider" />
          <button disabled={!view.foldable} title={foldTip ?? "Expand every object and array"} onClick={() => run("expand_all")}>
            Expand all
          </button>
          <button disabled={!view.foldable} title={foldTip ?? "Show only the top-level keys"} onClick={() => run("collapse_all")}>
            Collapse all
          </button>
          <button disabled={!view.foldable} title={foldTip ?? "Expand only what leads to a change"} onClick={() => run("reveal_changes")}>
            Reveal changes
          </button>
        </>
      )}
      <IgnoredButton view={view} run={run} />
      <PanelToggles view={view} run={run} />
    </div>
  );
}

/** Show or hide the explorer and the assistant (desktop). */
function PanelToggles({ view, run }: { view: View; run: Run }) {
  return (
    <>
      <span className="divider" />
      <button className={`toggle${view.showExplorer ? " active" : ""}`} title="Toggle the folder explorer" onClick={() => run("set_panel_visible", { panel: "explorer" })}>
        Explorer
      </button>
      {!WEB && (
        <button className={`toggle${view.showAgent ? " active" : ""}`} title="Toggle the assistant panel" onClick={() => run("set_panel_visible", { panel: "agent" })}>
          Assistant
        </button>
      )}
    </>
  );
}

/** "3 ignored": the keys whose changes are ignored, each of which can be
 *  counted again. Hidden when nothing is ignored. */
function IgnoredButton({ view, run }: { view: View; run: Run }) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  if (view.ignored.length === 0) return null;
  const items: MenuEntry[] = [
    ...view.ignored.map(({ label, rule }) => ({
      label: `Stop ignoring ${label}`,
      action: () => run("stop_ignoring", ruleArgs(rule)),
    })),
    "separator",
    { label: "Stop ignoring all", action: () => run("stop_ignoring", { all: true }) },
  ];
  const changes = view.ignoredChanges;
  return (
    <>
      <span className="divider" />
      <button
        className="toggle active ignored-button"
        title={`Changes to these keys are left out of the diff, the assistant's answers and PDF reports (${changes} in this comparison):\n${view.ignored.map((ignored) => ignored.label).join("\n")}`}
        onClick={(event) => {
          const box = event.currentTarget.getBoundingClientRect();
          setMenu(menu ? null : { x: box.left, y: box.bottom + 2 });
        }}
      >
        {view.ignored.length} ignored <span className="codicon codicon-chevron-down" />
      </button>
      {menu && <ContextMenu {...menu} items={items} onClose={() => setMenu(null)} />}
    </>
  );
}

function StatusBar({ view, assistant }: { view: View; assistant: Assistant }) {
  const agent = assistant.connected
    ? assistant.busy
      ? "Assistant: working…"
      : `Assistant: ${assistant.connected}`
    : "Assistant: not connected";
  return (
    <footer className="statusbar">
      {view.status && (
        <span className={view.status.isError ? "status-error" : ""} title={view.status.text}>
          {view.status.isError ? "⚠ " : ""}
          {view.status.text}
        </span>
      )}
      <span className="spacer" />
      {view.semanticChanges !== null && (
        <span>
          {view.semanticChanges} semantic changes
          {view.ignoredChanges > 0 && ` (${view.ignoredChanges} ignored)`}
        </span>
      )}
      <span>
        {view.diff.hunks.length} changed blocks +{view.addedLines} −{view.removedLines}
      </span>
      {!WEB && <span>{Math.round(view.zoom * 100)}%</span>}
      {!WEB && <span>{agent}</span>}
    </footer>
  );
}

/** A side panel whose width can be dragged, and is remembered under
 *  `setting`. */
function Resizable({
  side,
  setting,
  initial,
  min,
  max,
  hidden = false,
  children,
}: {
  side: "left" | "right";
  setting: "explorerWidth" | "assistantWidth";
  initial: number;
  min: number;
  max: number;
  hidden?: boolean;
  children: ReactNode;
}) {
  const clamp = (value: number) => Math.min(max, Math.max(min, value));
  const [width, setWidth] = useState(() => clamp(uiSettings()[setting] ?? initial));
  const startDrag = (event: ReactPointerEvent) => {
    // A drag resizes; it must not also select text.
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = width;
    let latest = width;
    const move = (moved: PointerEvent) => {
      const delta = side === "left" ? moved.clientX - startX : startX - moved.clientX;
      latest = clamp(startWidth + delta);
      setWidth(latest);
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      updateUiSettings({ [setting]: Math.round(latest) });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
  };
  return (
    <aside className={`side-panel ${side}`} style={{ width, display: hidden ? "none" : undefined }}>
      {children}
      <div className="resize-handle" onPointerDown={startDrag} />
    </aside>
  );
}

/** F7 / Shift+F7 and Alt+↓ / Alt+↑ step through changes; F5 rescans. */
function useShortcuts(run: Run, editing: boolean) {
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      let command: string | null = null;
      if (event.key === "F7") command = event.shiftKey ? "previous_change" : "next_change";
      // In edit mode Alt+arrows move lines, as in any Monaco editor.
      else if (event.altKey && !editing && event.key === "ArrowDown") command = "next_change";
      else if (event.altKey && !editing && event.key === "ArrowUp") command = "previous_change";
      // On the web F5 reloads the page, as usual.
      else if (event.key === "F5" && !WEB) command = "refresh";
      if (command) {
        event.preventDefault();
        event.stopPropagation();
        void run(command);
      }
    };
    // Capture, so the keys work while a Monaco editor has focus.
    window.addEventListener("keydown", keydown, true);
    return () => window.removeEventListener("keydown", keydown, true);
  }, [run, editing]);
}

/** Dropped folders open in the explorer half they land on; anything else
 *  opens on the side of the window it lands on. Two items fill both sides. */
function useDrops(explorer: RefObject<HTMLDivElement | null>, run: Run, setView: (view: View) => void) {
  useEffect(() => {
    const sideAt = (x: number, y: number) => {
      let side: Side = x < window.innerWidth / 2 ? "left" : "right";
      const halves = explorer.current?.querySelectorAll<HTMLElement>(".explorer-half");
      halves?.forEach((half) => {
        const box = half.getBoundingClientRect();
        if (x >= box.left && x <= box.right && y >= box.top && y <= box.bottom) {
          side = half.dataset.side as Side;
        }
      });
      return side;
    };
    return onDrop(sideAt, run, setView);
  }, [explorer, run, setView]);
}
