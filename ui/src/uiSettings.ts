// The frontend's part of the remembered settings (Rust keeps the rest and
// writes them all to one settings.json).

import { loadSavedUiSettings, saveUiSettings } from "./api";

export interface UiSettings {
  explorerWidth?: number;
  assistantWidth?: number;
  /** The original side's share of the diff's width, 0 to 1. */
  diffSplit?: number;
  provider?: string;
  model?: string;
  customModel?: boolean;
  /** The `provider:model` the assistant was connected to; reconnects at
   *  startup. */
  connected?: string | null;
  /** PDF export: unchanged lines around each change (null: whole documents). */
  pdfContext?: number | null;
  pdfPaper?: "letter" | "a4";
}

let current: UiSettings = {};
let loading: Promise<UiSettings> | null = null;

/** Reads the saved settings once; later calls share the result. */
export function loadUiSettings(): Promise<UiSettings> {
  loading ??= loadSavedUiSettings().then(
    (saved) => (current = saved ?? {}),
    () => current,
  );
  return loading;
}

/** The settings as loaded and changed since. */
export const uiSettings = (): UiSettings => current;

export function updateUiSettings(patch: Partial<UiSettings>) {
  current = { ...current, ...patch };
  saveUiSettings(current);
}
