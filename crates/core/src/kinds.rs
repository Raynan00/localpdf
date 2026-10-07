use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Pdf,
    Image,
    Office,
    Other,
}

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "gif", "webp"];
pub const OFFICE_EXTS: &[&str] = &[
    "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp",
];

pub fn kind_of(path: &Path) -> FileKind {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "pdf" {
        FileKind::Pdf
    } else if IMAGE_EXTS.contains(&ext.as_str()) {
        FileKind::Image
    } else if OFFICE_EXTS.contains(&ext.as_str()) {
        FileKind::Office
    } else {
        FileKind::Other
    }
}
