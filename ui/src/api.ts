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
  addedLines: number;
  removedLines: number;
  title: string;
}

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

export const agentConnect = (model: string, apiKey: string | null) =>
  invoke<void>("agent_connect", { model, apiKey });
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
