// The Rust backend's commands and events, typed. `View` mirrors
// `crates/json-diff-app/src/view.rs`; keep the two in step.
//
// The backend is the desktop app (Tauri) or the web app (WebAssembly),
// chosen when the frontend is built: `#backend` is `backend/desktop.ts` or
// `backend/web.ts` (vite.config.ts). Components only use this module.

import backend from "#backend";
import type { UiSettings } from "./uiSettings";

export type Side = "left" | "right";
export type LineStatus = "unchanged" | "added" | "removed";
export type PairStatus = "identical" | "modified" | "only_left" | "only_right";
export type ViewMode = "diff" | "tree" | "edit";
export type Theme = "dark" | "light";

export interface View {
  folders: [FolderView | null, FolderView | null];
  paired: boolean;
  docs: [DocView, DocView];
  diff: DiffView;
  /** Only in the tree view mode. */
  tree: TreeView | null;
  currentHunk: number | null;
  reveal: number | null;
  options: { format: boolean; sort_keys: boolean; ignore_whitespace: boolean };
  viewMode: ViewMode;
  theme: Theme;
  zoom: number;
  showExplorer: boolean;
  showAgent: boolean;
  status: { text: string; isError: boolean } | null;
  agentsMd: AgentsMdView[];
  foldable: boolean;
  semanticChanges: number | null;
  /** Keys whose changes are ignored (right-click a key in the diff). */
  ignored: IgnoredView[];
  /** Semantic changes left out because their key is ignored. */
  ignoredChanges: number;
  addedLines: number;
  removedLines: number;
  title: string;
}

export interface TreeView {
  /** `null` when a side is not valid JSON. */
  rows: TreeRow[] | null;
  showUnchanged: boolean;
}

/** `changed`: an object or array with changes inside. */
export type TreeStatus = "unchanged" | "added" | "removed" | "modified" | "changed";

/** One visible node of the merged tree (crates/json-diff-core/src/tree.rs). */
export interface TreeRow {
  /** For expand_tree_node / collapse_tree_node. */
  id: string;
  depth: number;
  /** The key, or `[index]` for an array element. */
  label: string;
  leftPointer: string | null;
  rightPointer: string | null;
  status: TreeStatus;
  ignoredBy: IgnoreRule | null;
  changes: number;
  /** Compact values, cut short; objects and arrays summarised (`{3 keys}`). */
  old: string | null;
  new: string | null;
  /** For a non-empty object or array: whether it is open. */
  expanded: boolean | null;
}

/** A pointer rule names a node as the original document has it. */
export type IgnoreRule = { kind: "pointer"; pointer: string } | { kind: "key"; key: string };

export interface IgnoredView {
  /** `/a/b` or `"name" everywhere`. */
  label: string;
  rule: IgnoreRule;
}

/** The key under a line of the diff. */
export interface KeyAt {
  pointer: string;
  /** `null` for an array element. */
  key: string | null;
  ignoredBy: IgnoredView | null;
}

/** The `stop_ignoring` arguments that name `rule`. */
export const ruleArgs = (rule: IgnoreRule) =>
  rule.kind === "pointer" ? { pointer: rule.pointer } : { key: rule.key };

export interface FolderView {
  root: string;
  name: string;
  files: FileView[];
}

export interface FileView {
  /** Relative to the folder, `/`-separated. */
  path: string;
  status: PairStatus | null;
  selected: boolean;
}

export interface DocView {
  label: string;
  path: string | null;
  error: { message: string; line: number; column: number } | null;
  /** Only sent in edit mode. */
  text: string | null;
}

export interface DiffView {
  left: SideView;
  right: SideView;
  hunks: HunkView[];
  /** Aligned blocks: `[left start, left end, right start, right end]`. */
  segments: [number, number, number, number][];
}

export interface SideView {
  text: string;
  /** One character per line: `.` unchanged, `+` added, `-` removed. */
  status: string;
  folds: FoldView[];
  inline: InlineView[];
}

export interface FoldView {
  /** 1-based. */
  line: number;
  pointer: string;
  collapsed: boolean;
  children: number;
  hiddenLines: number;
  isArray: boolean;
}

export interface InlineView {
  /** 1-based line; 1-based UTF-16 columns, end exclusive. */
  line: number;
  start: number;
  end: number;
}

export interface HunkView {
  /** 0-based, end exclusive; empty = insertion point. */
  left: [number, number];
  right: [number, number];
  leftStatus: LineStatus;
  rightStatus: LineStatus;
}

export interface AgentsMdView {
  path: string;
  label: string;
  source: "found" | "added";
  enabled: boolean;
}

export interface ModelChoice {
  id: string;
  name: string;
}

export type AgentEvent =
  | { kind: "connected"; model: string }
  | { kind: "tool"; name: string; arguments: string; result: string | null; error: string | null }
  | { kind: "reply"; text: string }
  | { kind: "failed"; message: string }
  | { kind: "cancelled" };

export interface RunOutput {
  result: unknown;
  error: string | null;
  view: View;
}

/** Runs a command, used by every button: returns the new view. */
export type Run = (name: string, args?: Record<string, unknown>) => Promise<void>;

export type Unlisten = () => void;

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
  /** This build installs updates itself (Windows). */
  canInstall: boolean;
  releaseUrl: string;
}

/** One release's entry in CHANGELOG.md. */
export interface Release {
  version: string;
  date: string | null;
  /** Markdown: `-` bullets with **bold** and `code`. */
  notes: string;
}

export type Paper = "letter" | "a4";

/** A finished PDF export: where it went (a path, or the downloaded file's
 *  name on the web) and its length. */
export interface Exported {
  path: string;
  pages: number;
}

export interface ExportOutput {
  result: Exported | null;
  error: string | null;
  view: View;
}

/** What the frontend needs from the app; see `backend/desktop.ts` and
 *  `backend/web.ts`. */
export interface Backend {
  platform: "desktop" | "web";
  getView(): Promise<View>;
  /** Runs one of the assistant's tools, the command behind every button. */
  runCommand(name: string, args: Record<string, unknown>): Promise<RunOutput>;
  editText(side: Side, text: string): Promise<View>;
  /** `line` is 1-based; `null` for the document's own brackets or plain text. */
  keyAt(side: Side, line: number): Promise<KeyAt | null>;
  /** Asks for a file and opens it on `side`. */
  pickFile(run: Run, side: Side): Promise<void>;
  /** Asks for a folder and opens it on `side` (desktop only). */
  pickFolder(run: Run, side: Side): Promise<void>;
  /** Opens what is dropped on the window. `sideAt` names the side a single
   *  item dropped at a point (CSS pixels) goes to; two fill both sides. */
  onDrop(sideAt: (x: number, y: number) => Side, run: Run, setView: (view: View) => void): Unlisten;
  /** The view after the assistant changed something. */
  onView(handler: (view: View) => void): Unlisten;
  uiSettings(): Promise<UiSettings | null>;
  setUiSettings(settings: UiSettings): void;
  /** Asks where to save (desktop) and writes the PDF, or downloads it (web);
   *  `null` if the user cancelled. `context` null means whole documents. */
  exportPdf(context: number | null, paper: Paper): Promise<ExportOutput | null>;
  /** Opens an exported PDF in the system's viewer (desktop only). */
  openPdf(path: string): Promise<void>;
  appVersion(): Promise<string>;
  /** The newest release if it is newer than this build, else null. Fails
   *  when updates are not set up (no signing key) or offline. */
  checkUpdate(): Promise<UpdateInfo | null>;
  /** Downloads, verifies and installs the update; the app then closes. */
  installUpdate(): Promise<void>;
  openRelease(url: string): Promise<void>;
  onUpdateProgress(handler: (progress: { downloaded: number; total: number | null }) => void): Unlisten;
  /** json-diff's license and those of every bundled library. */
  licenses(): Promise<string>;
  /** The releases since the version that last ran; empty after the first call. */
  whatsNew(): Promise<Release[]>;
  /** Every release up to this one, newest first. */
  releaseNotes(): Promise<Release[]>;
  // The assistant (desktop only).
  providers(): Promise<string[]>;
  models(provider: string): Promise<ModelChoice[]>;
  apiKeyEnv(provider: string): Promise<string | null>;
  startupModel(): Promise<string | null>;
  /** Connects the assistant. Rejects only if `remember` was set and the key
   *  could not be saved; the connection itself reports through `onAgent`. */
  agentConnect(model: string, apiKey: string | null, remember: boolean): Promise<void>;
  /** Whether a key for `provider` is saved in the system keychain. */
  savedKey(provider: string): Promise<boolean>;
  forgetKey(provider: string): Promise<void>;
  agentPrompt(text: string): Promise<void>;
  agentReset(): Promise<void>;
  agentCancel(): Promise<void>;
  onAgent(handler: (event: AgentEvent) => void): Unlisten;
}

/** Which app this frontend was built for. */
export const PLATFORM = backend.platform;
export const WEB = PLATFORM === "web";

export const {
  getView,
  editText,
  keyAt,
  pickFile,
  pickFolder,
  onDrop,
  onView,
  exportPdf,
  openPdf,
  appVersion,
  checkUpdate,
  installUpdate,
  openRelease,
  onUpdateProgress,
  licenses,
  whatsNew,
  releaseNotes,
  providers,
  models,
  apiKeyEnv,
  startupModel,
  agentConnect,
  savedKey,
  forgetKey,
  agentPrompt,
  agentReset,
  agentCancel,
  onAgent,
} = backend;

export const runCommand = (name: string, args: Record<string, unknown> = {}) => backend.runCommand(name, args);
export const loadSavedUiSettings = () => backend.uiSettings();
export const saveUiSettings = (settings: UiSettings) => backend.setUiSettings(settings);

export function sideName(side: Side) {
  return side === "left" ? "original" : "modified";
}
