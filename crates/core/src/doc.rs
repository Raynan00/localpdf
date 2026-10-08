//! Structural PDF operations on top of qpdf, compiled into the binary
//! (qpdf-sys "vendored"): merge, split, rotate, encrypt, decrypt, compress.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use qpdf::{
    EncryptionParams, EncryptionParamsR6, ObjectStreamMode, PrintPermission, QPdf, QPdfArray,
    QPdfDictionary, QPdfError, QPdfErrorCode, QPdfObject, QPdfObjectLike, QPdfObjectType, QPdfScalar,
    QPdfStream, QPdfWriter, StreamDataMode, StreamDecodeLevel,
};

use crate::error::{LpError, Result};
use crate::naming;

/// Open a PDF read-only. `password` is only needed for files that require one.
pub fn open(path: &Path, password: Option<&str>) -> Result<QPdf> {
    if let Err(source) = std::fs::metadata(path) {
        return Err(LpError::Read { path: path.to_path_buf(), source });
    }
    let res = match password {
        Some(pw) => QPdf::read_encrypted(path, pw),
        None => QPdf::read(path),
    };
    let doc = res.map_err(|e| map_open_error(path, password.is_some(), e))?;
    // Force the page tree to load so damage shows up here, not mid-operation.
    doc.get_num_pages().map_err(|e| map_open_error(path, password.is_some(), e))?;
    Ok(doc)
}

fn map_open_error(path: &Path, had_password: bool, e: QPdfError) -> LpError {
    match e.error_code() {
        QPdfErrorCode::InvalidPassword if had_password => LpError::WrongPassword(path.to_path_buf()),
        QPdfErrorCode::InvalidPassword => LpError::Encrypted(path.to_path_buf()),
        _ => LpError::Damaged { path: path.to_path_buf(), detail: e.to_string() },
    }
}

fn engine(e: QPdfError) -> LpError {
    LpError::Engine(format!("PDF engine error: {e}"))
}

/// Write `doc` to `target` (which must not exist yet) via a temp file.
fn save(doc: &QPdf, target: &Path, configure: impl FnOnce(&mut QPdfWriter)) -> Result<()> {
    let tmp = naming::temp_sibling(target)?;
    {
        // The writer must be gone (and its file closed) before the rename.
        let mut w = doc.writer();
        w.object_stream_mode(ObjectStreamMode::Preserve);
        configure(&mut w);
        w.write(&*tmp).map_err(engine)?;
    }
    naming::persist(tmp, target)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PdfInfo {
    pub pages: u32,
    /// Opening needs a password.
    pub needs_password: bool,
    /// Encrypted at all (including owner-only permission restrictions).
    pub encrypted: bool,
}

pub fn inspect(path: &Path) -> Result<PdfInfo> {
    match open(path, None) {
        Ok(doc) => Ok(PdfInfo {
            pages: doc.get_num_pages().map_err(engine)?,
            needs_password: false,
            encrypted: doc.is_encrypted(),
        }),
        Err(LpError::Encrypted(_)) => Ok(PdfInfo { pages: 0, needs_password: true, encrypted: true }),
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------- merge

pub fn merge(inputs: &[PathBuf], target: &Path) -> Result<u32> {
    if inputs.len() < 2 {
        return Err(LpError::Invalid("Select at least two PDFs to merge.".into()));
    }
    let out = QPdf::empty();
    let mut total = 0;
    for path in inputs {
        let src = open(path, None).map_err(|e| match e {
            LpError::Encrypted(p) => LpError::MergeFailed {
                path: p,
                reason: "it's password-protected. Unlock it first.".into(),
            },
            LpError::Damaged { path, .. } => LpError::MergeFailed {
                path,
                reason: "it's damaged or not a PDF.".into(),
            },
            other => other,
        })?;
        let pages = src.get_pages().map_err(engine)?;
        for page in pages {
            out.add_page(&page, false).map_err(|e| LpError::MergeFailed {
                path: path.clone(),
                reason: format!("a page couldn't be copied ({e})."),
            })?;
            total += 1;
        }
    }
    save(&out, target, |w| {
        w.object_stream_mode(ObjectStreamMode::Generate);
    })?;
    Ok(total)
}

// ---------------------------------------------------------------- split / extract

/// Copy the given 1-based pages of `source` into a new PDF at `target`.
pub fn extract(source: &Path, pages: &[u32], target: &Path) -> Result<()> {
    let src = open(source, None)?;
    extract_from(&src, pages, target)
}

pub fn extract_from(src: &QPdf, pages: &[u32], target: &Path) -> Result<()> {
    let out = QPdf::empty();
    for &p in pages {
        let page = src
            .get_page(p - 1)
            .ok_or_else(|| LpError::Engine(format!("page {p} couldn't be read")))?;
        out.add_page(&page, false).map_err(engine)?;
    }
    save(&out, target, |w| {
        w.object_stream_mode(ObjectStreamMode::Generate);
    })
}

// ---------------------------------------------------------------- rotate

/// Rotate the given 1-based pages clockwise by `degrees` (multiple of 90).
pub fn rotate(source: &Path, pages: &[u32], degrees: i32, target: &Path) -> Result<()> {
    if degrees % 90 != 0 {
        return Err(LpError::Invalid("Rotation must be a multiple of 90°.".into()));
    }
    let doc = open(source, None)?;
    for &p in pages {
        let page = doc
            .get_page(p - 1)
            .ok_or_else(|| LpError::Engine(format!("page {p} couldn't be read")))?;
        let current = inherited(&page, "/Rotate").map(|o| QPdfScalar::from(o).as_i64()).unwrap_or(0) as i32;
        let next = (current + degrees).rem_euclid(360);
        page.set("/Rotate", doc.new_integer(next as i64));
    }
    save(&doc, target, |_| {})
}

/// Look up a page attribute, following /Parent for inheritable keys.
fn inherited(page: &QPdfDictionary, key: &str) -> Option<QPdfObject> {
    let mut node = QPdfDictionary::from(AsRef::<QPdfObject>::as_ref(page).clone());
    for _ in 0..64 {
        if let Some(v) = node.get(key) {
            if v.get_type() != QPdfObjectType::Null {
                return Some(v);
            }
        }
        let parent = node.get("/Parent")?;
        if parent.get_type() != QPdfObjectType::Dictionary {
            return None;
        }
        node = QPdfDictionary::from(parent);
    }
    None
}

// ---------------------------------------------------------------- protect / unlock

pub fn protect(source: &Path, password: &str, target: &Path) -> Result<()> {
    if password.is_empty() {
        return Err(LpError::Invalid("Enter a password.".into()));
    }
    let doc = open(source, None).map_err(|e| match e {
        LpError::Encrypted(p) => LpError::AlreadyProtected(p),
        other => other,
    })?;
    save(&doc, target, |w| {
        w.preserve_encryption(false);
        w.encryption_params(EncryptionParams::R6(EncryptionParamsR6 {
            user_password: password.to_string(),
            owner_password: password.to_string(),
            allow_accessibility: true,
            allow_extract: true,
            allow_assemble: true,
            allow_annotate_and_form: true,
            allow_form_filling: true,
            allow_modify_other: true,
            allow_print: PrintPermission::Full,
            encrypt_metadata: true,
        }));
    })
}

pub fn unlock(source: &Path, password: &str, target: &Path) -> Result<()> {
    if password.is_empty() {
        return Err(LpError::Invalid("Enter the file's password.".into()));
    }
    let doc = match open(source, Some(password)) {
        Ok(doc) => doc,
        // Unencrypted files reject any non-empty password the same way
        // encrypted ones do; tell the two apart.
        Err(LpError::WrongPassword(p)) if !open(source, None).map(|d| d.is_encrypted()).unwrap_or(true) => {
            return Err(LpError::NotEncrypted(p));
        }
        Err(e) => return Err(e),
    };
    if !doc.is_encrypted() {
        return Err(LpError::NotEncrypted(source.to_path_buf()));
    }
    save(&doc, target, |w| {
        w.preserve_encryption(false);
    })
}

// ---------------------------------------------------------------- compress

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressLevel {
    /// Lossless only: recompress streams, pack objects.
    Light,
    /// Re-encode photos as JPEG q72, cap at 2000 px on the long side.
    Balanced,
    /// JPEG q55, cap at 1400 px.
    Strong,
}

impl CompressLevel {
    fn image_params(self) -> Option<(u32, u8)> {
        match self {
            CompressLevel::Light => None,
            CompressLevel::Balanced => Some((2000, 72)),
            CompressLevel::Strong => Some((1400, 55)),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CompressResult {
    pub before: u64,
    pub after: u64,
    /// False when the result wasn't meaningfully smaller and nothing was written.
    pub written: bool,
    pub images_recompressed: u32,
}

pub fn compress(source: &Path, level: CompressLevel, target: &Path) -> Result<CompressResult> {
    let before = std::fs::metadata(source)
        .map_err(|e| LpError::Read { path: source.to_path_buf(), source: e })?
        .len();
    let doc = open(source, None)?;
    let mut images = 0;
    if let Some((max_side, quality)) = level.image_params() {
        images = recompress_images(&doc, max_side, quality);
    }
    let tmp = naming::temp_sibling(target)?;
    {
        let mut w = doc.writer();
        w.object_stream_mode(ObjectStreamMode::Generate)
            .stream_data_mode(StreamDataMode::Compress)
            .stream_decode_level(StreamDecodeLevel::Generalized)
            .compress_streams(true);
        w.write(&*tmp).map_err(engine)?;
    }
    let after = std::fs::metadata(&*tmp).map(|m| m.len()).unwrap_or(u64::MAX);
    // Under 3% saved isn't worth a second copy of the file.
    if after >= before.saturating_sub(before / 33) {
        return Ok(CompressResult { before, after: before, written: false, images_recompressed: images });
    }
    naming::persist(tmp, target)?;
    Ok(CompressResult { before, after, written: true, images_recompressed: images })
}

/// Re-encode raster images as baseline JPEG. Returns how many were replaced.
fn recompress_images(doc: &QPdf, max_side: u32, quality: u8) -> u32 {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    let Ok(pages) = doc.get_pages() else { return 0 };
    for page in &pages {
        if let Some(res) = inherited(page, "/Resources") {
            collect_images(&res, &mut seen, &mut found, 0);
        }
    }
    // Soft masks are alpha channels; JPEG artifacts there show as halos.
    let masks: HashSet<u32> = found
        .iter()
        .filter_map(|s| s.get_dictionary().get("/SMask"))
        .filter(|m| m.is_indirect())
        .map(|m| m.get_id())
        .collect();
    let mut count = 0;
    for img in found {
        if masks.contains(&img.get_id()) {
            continue;
        }
        if recompress_one(doc, &img, max_side, quality).is_some() {
            count += 1;
        }
    }
    count
}

fn collect_images(res: &QPdfObject, seen: &mut HashSet<u32>, out: &mut Vec<QPdfStream>, depth: u32) {
    if depth > 12 || res.get_type() != QPdfObjectType::Dictionary {
        return;
    }
    let Some(xobjs) = QPdfDictionary::from(res.clone()).get("/XObject") else { return };
    if xobjs.get_type() != QPdfObjectType::Dictionary {
        return;
    }
    let xobjs = QPdfDictionary::from(xobjs);
    for key in xobjs.keys() {
        let Some(obj) = xobjs.get(&key) else { continue };
        if obj.get_type() != QPdfObjectType::Stream || !obj.is_indirect() || !seen.insert(obj.get_id()) {
            continue;
        }
        let stream = QPdfStream::from(obj);
        let dict = stream.get_dictionary();
        match dict.get("/Subtype").map(|s| s.as_name()).as_deref() {
            Some("/Image") => out.push(stream),
            Some("/Form") => {
                if let Some(inner) = dict.get("/Resources") {
                    collect_images(&inner, seen, out, depth + 1);
                }
            }
            _ => {}
        }
    }
}

fn recompress_one(doc: &QPdf, img: &QPdfStream, max_side: u32, quality: u8) -> Option<()> {
    let dict = img.get_dictionary();
    let int = |k: &str| dict.get(k).filter(|o| o.get_type() == QPdfObjectType::Integer).map(|o| QPdfScalar::from(o).as_i64());
    let (w, h) = (int("/Width")? as u32, int("/Height")? as u32);
    if int("/BitsPerComponent")? != 8 || w < 64 || h < 64 {
        return None;
    }
    // Anything that changes how samples map to colour stays untouched.
    let is_true = |k: &str| dict.get(k).map(|o| o.get_type() == QPdfObjectType::Boolean && o.as_bool()).unwrap_or(false);
    if is_true("/ImageMask") || dict.has("/Decode") || dict.has("/Mask") || dict.has("/SMaskInData") {
        return None;
    }
    let channels = color_channels(&dict.get("/ColorSpace")?)?;
    let filter = dict.get("/Filter");
    let filter_name = match filter.as_ref().map(|f| f.get_type()) {
        None | Some(QPdfObjectType::Null) => String::new(),
        Some(QPdfObjectType::Name) => filter.unwrap().as_name(),
        Some(QPdfObjectType::Array) => {
            let arr = QPdfArray::from(filter.unwrap());
            if arr.len() != 1 {
                return None;
            }
            arr.get(0)?.as_name()
        }
        _ => return None,
    };
    if !matches!(filter_name.as_str(), "" | "/DCTDecode" | "/FlateDecode") {
        return None;
    }
    let encoded_len = img.get_data(StreamDecodeLevel::None).ok()?.len();
    let raw = img.get_data(StreamDecodeLevel::All).ok()?;
    let raw: &[u8] = raw.as_ref();
    if raw.len() != (w as usize) * (h as usize) * channels {
        return None;
    }
    let mut pixels = if channels == 3 {
        image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, raw.to_vec())?)
    } else {
        image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, raw.to_vec())?)
    };
    let scaled = w.max(h) > max_side;
    if scaled {
        pixels = pixels.resize(max_side, max_side, image::imageops::FilterType::Triangle);
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
        .encode_image(&pixels)
        .ok()?;
    // Keep the original unless the new encoding is clearly smaller.
    if jpeg.len() as f64 > encoded_len as f64 * 0.9 {
        return None;
    }
    img.replace_data(&jpeg, doc.new_name("/DCTDecode"), doc.new_null());
    dict.remove("/DecodeParms");
    if scaled {
        dict.set("/Width", doc.new_integer(pixels.width() as i64));
        dict.set("/Height", doc.new_integer(pixels.height() as i64));
    }
    Some(())
}

fn color_channels(cs: &QPdfObject) -> Option<usize> {
    match cs.get_type() {
        QPdfObjectType::Name => match cs.as_name().as_str() {
            "/DeviceRGB" | "/CalRGB" => Some(3),
            "/DeviceGray" | "/CalGray" => Some(1),
            _ => None,
        },
        QPdfObjectType::Array => {
            let arr = QPdfArray::from(cs.clone());
            let family = arr.get(0)?.as_name();
            match family.as_str() {
                "/ICCBased" => {
                    let profile = arr.get(1)?;
                    if profile.get_type() != QPdfObjectType::Stream {
                        return None;
                    }
                    let n = QPdfScalar::from(QPdfStream::from(profile).get_dictionary().get("/N")?).as_i64();
                    match n {
                        3 => Some(3),
                        1 => Some(1),
                        _ => None,
                    }
                }
                "/CalRGB" => Some(3),
                "/CalGray" => Some(1),
                _ => None,
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- images -> PDF

/// One raster image, prepared for embedding.
pub struct EmbeddedImage {
    pub width: u32,
    pub height: u32,
    /// 1 (gray) or 3 (RGB).
    pub channels: u8,
    /// Either a JPEG file's bytes (DCT) or raw 8-bit samples (compressed on write).
    pub data: Vec<u8>,
    pub is_jpeg: bool,
    /// Clockwise page rotation to honour EXIF orientation.
    pub rotate: i32,
}

/// Build a PDF with one image per page, each page fitted to A4's long side.
pub fn images_to_pdf(images: &[EmbeddedImage], target: &Path) -> Result<()> {
    const LONG_SIDE: f64 = 842.0;
    let doc = QPdf::empty();
    for img in images {
        let scale = LONG_SIDE / img.width.max(img.height) as f64;
        let (pw, ph) = (img.width as f64 * scale, img.height as f64 * scale);
        let cs = if img.channels == 3 { "/DeviceRGB" } else { "/DeviceGray" };
        let mut entries: Vec<(&str, QPdfObject)> = vec![
            ("/Type", doc.new_name("/XObject")),
            ("/Subtype", doc.new_name("/Image")),
            ("/Width", doc.new_integer(img.width as i64).into()),
            ("/Height", doc.new_integer(img.height as i64).into()),
            ("/ColorSpace", doc.new_name(cs)),
            ("/BitsPerComponent", doc.new_integer(8).into()),
        ];
        if img.is_jpeg {
            entries.push(("/Filter", doc.new_name("/DCTDecode")));
        }
        let xobj: QPdfObject = doc.new_stream_with_dictionary(entries, &img.data).into();
        let xobj = xobj.into_indirect();
        let content = format!("q {pw:.3} 0 0 {ph:.3} 0 0 cm /Im0 Do Q");
        let content: QPdfObject = doc.new_stream(content.as_bytes()).into();
        let resources = doc.new_dictionary_from([("/XObject", doc.new_dictionary_from([("/Im0", xobj)]))]);
        let media = doc.new_array_from([
            doc.new_integer(0).into(),
            doc.new_integer(0).into(),
            doc.new_real(pw, 3).into(),
            doc.new_real(ph, 3).into(),
        ]);
        let mut page_entries: Vec<(&str, QPdfObject)> = vec![
            ("/Type", doc.new_name("/Page")),
            ("/MediaBox", media.into()),
            ("/Resources", resources.into()),
            ("/Contents", content.into_indirect()),
        ];
        if img.rotate != 0 {
            page_entries.push(("/Rotate", doc.new_integer(img.rotate as i64).into()));
        }
        let page: QPdfObject = doc.new_dictionary_from(page_entries).into();
        doc.add_page(page.into_indirect(), false).map_err(engine)?;
    }
    save(&doc, target, |w| {
        w.object_stream_mode(ObjectStreamMode::Generate)
            .stream_data_mode(StreamDataMode::Compress)
            .compress_streams(true);
    })
}
