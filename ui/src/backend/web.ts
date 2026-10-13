// The web app's backend: the same Rust app state, compiled to WebAssembly
// (crates/json-diff-web) and running in the page. There are no folders,
// assistant or updater; files come from the file picker or a drop, settings
// live in the browser's storage, and PDFs download.

import init, { App } from "#wasm";
import type { Backend, ExportOutput, KeyAt, Release, RunOutput, View } from "../api";
import type { UiSettings } from "../uiSettings";

/** Where the settings are kept, in this browser only. */
const STORAGE_KEY = "json-diff.settings";

const app: Promise<App> = init().then(() => new App(load()));

function load(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

/** Remembers the settings after anything that may have changed them. */
function remember(app: App) {
  try {
    localStorage.setItem(STORAGE_KEY, app.settings());
  } catch {
    // Storage is off (private browsing): settings last for this visit.
  }
}

/** Runs `work` on the app, then saves the settings. */
async function update<T>(work: (app: App) => T): Promise<T> {
  const ready = await app;
  const result = work(ready);
  remember(ready);
  return result;
}

const view = (json: string) => JSON.parse(json) as View;

/** Asks for one file with the browser's picker; `null` if cancelled. */
function chooseFile(): Promise<File | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,application/json,text/plain";
    input.addEventListener("change", () => resolve(input.files?.[0] ?? null));
    input.addEventListener("cancel", () => resolve(null));
    input.click();
  });
}

/** Opens a picked or dropped file through `run`, so the view updates like
 *  after any button. */
async function openFile(run: Parameters<Backend["pickFile"]>[0], side: string, file: File) {
  await run("open_file", { side, name: file.name, text: await file.text() });
}

/** Saves `bytes` as a download named `name`. */
function download(bytes: Uint8Array, name: string) {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: "application/pdf" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

const unsupported = (what: string) => () => Promise.reject(new Error(`${what} is only in the desktop app`));
const never = () => () => {};

const backend: Backend = {
  platform: "web",
  getView: async () => view((await app).view()),

  runCommand: (name, args) =>
    update((app) => {
      // Files arrive as text here; everything else is one of the tools.
      if (name === "open_file") {
        const opened = view(app.open_text(String(args.side), String(args.name), String(args.text)));
        return { result: null, error: null, view: opened } satisfies RunOutput;
      }
      try {
        return JSON.parse(app.run(name, JSON.stringify(args))) as RunOutput;
      } catch (error) {
        return { result: null, error: String(error), view: JSON.parse(app.view()) as View };
      }
    }),

  editText: (side, text) => update((app) => view(app.edit_text(side, text))),
  keyAt: async (side, line) => JSON.parse((await app).key_at(side, line)) as KeyAt | null,

  pickFile: async (run, side) => {
    const file = await chooseFile();
    if (file) await openFile(run, side, file);
  },
  pickFolder: unsupported("Comparing folders"),

  onDrop: (sideAt, run) => {
    const over = (event: DragEvent) => event.preventDefault();
    const drop = (event: DragEvent) => {
      event.preventDefault();
      const files = [...(event.dataTransfer?.files ?? [])];
      if (files.length >= 2) {
        void openFile(run, "left", files[0]).then(() => openFile(run, "right", files[1]));
      } else if (files.length === 1) {
        void openFile(run, sideAt(event.clientX, event.clientY), files[0]);
      }
    };
    window.addEventListener("dragover", over);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("dragover", over);
      window.removeEventListener("drop", drop);
    };
  },

  onView: never,
  uiSettings: async () => JSON.parse((await app).ui_settings()) as UiSettings | null,
  setUiSettings: (settings) => void update((app) => app.set_ui_settings(JSON.stringify(settings))),

  exportPdf: (context, paper) =>
    update((app): ExportOutput => {
      try {
        const pdf = app.export_pdf(context, paper, Date.now());
        const name = app.pdf_name();
        download(pdf.bytes, name);
        const pages = pdf.pages;
        pdf.free();
        return { result: { path: name, pages }, error: null, view: view(app.view()) };
      } catch (error) {
        return { result: null, error: String(error), view: view(app.view()) };
      }
    }),
  openPdf: unsupported("Opening a PDF"),

  appVersion: async () => {
    await app;
    return App.version();
  },
  // A web page is always the latest version: reloading updates it.
  checkUpdate: async () => null,
  installUpdate: unsupported("Installing updates"),
  openRelease: async (url) => void window.open(url, "_blank", "noopener"),
  onUpdateProgress: never,
  licenses: async () => (await import("#notices")).default,
  whatsNew: async () => JSON.parse((await app).whats_new()) as Release[],
  releaseNotes: async () => JSON.parse((await app).release_notes()) as Release[],

  providers: async () => [],
  models: async () => [],
  apiKeyEnv: async () => null,
  startupModel: async () => null,
  agentConnect: unsupported("The assistant"),
  savedKey: async () => false,
  forgetKey: unsupported("The assistant"),
  agentPrompt: unsupported("The assistant"),
  agentReset: unsupported("The assistant"),
  agentCancel: unsupported("The assistant"),
  onAgent: never,
};

export default backend;
