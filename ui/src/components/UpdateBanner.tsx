// "A new version is available": checked once at startup, or from
// Help > Check for Updates. On Windows one click downloads, verifies and
// installs it; elsewhere the banner links to the release.

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { appVersion, checkUpdate, installUpdate, onUpdateProgress, openRelease, type UpdateInfo } from "../api";

type Phase =
  | { kind: "idle" }
  | { kind: "available"; info: UpdateInfo }
  | { kind: "installing"; info: UpdateInfo; percent: number | null }
  | { kind: "current"; version: string }
  | { kind: "failed"; message: string };

export interface Updates {
  phase: Phase;
  /** This build's version. */
  version: string;
  /** `manual`: also report "up to date" and failures, not just news. */
  check: (manual: boolean) => void;
  install: () => void;
  dismiss: () => void;
}

export function useUpdates(): Updates {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [version, setVersion] = useState("");

  const check = useCallback((manual: boolean) => {
    checkUpdate().then(
      (info) => {
        if (info) setPhase({ kind: "available", info });
        else if (manual) void appVersion().then((version) => setPhase({ kind: "current", version }));
      },
      // At startup, being offline or unconfigured is not worth a banner.
      (error) => manual && setPhase({ kind: "failed", message: String(error) }),
    );
  }, []);

  useEffect(() => {
    void appVersion().then(setVersion);
    check(false);
    const unlisten = onUpdateProgress(({ downloaded, total }) =>
      setPhase((phase) =>
        phase.kind === "installing"
          ? { ...phase, percent: total ? Math.round((downloaded / total) * 100) : null }
          : phase,
      ),
    );
    return () => void unlisten.then((stop) => stop());
  }, [check]);

  const install = () => {
    if (phase.kind !== "available") return;
    setPhase({ kind: "installing", info: phase.info, percent: null });
    // On success the installer takes over and the app closes.
    installUpdate().catch((error) => setPhase({ kind: "failed", message: String(error) }));
  };

  return { phase, version, check, install, dismiss: () => setPhase({ kind: "idle" }) };
}

/** Release notes in the update's manifest; older releases put a link there. */
const hasNotes = (info: UpdateInfo) => !!info.notes?.trim() && !/^https?:\/\/\S*$/.test(info.notes.trim());

export function UpdateBanner({ updates, onNotes }: { updates: Updates; onNotes: (info: UpdateInfo) => void }) {
  const { phase, dismiss } = updates;
  switch (phase.kind) {
    case "idle":
      return null;
    case "current":
      return (
        <Banner tone="info" onClose={dismiss}>
          json-diff {phase.version} is the latest version.
        </Banner>
      );
    case "failed":
      return (
        <Banner tone="error" onClose={dismiss}>
          {phase.message}
        </Banner>
      );
    case "installing":
      return (
        <Banner tone="info">
          Downloading json-diff {phase.info.version}…{phase.percent === null ? "" : ` ${phase.percent}%`}
          <span className="progress">
            <span style={{ width: `${phase.percent ?? 0}%` }} />
          </span>
        </Banner>
      );
    case "available":
      return (
        <Banner tone="news" onClose={dismiss}>
          <span>
            json-diff {phase.info.version} is available (you have {phase.info.currentVersion}).
          </span>
          <button
            className="link"
            onClick={() => (hasNotes(phase.info) ? onNotes(phase.info) : void openRelease(phase.info.releaseUrl))}
          >
            What's new
          </button>
          {phase.info.canInstall ? (
            <button className="primary" onClick={updates.install}>
              Update now
            </button>
          ) : (
            <span className="weak">Update it with your package manager, e.g. nix profile upgrade.</span>
          )}
        </Banner>
      );
  }
}

function Banner({
  tone,
  onClose,
  children,
}: {
  tone: "news" | "info" | "error";
  onClose?: () => void;
  children: ReactNode;
}) {
  return (
    <div className={`banner ${tone}`} role="status">
      {children}
      <span className="spacer" />
      {onClose && (
        <button className="icon" title="Dismiss" onClick={onClose}>
          <span className="codicon codicon-close" />
        </button>
      )}
    </div>
  );
}
