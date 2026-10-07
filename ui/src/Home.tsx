import { useEffect, useState } from "react";
import { api, type AppInfo, type ShellStatus } from "./api";
import { useFitWindow } from "./useFitWindow";

const ACTIONS = ["Convert", "Compress", "Merge", "Split", "Rotate", "Unlock", "Protect"];

export function Home() {
  const [status, setStatus] = useState<ShellStatus | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ref = useFitWindow([status, info, error]);

  useEffect(() => {
    api.shellStatus().then(setStatus);
    api.appInfo().then(setInfo);
  }, []);

  const toggle = (enabled: boolean) => {
    setBusy(true);
    setError(null);
    api
      .shellSet(enabled)
      .then(setStatus)
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };

  const on = status?.registered && !status.stale;
  const where = info?.platform === "macos" ? "Finder" : "Explorer";

  return (
    <div className="sheet home" ref={ref}>
      <header className="head">
        <div className="eyebrow">LocalPDF{info && <span className="dim"> · {info.version}</span>}</div>
        <h1 className="display">
          PDF tools in your right-click menu. Files <span className="key">never leave</span> this computer.
        </h1>
      </header>

      <p className="body">
        Right-click PDFs in {where} and choose{" "}
        {info?.platform === "macos" ? "Quick Actions → LocalPDF" : "LocalPDF"}. Images and Office files get
        Convert to PDF.
      </p>

      <div className="tags">{ACTIONS.join("  ·  ")}</div>

      {status?.supported && (
        <div className="row">
          <div>
            <div className="label">{status.location}</div>
            <div className="state">{on ? "On" : status.stale ? "Points to another copy" : "Off"}</div>
          </div>
          {on ? (
            <button className="ghost" disabled={busy} onClick={() => toggle(false)}>
              Remove
            </button>
          ) : (
            <button className="primary" disabled={busy} onClick={() => toggle(true)}>
              Add to menu
            </button>
          )}
        </div>
      )}

      {info && (
        <div className="row">
          <div>
            <div className="label">Office to PDF</div>
            <div className="state">{info.libreoffice ? "LibreOffice found" : "Needs LibreOffice (free, installs separately)"}</div>
          </div>
        </div>
      )}

      {error && <div className="hint bad">{error}</div>}

      <footer className="foot">
        <span className="dim small">No uploads · No account · No telemetry</span>
        <button className="ghost" onClick={() => api.close()}>
          Close
        </button>
      </footer>
    </div>
  );
}
