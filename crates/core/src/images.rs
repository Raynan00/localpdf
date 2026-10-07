//! Prepare raster files for embedding in a PDF.

use std::io::Cursor;
use std::path::Path;

use image::{ColorType, DynamicImage, ImageDecoder, ImageReader};
use image::metadata::Orientation;

use crate::doc::EmbeddedImage;
use crate::error::{LpError, Result};

pub fn load(path: &Path) -> Result<EmbeddedImage> {
    let bytes = std::fs::read(path).map_err(|source| LpError::Read { path: path.to_path_buf(), source })?;
    let unsupported = |reason: String| LpError::Unsupported { path: path.to_path_buf(), reason };
    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| unsupported(e.to_string()))?;
    let is_jpeg = reader.format() == Some(image::ImageFormat::Jpeg);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| unsupported(format!("the image couldn't be decoded ({e})")))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (width, height) = decoder.dimensions();
    let color = decoder.color_type();

    // Plain RGB/gray JPEGs go in untouched; EXIF rotation becomes /Rotate.
    let page_rotate = match orientation {
        Orientation::NoTransforms => Some(0),
        Orientation::Rotate90 => Some(90),
        Orientation::Rotate180 => Some(180),
        Orientation::Rotate270 => Some(270),
        _ => None,
    };
    if is_jpeg && matches!(color, ColorType::L8 | ColorType::Rgb8) {
        if let Some(rotate) = page_rotate {
            drop(decoder);
            return Ok(EmbeddedImage {
                width,
                height,
                channels: if color == ColorType::L8 { 1 } else { 3 },
                data: bytes,
                is_jpeg: true,
                rotate,
            });
        }
    }

    let mut img = DynamicImage::from_decoder(decoder)
        .map_err(|e| unsupported(format!("the image couldn't be decoded ({e})")))?;
    img.apply_orientation(orientation);
    let (width, height) = (img.width(), img.height());
    let gray = matches!(img.color(), ColorType::L8 | ColorType::L16 | ColorType::La8 | ColorType::La16);
    let flat = flatten_on_white(img);
    let (channels, data) = if gray {
        (1, flat.to_luma8().into_raw())
    } else {
        (3, flat.to_rgb8().into_raw())
    };
    Ok(EmbeddedImage { width, height, channels, data, is_jpeg: false, rotate: 0 })
}

/// Transparent pixels would turn black once the alpha channel is dropped.
fn flatten_on_white(img: DynamicImage) -> DynamicImage {
    if !img.color().has_alpha() {
        return img;
    }
    let mut rgba = img.to_rgba8();
    for p in rgba.pixels_mut() {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = ((p[c] as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
        p[3] = 255;
    }
    DynamicImage::ImageRgba8(rgba)
}
