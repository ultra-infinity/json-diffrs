// The frontend's part of the remembered settings (Rust keeps the rest and
// writes them all to one settings.json).

import { invoke } from "@tauri-apps/api/core";

export interface UiSettings {
  explorerWidth?: number;
  assistantWidth?: number;
  provider?: string;
  model?: string;
  customModel?: boolean;
  /** The `provider:model` the assistant was connected to; reconnects at
   *  startup. */
  connected?: string | null;
}

let current: UiSettings = {};
let loading: Promise<UiSettings> | null = null;

/** Reads the saved settings once; later calls share the result. */
export function loadUiSettings(): Promise<UiSettings> {
  loading ??= invoke<UiSettings | null>("ui_settings").then(
    (saved) => (current = saved ?? {}),
    () => current,
  );
  return loading;
}

/** The settings as loaded and changed since. */
export const uiSettings = (): UiSettings => current;

export function updateUiSettings(patch: Partial<UiSettings>) {
  current = { ...current, ...patch };
  void invoke("set_ui_settings", { settings: current });
}
