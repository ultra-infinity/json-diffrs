// The Rust backend's commands and events, typed. `View` mirrors
// `crates/json-diff/src/view.rs`; keep the two in step.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Side = "left" | "right";
export type LineStatus = "unchanged" | "added" | "removed";
export type PairStatus = "identical" | "modified" | "only_left" | "only_right";
export type ViewMode = "diff" | "edit";
export type Theme = "dark" | "light";

export interface View {
  folders: [FolderView | null, FolderView | null];
  paired: boolean;
  docs: [DocView, DocView];
  diff: DiffView;
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

/** `line` is 1-based; `null` for the document's own brackets or plain text. */
export const keyAt = (side: Side, line: number) => invoke<KeyAt | null>("key_at", { side, line });

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

interface RunOutput {
  result: unknown;
  error: string | null;
  view: View;
}

export const getView = () => invoke<View>("get_view");

/** Runs one of the assistant's tools, the command behind every button. */
export const runCommand = (name: string, args: Record<string, unknown> = {}) =>
  invoke<RunOutput>("run", { name, args });

export const editText = (side: Side, text: string) => invoke<View>("edit_text", { side, text });

export const openPaths = (paths: string[], side: Side) =>
  invoke<View>("open_paths", { paths, side });

export const providers = () => invoke<string[]>("providers");
export const models = (provider: string) => invoke<ModelChoice[]>("models", { provider });
export const apiKeyEnv = (provider: string) => invoke<string | null>("api_key_env", { provider });
export const startupModel = () => invoke<string | null>("startup_model");

/** Connects the assistant. Rejects only if `remember` was set and the key
 *  could not be saved; the connection itself reports through `onAgent`. */
export const agentConnect = (model: string, apiKey: string | null, remember: boolean) =>
  invoke<void>("agent_connect", { model, apiKey, remember });
/** Whether a key for `provider` is saved in the system keychain. */
export const savedKey = (provider: string) => invoke<boolean>("saved_key", { provider });
export const forgetKey = (provider: string) => invoke<void>("forget_key", { provider });
export const agentPrompt = (text: string) => invoke<void>("agent_prompt", { text });
export const agentReset = () => invoke<void>("agent_reset");
export const agentCancel = () => invoke<void>("agent_cancel");

/** The view after the assistant changed something. */
export const onView = (handler: (view: View) => void): Promise<UnlistenFn> =>
  listen<View>("view", (event) => handler(event.payload));

export const onAgent = (handler: (event: AgentEvent) => void): Promise<UnlistenFn> =>
  listen<AgentEvent>("agent", (event) => handler(event.payload));

/** Runs a command, used by every button: returns the new view. */
export type Run = (name: string, args?: Record<string, unknown>) => Promise<void>;

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
  /** This build installs updates itself (Windows). */
  canInstall: boolean;
  releaseUrl: string;
}

/** The newest release if it is newer than this build, else null. Fails when
 *  updates are not set up (no signing key) or offline. */
export const checkUpdate = () => invoke<UpdateInfo | null>("check_update");
/** Downloads, verifies and installs the update; the app then closes. */
export const installUpdate = () => invoke<void>("install_update");
export const openRelease = (url: string) => invoke<void>("open_release", { url });
export const appVersion = () => invoke<string>("app_version");

export const onUpdateProgress = (
  handler: (progress: { downloaded: number; total: number | null }) => void,
): Promise<UnlistenFn> =>
  listen<{ downloaded: number; total: number | null }>("update-progress", (event) => handler(event.payload));

/** json-diff's license and those of every bundled library. */
export const licenses = () => invoke<string>("licenses");

/** One release's entry in CHANGELOG.md. */
export interface Release {
  version: string;
  date: string | null;
  /** Markdown: `-` bullets with **bold** and `code`. */
  notes: string;
}

/** The releases since the version that last ran; empty after the first call. */
export const whatsNew = () => invoke<Release[]>("whats_new");
/** Every release up to this one, newest first. */
export const releaseNotes = () => invoke<Release[]>("release_notes");

export type Paper = "letter" | "a4";

/** Writes the PDF report to `path` (chosen in the save dialog). `context`
 *  null means whole documents. */
export const exportPdf = async (path: string, context: number | null, paper: Paper) =>
  invoke<{ result: { path: string; pages: number } | null; error: string | null; view: View }>("export_pdf", {
    path,
    context,
    paper,
  });
/** Where the save dialog for a PDF should start. */
export const pdfSuggestedPath = () => invoke<string>("pdf_suggested_path");
export const openPdf = (path: string) => invoke<void>("open_pdf", { path });
