//! End-to-end checks of every action on generated files. PDFium-backed
//! conversions run only when the library is available (LOCALPDF_PDFIUM_DIR
//! or src-tauri/engines), so `cargo test` works on a fresh checkout.

use std::fs;
use std::path::{Path, PathBuf};

use localpdf_core::doc::{self, EmbeddedImage};
use localpdf_core::{plan, run, Action, ConvertTarget, LpError, Options, SplitMode};

fn photo(w: u32, h: u32) -> EmbeddedImage {
    let mut data = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, ((x ^ y) & 0xff) as u8]);
        }
    }
    EmbeddedImage { width: w, height: h, channels: 3, data, is_jpeg: false, rotate: 0 }
}

/// A PDF with `pages` photo pages, written into `dir`.
fn make_pdf(dir: &Path, name: &str, pages: usize) -> PathBuf {
    let path = dir.join(name);
    let imgs: Vec<_> = (0..pages).map(|_| photo(1600, 1000)).collect();
    doc::images_to_pdf(&imgs, &path).unwrap();
    path
}

fn page_count(path: &Path) -> u32 {
    doc::inspect(path).unwrap().pages
}

fn go(action: Action, files: &[PathBuf], opts: Options) -> localpdf_core::Report {
    let p = plan(action, files).unwrap();
    run(&p, &opts, &mut |_| {})
}

#[test]
fn merge_split_rotate() {
    let dir = tempfile::tempdir().unwrap();
    let a = make_pdf(dir.path(), "part 2.pdf", 2);
    let b = make_pdf(dir.path(), "part 10.pdf", 3);
    let before_a = fs::read(&a).unwrap();

    let r = go(Action::Merge, &[b.clone(), a.clone()], Options::default());
    assert!(r.failures.is_empty(), "{:?}", r.failures);
    // Natural name order: "part 2" before "part 10".
    assert_eq!(r.outputs[0].file_name().unwrap(), "part 2-merged.pdf");
    assert_eq!(page_count(&r.outputs[0]), 5);

    let r = go(
        Action::Split,
        &[r.outputs[0].clone()],
        Options { split_mode: Some(SplitMode::Ranges), ranges: Some("1-2, 4-".into()), ..Default::default() },
    );
    assert_eq!(r.outputs.len(), 2);
    assert_eq!(page_count(&r.outputs[0]), 2);
    assert_eq!(page_count(&r.outputs[1]), 2);

    let r = go(Action::Split, &[b.clone()], Options { split_mode: Some(SplitMode::Each), ..Default::default() });
    let folder = &r.outputs[0];
    assert_eq!(fs::read_dir(folder).unwrap().count(), 3);

    let r = go(Action::Rotate, &[a.clone()], Options { angle: Some(270), ranges: Some("2".into()), ..Default::default() });
    assert!(r.failures.is_empty());
    assert_eq!(page_count(&r.outputs[0]), 2);

    // Originals are never touched.
    assert_eq!(fs::read(&a).unwrap(), before_a);
}

#[test]
fn invalid_ranges_are_reported_per_file() {
    let dir = tempfile::tempdir().unwrap();
    let a = make_pdf(dir.path(), "a.pdf", 2);
    let r = go(Action::Split, &[a], Options { ranges: Some("3".into()), ..Default::default() });
    assert!(r.outputs.is_empty());
    assert!(r.failures[0].message.contains("page 3 doesn't exist"), "{}", r.failures[0].message);
    assert_eq!(r.failures[0].retry, Some("ranges"));
}

#[test]
fn protect_unlock_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let a = make_pdf(dir.path(), "a.pdf", 1);
    let pw = Options { password: Some("correct horse".into()), ..Default::default() };

    let r = go(Action::Protect, &[a.clone()], pw.clone());
    let locked = r.outputs[0].clone();
    assert!(doc::inspect(&locked).unwrap().needs_password);

    // Every other action refuses it with a clear message.
    let r = go(Action::Compress, &[locked.clone()], Options::default());
    assert!(r.failures[0].message.contains("password-protected"));
    let r = go(Action::Merge, &[a.clone(), locked.clone()], Options::default());
    assert!(r.failures[0].message.contains("Merge failed on a-protected.pdf"));

    let r = go(Action::Unlock, &[locked.clone()], Options { password: Some("nope".into()), ..Default::default() });
    assert_eq!(r.failures[0].retry, Some("password"));

    let r = go(Action::Unlock, &[locked.clone()], pw.clone());
    assert!(r.failures.is_empty(), "{:?}", r.failures);
    assert!(!doc::inspect(&r.outputs[0]).unwrap().encrypted);

    // Unlock on a file without a password is refused before anything runs.
    assert!(matches!(plan(Action::Unlock, &[a.clone()]), Err(LpError::NotEncrypted(_))));
    // Protecting a protected file asks to unlock first.
    let r = go(Action::Protect, &[locked], pw);
    assert!(r.failures[0].message.contains("already has a password"));
}

#[test]
fn compress_shrinks_photos() {
    let dir = tempfile::tempdir().unwrap();
    let a = make_pdf(dir.path(), "scan.pdf", 2);
    let r = go(Action::Compress, &[a.clone()], Options::default());
    assert!(r.failures.is_empty(), "{:?}", r.failures);
    let before = fs::metadata(&a).unwrap().len();
    let after = fs::metadata(&r.outputs[0]).unwrap().len();
    assert!(after * 2 < before, "{before} -> {after}");
    assert_eq!(page_count(&r.outputs[0]), 2);
}

#[test]
fn bad_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.pdf");
    fs::write(&junk, b"definitely not a pdf").unwrap();
    let r = go(Action::Compress, &[junk.clone()], Options::default());
    assert!(r.failures[0].message.contains("damaged"));

    let a = make_pdf(dir.path(), "a.pdf", 1);
    let r = go(Action::Merge, &[a.clone(), junk], Options::default());
    assert!(r.outputs.is_empty());
    assert!(r.failures[0].message.starts_with("Merge failed on junk.pdf"));

    let txt = dir.path().join("notes.txt");
    fs::write(&txt, b"hi").unwrap();
    assert!(matches!(plan(Action::Convert, &[txt]), Err(LpError::Unsupported { .. })));
    assert!(matches!(plan(Action::Merge, &[a.clone()]), Err(LpError::Invalid(_))));

    let img = dir.path().join("pic.png");
    image::RgbImage::new(10, 10).save(&img).unwrap();
    assert!(plan(Action::Convert, &[a, img.clone()]).is_err(), "PDFs and images can't mix");
    assert!(matches!(plan(Action::Rotate, &[img]), Err(LpError::Unsupported { .. })));
}

#[test]
fn images_to_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("b.png");
    let jpg = dir.path().join("a.jpg");
    image::RgbaImage::from_pixel(300, 200, image::Rgba([10, 20, 30, 128])).save(&png).unwrap();
    image::RgbImage::from_pixel(200, 300, image::Rgb([200, 30, 30])).save(&jpg).unwrap();

    let p = plan(Action::Convert, &[png.clone(), jpg.clone()]).unwrap();
    assert!(p.needs_input, "several images ask one-or-many");
    let r = run(&p, &Options::default(), &mut |_| {});
    assert_eq!(page_count(&r.outputs[0]), 2);

    let p = plan(Action::Convert, &[png]).unwrap();
    assert!(!p.needs_input, "one image converts in one click");
    let r = run(&p, &Options::default(), &mut |_| {});
    assert_eq!(r.outputs[0].file_name().unwrap(), "b.pdf");
}

fn have_pdfium() -> bool {
    if std::env::var_os("LOCALPDF_PDFIUM_DIR").is_none() {
        let engines = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/engines");
        localpdf_core::render::add_search_dir(engines);
    }
    localpdf_core::render::pdfium().is_ok()
}

#[test]
fn pdf_to_images_word_text() {
    if !have_pdfium() {
        eprintln!("PDFium not available; skipping render tests");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let a = make_pdf(dir.path(), "a.pdf", 2);
    let r = go(Action::Convert, &[a.clone()], Options { target: Some(ConvertTarget::Png), dpi: Some(72), ..Default::default() });
    assert!(r.failures.is_empty(), "{:?}", r.failures);
    assert_eq!(fs::read_dir(&r.outputs[0]).unwrap().count(), 2);

    let r = go(Action::Convert, &[a.clone()], Options { target: Some(ConvertTarget::Docx), ..Default::default() });
    assert!(r.failures.is_empty(), "{:?}", r.failures);
    let docx = fs::read(&r.outputs[0]).unwrap();
    assert_eq!(&docx[..2], b"PK");

    // Image-only pages have no text layer: say so instead of writing an empty file.
    let r = go(Action::Convert, &[a], Options { target: Some(ConvertTarget::Txt), ..Default::default() });
    assert!(r.failures[0].message.contains("no text layer"));
}
