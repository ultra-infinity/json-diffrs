// The explorer's folder tree, built from the flat list of relative paths
// Rust sends.

import type { FileView, PairStatus } from "./api";

export interface Directory {
  name: string;
  /** Relative to the opened folder, `/`-separated; empty for the root. */
  path: string;
  directories: Directory[];
  files: FileView[];
  /** Holds a file that differs from its twin, at any depth. */
  changed: boolean;
}

export function buildTree(files: FileView[]): Directory {
  const root: Directory = { name: "", path: "", directories: [], files: [], changed: false };
  for (const file of files) {
    const parts = file.path.split("/");
    let directory = root;
    for (const part of parts.slice(0, -1)) {
      const path = directory.path ? `${directory.path}/${part}` : part;
      let child = directory.directories.find((candidate) => candidate.name === part);
      if (!child) {
        child = { name: part, path, directories: [], files: [], changed: false };
        directory.directories.push(child);
      }
      directory = child;
    }
    directory.files.push(file);
  }
  const mark = (directory: Directory): boolean => {
    const inFiles = directory.files.some((file) => file.status !== null && file.status !== "identical");
    const inDirectories = directory.directories.map(mark).some(Boolean);
    directory.changed = inFiles || inDirectories;
    return directory.changed;
  };
  mark(root);
  return root;
}

/** The explorer's badge: Modified, Deleted (only in the original) or Added. */
export function badgeOf(status: PairStatus | null): "M" | "D" | "A" | null {
  switch (status) {
    case "modified":
      return "M";
    case "only_left":
      return "D";
    case "only_right":
      return "A";
    default:
      return null;
  }
}
