//! Minimal WordprocessingML writer: styled paragraphs, headings, bullet
//! lists, tables, inline pictures and one section per source page.

use std::io::Write;

use crate::error::{LpError, Result};

const EMU_PER_PT: f64 = 12_700.0;

fn twips(pt: f32) -> i64 {
    (pt * 20.0).round() as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// In a header or footer run, stands for the current page number.
pub const PAGE: char = '\u{1}';
/// In a header or footer run, stands for the number of pages.
pub const NUM_PAGES: char = '\u{2}';

/// A stretch of text with one look.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub font: Option<String>,
    /// Points.
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    /// RGB; `None` for the default (black).
    pub color: Option<[u8; 3]>,
    /// 1 superscript, -1 subscript.
    pub vert: i8,
}

#[derive(Debug, Clone, Default)]
pub struct Para {
    pub runs: Vec<Run>,
    pub align: Align,
    /// Points, from the section's left margin.
    pub indent_left: f32,
    /// Points, relative to `indent_left` (negative for a hanging indent).
    pub indent_first: f32,
    /// Extra space above, points.
    pub space_before: f32,
    /// Baseline-to-baseline distance, points (`None`: Word's single spacing).
    pub line: Option<f32>,
    /// Outline level 1–3 for headings.
    pub heading: Option<u8>,
    /// Bullet list level (0-based).
    pub bullet: Option<u8>,
    /// Right-aligned tab stop with a dot leader (contents pages), points
    /// from the left margin.
    pub leader_tab: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct Cell {
    /// Number of grid columns this cell spans.
    pub span: u32,
    pub paras: Vec<Para>,
}

#[derive(Debug, Clone, Default)]
pub struct Table {
    /// Points.
    pub col_widths: Vec<f32>,
    pub rows: Vec<Vec<Cell>>,
    /// Points, from the section's left margin.
    pub indent_left: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Section {
    pub page_w: f32,
    pub page_h: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    /// Text columns (1 or 2) and the gap between them, points.
    pub columns: u8,
    pub column_gap: f32,
    /// Starts on the same page as the section before it (a one-column
    /// heading block followed by two columns).
    pub start_continuous: bool,
    /// Distance of the header from the top edge and of the footer from
    /// the bottom edge, points.
    pub header_dist: f32,
    pub footer_dist: f32,
}

impl Default for Section {
    fn default() -> Self {
        // US Letter, one-inch margins.
        Section {
            page_w: 612.0,
            page_h: 792.0,
            margin_left: 72.0,
            margin_right: 72.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
            columns: 1,
            column_gap: 36.0,
            start_continuous: false,
            header_dist: 36.0,
            footer_dist: 36.0,
        }
    }
}

impl Section {
    pub fn text_width(&self) -> f32 {
        (self.page_w - self.margin_left - self.margin_right).max(72.0)
    }

    /// `refs`: header/footer references, which come first in a sectPr.
    fn xml(&self, refs: &str) -> String {
        let orient = if self.page_w > self.page_h { " w:orient=\"landscape\"" } else { "" };
        let cols = if self.columns > 1 {
            format!("<w:cols w:num=\"{}\" w:space=\"{}\"/>", self.columns, twips(self.column_gap))
        } else {
            String::new()
        };
        let kind = if self.start_continuous { "<w:type w:val=\"continuous\"/>" } else { "" };
        format!(
            "<w:sectPr>{refs}{kind}<w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/>\
<w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"0\"/>{cols}</w:sectPr>",
            twips(self.page_w),
            twips(self.page_h),
            twips(self.margin_top),
            twips(self.margin_right),
            twips(self.margin_bottom),
            twips(self.margin_left),
            twips(self.header_dist),
            twips(self.footer_dist),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Picture {
    Jpeg,
    Png,
}

impl Picture {
    fn ext(self) -> &'static str {
        match self {
            Picture::Jpeg => "jpeg",
            Picture::Png => "png",
        }
    }
}

#[derive(Default)]
pub struct Builder {
    body: String,
    images: Vec<(Vec<u8>, Picture)>,
    section: Section,
    /// True once anything has been written to the current section.
    section_has_content: bool,
    header: Vec<Para>,
    footer: Vec<Para>,
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Characters XML 1.0 can't carry at all.
            c if ((c as u32) < 0x20 && c != '\t') || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

fn run_xml(run: &Run) -> String {
    let mut props = String::new();
    if let Some(font) = &run.font {
        let f = escape(font);
        props.push_str(&format!("<w:rFonts w:ascii=\"{f}\" w:hAnsi=\"{f}\" w:cs=\"{f}\"/>"));
    }
    if run.bold {
        props.push_str("<w:b/><w:bCs/>");
    }
    if run.italic {
        props.push_str("<w:i/><w:iCs/>");
    }
    if let Some([r, g, b]) = run.color {
        props.push_str(&format!("<w:color w:val=\"{r:02X}{g:02X}{b:02X}\"/>"));
    }
    let half_points = (run.size * 2.0).round().clamp(2.0, 3276.0) as u32;
    props.push_str(&format!("<w:sz w:val=\"{half_points}\"/><w:szCs w:val=\"{half_points}\"/>"));
    match run.vert {
        1 => props.push_str("<w:vertAlign w:val=\"superscript\"/>"),
        -1 => props.push_str("<w:vertAlign w:val=\"subscript\"/>"),
        _ => {}
    }

    // Tabs are their own element, page-number placeholders become fields;
    // everything else is preserved text.
    let mut out = format!("<w:r><w:rPr>{props}</w:rPr>");
    let mut text = String::new();
    let flush = |out: &mut String, text: &mut String| {
        if !text.is_empty() {
            out.push_str("<w:t xml:space=\"preserve\">");
            out.push_str(&escape(text));
            out.push_str("</w:t>");
            text.clear();
        }
    };
    for c in run.text.chars() {
        match c {
            '\t' => {
                flush(&mut out, &mut text);
                out.push_str("<w:tab/>");
            }
            PAGE | NUM_PAGES => {
                flush(&mut out, &mut text);
                let instr = if c == PAGE { "PAGE" } else { "NUMPAGES" };
                out.push_str(&format!(
                    "</w:r><w:fldSimple w:instr=\"{instr}\"><w:r><w:rPr>{props}</w:rPr><w:t>1</w:t></w:r></w:fldSimple><w:r><w:rPr>{props}</w:rPr>"
                ));
            }
            c => text.push(c),
        }
    }
    flush(&mut out, &mut text);
    out.push_str("</w:r>");
    out
}

fn para_xml(p: &Para, extra_ppr: &str) -> String {
    let mut ppr = String::new();
    if let Some(level) = p.heading {
        ppr.push_str(&format!("<w:pStyle w:val=\"Heading{}\"/>", level.clamp(1, 3)));
    }
    if let Some(level) = p.bullet {
        ppr.push_str(&format!("<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"1\"/></w:numPr>", level.min(2)));
    }
    if let Some(pos) = p.leader_tab {
        ppr.push_str(&format!("<w:tabs><w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"{}\"/></w:tabs>", twips(pos)));
    }
    let before = twips(p.space_before.clamp(0.0, 144.0));
    match p.line {
        Some(line) if line > 1.0 => ppr.push_str(&format!(
            "<w:spacing w:before=\"{before}\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"atLeast\"/>",
            twips(line)
        )),
        _ => ppr.push_str(&format!("<w:spacing w:before=\"{before}\" w:after=\"0\"/>")),
    }
    let left = twips(p.indent_left.max(0.0));
    let first = twips(p.indent_first);
    if left != 0 || first != 0 {
        if first < 0 {
            ppr.push_str(&format!("<w:ind w:left=\"{left}\" w:hanging=\"{}\"/>", -first));
        } else {
            ppr.push_str(&format!("<w:ind w:left=\"{left}\" w:firstLine=\"{first}\"/>"));
        }
    }
    let jc = match p.align {
        Align::Left => "",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "both",
    };
    if !jc.is_empty() {
        ppr.push_str(&format!("<w:jc w:val=\"{jc}\"/>"));
    }
    ppr.push_str(extra_ppr);
    let mut out = format!("<w:p><w:pPr>{ppr}</w:pPr>");
    for r in &p.runs {
        out.push_str(&run_xml(r));
    }
    out.push_str("</w:p>");
    out
}

impl Builder {
    /// Running header and footer, the same on every page.
    pub fn header_footer(&mut self, header: Vec<Para>, footer: Vec<Para>) {
        self.header = header;
        self.footer = footer;
    }

    fn refs(&self) -> String {
        let mut r = String::new();
        if !self.header.is_empty() {
            r.push_str("<w:headerReference w:type=\"default\" r:id=\"rIdHeader\"/>");
        }
        if !self.footer.is_empty() {
            r.push_str("<w:footerReference w:type=\"default\" r:id=\"rIdFooter\"/>");
        }
        r
    }

    /// Layout of the section currently being written (the current page).
    pub fn section(&mut self, section: Section) {
        self.section = section;
    }

    pub fn current_section(&self) -> Section {
        self.section
    }

    pub fn paragraph(&mut self, p: &Para) {
        self.body.push_str(&para_xml(p, ""));
        self.section_has_content = true;
    }

    /// Plain paragraph of default-styled text.
    pub fn text(&mut self, text: &str) {
        let run = Run { text: text.to_string(), font: None, size: 11.0, bold: false, italic: false, color: None, vert: 0 };
        self.paragraph(&Para { runs: vec![run], ..Default::default() });
    }

    pub fn table(&mut self, t: &Table) {
        if t.rows.is_empty() || t.col_widths.is_empty() {
            return;
        }
        let border = "w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"000000\"";
        let mut out = format!(
            "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblInd w:w=\"{}\" w:type=\"dxa\"/>\
<w:tblBorders><w:top {border}/><w:left {border}/><w:bottom {border}/><w:right {border}/>\
<w:insideH {border}/><w:insideV {border}/></w:tblBorders><w:tblLayout w:type=\"fixed\"/>\
<w:tblCellMar><w:left w:w=\"72\" w:type=\"dxa\"/><w:right w:w=\"72\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>",
            twips(t.indent_left.max(0.0))
        );
        for w in &t.col_widths {
            out.push_str(&format!("<w:gridCol w:w=\"{}\"/>", twips(*w)));
        }
        out.push_str("</w:tblGrid>");
        for row in &t.rows {
            out.push_str("<w:tr>");
            let mut col = 0usize;
            for cell in row {
                let span = cell.span.max(1) as usize;
                let width: f32 = t.col_widths.iter().skip(col).take(span).sum();
                col += span;
                out.push_str(&format!("<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/>", twips(width)));
                if span > 1 {
                    out.push_str(&format!("<w:gridSpan w:val=\"{span}\"/>"));
                }
                out.push_str("</w:tcPr>");
                if cell.paras.is_empty() {
                    out.push_str("<w:p/>");
                } else {
                    for p in &cell.paras {
                        out.push_str(&para_xml(p, ""));
                    }
                }
                out.push_str("</w:tc>");
            }
            out.push_str("</w:tr>");
        }
        out.push_str("</w:tbl>");
        self.body.push_str(&out);
        self.section_has_content = true;
    }

    /// Inline picture at its size on the page (points), shrunk to fit the
    /// text width and height if needed.
    pub fn picture(&mut self, data: Vec<u8>, kind: Picture, w_pt: f32, h_pt: f32, para: &Para) {
        self.images.push((data, kind));
        let n = self.images.len();
        let s = self.section;
        let max_w = (s.text_width() / s.columns.max(1) as f32) as f64;
        let max_h = (s.page_h - s.margin_top - s.margin_bottom - 14.0).max(72.0) as f64;
        let (w, h) = (w_pt.max(1.0) as f64, h_pt.max(1.0) as f64);
        let scale = (max_w / w).min(max_h / h).min(1.0);
        let cx = (w * scale * EMU_PER_PT) as u64;
        let cy = (h * scale * EMU_PER_PT) as u64;
        let ext = kind.ext();
        let drawing = format!(
            "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Picture {n}\"/>\
<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"image{n}.{ext}\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip r:embed=\"rIdImg{n}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>\
</wp:inline></w:drawing></w:r>"
        );
        let mut xml = para_xml(&Para { runs: vec![], ..para.clone() }, "");
        // Put the drawing inside the (otherwise empty) paragraph.
        xml.truncate(xml.len() - "</w:p>".len());
        xml.push_str(&drawing);
        xml.push_str("</w:p>");
        self.body.push_str(&xml);
        self.section_has_content = true;
    }

    /// End the current page: its section properties close it, and the next
    /// section starts on a new page.
    pub fn end_page(&mut self) {
        self.end_section(true);
    }

    /// Close the current section. `next_page` false lets the next section
    /// continue on the same page (it must set `start_continuous`).
    pub fn end_section(&mut self, next_page: bool) {
        if !self.section_has_content && next_page {
            self.body.push_str("<w:p/>");
        }
        let sect = self.section.xml(&self.refs());
        self.body.push_str(&format!("<w:p><w:pPr>{sect}</w:pPr></w:p>"));
        self.section_has_content = false;
        // The section after a page break starts on its own page.
        if next_page {
            self.section.start_continuous = false;
        }
    }

    /// The last page's section properties close the body.
    pub fn finish(mut self) -> Result<Vec<u8>> {
        let last_sect = self.section.xml(&self.refs());
        if self.body.is_empty() {
            self.body.push_str("<w:p/>");
        }
        let document = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\
<w:body>{body}{last_sect}</w:body></w:document>",
            body = self.body,
        );
        let mut rels = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rIdStyles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
<Relationship Id=\"rIdNumbering\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>",
        );
        for (i, (_, kind)) in self.images.iter().enumerate() {
            let n = i + 1;
            rels.push_str(&format!(
                "<Relationship Id=\"rIdImg{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{n}.{}\"/>",
                kind.ext()
            ));
        }
        let mut types = CONTENT_TYPES.to_string();
        let mut parts = Vec::new();
        for (kind, paras) in [("header", &self.header), ("footer", &self.footer)] {
            if paras.is_empty() {
                continue;
            }
            let tag = if kind == "header" { "hdr" } else { "ftr" };
            let body: String = paras.iter().map(|p| para_xml(p, "")).collect();
            parts.push((
                format!("word/{kind}1.xml"),
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:{tag} xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{body}</w:{tag}>"
                ),
            ));
            let id = if kind == "header" { "rIdHeader" } else { "rIdFooter" };
            rels.push_str(&format!(
                "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{kind}1.xml\"/>"
            ));
            types = types.replace(
                "</Types>",
                &format!(
                    "<Override PartName=\"/word/{kind}1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml\"/></Types>"
                ),
            );
        }
        rels.push_str("</Relationships>");

        let mut buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let put = |zip: &mut zip::ZipWriter<_>, name: &str, data: &[u8], o| -> std::io::Result<()> {
            zip.start_file(name, o)?;
            zip.write_all(data)
        };
        let images = std::mem::take(&mut self.images);
        let res: std::io::Result<()> = (|| {
            put(&mut zip, "[Content_Types].xml", types.as_bytes(), opts)?;
            for (name, xml) in &parts {
                put(&mut zip, name, xml.as_bytes(), opts)?;
            }
            put(&mut zip, "_rels/.rels", ROOT_RELS.as_bytes(), opts)?;
            put(&mut zip, "word/document.xml", document.as_bytes(), opts)?;
            put(&mut zip, "word/styles.xml", STYLES.as_bytes(), opts)?;
            put(&mut zip, "word/numbering.xml", NUMBERING.as_bytes(), opts)?;
            put(&mut zip, "word/_rels/document.xml.rels", rels.as_bytes(), opts)?;
            for (i, (img, kind)) in images.iter().enumerate() {
                put(&mut zip, &format!("word/media/image{}.{}", i + 1, kind.ext()), img, stored)?;
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
<Default Extension=\"png\" ContentType=\"image/png\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
<Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>\
</Types>";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";

/// Headings only carry the outline level (for Word's navigation pane); the
/// look comes from the runs, which copy the PDF.
const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"Calibri\" w:cs=\"Calibri\"/>\
<w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val=\"0\"/></w:pPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val=\"1\"/></w:pPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading3\"><w:name w:val=\"heading 3\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val=\"2\"/></w:pPr></w:style>\
</w:styles>";

const NUMBERING: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\
<w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"\u{2022}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr></w:lvl>\
<w:lvl w:ilvl=\"1\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"\u{25E6}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"1440\" w:hanging=\"360\"/></w:pPr></w:lvl>\
<w:lvl w:ilvl=\"2\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"\u{25AA}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"2160\" w:hanging=\"360\"/></w:pPr></w:lvl>\
</w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num></w:numbering>";
