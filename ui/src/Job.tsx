import { useEffect, useMemo, useRef, useState } from "react";
import { api, baseName, humanSize, type Action, type JobView, type Options, type Plan } from "./api";
import { Chips } from "./Chips";
import { useFitWindow } from "./useFitWindow";

const NAMES: Record<Action, string> = {
  convert: "Convert",
  compress: "Compress",
  merge: "Merge",
  split: "Split / extract",
  rotate: "Rotate",
  unlock: "Unlock",
  protect: "Protect",
};

const VERBS: Record<Action, string> = {
  convert: "Converting",
  compress: "Compressing",
  merge: "Merging",
  split: "Splitting",
  rotate: "Rotating",
  unlock: "Unlocking",
  protect: "Protecting",
};

export function Job({ id }: { id: number }) {
  const [job, setJob] = useState<JobView | null>(null);

  useEffect(() => {
    let alive = true;
    const unlisten = api.onJob((v) => alive && v.id === id && setJob(v));
    // Subscribe first, then read: no update can fall between the two.
    api.getJob(id).then((v) => alive && v && setJob((cur) => cur ?? v));
    return () => {
      alive = false;
      unlisten.then((f) => f());
    };
  }, [id]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && job?.state !== "running") api.close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [job?.state]);

  if (!job) return null;
  if (job.state === "input" && job.plan) return <Dialog job={job} plan={job.plan} />;
  return <Status job={job} />;
}

// ------------------------------------------------------------------ header

function Header({ action, plan, eyebrow }: { action: Action | null; plan: Plan | null; eyebrow?: string }) {
  const files = plan?.files ?? [];
  const first = files[0];
  const meta: string[] = [];
  if (eyebrow) {
    // Result screens: the eyebrow says what happened; no file stats.
  } else if (files.length === 1 && first) {
    if (first.pages != null) meta.push(`${first.pages} page${first.pages === 1 ? "" : "s"}`);
    meta.push(humanSize(first.size));
    if (first.needsPassword) meta.push("password-protected");
  } else if (files.length > 1) {
    meta.push(`${files.length} files`);
    meta.push(humanSize(files.reduce((n, f) => n + f.size, 0)));
  }
  return (
    <header className="head">
      <div className="eyebrow">
        {eyebrow ?? (action ? NAMES[action] : "LocalPDF")}
        {meta.length > 0 && <span className="dim"> · {meta.join(" · ")}</span>}
      </div>
      {first && (
        <h1 className="filename" title={first.path}>
          {first.name}
          {files.length > 1 && <span className="more"> + {files.length - 1} more</span>}
        </h1>
      )}
    </header>
  );
}

function Footer(props: { primary: string; disabled?: boolean; onPrimary: () => void; cancel?: string }) {
  return (
    <footer className="foot">
      <button type="button" className="ghost" onClick={() => api.close()}>
        {props.cancel ?? "Cancel"}
      </button>
      <button type="button" className="primary" disabled={props.disabled} onClick={props.onPrimary}>
        {props.primary}
      </button>
    </footer>
  );
}

// ------------------------------------------------------------------ dialog

type Target = NonNullable<Options["target"]>;
type Mode = NonNullable<Options["splitMode"]>;

function Dialog({ job, plan }: { job: JobView; plan: Plan }) {
  const action = plan.action;
  const [target, setTarget] = useState<Target>("docx");
  const [dpi, setDpi] = useState("150");
  const [combine, setCombine] = useState("one");
  const [mode, setMode] = useState<Mode>("extract");
  const [ranges, setRanges] = useState("");
  const [angle, setAngle] = useState("90");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [rangeError, setRangeError] = useState<string | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(job.error);
  const firstInput = useRef<HTMLInputElement>(null);

  // Back from a failed attempt (wrong password, bad range): show why, retry.
  useEffect(() => {
    if (!job.error) return;
    setSubmitError(job.error);
    if (action === "unlock") setPassword("");
    firstInput.current?.focus();
  }, [job, action]);

  const usesRanges = action === "rotate" || (action === "split" && mode !== "each");
  const rangeRequired = action === "split" && mode !== "each";

  useEffect(() => {
    if (!usesRanges || ranges.trim() === "") {
      setRangeError(null);
      return;
    }
    const t = setTimeout(() => api.checkRanges(job.id, ranges).then(setRangeError), 120);
    return () => clearTimeout(t);
  }, [ranges, usesRanges, job.id]);

  useEffect(() => firstInput.current?.focus(), [action, mode]);

  const pwMismatch = action === "protect" && confirm !== "" && confirm !== password;
  const invalid =
    (usesRanges && rangeError != null) ||
    (rangeRequired && ranges.trim() === "") ||
    ((action === "unlock" || action === "protect") && password === "") ||
    (action === "protect" && confirm !== password);

  const submit = () => {
    if (invalid) return;
    setSubmitError(null);
    const options: Options = {};
    if (action === "convert") {
      if (plan.input === "pdf") {
        options.target = target;
        if (target === "png" || target === "jpeg") options.dpi = Number(dpi);
      } else {
        options.combine = combine === "one";
      }
    }
    if (action === "split") {
      options.splitMode = mode;
      options.ranges = ranges;
    }
    if (action === "rotate") {
      options.angle = Number(angle);
      options.ranges = ranges;
    }
    if (action === "unlock" || action === "protect") options.password = password;
    api.startJob(job.id, options).catch((e) => setSubmitError(String(e)));
  };

  const pageCount = plan.files.length === 1 ? plan.files[0].pages : null;
  const primary = (() => {
    switch (action) {
      case "convert":
        if (plan.input !== "pdf") return combine === "one" ? "Make one PDF" : "Make PDFs";
        return { png: "Convert to PNG", jpeg: "Convert to JPEG", docx: "Convert to Word", txt: "Convert to text", pdf: "Convert" }[target];
      case "split":
        return { extract: "Extract pages", ranges: "Split", each: "Split every page" }[mode];
      case "rotate":
        return "Rotate";
      case "unlock":
        return "Unlock";
      case "protect":
        return "Protect";
      default:
        return NAMES[action];
    }
  })();

  const ref = useFitWindow([action, target, mode, rangeError, pwMismatch, submitError]);

  return (
    <form
      ref={ref as unknown as React.RefObject<HTMLFormElement>}
      className="sheet"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <Header action={action} plan={plan} />

      {action === "convert" && plan.input === "pdf" && (
        <>
          <Chips<Target>
            label="Convert to"
            value={target}
            onChange={setTarget}
            options={[
              { value: "docx", label: "Word" },
              { value: "png", label: "PNG" },
              { value: "jpeg", label: "JPEG" },
              { value: "txt", label: "Text" },
            ]}
          />
          {(target === "png" || target === "jpeg") && (
            <Chips
              label="Resolution"
              value={dpi}
              onChange={setDpi}
              options={[
                { value: "72", label: "Screen · 72 dpi" },
                { value: "150", label: "Standard · 150 dpi" },
                { value: "300", label: "Print · 300 dpi" },
              ]}
            />
          )}
        </>
      )}

      {action === "convert" && plan.input !== "pdf" && (
        <Chips
          label={`${plan.files.length} images to PDF`}
          value={combine}
          onChange={setCombine}
          options={[
            { value: "one", label: "One PDF, in name order" },
            { value: "each", label: "One PDF per image" },
          ]}
        />
      )}

      {action === "split" && (
        <Chips<Mode>
          label="Mode"
          value={mode}
          onChange={setMode}
          options={[
            { value: "extract", label: "Extract into one PDF" },
            { value: "ranges", label: "One PDF per range" },
            { value: "each", label: "Every page" },
          ]}
        />
      )}

      {action === "rotate" && (
        <Chips
          label="Rotate"
          value={angle}
          onChange={setAngle}
          options={[
            { value: "90", label: "90° right" },
            { value: "180", label: "180°" },
            { value: "270", label: "90° left" },
          ]}
        />
      )}

      {usesRanges && (
        <div className="field">
          <label className="label" htmlFor="ranges">
            Pages{pageCount != null && <span className="dim"> · 1–{pageCount}</span>}
          </label>
          <input
            id="ranges"
            ref={firstInput}
            className={rangeError ? "input bad" : "input"}
            placeholder={action === "rotate" ? "All pages" : mode === "ranges" ? "1-3, 4-8, 9-" : "1-3, 5, 8-"}
            value={ranges}
            onChange={(e) => setRanges(e.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
          {rangeError && <div className="hint bad">{rangeError}</div>}
        </div>
      )}

      {(action === "unlock" || action === "protect") && (
        <div className="field">
          <label className="label" htmlFor="pw">
            {action === "unlock" ? "Password for this file" : "New password"}
          </label>
          <input
            id="pw"
            ref={firstInput}
            className="input"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="off"
          />
        </div>
      )}
      {action === "protect" && (
        <div className="field">
          <label className="label" htmlFor="pw2">
            Repeat password
          </label>
          <input
            id="pw2"
            className={pwMismatch ? "input bad" : "input"}
            type="password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            autoComplete="off"
          />
          {pwMismatch && <div className="hint bad">The passwords don't match.</div>}
          {!pwMismatch && <div className="hint">AES-256. There is no way to recover a forgotten password.</div>}
        </div>
      )}

      {submitError && <div className="hint bad">{submitError}</div>}
      <Footer primary={primary} disabled={invalid} onPrimary={submit} />
      <button type="submit" hidden />
    </form>
  );
}

// ------------------------------------------------------------------ status

function Status({ job }: { job: JobView }) {
  const action = job.action;
  const report = job.report;
  const [hover, setHover] = useState(false);
  const ref = useFitWindow([job.state, report?.outputs.length, report?.failures.length, job.error]);

  const succeeded = job.state === "done" && report != null && report.failures.length === 0;
  // A clean one-click result clears itself; anything needing attention stays.
  useEffect(() => {
    if (!succeeded || hover) return;
    const t = setTimeout(() => api.close(), 9000);
    return () => clearTimeout(t);
  }, [succeeded, hover]);

  const pct = useMemo(() => {
    const p = job.progress;
    if (!p || p.total === 0 || p.done === 0) return null;
    return Math.min(100, Math.round((p.done / p.total) * 100));
  }, [job.progress]);

  if (job.state === "loading" || job.state === "running" || job.state === "input") {
    return (
      <div className="sheet" ref={ref}>
        <Header action={action} plan={job.plan} />
        <div className="working">
          <div className="working-line">
            <span className="key">{action ? VERBS[action] : "Working"}</span>
            {job.progress?.label ? <span className="dim"> · {job.progress.label}</span> : job.state === "loading" ? <span className="dim"> · reading files</span> : null}
          </div>
          <div className="bar">
            <div className={pct == null ? "bar-fill indeterminate" : "bar-fill"} style={pct == null ? undefined : { width: `${Math.max(pct, 4)}%` }} />
          </div>
        </div>
        <footer className="foot">
          <span className="dim small">Working on this computer only.</span>
        </footer>
      </div>
    );
  }

  const outputs = report?.outputs ?? [];
  const failures = report?.failures ?? [];
  const notes = report?.notes ?? [];
  const failedOutright = job.state === "failed";

  return (
    <div className="sheet" ref={ref} onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}>
      <Header
        action={action}
        plan={job.plan}
        eyebrow={failedOutright ? "Couldn't finish" : outputs.length ? "Saved next to the original" : "Nothing new to save"}
      />

      {job.error && <p className="message">{job.error}</p>}

      {outputs.length > 0 && (
        <ul className="outputs">
          {outputs.map((o, i) => (
            <li key={o}>
              <button className="link" onClick={() => api.reveal(job.id, i)} title={o}>
                <span className="key">{baseName(o)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}

      {notes.length > 0 && (
        <ul className="notes">
          {notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}

      {failures.length > 0 && (
        <ul className="failures">
          {failures.map((f, i) => (
            <li key={i}>{f.message}</li>
          ))}
        </ul>
      )}

      <footer className="foot">
        <button className="ghost" onClick={() => api.close()}>
          Close
        </button>
        {outputs.length > 0 && (
          <button className="primary" autoFocus onClick={() => api.reveal(job.id, 0).then(() => api.close())}>
            Show in folder
          </button>
        )}
      </footer>
    </div>
  );
}
