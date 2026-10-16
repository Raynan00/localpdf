//! Page rendering and text extraction through PDFium, loaded from the copy
//! bundled next to the app.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pdfium_render::prelude::*;

use crate::error::{LpError, Result};
use crate::{docx, naming, pdf2word};

static PDFIUM: Mutex<Option<&'static Pdfium>> = Mutex::new(None);
static SEARCH_DIRS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Directories to look in for the PDFium library, before the defaults
/// (next to the executable, ../Resources on macOS, ./resources).
pub fn add_search_dir(dir: PathBuf) {
    SEARCH_DIRS.lock().unwrap().push(dir);
}

/// Where bundled engines may live, most specific first.
pub fn engine_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("LOCALPDF_PDFIUM_DIR") {
        dirs.push(d.into());
    }
    dirs.extend(SEARCH_DIRS.lock().unwrap().iter().cloned());
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join("engines"));
        dirs.push(exe_dir.join("resources").join("engines"));
        dirs.push(exe_dir.join("../Resources/engines"));
        dirs.push(exe_dir.clone());
    }
    dirs
}

pub fn pdfium() -> Result<&'static Pdfium> {
    let mut slot = PDFIUM.lock().unwrap();
    if let Some(p) = *slot {
        return Ok(p);
    }
    let mut last_err = String::from("not found");
    for dir in engine_dirs() {
        let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if !lib.exists() {
            continue;
        }
        match Pdfium::bind_to_library(&lib) {
            Ok(bindings) => {
                let p: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
                *slot = Some(p);
                return Ok(p);
            }
            Err(e) => last_err = format!("{}: {e}", lib.display()),
        }
    }
    Err(LpError::EngineMissing(format!(
        "The page renderer (PDFium) is missing from this install ({last_err}). Reinstall LocalPDF."
    )))
}

pub(crate) fn load<'a>(pdfium: &'a Pdfium, path: &Path) -> Result<PdfDocument<'a>> {
    pdfium.load_pdf_from_file(path, None).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            LpError::Encrypted(path.to_path_buf())
        }
        PdfiumError::IoError(source) => LpError::Read { path: path.to_path_buf(), source },
        other => LpError::Damaged { path: path.to_path_buf(), detail: other.to_string() },
    })
}

pub(crate) fn engine(e: PdfiumError) -> LpError {
    LpError::Engine(format!("Page renderer error: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

const MAX_SIDE_PX: i32 = 12_000;

pub(crate) fn render_page(page: &PdfPage, dpi: u32) -> Result<image::DynamicImage> {
    let px = |pts: f32| ((pts * dpi as f32 / 72.0).round() as i32).clamp(1, MAX_SIDE_PX);
    let cfg = PdfRenderConfig::new()
        .set_target_width(px(page.width().value))
        .set_maximum_height(px(page.height().value))
        .render_form_data(true);
    page.render_with_config(&cfg).map_err(engine)?.as_image().map_err(engine)
}

pub(crate) fn encode(img: &image::DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let res = match format {
        ImageFormat::Png => img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png),
        ImageFormat::Jpeg => image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
            .encode_image(&image::DynamicImage::ImageRgb8(img.to_rgb8())),
    };
    res.map_err(|e| LpError::Engine(format!("Image encoding failed: {e}")))?;
    Ok(out)
}

/// Render every page. One page -> `name.png` next to the source; more pages
/// -> a `name (images)` folder with `name-p01.png`, `name-p02.png`, …
pub fn to_images(
    source: &Path,
    format: ImageFormat,
    dpi: u32,
    progress: &mut dyn FnMut(u32, u32),
) -> Result<Vec<PathBuf>> {
    let pdfium = pdfium()?;
    let doc = load(pdfium, source)?;
    let total = doc.pages().len() as u32;
    let ext = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
    };
    let stem = naming::stem(source);
    if total == 1 {
        let page = doc.pages().get(0).map_err(engine)?;
        let target = naming::sibling(source, "", ext);
        naming::write_atomic(&target, &encode(&render_page(&page, dpi)?, format)?)?;
        return Ok(vec![target]);
    }
    let dir = naming::unique_dir(&naming::parent(source), &format!("{stem} (images)"))?;
    for (i, page) in doc.pages().iter().enumerate() {
        let n = i as u32 + 1;
        progress(n - 1, total);
        let target = dir.join(format!("{stem}-{}.{ext}", naming::page_label(n, total)));
        naming::write_atomic(&target, &encode(&render_page(&page, dpi)?, format)?)?;
    }
    Ok(vec![dir])
}

fn page_texts(doc: &PdfDocument) -> Result<Vec<String>> {
    doc.pages()
        .iter()
        .map(|p| Ok(p.text().map_err(engine)?.all().replace("\r\n", "\n").replace('\r', "\n")))
        .collect()
}

pub fn to_text(source: &Path) -> Result<PathBuf> {
    let pdfium = pdfium()?;
    let doc = load(pdfium, source)?;
    let texts = page_texts(&doc)?;
    if texts.iter().all(|t| t.trim().is_empty()) {
        return Err(LpError::Unsupported {
            path: source.to_path_buf(),
            reason: "it has no text layer (it's probably a scan). OCR isn't part of LocalPDF.".into(),
        });
    }
    let mut out = texts.join("\n\n");
    out.push('\n');
    let target = naming::sibling(source, "", "txt");
    naming::write_atomic(&target, out.as_bytes())?;
    Ok(target)
}

/// Editable Word document that keeps the PDF's formatting: fonts, sizes,
/// bold/italic/colour, alignment, headings, bullet lists, tables, pictures
/// and two-column pages (see `pdf2word`). Pages without a text layer
/// (scans) become a full-page picture.
pub fn to_docx(source: &Path, progress: &mut dyn FnMut(u32, u32)) -> Result<PathBuf> {
    let pdfium = pdfium()?;
    let doc = load(pdfium, source)?;
    let total = doc.pages().len() as u32;
    let mut contents = Vec::new();
    for (i, page) in doc.pages().iter().enumerate() {
        progress(i as u32, total);
        contents.push(pdf2word::extract(&doc, &page));
    }
    let mut furniture = pdf2word::find_furniture(&contents);
    let margins = pdf2word::page_margins(&contents, &furniture);
    furniture.place(&contents, margins);
    let mut layouts: Vec<_> = contents.iter().map(|c| pdf2word::layout(c, margins, &furniture)).collect();
    let typo = pdf2word::typography(&layouts);
    layouts.iter_mut().for_each(|l| pdf2word::apply_headings(l, &typo));
    pdf2word::join_across_pages(&mut layouts);

    let mut builder = docx::Builder::default();
    builder.header_footer(std::mem::take(&mut furniture.header), std::mem::take(&mut furniture.footer));
    for (i, (content, layout)) in contents.into_iter().zip(&layouts).enumerate() {
        if i > 0 {
            builder.end_page();
        }
        let scanned = layout.text_chars < 10 && !layout.blocks.iter().any(|b| matches!(b, pdf2word::Block::Table(_)));
        if scanned {
            let page = doc.pages().get(i as i32).map_err(engine)?;
            let jpeg = encode(&render_page(&page, 150)?, ImageFormat::Jpeg)?;
            let m = 18.0;
            builder.section(docx::Section {
                page_w: content.width,
                page_h: content.height,
                margin_left: m,
                margin_right: m,
                margin_top: m,
                margin_bottom: m,
                ..Default::default()
            });
            let para = docx::Para { align: docx::Align::Center, ..Default::default() };
            builder.picture(jpeg, docx::Picture::Jpeg, content.width, content.height, &para);
            continue;
        }
        let mut images: Vec<_> = content.images.into_iter().map(|im| Some((im.data, im.kind))).collect();
        pdf2word::write_page(&mut builder, layout, &mut images);
    }
    let target = naming::sibling(source, "", "docx");
    naming::write_atomic(&target, &builder.finish()?)?;
    Ok(target)
}

