//! Output naming. Results go next to the source file and never replace
//! anything that already exists, the source included.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{LpError, Result};

pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".into())
}

pub fn parent(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// `dir/base.ext`, or `dir/base (2).ext`, `dir/base (3).ext`… if taken.
pub fn unique_file(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let make = |n: u32| {
        let name = if n == 1 { format!("{base}.{ext}") } else { format!("{base} ({n}).{ext}") };
        dir.join(name)
    };
    (1..).map(make).find(|p| !p.exists()).expect("unbounded")
}

/// Same as [`unique_file`] for a directory. The directory is created.
pub fn unique_dir(dir: &Path, base: &str) -> Result<PathBuf> {
    for n in 1u32.. {
        let p = if n == 1 { dir.join(base) } else { dir.join(format!("{base} ({n})")) };
        match fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(LpError::Write { path: p, source }),
        }
    }
    unreachable!()
}

/// Output next to `source`: `report.pdf` + `compressed` -> `report-compressed.pdf`.
pub fn sibling(source: &Path, suffix: &str, ext: &str) -> PathBuf {
    let base = if suffix.is_empty() { stem(source) } else { format!("{}-{suffix}", stem(source)) };
    unique_file(&parent(source), &base, ext)
}

/// Write via a temporary file in the target directory and move it into place,
/// so a crash never leaves a half-written result under the final name.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    let dir = parent(target);
    let err = |source| LpError::Write { path: target.to_path_buf(), source };
    let mut tmp = tempfile::Builder::new()
        .prefix(".localpdf-")
        .suffix(".part")
        .tempfile_in(&dir)
        .map_err(err)?;
    std::io::Write::write_all(&mut tmp, bytes).map_err(err)?;
    tmp.persist_noclobber(target).map_err(|e| err(e.error))?;
    Ok(())
}

/// Pick a working path for an engine that insists on writing a file itself.
pub fn temp_sibling(target: &Path) -> Result<tempfile::TempPath> {
    let dir = parent(target);
    tempfile::Builder::new()
        .prefix(".localpdf-")
        .suffix(".part")
        .tempfile_in(&dir)
        .map(|f| f.into_temp_path())
        .map_err(|source| LpError::Write { path: target.to_path_buf(), source })
}

pub fn persist(tmp: tempfile::TempPath, target: &Path) -> Result<()> {
    // Rename never crosses volumes here: the temp file lives in the target dir.
    tmp.persist_noclobber(target)
        .map_err(|e| LpError::Write { path: target.to_path_buf(), source: e.error })
}

/// Zero-padded page label so files sort correctly: p01 … p12.
pub fn page_label(page: u32, total: u32) -> String {
    let width = total.to_string().len().max(2);
    format!("p{page:0width$}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_names_never_collide() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("report.pdf");
        fs::write(&src, b"x").unwrap();
        let a = sibling(&src, "compressed", "pdf");
        assert_eq!(a.file_name().unwrap(), "report-compressed.pdf");
        write_atomic(&a, b"1").unwrap();
        let b = sibling(&src, "compressed", "pdf");
        assert_eq!(b.file_name().unwrap(), "report-compressed (2).pdf");
        assert!(write_atomic(&a, b"2").is_err(), "must not clobber");
        assert_eq!(fs::read(&a).unwrap(), b"1");
    }

    #[test]
    fn labels_pad() {
        assert_eq!(page_label(3, 9), "p03");
        assert_eq!(page_label(7, 120), "p007");
    }
}
