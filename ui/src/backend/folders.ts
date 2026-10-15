// Folders in the browser: the folder picker and dropped folders hand over
// every file inside, which are read here (nothing leaves the page) and passed
// to Rust as one list. Only visible .json files are kept, as when the desktop
// app scans a folder; hidden folders such as .git are not even read.

/** A file inside a picked or dropped folder, by its path in that folder. */
export interface FolderEntry {
  path: string;
  file: File;
}

export interface PickedFolder {
  name: string;
  entries: FolderEntry[];
}

/** Files larger than this are left out, as the desktop app refuses them. */
const MAX_FILE_BYTES = 64 * 1024 * 1024;
/** Folders are read up to this many JSON files, as on the desktop. */
const MAX_FILES = 10_000;
/** Files read at the same time. */
const READ_BATCH = 32;

/** A `.json` file with no hidden part in its path (`.git/x.json`). */
export function isListedJson(path: string): boolean {
  return /\.json$/i.test(path) && path.split("/").every((part) => !part.startsWith("."));
}

/** Asks for a folder for `side` (`left` or `right`); `null` if cancelled.
 *
 *  Where the browser has the File System Access API (Chrome, Edge), its
 *  prompt asks to "view files". Elsewhere (Firefox, Safari) the only picker
 *  is a file input, whose prompt says "upload". Either way a note first says
 *  that the files stay on this computer, until the user turns it off. Must
 *  be called from a click, as both pickers are. */
export function chooseFolder(side: string): Promise<PickedFolder | null> {
  const access = typeof window.showDirectoryPicker === "function";
  const choose = access ? () => chooseWithAccess(side) : chooseWithInput;
  if (explained()) return choose();
  return explainThenChoose(access, choose);
}

/** The File System Access API's directory handle, as far as it is used. */
interface DirectoryHandle {
  kind: "directory";
  name: string;
  values(): AsyncIterable<DirectoryHandle | FileHandle>;
}

interface FileHandle {
  kind: "file";
  name: string;
  getFile(): Promise<File>;
}

declare global {
  interface Window {
    showDirectoryPicker?: (options?: { id?: string; mode?: "read" }) => Promise<unknown>;
  }
}

async function chooseWithAccess(side: string): Promise<PickedFolder | null> {
  let root: DirectoryHandle;
  try {
    // The id makes the browser remember each side's last folder.
    root = (await window.showDirectoryPicker!({ id: `json-diff-${side}`, mode: "read" })) as DirectoryHandle;
  } catch (error) {
    // Cancelled (AbortError), or a folder the browser will not share.
    if (!(error instanceof DOMException && error.name === "AbortError")) console.warn(error);
    return null;
  }
  const entries: FolderEntry[] = [];
  const visit = async (folder: DirectoryHandle, prefix: string) => {
    for await (const child of folder.values()) {
      if (child.name.startsWith(".")) continue;
      const path = prefix + child.name;
      if (child.kind === "directory") await visit(child, `${path}/`);
      else if (isListedJson(path)) entries.push({ path, file: await child.getFile() });
    }
  };
  await visit(root, "");
  return { name: root.name, entries };
}

/** The classic folder picker (`<input webkitdirectory>`). */
function chooseWithInput(): Promise<PickedFolder | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.webkitdirectory = true;
    input.addEventListener("change", () => {
      const files = [...(input.files ?? [])];
      if (files.length === 0) return resolve(null);
      // `webkitRelativePath` is `folder/inner/path.json`.
      const name = files[0].webkitRelativePath.split("/")[0] || "folder";
      const entries = files.map((file) => ({
        path: file.webkitRelativePath.split("/").slice(1).join("/"),
        file,
      }));
      resolve({ name, entries });
    });
    input.addEventListener("cancel", () => resolve(null));
    input.click();
  });
}

/** Set once the user ticks "Don't explain this again". */
const EXPLAINED_KEY = "json-diff.upload-prompt-explained";

function explained(): boolean {
  try {
    return localStorage.getItem(EXPLAINED_KEY) === "yes";
  } catch {
    return false;
  }
}

/** A note before the picker that the files stay on this computer, worded
 *  for the browser's prompt (`access`: "view files"; otherwise "upload").
 *  Its button runs `choose`, within the click the browser needs. */
function explainThenChoose(
  access: boolean,
  choose: () => Promise<PickedFolder | null>,
): Promise<PickedFolder | null> {
  return new Promise((resolve) => {
    const element = <K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = "") => {
      const node = document.createElement(tag);
      node.className = className;
      node.textContent = text;
      return node;
    };
    const backdrop = element("div", "modal-backdrop");
    const dialog = element("div", "modal small");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-label", "Open a folder");
    const header = element("header", "panel-header");
    header.append(element("span", "section-title", "Open a folder"));
    const body = element("div", "export-body");
    const prompt = access
      ? "Your browser will now ask whether this site may view the folder\u2019s files. "
      : "Your browser will now ask whether to \u201cupload\u201d the folder. That is its usual " +
        "wording for choosing a folder: ";
    body.append(
      element(
        "p",
        "",
        `${prompt}json-diff reads the files right here in this page. They stay on your ` +
          "computer, and nothing is sent anywhere.",
      ),
    );
    const remember = element("label");
    const checkbox = element("input");
    checkbox.type = "checkbox";
    remember.append(checkbox, " Don\u2019t explain this again");
    const buttons = element("div", "row end");
    const cancel = element("button", "", "Cancel");
    const chooseButton = element("button", "primary", "Choose folder\u2026");
    buttons.append(cancel, chooseButton);
    body.append(remember, buttons);
    dialog.append(header, body);
    backdrop.append(dialog);

    const close = () => {
      backdrop.remove();
      window.removeEventListener("keydown", escape, true);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
        resolve(null);
      }
    };
    cancel.addEventListener("click", () => {
      close();
      resolve(null);
    });
    backdrop.addEventListener("pointerdown", (event) => {
      if (event.target === backdrop) {
        close();
        resolve(null);
      }
    });
    chooseButton.addEventListener("click", () => {
      if (checkbox.checked) {
        try {
          localStorage.setItem(EXPLAINED_KEY, "yes");
        } catch {
          // Storage is off: the note shows again next time.
        }
      }
      close();
      void choose().then(resolve, () => resolve(null));
    });
    window.addEventListener("keydown", escape, true);
    document.body.append(backdrop);
    chooseButton.focus();
  });
}

/** Every file in a dropped folder, recursively, skipping hidden folders. */
export async function readDroppedFolder(directory: FileSystemDirectoryEntry): Promise<PickedFolder> {
  const entries: FolderEntry[] = [];
  const visit = async (folder: FileSystemDirectoryEntry, prefix: string) => {
    for (const child of await listDirectory(folder)) {
      if (child.name.startsWith(".")) continue;
      const path = prefix + child.name;
      if (child.isDirectory) {
        await visit(child as FileSystemDirectoryEntry, `${path}/`);
      } else if (isListedJson(path)) {
        entries.push({ path, file: await fileOf(child as FileSystemFileEntry) });
      }
    }
  };
  await visit(directory, "");
  return { name: directory.name, entries };
}

/** The `[path, text]` pairs Rust takes: listed JSON files, read in batches. */
export async function readTexts(entries: FolderEntry[]): Promise<[string, string][]> {
  const wanted = entries
    .filter((entry) => isListedJson(entry.path) && entry.file.size <= MAX_FILE_BYTES)
    .slice(0, MAX_FILES);
  const texts: [string, string][] = [];
  for (let start = 0; start < wanted.length; start += READ_BATCH) {
    const batch = wanted.slice(start, start + READ_BATCH);
    texts.push(...(await Promise.all(batch.map(async (entry) => [entry.path, await entry.file.text()] as [string, string]))));
  }
  return texts;
}

/** All of a directory's entries: the reader hands them over in batches. */
function listDirectory(folder: FileSystemDirectoryEntry): Promise<FileSystemEntry[]> {
  const reader = folder.createReader();
  const all: FileSystemEntry[] = [];
  return new Promise((resolve, reject) => {
    const next = () =>
      reader.readEntries((batch) => {
        if (batch.length === 0) return resolve(all);
        all.push(...batch);
        next();
      }, reject);
    next();
  });
}

function fileOf(entry: FileSystemFileEntry): Promise<File> {
  return new Promise((resolve, reject) => entry.file(resolve, reject));
}
