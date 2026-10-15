// Help > Privacy & Security: where files go (nowhere), and what makes that
// so, worded for the web app (the main one) or the desktop app.

import { useEffect } from "react";
import { WEB } from "../api";
import { CONTENT_SECURITY_POLICY } from "../csp";

export function PrivacyDialog({ onClose }: { onClose: () => void }) {
  useEffect(() => {
    const key = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onPointerDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="modal medium" role="dialog" aria-label="Privacy and security">
        <header className="panel-header">
          <span className="section-title">Privacy &amp; security</span>
          <span className="spacer" />
          <button className="icon" title="Close (Esc)" onClick={onClose}>
            <span className="codicon codicon-close" />
          </button>
        </header>
        <div className="whats-new privacy">{WEB ? <Web /> : <Desktop />}</div>
        <footer className="row end whats-new-actions">
          <button onClick={onClose}>Close</button>
        </footer>
      </div>
    </div>
  );
}

function Web() {
  return (
    <>
      <h3>Your files stay on your computer</h3>
      <ul>
        <li>
          json-diff runs entirely in this page. Files and folders you open are read and compared here, by json-diff's
          engine compiled to WebAssembly. They are never uploaded: json-diff has no server to send them to.
        </li>
        <li>Exported PDFs are made in the page and saved by your browser.</li>
        <li>Your settings and ignored keys are kept in this browser's storage, on this computer only.</li>
        <li>No accounts, no cookies, no analytics, no tracking.</li>
      </ul>
      <h3>Enforced by your browser</h3>
      <p>
        The page carries a <strong>Content Security Policy</strong>: a rule your browser applies to it, which only lets
        it load files from and connect to its own site. Any request to anywhere else is refused by the browser, so no
        code in the page could send your data away. This is the policy, which you can also see in the page's source and
        your browser's developer tools:
      </p>
      <pre className="policy">{CONTENT_SECURITY_POLICY.split("; ").join(";\n")}</pre>
      <h3>See for yourself</h3>
      <ul>
        <li>
          json-diff is open source (MIT license): Help → Source Code shows how the page is made.
        </li>
        <li>
          The web app is a folder of static files, so your organisation can also host it on its own servers.
        </li>
      </ul>
    </>
  );
}

function Desktop() {
  return (
    <>
      <h3>Your files stay on your computer</h3>
      <ul>
        <li>Files and folders are read from your disk and compared by the app itself. They are never uploaded.</li>
        <li>Settings are saved in your user folder; assistant API keys only in your system keychain.</li>
        <li>No accounts, no analytics, no tracking.</li>
      </ul>
      <h3>When the app goes online</h3>
      <ul>
        <li>It checks GitHub for a new version at start and from Help → Check for Updates.</li>
        <li>
          The assistant, only if you connect one, sends your questions and the parts of the documents it looks at to
          the AI provider you chose, as its tool calls show.
        </li>
      </ul>
      <p>The window also runs under a Content Security Policy that only lets it load the app's own files.</p>
    </>
  );
}
