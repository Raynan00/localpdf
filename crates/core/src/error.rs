use std::path::{Path, PathBuf};

use crate::ranges::RangeError;

/// Every error carries a message written for the person who right-clicked the
/// file, not for a developer. `Display` is what the dialog shows.
#[derive(Debug, thiserror::Error)]
pub enum LpError {
    #[error("{name} is password-protected. Use Unlock first, then try again.", name = file_name(.0))]
    Encrypted(PathBuf),

    #[error("That password doesn't open {name}.", name = file_name(.0))]
    WrongPassword(PathBuf),

    #[error("{name} isn't password-protected, so there's nothing to unlock.", name = file_name(.0))]
    NotEncrypted(PathBuf),

    #[error("{name} already has a password. Unlock it first if you want to change it.", name = file_name(.0))]
    AlreadyProtected(PathBuf),

    #[error("{name} couldn't be read. It may be damaged or not really a PDF.", name = file_name(.path))]
    Damaged { path: PathBuf, detail: String },

    #[error("Page range: {0}")]
    InvalidRange(#[from] RangeError),

    #[error("Merge failed on {name}: {reason}", name = file_name(.path))]
    MergeFailed { path: PathBuf, reason: String },

    #[error("{name} can't be converted: {reason}", name = file_name(.path))]
    Unsupported { path: PathBuf, reason: String },

    #[error("{0}")]
    EngineMissing(String),

    #[error("{0}")]
    Invalid(String),

    #[error("Couldn't write next to {name}: {source}", name = file_name(.path))]
    Write { path: PathBuf, source: std::io::Error },

    #[error("Couldn't open {name}: {source}", name = file_name(.path))]
    Read { path: PathBuf, source: std::io::Error },

    #[error("{0}")]
    Engine(String),
}

pub type Result<T> = std::result::Result<T, LpError>;

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
