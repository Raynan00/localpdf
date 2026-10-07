//! Minimal WordprocessingML writer: paragraphs, page breaks, inline JPEGs.

use std::io::Write;

use crate::error::{LpError, Result};

const EMU_PER_PT: f64 = 12_700.0;
const MARGIN_PT: f32 = 72.0;

pub struct Builder {
    body: String,
    images: Vec<Vec<u8>>,
    page_w: f32,
    page_h: f32,
}

impl Default for Builder {
    fn default() -> Self {
        // US Letter until told otherwise.
        Builder { body: String::new(), images: Vec::new(), page_w: 612.0, page_h: 792.0 }
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push(' '),
            // Characters XML 1.0 can't carry at all.
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

impl Builder {
    pub fn page_size(&mut self, w_pt: f32, h_pt: f32) {
        if w_pt > 72.0 && h_pt > 72.0 {
            self.page_w = w_pt;
            self.page_h = h_pt;
        }
    }

    pub fn paragraph(&mut self, text: &str) {
        self.body.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
        self.body.push_str(&escape(text));
        self.body.push_str("</w:t></w:r></w:p>");
    }

    pub fn page_break(&mut self) {
        self.body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
    }

    /// Inline JPEG scaled to the text width (and height), keeping aspect.
    pub fn image(&mut self, jpeg: Vec<u8>, w_pt: f32, h_pt: f32) {
        self.images.push(jpeg);
        let n = self.images.len();
        let max_w = (self.page_w - 2.0 * MARGIN_PT) as f64;
        let max_h = (self.page_h - 2.0 * MARGIN_PT) as f64 - 14.0;
        let scale = (max_w / w_pt as f64).min(max_h / h_pt as f64);
        let cx = (w_pt as f64 * scale * EMU_PER_PT) as u64;
        let cy = (h_pt as f64 * scale * EMU_PER_PT) as u64;
        self.body.push_str(&format!(
            "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Page image {n}\"/>\
<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"image{n}.jpeg\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip r:embed=\"rIdImg{n}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>\
</wp:inline></w:drawing></w:r></w:p>"
        ));
    }

    pub fn finish(self) -> Result<Vec<u8>> {
        let twips = |pt: f32| (pt * 20.0).round() as u32;
        let margin = twips(MARGIN_PT);
        let orient = if self.page_w > self.page_h { " w:orient=\"landscape\"" } else { "" };
        let document = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\
<w:body>{body}<w:sectPr><w:pgSz w:w=\"{w}\" w:h=\"{h}\"{orient}/>\
<w:pgMar w:top=\"{margin}\" w:right=\"{margin}\" w:bottom=\"{margin}\" w:left=\"{margin}\" \
w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>",
            body = self.body,
            w = twips(self.page_w),
            h = twips(self.page_h),
        );
        let mut rels = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rIdStyles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
        );
        for n in 1..=self.images.len() {
            rels.push_str(&format!(
                "<Relationship Id=\"rIdImg{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{n}.jpeg\"/>"
            ));
        }
        rels.push_str("</Relationships>");

        let mut buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let put = |zip: &mut zip::ZipWriter<_>, name: &str, data: &[u8], o| -> std::io::Result<()> {
            zip.start_file(name, o)?;
            zip.write_all(data)
        };
        let res: std::io::Result<()> = (|| {
            put(&mut zip, "[Content_Types].xml", CONTENT_TYPES.as_bytes(), opts)?;
            put(&mut zip, "_rels/.rels", ROOT_RELS.as_bytes(), opts)?;
            put(&mut zip, "word/document.xml", document.as_bytes(), opts)?;
            put(&mut zip, "word/styles.xml", STYLES.as_bytes(), opts)?;
            put(&mut zip, "word/_rels/document.xml.rels", rels.as_bytes(), opts)?;
            for (i, img) in self.images.iter().enumerate() {
                put(&mut zip, &format!("word/media/image{}.jpeg", i + 1), img, stored)?;
            }
            zip.finish()?;
            Ok(())
        })();
        res.map_err(|e| LpError::Engine(format!("Couldn't build the Word file: {e}")))?;
        Ok(buf.into_inner())
    }
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
</Types>";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";

const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"Calibri\" w:cs=\"Calibri\"/>\
<w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"264\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
</w:styles>";
