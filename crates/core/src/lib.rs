//! LocalPDF engine. Everything here runs on the local machine: qpdf is
//! compiled in, PDFium is a bundled library, LibreOffice (optional) is a
//! locally installed program. Nothing in this crate opens a socket.

pub mod doc;
pub mod docx;
pub mod error;
pub mod images;
pub mod kinds;
pub mod naming;
pub mod office;
pub mod ranges;
pub mod render;

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use doc::CompressLevel;
pub use error::{LpError, Result};
use kinds::{kind_of, FileKind};
pub use render::ImageFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Convert,
    Compress,
    Merge,
    Split,
    Rotate,
    Unlock,
    Protect,
}

impl Action {
    pub fn parse(s: &str) -> Option<Action> {
        Some(match s.to_ascii_lowercase().as_str() {
            "convert" => Action::Convert,
            "compress" => Action::Compress,
            "merge" => Action::Merge,
            "split" | "extract" => Action::Split,
            "rotate" => Action::Rotate,
            "unlock" => Action::Unlock,
            "protect" => Action::Protect,
            _ => return None,
        })
    }

    pub fn verb(self) -> &'static str {
        match self {
            Action::Convert => "Converting",
            Action::Compress => "Compressing",
            Action::Merge => "Merging",
            Action::Split => "Splitting",
            Action::Rotate => "Rotating",
            Action::Unlock => "Unlocking",
            Action::Protect => "Protecting",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConvertTarget {
    Png,
    Jpeg,
    Docx,
    Txt,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitMode {
    /// Chosen pages into one new PDF.
    Extract,
    /// One PDF per comma-separated range.
    Ranges,
    /// One PDF per page.
    Each,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub target: Option<ConvertTarget>,
    pub dpi: Option<u32>,
    /// Images -> PDF: one combined file (true) or one per image.
    pub combine: Option<bool>,
    pub split_mode: Option<SplitMode>,
    pub ranges: Option<String>,
    /// Clockwise degrees.
    pub angle: Option<i32>,
    pub password: Option<String>,
    pub level: Option<CompressLevel>,
}

/// What the dialog needs to know before anything runs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub action: Action,
    pub files: Vec<FileInfo>,
    /// Which kind of input the selection is: "pdf", "image", "office", "mixed".
    pub input: &'static str,
    /// False for one-click actions: run immediately.
    pub needs_input: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub kind: FileKind,
    pub pages: Option<u32>,
    pub needs_password: bool,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u32,
    pub total: u32,
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub outputs: Vec<PathBuf>,
    pub failures: Vec<Failure>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub file: String,
    pub message: String,
    /// Set for failures the user can fix by retrying with other input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<&'static str>,
}

impl Failure {
    fn new(file: &str, e: &LpError) -> Failure {
        let retry = match e {
            LpError::WrongPassword(_) => Some("password"),
            LpError::InvalidRange(_) => Some("ranges"),
            _ => None,
        };
        Failure { file: file.to_string(), message: e.to_string(), retry }
    }
}

/// Natural order: "page 2" before "page 10".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (na, nb) = (take(&mut a), take(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                a.next();
                b.next();
            }
        }
    }
}

fn describe(path: &Path) -> Result<FileInfo> {
    let meta = std::fs::metadata(path).map_err(|source| LpError::Read { path: path.to_path_buf(), source })?;
    if meta.is_dir() {
        return Err(LpError::Unsupported { path: path.to_path_buf(), reason: "it's a folder.".into() });
    }
    let kind = kind_of(path);
    let mut info = FileInfo {
        path: path.to_path_buf(),
        name: error::file_name(path),
        size: meta.len(),
        kind,
        pages: None,
        needs_password: false,
        encrypted: false,
    };
    if kind == FileKind::Pdf {
        // A damaged PDF is reported when the action runs, per file.
        if let Ok(pdf) = doc::inspect(path) {
            info.pages = (!pdf.needs_password).then_some(pdf.pages);
            info.needs_password = pdf.needs_password;
            info.encrypted = pdf.encrypted;
        }
    }
    Ok(info)
}

/// Validate a selection for an action and decide whether to ask anything.
pub fn plan(action: Action, paths: &[PathBuf]) -> Result<Plan> {
    if paths.is_empty() {
        return Err(LpError::Invalid("No files were selected.".into()));
    }
    let mut files = paths.iter().map(|p| describe(p)).collect::<Result<Vec<_>>>()?;
    files.dedup_by(|a, b| a.path == b.path);

    let count = |k: FileKind| files.iter().filter(|f| f.kind == k).count();
    let (pdfs, imgs, office) = (count(FileKind::Pdf), count(FileKind::Image), count(FileKind::Office));
    if let Some(other) = files.iter().find(|f| f.kind == FileKind::Other) {
        return Err(LpError::Unsupported {
            path: other.path.clone(),
            reason: "LocalPDF works with PDFs, images (PNG, JPEG, TIFF, BMP, GIF, WebP) \
                     and Office documents."
                .into(),
        });
    }
    let input = match (pdfs > 0, imgs > 0, office > 0) {
        (true, false, false) => "pdf",
        (false, true, false) => "image",
        (false, false, true) => "office",
        _ => "mixed",
    };

    if action != Action::Convert {
        if let Some(f) = files.iter().find(|f| f.kind != FileKind::Pdf) {
            return Err(LpError::Unsupported {
                path: f.path.clone(),
                reason: "this action only works on PDFs. Use Convert to make it a PDF first.".into(),
            });
        }
    }

    let needs_input = match action {
        Action::Convert => {
            if pdfs > 0 && pdfs != files.len() {
                return Err(LpError::Invalid(
                    "Convert PDFs and other files separately: PDFs convert to images, Word or text; \
                     images and Office files convert to PDF."
                        .into(),
                ));
            }
            pdfs > 0 || (input == "image" && imgs > 1)
        }
        Action::Compress => false,
        Action::Merge => {
            if files.len() < 2 {
                return Err(LpError::Invalid("Select two or more PDFs to merge them.".into()));
            }
            files.sort_by(|a, b| natural_cmp(&a.name, &b.name));
            false
        }
        Action::Unlock => {
            if files.iter().all(|f| !f.encrypted) {
                return Err(LpError::NotEncrypted(files[0].path.clone()));
            }
            true
        }
        Action::Split | Action::Rotate | Action::Protect => true,
    };
    Ok(Plan { action, files, input, needs_input })
}

/// Check a page-range spec against every selected PDF; first problem wins.
pub fn check_ranges(spec: &str, files: &[FileInfo]) -> std::result::Result<(), String> {
    for f in files {
        if let Some(pages) = f.pages {
            if let Err(e) = ranges::parse(spec, pages) {
                return Err(if files.len() > 1 { format!("{}: {e}", f.name) } else { e.to_string() });
            }
        }
    }
    Ok(())
}

/// Run an action. Errors on one file don't stop the others, except for
/// merge, which produces one file from all of them.
pub fn run(plan: &Plan, opts: &Options, progress: &mut dyn FnMut(Progress)) -> Report {
    let mut report = Report::default();
    let files: Vec<&FileInfo> = plan.files.iter().collect();
    let total = files.len() as u32;

    match plan.action {
        Action::Merge => {
            progress(Progress { done: 0, total: 1, label: format!("{} PDFs", files.len()) });
            let paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
            let target = naming::sibling(&paths[0], "merged", "pdf");
            match doc::merge(&paths, &target) {
                Ok(pages) => {
                    report.notes.push(format!("{} files, {pages} pages", paths.len()));
                    report.outputs.push(target);
                }
                Err(e) => report.failures.push(Failure::new("", &e)),
            }
            return report;
        }
        Action::Convert if plan.input == "image" && files.len() > 1 && opts.combine.unwrap_or(true) => {
            let mut imgs = Vec::new();
            for (i, f) in files.iter().enumerate() {
                progress(Progress { done: i as u32, total, label: f.name.clone() });
                match images::load(&f.path) {
                    Ok(img) => imgs.push(img),
                    Err(e) => report.failures.push(Failure::new(&f.name, &e)),
                }
            }
            if !imgs.is_empty() {
                let target = naming::sibling(&files[0].path, "combined", "pdf");
                match doc::images_to_pdf(&imgs, &target) {
                    Ok(()) => report.outputs.push(target),
                    Err(e) => report.failures.push(Failure::new("", &e)),
                }
            }
            return report;
        }
        _ => {}
    }

    for (i, f) in files.iter().enumerate() {
        progress(Progress { done: i as u32, total, label: f.name.clone() });
        let mut sub = |done: u32, of: u32| {
            progress(Progress { done: i as u32, total, label: format!("{} · page {} of {of}", f.name, done + 1) })
        };
        match run_one(plan.action, f, opts, &mut report, &mut sub) {
            Ok(mut outs) => report.outputs.append(&mut outs),
            Err(e) => report.failures.push(Failure::new(&f.name, &e)),
        }
    }
    progress(Progress { done: total, total, label: String::new() });
    report
}

fn run_one(
    action: Action,
    f: &FileInfo,
    opts: &Options,
    report: &mut Report,
    sub: &mut dyn FnMut(u32, u32),
) -> Result<Vec<PathBuf>> {
    let src = f.path.as_path();
    let pages = || -> Result<u32> {
        if f.needs_password {
            return Err(LpError::Encrypted(src.to_path_buf()));
        }
        f.pages.ok_or_else(|| doc::open(src, None).err().unwrap_or(LpError::Damaged {
            path: src.to_path_buf(),
            detail: "page count unavailable".into(),
        }))
    };
    match action {
        Action::Compress => {
            let target = naming::sibling(src, "compressed", "pdf");
            let r = doc::compress(src, opts.level.unwrap_or(CompressLevel::Balanced), &target)?;
            if r.written {
                report.notes.push(format!("{}: {} → {}", f.name, human_size(r.before), human_size(r.after)));
                Ok(vec![target])
            } else {
                report.notes.push(format!("{} is already compact, so no smaller copy was written.", f.name));
                Ok(vec![])
            }
        }
        Action::Rotate => {
            let total = pages()?;
            let spec = opts.ranges.as_deref().unwrap_or("").trim();
            let list = if spec.is_empty() { (1..=total).collect() } else { ranges::flatten(&ranges::parse(spec, total)?) };
            let angle = opts.angle.unwrap_or(90);
            let target = naming::sibling(src, "rotated", "pdf");
            doc::rotate(src, &list, angle, &target)?;
            Ok(vec![target])
        }
        Action::Split => {
            let total = pages()?;
            let mode = opts.split_mode.unwrap_or(SplitMode::Extract);
            let stem = naming::stem(src);
            let dir = naming::parent(src);
            let doc = doc::open(src, None)?;
            match mode {
                SplitMode::Each => {
                    let out = naming::unique_dir(&dir, &format!("{stem} (pages)"))?;
                    for p in 1..=total {
                        sub(p - 1, total);
                        let target = out.join(format!("{stem}-{}.pdf", naming::page_label(p, total)));
                        doc::extract_from(&doc, &[p], &target)?;
                    }
                    Ok(vec![out])
                }
                SplitMode::Extract | SplitMode::Ranges => {
                    let spec = opts.ranges.as_deref().unwrap_or("");
                    let groups = ranges::parse(spec, total)?;
                    if mode == SplitMode::Extract {
                        let label: String = groups.iter().map(|g| g.label.as_str()).collect::<Vec<_>>().join(",");
                        let base = if label.len() <= 32 { format!("{stem}-p{label}") } else { format!("{stem}-extract") };
                        let target = naming::unique_file(&dir, &base, "pdf");
                        doc::extract_from(&doc, &ranges::flatten(&groups), &target)?;
                        Ok(vec![target])
                    } else {
                        let mut outs = Vec::new();
                        for g in &groups {
                            let target = naming::unique_file(&dir, &format!("{stem}-p{}", g.label), "pdf");
                            doc::extract_from(&doc, &g.pages, &target)?;
                            outs.push(target);
                        }
                        Ok(outs)
                    }
                }
            }
        }
        Action::Unlock => {
            if !f.encrypted {
                return Err(LpError::NotEncrypted(src.to_path_buf()));
            }
            let target = naming::sibling(src, "unlocked", "pdf");
            doc::unlock(src, opts.password.as_deref().unwrap_or(""), &target)?;
            Ok(vec![target])
        }
        Action::Protect => {
            if f.needs_password {
                return Err(LpError::AlreadyProtected(src.to_path_buf()));
            }
            let target = naming::sibling(src, "protected", "pdf");
            doc::protect(src, opts.password.as_deref().unwrap_or(""), &target)?;
            Ok(vec![target])
        }
        Action::Convert => match f.kind {
            FileKind::Pdf => {
                if f.needs_password {
                    return Err(LpError::Encrypted(src.to_path_buf()));
                }
                let dpi = opts.dpi.unwrap_or(150).clamp(36, 600);
                match opts.target {
                    Some(ConvertTarget::Png) => render::to_images(src, ImageFormat::Png, dpi, sub),
                    Some(ConvertTarget::Jpeg) => render::to_images(src, ImageFormat::Jpeg, dpi, sub),
                    Some(ConvertTarget::Docx) => Ok(vec![render::to_docx(src, sub)?]),
                    Some(ConvertTarget::Txt) => Ok(vec![render::to_text(src)?]),
                    Some(ConvertTarget::Pdf) | None => Err(LpError::Invalid("Choose what to convert the PDF to.".into())),
                }
            }
            FileKind::Image => {
                let img = images::load(src)?;
                let target = naming::sibling(src, "", "pdf");
                doc::images_to_pdf(&[img], &target)?;
                Ok(vec![target])
            }
            FileKind::Office => Ok(vec![office::to_pdf(src)?]),
            FileKind::Other => Err(LpError::Unsupported { path: src.to_path_buf(), reason: "unknown file type.".into() }),
        },
        Action::Merge => unreachable!("handled above"),
    }
}

pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", b / 1024.0 / 1024.0 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering;

    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("scan 2.pdf", "scan 10.pdf"), Ordering::Less);
        assert_eq!(natural_cmp("B.pdf", "a.pdf"), Ordering::Greater);
        assert_eq!(natural_cmp("p007", "p7"), Ordering::Equal);
    }
}
