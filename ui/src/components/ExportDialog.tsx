// File > Export as PDF: a few options, then the save dialog. The report
// itself is made in Rust (report.rs, layout.rs, pdf.rs).

import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { exportPdf, openPdf, pdfSuggestedPath, type Paper, type View } from "../api";
import { uiSettings, updateUiSettings } from "../uiSettings";

const CONTEXTS: { value: number | null; label: string }[] = [
  { value: 3, label: "3 lines around each change" },
  { value: 10, label: "10 lines around each change" },
  { value: null, label: "Whole documents" },
];

export function ExportDialog({
  onClose,
  onView,
  onDone,
}: {
  onClose: () => void;
  onView: (view: View) => void;
  onDone: (exported: { path: string; pages: number }) => void;
}) {
  const saved = uiSettings();
  const [context, setContext] = useState<number | null>(saved.pdfContext === undefined ? 3 : saved.pdfContext);
  const [paper, setPaper] = useState<Paper>(saved.pdfPaper ?? "letter");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    const key = (event: KeyboardEvent) => event.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [busy, onClose]);

  const start = async () => {
    updateUiSettings({ pdfContext: context, pdfPaper: paper });
    const path = await save({
      title: "Save the PDF report",
      defaultPath: await pdfSuggestedPath(),
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path) return;
    setBusy(true);
    setProblem(null);
    try {
      const output = await exportPdf(path, context, paper);
      onView(output.view);
      if (output.result) {
        onDone(output.result);
        onClose();
      } else {
        setProblem(output.error ?? "The export failed.");
      }
    } catch (error) {
      setProblem(String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onPointerDown={(event) => event.target === event.currentTarget && !busy && onClose()}>
      <div className="modal small" role="dialog" aria-label="Export as PDF">
        <header className="panel-header">
          <span className="section-title">Export as PDF</span>
          <span className="spacer" />
          <button className="icon" title="Close (Esc)" disabled={busy} onClick={onClose}>
            <span className="codicon codicon-close" />
          </button>
        </header>
        <div className="export-body">
          <p className="weak">
            A summary of every change, then the side-by-side diff. Folded parts are always included.
          </p>
          <fieldset>
            <legend>Side-by-side pages show</legend>
            {CONTEXTS.map((option) => (
              <label key={option.label}>
                <input type="radio" checked={context === option.value} onChange={() => setContext(option.value)} />
                {option.label}
              </label>
            ))}
          </fieldset>
          <fieldset>
            <legend>Paper</legend>
            <label>
              <input type="radio" checked={paper === "letter"} onChange={() => setPaper("letter")} /> US Letter
            </label>
            <label>
              <input type="radio" checked={paper === "a4"} onChange={() => setPaper("a4")} /> A4
            </label>
          </fieldset>
          {problem && <p className="note error">{problem}</p>}
          <div className="row end">
            <button disabled={busy} onClick={onClose}>
              Cancel
            </button>
            <button className="primary" disabled={busy} onClick={() => void start()}>
              {busy ? "Exporting…" : "Export…"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

/** After an export: where it went, and a button to open it. */
export function ExportedBar({ exported, onClose }: { exported: { path: string; pages: number }; onClose: () => void }) {
  return (
    <div className="banner info" role="status">
      <span>
        Exported a {exported.pages}-page PDF to {exported.path}
      </span>
      <button className="link" onClick={() => void openPdf(exported.path)}>
        Open
      </button>
      <span className="spacer" />
      <button className="icon" title="Dismiss" onClick={onClose}>
        <span className="codicon codicon-close" />
      </button>
    </div>
  );
}
