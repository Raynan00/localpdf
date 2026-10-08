//! Office documents -> PDF through LibreOffice, run headless with a throwaway
//! profile. Release builds ship their own trimmed LibreOffice; development
//! builds fall back to one installed on the system.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{LpError, Result};
use crate::naming;

pub fn find_soffice() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LOCALPDF_SOFFICE") {
        return Some(p.into());
    }
    bundled_soffice().or_else(system_soffice)
}

/// The LibreOffice that ships inside LocalPDF: `engines/libreoffice` next to
/// the app on Windows, `Contents/Resources/LibreOffice.app` on macOS.
pub fn bundled_soffice() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "soffice.exe" } else { "soffice" };
    let mut candidates = Vec::new();
    for dir in crate::render::engine_dirs() {
        candidates.push(dir.join("libreoffice").join("program").join(exe));
        candidates.push(dir.join("LibreOffice.app/Contents/MacOS").join(exe));
        candidates.push(dir.join("../LibreOffice.app/Contents/MacOS").join(exe));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// A LibreOffice the user installed themselves (development builds, Linux).
fn system_soffice() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(base) = std::env::var_os(var) {
                candidates.push(PathBuf::from(base).join("LibreOffice\\program\\soffice.exe"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join("Applications/LibreOffice.app/Contents/MacOS/soffice"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        candidates.push("/usr/bin/soffice".into());
        candidates.push("/usr/lib/libreoffice/program/soffice".into());
        candidates.push("/opt/libreoffice/program/soffice".into());
    }
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }
    let exe = if cfg!(windows) { "soffice.exe" } else { "soffice" };
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

const TIMEOUT: Duration = Duration::from_secs(180);

pub fn to_pdf(source: &Path) -> Result<PathBuf> {
    let soffice = find_soffice().ok_or_else(|| {
        LpError::EngineMissing(
            "No Office converter found. The full LocalPDF installer includes one; with \
             LocalPDF Lite, install LibreOffice (libreoffice.org) to convert Office files."
                .into(),
        )
    })?;
    let work = tempfile::Builder::new()
        .prefix("localpdf-office-")
        .tempdir()
        .map_err(|e| LpError::Engine(format!("Couldn't create a work folder: {e}")))?;
    let profile = url::Url::from_directory_path(work.path().join("profile"))
        .map_err(|_| LpError::Engine("Bad work folder path".into()))?;
    let out_dir = work.path().join("out");

    let mut cmd = Command::new(&soffice);
    cmd.arg(format!("-env:UserInstallation={profile}"))
        .args(["--headless", "--invisible", "--norestore", "--nolockcheck", "--nodefault"])
        .args(["--convert-to", "pdf", "--outdir"])
        .arg(&out_dir)
        .arg(source)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| LpError::Engine(format!("Couldn't start LibreOffice: {e}")))?;
    let started = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| LpError::Engine(e.to_string()))? {
            break s;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            return Err(LpError::Engine(format!(
                "LibreOffice took longer than {} s on {}, so it was stopped.",
                TIMEOUT.as_secs(),
                crate::error::file_name(source)
            )));
        }
        std::thread::sleep(Duration::from_millis(150));
    };
    let produced = out_dir.join(format!("{}.pdf", naming::stem(source)));
    if !status.success() || !produced.is_file() {
        let mut stderr = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut e, &mut stderr);
        }
        return Err(LpError::Unsupported {
            path: source.to_path_buf(),
            reason: format!(
                "LibreOffice couldn't open it{}",
                if stderr.trim().is_empty() { ".".to_string() } else { format!(" ({})", stderr.trim()) }
            ),
        });
    }
    let target = naming::sibling(source, "", "pdf");
    let bytes = std::fs::read(&produced).map_err(|source| LpError::Read { path: produced.clone(), source })?;
    naming::write_atomic(&target, &bytes)?;
    Ok(target)
}
