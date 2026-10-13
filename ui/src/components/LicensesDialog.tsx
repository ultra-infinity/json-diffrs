// Help > Licenses: json-diff's license and those of every library it bundles.

import { useEffect, useState } from "react";
import { licenses } from "../api";

export function LicensesDialog({ onClose }: { onClose: () => void }) {
  const [text, setText] = useState("Loading…");

  useEffect(() => {
    licenses().then(setText, (error) => setText(String(error)));
    const key = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onPointerDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-label="Licenses">
        <header className="panel-header">
          <span className="section-title">Licenses</span>
          <span className="spacer" />
          <button className="icon" title="Close (Esc)" onClick={onClose}>
            <span className="codicon codicon-close" />
          </button>
        </header>
        <pre className="licenses">{text}</pre>
      </div>
    </div>
  );
}
