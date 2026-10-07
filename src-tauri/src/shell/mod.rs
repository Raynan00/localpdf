#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
//! File-manager integration: Explorer context menu on Windows, Finder
//! Quick Actions on macOS. Both are per-user and need no admin rights.

use serde::Serialize;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

/// The PDF actions offered on the menu: (id, label, CLI action).
pub const PDF_ACTIONS: &[(&str, &str, &str)] = &[
    ("convert", "Convert…", "convert"),
    ("compress", "Compress", "compress"),
    ("merge", "Merge selected files", "merge"),
    ("split", "Split / extract pages…", "split"),
    ("rotate", "Rotate pages…", "rotate"),
    ("unlock", "Unlock…", "unlock"),
    ("protect", "Protect with password…", "protect"),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub supported: bool,
    pub registered: bool,
    /// Registered, but pointing at another copy of the app.
    pub stale: bool,
    pub location: &'static str,
}

fn exe() -> std::io::Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(exe.canonicalize().unwrap_or(exe))
}

#[cfg(any(windows, target_os = "macos"))]
pub fn register() -> std::io::Result<()> {
    platform::register(&exe()?)
}

#[cfg(any(windows, target_os = "macos"))]
pub fn unregister() -> std::io::Result<()> {
    platform::unregister()
}

#[cfg(any(windows, target_os = "macos"))]
pub fn status() -> Status {
    let exe = exe().ok();
    platform::status(exe.as_deref())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn register() -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "file-manager menus are only supported on Windows and macOS"))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn unregister() -> std::io::Result<()> {
    register()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn status() -> Status {
    Status { supported: false, registered: false, stale: false, location: "file manager" }
}

const OPT_OUT: &str = "menu-removed";

/// Remember that the user removed the menu, so launching the app doesn't
/// quietly add it back.
pub fn remember_choice(app: &AppHandle, enabled: bool) {
    let Ok(dir) = app.path().app_config_dir() else { return };
    let marker = dir.join(OPT_OUT);
    if enabled {
        let _ = std::fs::remove_file(marker);
    } else {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(marker, b"");
    }
}

/// Add (or repoint) the menu on a plain launch unless the user removed it.
/// Covers macOS, where there is no installer, and app copies that moved.
pub fn ensure_registered_on_launch(app: &AppHandle) {
    let opted_out = app
        .path()
        .app_config_dir()
        .map(|d| d.join(OPT_OUT).exists())
        .unwrap_or(false);
    let s = status();
    if s.supported && !opted_out && (!s.registered || s.stale) {
        if let Err(e) = register() {
            eprintln!("LocalPDF: couldn't add the menu: {e}");
        }
    }
}
