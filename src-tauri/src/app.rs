//! Turns shell invocations into jobs and job windows.
//!
//! Explorer starts one process per selected file ("%1"), so the first
//! process becomes the single instance and every later one forwards its
//! arguments here. Invocations for the same action that arrive within a
//! short quiet window are collected into one job, which is how "Merge
//! selected files" sees the whole selection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use localpdf_core::{Action, Options, Plan, Progress, Report};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::shell;

const QUIET: Duration = Duration::from_millis(500);
const WIDTH: f64 = 480.0;
const BACKGROUND: tauri::window::Color = tauri::window::Color(14, 14, 16, 255);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum JobState {
    Loading,
    Input,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobView {
    id: u32,
    action: Option<Action>,
    state: JobState,
    plan: Option<Plan>,
    error: Option<String>,
    progress: Option<Progress>,
    report: Option<Report>,
}

struct Pending {
    action: Option<Action>,
    files: Vec<PathBuf>,
    last: Instant,
}

#[derive(Default)]
struct Inner {
    pending: Mutex<Option<Pending>>,
    jobs: Mutex<HashMap<u32, JobView>>,
    next_id: AtomicU32,
}

#[derive(Default, Clone)]
pub struct Jobs(Arc<Inner>);

pub fn run(args: Vec<String>) {
    let jobs = Jobs::default();
    let startup = jobs.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let jobs = app.state::<Jobs>().inner().clone();
            jobs.invoke(app, argv.into_iter().skip(1).collect(), Some(PathBuf::from(cwd)));
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(jobs)
        .invoke_handler(tauri::generate_handler![
            get_job,
            start_job,
            check_ranges,
            reveal,
            ready,
            close,
            shell_status,
            shell_set,
            app_info,
        ])
        .setup(move |app| {
            if let Ok(dir) = app.path().resource_dir() {
                localpdf_core::render::add_search_dir(dir.join("engines"));
            }
            startup.invoke(app.handle(), args, None);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start LocalPDF")
        .run(|_, _| {});
}

impl Jobs {
    /// Handle one command line: `<action> <files…>`, or nothing for the home window.
    fn invoke(&self, app: &AppHandle, args: Vec<String>, cwd: Option<PathBuf>) {
        // macOS may add a process serial number when launched from Finder.
        let args: Vec<String> = args.into_iter().filter(|a| !a.starts_with("-psn_")).collect();
        let Some(first) = args.first() else {
            open_home(app);
            return;
        };
        let action = Action::parse(first);
        let files: Vec<PathBuf> = args[1..]
            .iter()
            .map(|a| absolutize(Path::new(a), cwd.as_deref()))
            .collect();

        let mut pending = self.0.pending.lock().unwrap();
        if let Some(p) = pending.as_mut() {
            if p.action == action && action.is_some() {
                p.files.extend(files);
                p.last = Instant::now();
                return;
            }
        }
        // A different action: flush whatever was collecting first.
        if let Some(prev) = pending.take() {
            self.create_job(app, prev);
        }
        *pending = Some(Pending { action, files, last: Instant::now() });
        drop(pending);

        let (jobs, app) = (self.clone(), app.clone());
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            let mut pending = jobs.0.pending.lock().unwrap();
            match pending.as_ref() {
                None => return, // flushed by a different action
                Some(p) if p.last.elapsed() >= QUIET => {
                    let p = pending.take().unwrap();
                    drop(pending);
                    jobs.create_job(&app, p);
                    return;
                }
                Some(_) => {}
            }
        });
    }

    fn create_job(&self, app: &AppHandle, p: Pending) {
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let mut view = JobView {
            id,
            action: p.action,
            state: JobState::Loading,
            plan: None,
            error: None,
            progress: None,
            report: None,
        };
        if p.action.is_none() {
            view.state = JobState::Failed;
            view.error = Some("LocalPDF was started with an unknown action.".into());
        }
        self.0.jobs.lock().unwrap().insert(id, view.clone());
        open_job_window(app, id, p.action);

        if let Some(action) = p.action {
            let (jobs, app) = (self.clone(), app.clone());
            std::thread::spawn(move || {
                let mut files = p.files;
                files.dedup();
                match localpdf_core::plan(action, &files) {
                    Ok(plan) if plan.needs_input => jobs.update(&app, id, |v| {
                        v.plan = Some(plan);
                        v.state = JobState::Input;
                    }),
                    Ok(plan) => {
                        jobs.update(&app, id, |v| {
                            v.plan = Some(plan.clone());
                            v.state = JobState::Running;
                        });
                        jobs.execute(&app, id, plan, Options::default());
                    }
                    Err(e) => jobs.update(&app, id, |v| {
                        v.error = Some(e.to_string());
                        v.state = JobState::Failed;
                    }),
                }
            });
        }
    }

    fn update(&self, app: &AppHandle, id: u32, f: impl FnOnce(&mut JobView)) {
        let view = {
            let mut jobs = self.0.jobs.lock().unwrap();
            let Some(v) = jobs.get_mut(&id) else { return };
            f(v);
            v.clone()
        };
        let _ = app.emit_to(label(id), "job", view);
    }

    /// Runs on the calling thread; call from a worker thread.
    fn execute(&self, app: &AppHandle, id: u32, plan: Plan, opts: Options) {
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let report = localpdf_core::run(&plan, &opts, &mut |p| {
            if last_emit.elapsed() >= Duration::from_millis(80) {
                last_emit = Instant::now();
                self.update(app, id, |v| v.progress = Some(p));
            }
        });
        // Nothing written and every failure is fixable in the dialog (wrong
        // password, bad range): go back to the dialog instead of ending.
        let retry = plan.needs_input
            && report.outputs.is_empty()
            && !report.failures.is_empty()
            && report.failures.iter().all(|f| f.retry.is_some());
        if retry {
            self.update(app, id, |v| {
                v.state = JobState::Input;
                v.progress = None;
                v.error = report.failures.first().map(|f| f.message.clone());
            });
            return;
        }
        self.update(app, id, |v| {
            v.error = None;
            v.state = if report.outputs.is_empty() && !report.failures.is_empty() {
                JobState::Failed
            } else {
                JobState::Done
            };
            v.report = Some(report);
        });
    }
}

fn absolutize(p: &Path, cwd: Option<&Path>) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match cwd.map(Path::to_path_buf).or_else(|| std::env::current_dir().ok()) {
        Some(base) => base.join(p),
        None => p.to_path_buf(),
    }
}

fn label(id: u32) -> String {
    format!("job-{id}")
}

fn title(action: Option<Action>) -> String {
    let name = match action {
        Some(Action::Convert) => "Convert",
        Some(Action::Compress) => "Compress",
        Some(Action::Merge) => "Merge",
        Some(Action::Split) => "Split / extract pages",
        Some(Action::Rotate) => "Rotate pages",
        Some(Action::Unlock) => "Unlock",
        Some(Action::Protect) => "Protect with password",
        None => "LocalPDF",
    };
    format!("{name} — LocalPDF")
}

fn base_window(app: &AppHandle, label: String, route: String, title: String) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(format!("index.html#{route}").into()))
        .title(title)
        .inner_size(WIDTH, 180.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .visible(false)
        .center()
        .theme(Some(tauri::Theme::Dark))
        .background_color(BACKGROUND)
        .build()
}

fn open_job_window(app: &AppHandle, id: u32, action: Option<Action>) {
    if let Err(e) = base_window(app, label(id), format!("/job/{id}"), title(action)) {
        eprintln!("LocalPDF: couldn't open a window: {e}");
    }
}

fn open_home(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("home") {
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    // First plain launch on macOS adds the Finder actions; Windows does this
    // in the installer.
    shell::ensure_registered_on_launch(app);
    let _ = base_window(app, "home".into(), "/home".into(), "LocalPDF".into());
}

// ---------------------------------------------------------------- commands

#[tauri::command]
fn get_job(jobs: State<Jobs>, id: u32) -> Option<JobView> {
    jobs.0.jobs.lock().unwrap().get(&id).cloned()
}

#[tauri::command]
fn start_job(app: AppHandle, jobs: State<Jobs>, id: u32, options: Options) -> Result<(), String> {
    let plan = {
        let mut all = jobs.0.jobs.lock().unwrap();
        let v = all.get_mut(&id).ok_or("This job no longer exists.")?;
        if v.state != JobState::Input {
            return Err("This job has already run.".into());
        }
        v.state = JobState::Running;
        v.plan.clone().ok_or("Nothing to run.")?
    };
    let jobs = jobs.inner().clone();
    std::thread::spawn(move || {
        let _ = app.emit_to(label(id), "job", get_view(&jobs, id));
        jobs.execute(&app, id, plan, options);
    });
    Ok(())
}

fn get_view(jobs: &Jobs, id: u32) -> Option<JobView> {
    jobs.0.jobs.lock().unwrap().get(&id).cloned()
}

#[tauri::command]
fn check_ranges(jobs: State<Jobs>, id: u32, spec: String) -> Option<String> {
    let all = jobs.0.jobs.lock().unwrap();
    let plan = all.get(&id)?.plan.as_ref()?;
    localpdf_core::check_ranges(&spec, &plan.files).err()
}

/// Reveal one of a job's outputs. Only paths LocalPDF wrote are accepted.
#[tauri::command]
fn reveal(jobs: State<Jobs>, id: u32, index: usize) -> Result<(), String> {
    let path = {
        let all = jobs.0.jobs.lock().unwrap();
        all.get(&id)
            .and_then(|v| v.report.as_ref())
            .and_then(|r| r.outputs.get(index).cloned())
            .ok_or("Nothing to show.")?
    };
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// The UI calls this once it has laid itself out, so windows never flash
/// empty or at the wrong size.
#[tauri::command]
fn ready(window: WebviewWindow, height: f64) {
    // Some platforms ignore programmatic resizes of fixed-size windows.
    let _ = window.set_resizable(true);
    let _ = window.set_size(tauri::LogicalSize::new(WIDTH, height.clamp(160.0, 760.0)));
    let _ = window.set_resizable(false);
    if window.is_visible().unwrap_or(true) {
        return;
    }
    let _ = window.center();
    let _ = window.show();
    // A forwarded invocation doesn't own the foreground; pin briefly so the
    // dialog lands in front of Explorer/Finder instead of blinking in the taskbar.
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let w = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        let _ = w.set_always_on_top(false);
    });
}

#[tauri::command]
fn close(window: WebviewWindow) {
    let _ = window.close();
}

#[tauri::command]
fn shell_status() -> shell::Status {
    shell::status()
}

#[tauri::command]
fn shell_set(app: AppHandle, enabled: bool) -> Result<shell::Status, String> {
    let r = if enabled { shell::register() } else { shell::unregister() };
    r.map_err(|e| e.to_string())?;
    shell::remember_choice(&app, enabled);
    Ok(shell::status())
}

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    platform: &'static str,
    libreoffice: bool,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        libreoffice: localpdf_core::office::find_soffice().is_some(),
    }
}
