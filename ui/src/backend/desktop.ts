// The desktop app's backend: Tauri commands and events, handled by the Rust
// side in crates/json-diff/src/main.rs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { sideName, type AgentEvent, type Backend, type ExportOutput, type Side, type Unlisten, type View } from "../api";

/** A Tauri event listener that can be stopped straight away. */
function subscribe<T>(event: string, handler: (payload: T) => void): Unlisten {
  const callback: EventCallback<T> = (message) => handler(message.payload);
  const stop = listen<T>(event, callback);
  return () => void stop.then((unlisten) => unlisten());
}

const backend: Backend = {
  platform: "desktop",
  getView: () => invoke<View>("get_view"),
  runCommand: (name, args) => invoke("run", { name, args }),
  editText: (side, text) => invoke<View>("edit_text", { side, text }),
  keyAt: (side, line) => invoke("key_at", { side, line }),

  pickFile: async (run, side) => {
    const path = await open({
      title: `Open ${sideName(side)} file`,
      filters: [
        { name: "JSON files", extensions: ["json"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof path === "string") await run("select_file", { side, path });
  },

  pickFolder: async (run, side) => {
    const path = await open({ directory: true, title: `Open ${sideName(side)} folder` });
    if (typeof path === "string") await run("open_folder", { side, path });
  },

  onDrop: (sideAt, _run, setView) => {
    const stop = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return;
      const ratio = window.devicePixelRatio || 1;
      const side: Side = sideAt(event.payload.position.x / ratio, event.payload.position.y / ratio);
      void invoke<View>("open_paths", { paths: event.payload.paths, side }).then(setView);
    });
    return () => void stop.then((unlisten) => unlisten());
  },

  onView: (handler) => subscribe("view", handler),
  uiSettings: () => invoke("ui_settings"),
  setUiSettings: (settings) => void invoke("set_ui_settings", { settings }),

  exportPdf: async (context, paper) => {
    const path = await save({
      title: "Save the PDF report",
      defaultPath: await invoke<string>("pdf_suggested_path"),
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path) return null;
    return invoke<ExportOutput>("export_pdf", { path, context, paper });
  },
  openPdf: (path) => invoke("open_pdf", { path }),

  appVersion: () => invoke("app_version"),
  checkUpdate: () => invoke("check_update"),
  installUpdate: () => invoke("install_update"),
  openRelease: (url) => invoke("open_release", { url }),
  onUpdateProgress: (handler) => subscribe("update-progress", handler),
  licenses: () => invoke("licenses"),
  whatsNew: () => invoke("whats_new"),
  releaseNotes: () => invoke("release_notes"),

  providers: () => invoke("providers"),
  models: (provider) => invoke("models", { provider }),
  apiKeyEnv: (provider) => invoke("api_key_env", { provider }),
  startupModel: () => invoke("startup_model"),
  agentConnect: (model, apiKey, remember) => invoke("agent_connect", { model, apiKey, remember }),
  savedKey: (provider) => invoke("saved_key", { provider }),
  forgetKey: (provider) => invoke("forget_key", { provider }),
  agentPrompt: (text) => invoke("agent_prompt", { text }),
  agentReset: () => invoke("agent_reset"),
  agentCancel: () => invoke("agent_cancel"),
  onAgent: (handler) => subscribe<AgentEvent>("agent", handler),
};

export default backend;
