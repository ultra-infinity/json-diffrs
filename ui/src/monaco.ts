// Monaco, bundled locally: just the editor and the JSON language, with their
// web workers. Nothing is fetched from a CDN.

import * as monaco from "monaco-editor/editor/editor.api";
import "monaco-editor/language/json/monaco.contribution";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/language/json/json.worker?worker";

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    return label === "json" ? new JsonWorker() : new EditorWorker();
  },
};

export { monaco };

export const FONT_FAMILY =
  "'JetBrains Mono', 'Cascadia Code', Consolas, 'DejaVu Sans Mono', monospace";
export const FONT_SIZE = 13;
export const LINE_HEIGHT = 20;

export function monacoTheme(theme: "dark" | "light") {
  return theme === "dark" ? "vs-dark" : "vs";
}
