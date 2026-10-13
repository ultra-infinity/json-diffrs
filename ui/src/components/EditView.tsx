// Edit mode: both documents as editable JSON, with Monaco's own syntax
// checking. Typing is sent to Rust after a short pause; the diff is
// recomputed there and shown when switching back.

import { useEffect, useRef } from "react";
import { editText, type Side, type View } from "../api";
import { FONT_FAMILY, FONT_SIZE, LINE_HEIGHT, monaco, monacoTheme } from "../monaco";

/** Typing pauses this long before the text is sent. */
const SEND_DELAY_MS = 300;

export function EditView({ view, onView }: { view: View; onView: (view: View) => void }) {
  return (
    <div className="edit">
      <TextPane view={view} side="left" onView={onView} />
      <TextPane view={view} side="right" onView={onView} />
    </div>
  );
}

function TextPane({ view, side, onView }: { view: View; side: Side; onView: (view: View) => void }) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  // The text last sent to Rust: when the view brings it back, it is not news.
  const sent = useRef<string | null>(null);
  const doc = view.docs[side === "left" ? 0 : 1];
  const latest = useRef(onView);
  latest.current = onView;

  useEffect(() => {
    const instance = monaco.editor.create(host.current!, {
      value: doc.text ?? "",
      language: "json",
      theme: monacoTheme(view.theme),
      automaticLayout: true,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      fontFamily: FONT_FAMILY,
      fontSize: FONT_SIZE,
      lineHeight: LINE_HEIGHT,
      tabSize: 2,
    });
    editor.current = instance;
    let timer: number | undefined;
    const subscription = instance.onDidChangeModelContent(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const text = instance.getValue();
        sent.current = text;
        editText(side, text).then(latest.current, console.error);
      }, SEND_DELAY_MS);
    });
    return () => {
      window.clearTimeout(timer);
      subscription.dispose();
      instance.dispose();
      editor.current = null;
    };
  }, [side]);

  // Text changed elsewhere (the assistant, Swap sides, a file opened).
  useEffect(() => {
    const instance = editor.current;
    if (!instance || doc.text === null || doc.text === sent.current) return;
    if (doc.text !== instance.getValue()) instance.setValue(doc.text);
  }, [doc.text]);

  return (
    <div className="edit-pane">
      <div className="diff-header">
        <span className="diff-title" title={doc.path ?? undefined}>
          {side === "left" ? "Original" : "Modified"} — {doc.label}
        </span>
        {doc.error && (
          <span className="diff-error" title={doc.error.message}>
            ⚠ line {doc.error.line}, column {doc.error.column}: {doc.error.message}
          </span>
        )}
      </div>
      <div className="edit-editor" ref={host} />
    </div>
  );
}
