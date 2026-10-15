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

/** Asks for a folder with the browser's picker; `null` if cancelled. */
export function chooseFolder(): Promise<PickedFolder | null> {
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
