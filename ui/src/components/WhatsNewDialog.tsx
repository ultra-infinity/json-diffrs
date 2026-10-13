// What changed: shown once after an update, from the update bar before
// installing one, and under Help > What's New.

import { useEffect, type ReactNode } from "react";
import type { Release } from "../api";
import { parseNotes, type Span } from "../notes";

export function WhatsNewDialog({
  title,
  releases,
  onClose,
  actions,
}: {
  title: string;
  releases: Release[];
  onClose: () => void;
  /** Buttons beside Close, e.g. Update now. */
  actions?: ReactNode;
}) {
  useEffect(() => {
    const key = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onPointerDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="modal medium" role="dialog" aria-label={title}>
        <header className="panel-header">
          <span className="section-title">{title}</span>
          <span className="spacer" />
          <button className="icon" title="Close (Esc)" onClick={onClose}>
            <span className="codicon codicon-close" />
          </button>
        </header>
        <div className="whats-new">
          {releases.length === 0 && <p className="weak">No release notes.</p>}
          {releases.map((release) => (
            <section key={release.version}>
              <h3>
                {release.version}
                {release.date && <span className="weak"> · {release.date}</span>}
              </h3>
              <Notes notes={release.notes} />
            </section>
          ))}
        </div>
        <footer className="row end whats-new-actions">
          {actions}
          <button onClick={onClose}>Close</button>
        </footer>
      </div>
    </div>
  );
}

function Notes({ notes }: { notes: string }) {
  const blocks = parseNotes(notes);
  const items: ReactNode[] = [];
  const out: ReactNode[] = [];
  const flush = () => {
    if (items.length) out.push(<ul key={out.length}>{items.splice(0)}</ul>);
  };
  blocks.forEach((block, index) => {
    if (block.kind === "item") {
      items.push(<li key={index}>{block.spans.map(span)}</li>);
    } else {
      flush();
      out.push(<p key={out.length}>{block.spans.map(span)}</p>);
    }
  });
  flush();
  return <>{out}</>;
}

function span({ text, style }: Span, index: number) {
  return style === "bold" ? <strong key={index}>{text}</strong> : style === "code" ? <code key={index}>{text}</code> : text;
}
