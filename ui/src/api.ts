import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Action = "convert" | "compress" | "merge" | "split" | "rotate" | "unlock" | "protect";
export type FileKind = "pdf" | "image" | "office" | "other";

export interface FileInfo {
  path: string;
  name: string;
  size: number;
  kind: FileKind;
  pages: number | null;
  needsPassword: boolean;
  encrypted: boolean;
}

export interface Plan {
  action: Action;
  files: FileInfo[];
  input: "pdf" | "image" | "office" | "mixed";
  needsInput: boolean;
}

export interface Progress {
  done: number;
  total: number;
  label: string;
}

export interface Report {
  outputs: string[];
  failures: { file: string; message: string; retry?: "password" | "ranges" }[];
  notes: string[];
}

export type JobState = "loading" | "input" | "running" | "done" | "failed";

export interface JobView {
  id: number;
  action: Action | null;
  state: JobState;
  plan: Plan | null;
  error: string | null;
  progress: Progress | null;
  report: Report | null;
}

export interface Options {
  target?: "png" | "jpeg" | "docx" | "txt" | "pdf";
  dpi?: number;
  combine?: boolean;
  splitMode?: "extract" | "ranges" | "each";
  ranges?: string;
  angle?: number;
  password?: string;
}

export interface ShellStatus {
  supported: boolean;
  registered: boolean;
  stale: boolean;
  managed: boolean;
  location: string;
}

export interface AppInfo {
  version: string;
  platform: string;
  libreoffice: boolean;
}

export const api = {
  getJob: (id: number) => invoke<JobView | null>("get_job", { id }),
  startJob: (id: number, options: Options) => invoke<void>("start_job", { id, options }),
  checkRanges: (id: number, spec: string) => invoke<string | null>("check_ranges", { id, spec }),
  reveal: (id: number, index: number) => invoke<void>("reveal", { id, index }),
  ready: (height: number) => invoke<void>("ready", { height }),
  close: () => invoke<void>("close"),
  shellStatus: () => invoke<ShellStatus>("shell_status"),
  shellSet: (enabled: boolean) => invoke<ShellStatus>("shell_set", { enabled }),
  appInfo: () => invoke<AppInfo>("app_info"),
  onJob: (cb: (v: JobView) => void) => listen<JobView>("job", (e) => cb(e.payload)),
};

export function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}
