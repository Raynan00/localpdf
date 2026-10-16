//! PDF → Word that keeps the look: fonts, sizes, bold/italic, colours,
//! alignment, headings, bullet lists, tables and pictures, rebuilt as real,
//! editable Word structure (flowing paragraphs, not positioned text boxes).
//!
//! `extract` reads one page through PDFium into plain data; everything after
//! that (`layout`) is pure geometry, so it can be unit-tested without PDFium.

use std::collections::HashMap;

use pdfium_render::prelude::*;

use crate::docx::{Align, Builder, Cell, Para, Picture, Run, Section, Table, NUM_PAGES, PAGE};

// ---------------------------------------------------------------- page data

#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    pub c: char,
    pub x0: f32,
    pub x1: f32,
    /// Baseline (PDF space: y grows upwards).
    pub base: f32,
    /// Font size in points.
    pub size: f32,
    /// Index into `PageContent::fonts`.
    pub font: usize,
    pub bold: bool,
    pub italic: bool,
    pub color: [u8; 3],
    /// 1 superscript, -1 subscript (raised or lowered smaller text).
    pub vert: i8,
}

impl Glyph {
    fn cx(&self) -> f32 {
        (self.x0 + self.x1) / 2.0
    }
    fn cy(&self) -> f32 {
        self.base + self.size * 0.3
    }
    fn top(&self) -> f32 {
        self.base + self.size * 0.8
    }
    fn is_space(&self) -> bool {
        self.c.is_whitespace()
    }
}

/// A straight ruling line (table border).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rule {
    H { y: f32, x0: f32, x1: f32 },
    V { x: f32, y0: f32, y1: f32 },
}

pub struct PageImage {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub data: Vec<u8>,
    pub kind: Picture,
}

#[derive(Default)]
pub struct PageContent {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<Glyph>,
    pub fonts: Vec<String>,
    pub rules: Vec<Rule>,
    pub images: Vec<PageImage>,
}

/// "ABCDEF+TimesNewRomanPS-BoldItalicMT" → "Times New Roman".
pub fn clean_font_name(raw: &str) -> String {
    let mut name = raw;
    if let Some((prefix, rest)) = name.split_once('+') {
        if prefix.len() == 6 && prefix.chars().all(|c| c.is_ascii_uppercase()) {
            name = rest;
        }
    }
    let name = name.split([',', '-']).next().unwrap_or(name);
    if let Some(sub) = substitute_font(name) {
        return sub.into();
    }
    let mut base = name.to_string();
    for suffix in ["PSMT", "MT", "PS", "Std", "Pro"] {
        if base.len() > suffix.len() + 2 && base.ends_with(suffix) {
            base.truncate(base.len() - suffix.len());
            break;
        }
    }
    // Split CamelCase: "LiberationSerif" → "Liberation Serif".
    let mut out = String::new();
    let chars: Vec<char> = base.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_ascii_uppercase() && chars[i - 1].is_ascii_lowercase() {
            out.push(' ');
        }
        out.push(c);
    }
    match out.as_str() {
        "Arial" | "Helvetica" | "Arial Narrow" => out,
        "Times" | "Times Roman" => "Times New Roman".into(),
        _ => out,
    }
}

/// TeX font name without the subset prefix, e.g. "CMBX12", "SFRM1000".
fn tex_name(name: &str) -> String {
    name.rsplit('+').next().unwrap_or(name).to_ascii_uppercase()
}

fn name_says_bold(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let tex = tex_name(name);
    ["bold", "black", "heavy", "semibold", "demi"].iter().any(|k| n.contains(k))
        || ["CMBX", "CMB1", "CMSSBX", "SFBX", "SFSX"].iter().any(|k| tex.starts_with(k))
}

fn name_says_italic(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let tex = tex_name(name);
    n.contains("italic") || n.contains("oblique") || ["CMTI", "CMSL", "CMBXTI", "CMMI", "SFTI", "SFSL"].iter().any(|k| tex.starts_with(k))
}

/// Fonts that only exist inside PDFs (TeX's Computer Modern, the URW
/// clones of the PostScript fonts): the closest font every Word install has.
fn substitute_font(base: &str) -> Option<&'static str> {
    let u = base.to_ascii_uppercase();
    let has = |p: &str| u.starts_with(p);
    // Computer Modern: CM + style letters + design size ("CMR10", "CMBX12").
    let digits_end = u.ends_with(|c: char| c.is_ascii_digit());
    let cm = u.len() > 2 && has("CM") && u[2..].chars().all(|c| c.is_ascii_alphanumeric()) && digits_end;
    let ec = (has("SF") || has("EC")) && digits_end;
    if ["CMMI", "CMSY", "CMEX", "MSAM", "MSBM", "LMMATH", "STIXMATH", "EUFM", "RSFS"].iter().any(|p| has(p)) {
        Some("Cambria Math")
    } else if (cm && (has("CMTT") || has("CMSLTT") || has("CMITT"))) || has("SFTT") || has("LMMONO") || has("NIMBUSMON") || has("COURIER") {
        Some("Courier New")
    } else if (cm && has("CMSS")) || has("SFSS") || has("SFSX") || has("LMSANS") || has("NIMBUSSAN") || has("TEXGYREHEROS") || u == "HELVETICA" {
        Some("Arial")
    } else if has("URWPALLADIO") || has("TEXGYREPAGELLA") || has("PALATINO") {
        Some("Palatino Linotype")
    } else if cm || ec || has("CMU") || has("LMROMAN") || has("NIMBUSROM") || has("TEXGYRETERMES") || has("TIMES") || has("DUTCH") {
        Some("Times New Roman")
    } else {
        None
    }
}

/// Read one page into plain data.
pub fn extract(document: &PdfDocument, page: &PdfPage) -> PageContent {
    let mut content = PageContent { width: page.width().value, height: page.height().value, ..Default::default() };
    let mut font_ids: HashMap<String, usize> = HashMap::new();

    if let Ok(text) = page.text() {
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            if c == '\r' || c == '\n' || c == '\u{2}' || c == '\u{0}' {
                continue;
            }
            let Ok(bounds) = ch.loose_bounds() else { continue };
            let (ox, oy) = match ch.origin() {
                Ok((x, y)) => (x.value, y.value),
                Err(_) => (bounds.left().value, bounds.bottom().value),
            };
            let size = ch.scaled_font_size().value;
            if !(size > 0.5 && size < 500.0) {
                continue;
            }
            let raw = ch.font_name();
            let weight_bold = matches!(
                ch.font_weight(),
                Some(PdfFontWeight::Weight600 | PdfFontWeight::Weight700Bold | PdfFontWeight::Weight800 | PdfFontWeight::Weight900)
            ) || matches!(ch.font_weight(), Some(PdfFontWeight::Custom(w)) if w >= 600);
            let bold = weight_bold || ch.font_is_bold_reenforced() || name_says_bold(&raw);
            let italic = ch.font_is_italic() || name_says_italic(&raw);
            let clean = clean_font_name(&raw);
            let next_id = font_ids.len();
            let font = *font_ids.entry(clean.clone()).or_insert_with(|| {
                content.fonts.push(clean);
                next_id
            });
            let color = ch.fill_color().map(|c| [c.red(), c.green(), c.blue()]).unwrap_or([0, 0, 0]);
            let x1 = bounds.right().value.max(ox);
            content.glyphs.push(Glyph {
                c,
                x0: ox.min(bounds.left().value.max(ox - size)),
                x1,
                base: oy,
                size,
                font,
                bold,
                italic,
                color,
                vert: 0,
            });
        }
    }

    for object in page.objects().iter() {
        let Ok(quad) = object.bounds() else { continue };
        let r = quad.to_rect();
        let (x0, x1, y0, y1) = (r.left().value, r.right().value, r.bottom().value, r.top().value);
        let (w, h) = (x1 - x0, y1 - y0);
        if let Some(path) = object.as_path_object() {
            if h <= 3.0 && w >= 4.0 {
                content.rules.push(Rule::H { y: (y0 + y1) / 2.0, x0, x1 });
            } else if w <= 3.0 && h >= 4.0 {
                content.rules.push(Rule::V { x: (x0 + x1) / 2.0, y0, y1 });
            } else if w > 4.0 && h > 4.0 && path.is_stroked().unwrap_or(false) && path.segments().len() <= 6 {
                // A stroked rectangle: its four edges.
                content.rules.push(Rule::H { y: y0, x0, x1 });
                content.rules.push(Rule::H { y: y1, x0, x1 });
                content.rules.push(Rule::V { x: x0, y0, y1 });
                content.rules.push(Rule::V { x: x1, y0, y1 });
            }
        } else if let Some(img) = object.as_image_object() {
            if w < 8.0 || h < 8.0 {
                continue;
            }
            let picture = img.get_processed_image(document).or_else(|_| img.get_raw_image());
            if let Ok(picture) = picture {
                if let Some((data, kind)) = encode_picture(picture, w, h) {
                    content.images.push(PageImage { x0, y0, x1, y1, data, kind });
                }
            }
        }
    }
    content
}

/// PNG when the picture has transparency, JPEG otherwise; never more pixels
/// than ~220 dpi at the size it is shown.
fn encode_picture(img: image::DynamicImage, w_pt: f32, h_pt: f32) -> Option<(Vec<u8>, Picture)> {
    let max_w = ((w_pt / 72.0) * 220.0).max(64.0) as u32;
    let max_h = ((h_pt / 72.0) * 220.0).max(64.0) as u32;
    let img = if img.width() > max_w * 2 || img.height() > max_h * 2 {
        img.resize(max_w, max_h, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let has_alpha = img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p.0[3] < 250);
    let mut out = Vec::new();
    if has_alpha {
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
        Some((out, Picture::Png))
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88)
            .encode_image(&image::DynamicImage::ImageRgb8(img.to_rgb8()))
            .ok()?;
        Some((out, Picture::Jpeg))
    }
}

// ---------------------------------------------------------------- lines

#[derive(Debug, Clone)]
struct Line {
    glyphs: Vec<Glyph>,
    x0: f32,
    x1: f32,
    base: f32,
    size: f32,
}

impl Line {
    fn new(mut glyphs: Vec<Glyph>) -> Line {
        glyphs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        // Trim spaces at both ends.
        while glyphs.last().is_some_and(Glyph::is_space) {
            glyphs.pop();
        }
        let start = glyphs.iter().position(|g| !g.is_space()).unwrap_or(glyphs.len());
        glyphs.drain(..start);
        let x0 = glyphs.first().map(|g| g.x0).unwrap_or(0.0);
        let x1 = glyphs.iter().map(|g| g.x1).fold(f32::MIN, f32::max);
        let size = dominant_size(&glyphs);
        let base = median(glyphs.iter().filter(|g| !g.is_space()).map(|g| g.base).collect()).unwrap_or(0.0);
        Line { glyphs, x0, x1, base, size }
    }

    fn top(&self) -> f32 {
        self.glyphs.iter().map(Glyph::top).fold(self.base, f32::max)
    }

    fn text(&self) -> String {
        assemble(&self.glyphs, self.size).into_iter().map(|(c, _)| c).collect()
    }

    fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[v.len() / 2])
}

fn dominant_size(glyphs: &[Glyph]) -> f32 {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for g in glyphs.iter().filter(|g| !g.is_space()) {
        *counts.entry((g.size * 2.0).round() as i32).or_default() += 1;
    }
    counts.into_iter().max_by_key(|&(s, n)| (n, s)).map(|(s, _)| s as f32 / 2.0).unwrap_or(11.0)
}

/// Group glyphs into lines by baseline, then split each line where a gap is
/// wide enough to be a column gutter or a tab stop.
fn build_lines(glyphs: Vec<Glyph>) -> Vec<Line> {
    let mut glyphs = glyphs;
    glyphs.sort_by(|a, b| b.base.total_cmp(&a.base).then(a.x0.total_cmp(&b.x0)));
    let mut rows: Vec<Vec<Glyph>> = Vec::new();
    for g in glyphs {
        let joined = rows.last_mut().filter(|row| {
            let size = row.iter().map(|r| r.size).fold(g.size, f32::max);
            let base = row.iter().filter(|r| !r.is_space()).map(|r| r.base).next().unwrap_or(row[0].base);
            (base - g.base).abs() <= size * 0.35
        });
        match joined {
            Some(row) => row.push(g),
            None => rows.push(vec![g]),
        }
    }
    let mut lines: Vec<Line> = rows.into_iter().map(Line::new).filter(|l| !l.is_empty()).collect();

    // Smaller text just above or below a line (footnote marks, exponents,
    // chemical formulas) belongs to it as superscript or subscript.
    let mut i = 0;
    while i < lines.len() {
        let host = [i + 1, i.wrapping_sub(1)].into_iter().filter(|&j| j < lines.len()).find(|&j| {
            let (s, h) = (&lines[i], &lines[j]);
            s.size < h.size * 0.85
                && (s.base - h.base).abs() <= h.size * 0.55
                && s.x0 >= h.x0 - h.size * 2.0
                && s.x1 <= h.x1 + h.size * 2.0
        });
        let Some(j) = host else {
            i += 1;
            continue;
        };
        let small = lines.remove(i);
        let j = if j > i { j - 1 } else { j };
        let (base, size) = (lines[j].base, lines[j].size);
        let mut glyphs = std::mem::take(&mut lines[j].glyphs);
        for mut g in small.glyphs {
            g.vert = if g.base > base + size * 0.12 {
                1
            } else if g.base < base - size * 0.08 {
                -1
            } else {
                0
            };
            if g.vert != 0 {
                g.base = base;
                g.size = size;
            }
            glyphs.push(g);
        }
        lines[j] = Line::new(glyphs);
        i = i.min(j);
    }
    lines
}

/// Split a line into pieces separated by gaps wider than `min_gap`.
fn split_line(line: &Line, min_gap: f32) -> Vec<Line> {
    let mut pieces: Vec<Vec<Glyph>> = vec![vec![]];
    let mut last_x1: Option<f32> = None;
    for g in &line.glyphs {
        if g.is_space() {
            pieces.last_mut().unwrap().push(g.clone());
            continue;
        }
        if let Some(x1) = last_x1 {
            if g.x0 - x1 > min_gap {
                pieces.push(vec![]);
            }
        }
        last_x1 = Some(g.x1);
        pieces.last_mut().unwrap().push(g.clone());
    }
    pieces.into_iter().map(Line::new).filter(|l| !l.is_empty()).collect()
}

const LIGATURES: &[(char, &str)] = &[('\u{FB00}', "ff"), ('\u{FB01}', "fi"), ('\u{FB02}', "fl"), ('\u{FB03}', "ffi"), ('\u{FB04}', "ffl")];

/// Text of a run of glyphs with spaces/tabs restored from the geometry.
/// Each output char carries the index of the glyph whose style it takes.
fn assemble(glyphs: &[Glyph], size: f32) -> Vec<(char, usize)> {
    let mut out: Vec<(char, usize)> = Vec::new();
    let mut prev: Option<&Glyph> = None;
    for (i, g) in glyphs.iter().enumerate() {
        if let Some(p) = prev {
            let gap = g.x0 - p.x1;
            let last_is_space = out.last().is_some_and(|(c, _)| c.is_whitespace());
            if !g.is_space() && !last_is_space {
                if gap > size * 2.5 {
                    out.push(('\t', i - 1));
                } else if gap > size * 0.22 {
                    out.push((' ', i - 1));
                }
            }
        }
        if g.is_space() {
            if !out.last().is_some_and(|(c, _)| c.is_whitespace()) {
                out.push((' ', i));
            }
        } else if let Some((_, s)) = LIGATURES.iter().find(|(c, _)| *c == g.c) {
            out.extend(s.chars().map(|c| (c, i)));
        } else if !('\u{E000}'..='\u{F8FF}').contains(&g.c) {
            out.push((g.c, i));
        }
        prev = Some(g);
    }
    out
}

// ---------------------------------------------------------------- tables

struct Grid {
    /// Column edges, left to right.
    xs: Vec<f32>,
    /// Row edges, top to bottom (descending y).
    ys: Vec<f32>,
    verticals: Vec<(f32, f32, f32)>,
}

impl Grid {
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.xs[0] - 1.0 && x <= *self.xs.last().unwrap() + 1.0 && y <= self.ys[0] + 1.0 && y >= *self.ys.last().unwrap() - 1.0
    }

    /// Whether a vertical border separates columns `col-1` and `col` in `row`.
    fn has_border(&self, col: usize, row: usize) -> bool {
        let x = self.xs[col];
        let (top, bottom) = (self.ys[row], self.ys[row + 1]);
        let mid = (top + bottom) / 2.0;
        self.verticals.iter().any(|&(vx, y0, y1)| (vx - x).abs() < 2.5 && y0 - 1.5 <= mid && mid <= y1 + 1.5)
    }
}

fn cluster(values: &mut [f32], tol: f32) -> Vec<f32> {
    values.sort_by(f32::total_cmp);
    let mut out: Vec<f32> = Vec::new();
    for &v in values.iter() {
        match out.last() {
            Some(&last) if v - last <= tol => {}
            _ => out.push(v),
        }
    }
    out
}

fn find_grids(rules: &[Rule]) -> Vec<Grid> {
    let n = rules.len();
    if n < 4 {
        return vec![];
    }
    // Union-find over rules that touch.
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let touches = |a: &Rule, b: &Rule| -> bool {
        const T: f32 = 2.5;
        match (*a, *b) {
            (Rule::H { y, x0, x1 }, Rule::V { x, y0, y1 }) | (Rule::V { x, y0, y1 }, Rule::H { y, x0, x1 }) => {
                x >= x0 - T && x <= x1 + T && y >= y0 - T && y <= y1 + T
            }
            _ => false,
        }
    };
    for i in 0..n {
        for j in i + 1..n {
            if touches(&rules[i], &rules[j]) {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                parent[a] = b;
            }
        }
    }
    let mut groups: HashMap<usize, Vec<Rule>> = HashMap::new();
    for (i, rule) in rules.iter().enumerate() {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(*rule);
    }
    let mut grids = Vec::new();
    for rules in groups.into_values() {
        let mut ys: Vec<f32> = rules.iter().filter_map(|r| if let Rule::H { y, .. } = r { Some(*y) } else { None }).collect();
        let mut xs: Vec<f32> = rules.iter().filter_map(|r| if let Rule::V { x, .. } = r { Some(*x) } else { None }).collect();
        let xs = cluster(&mut xs, 2.5);
        let mut ys = cluster(&mut ys, 2.5);
        if xs.len() < 2 || ys.len() < 2 || (xs.len() - 1) * (ys.len() - 1) < 2 {
            continue;
        }
        if xs.last().unwrap() - xs[0] < 30.0 || ys.last().unwrap() - ys[0] < 8.0 {
            continue;
        }
        ys.reverse();
        let verticals = rules.iter().filter_map(|r| if let Rule::V { x, y0, y1 } = r { Some((*x, *y0, *y1)) } else { None }).collect();
        grids.push(Grid { xs, ys, verticals });
    }
    grids
}

// ---------------------------------------------------------------- paragraphs

const BULLETS: &[char] = &['•', '◦', '▪', '▫', '■', '□', '●', '○', '‣', '⁃', '∙', '·', '–', '-', '*', '➢', '►', '✓', '✔'];

fn bullet_glyph(line: &Line) -> Option<usize> {
    let first = line.glyphs.iter().position(|g| !g.is_space())?;
    let g = &line.glyphs[first];
    let is_bullet_char = BULLETS.contains(&g.c) || ('\u{E000}'..='\u{F8FF}').contains(&g.c);
    if !is_bullet_char {
        return None;
    }
    // A dash or hyphen only counts when it stands alone before the text.
    let next = line.glyphs.iter().skip(first + 1).find(|n| !n.is_space())?;
    if next.x0 - g.x1 < g.size * 0.2 {
        return None;
    }
    Some(first)
}

struct ParaDraft {
    lines: Vec<Line>,
    bullet_x: Option<f32>,
}

impl ParaDraft {
    fn size(&self) -> f32 {
        dominant_size(&self.lines.iter().flat_map(|l| l.glyphs.iter().cloned()).collect::<Vec<_>>())
    }
    fn top(&self) -> f32 {
        self.lines[0].top()
    }
    fn last_base(&self) -> f32 {
        self.lines.last().unwrap().base
    }
    fn leading(&self) -> Option<f32> {
        if self.lines.len() < 2 {
            return None;
        }
        median(self.lines.windows(2).map(|w| w[0].base - w[1].base).collect())
    }
    fn all_bold(&self) -> bool {
        self.lines.iter().flat_map(|l| &l.glyphs).filter(|g| !g.is_space()).all(|g| g.bold)
    }
}

/// Split ordered lines (one column) into paragraphs.
/// Width of a line's first word, to tell a wrapped line from a short one.
fn first_word_width(line: &Line) -> f32 {
    let mut end = line.x0;
    for (i, g) in line.glyphs.iter().enumerate() {
        if g.is_space() {
            break;
        }
        end = g.x1;
        if line.glyphs.get(i + 1).is_some_and(|n| n.x0 - g.x1 > g.size * 0.15) {
            break;
        }
    }
    end - line.x0
}

fn group_paragraphs(lines: Vec<Line>, right: f32) -> Vec<ParaDraft> {
    let mut paras: Vec<ParaDraft> = Vec::new();
    for line in lines {
        let bullet = bullet_glyph(&line);
        let new_para = match paras.last() {
            None => true,
            Some(p) => {
                let prev = p.lines.last().unwrap();
                // A contents line ("Intro ....... 4") stands alone.
                let toc = toc_leader(&prev.text()).is_some();
                let size = p.size().max(line.size);
                let gap = prev.base - line.base;
                let leading = p.leading().unwrap_or(size * 1.25);
                let size_changed = (line.size - p.size()).abs() > p.size() * 0.15;
                let too_far = gap > (leading * 1.35).max(size * 1.5) || gap < size * 0.5;
                // The line ended early although the next word would have fitted.
                let room = right - prev.x1;
                let prev_short = room > first_word_width(&line) + size * 0.6 && room > size;
                let line_bold = line.glyphs.iter().filter(|g| !g.is_space()).all(|g| g.bold);
                let style_changed = p.all_bold() != line_bold && p.lines.len() == 1;
                let indent_ok = if let Some(bx) = p.bullet_x {
                    // Continuation of a bullet: text aligned after the bullet.
                    line.x0 > bx + size * 0.3
                } else if p.lines.len() == 1 {
                    line.x0 <= p.lines[0].x0 + size * 0.6
                } else {
                    (line.x0 - p.lines[1].x0).abs() <= size * 0.6
                };
                // Centred text: lines start at different x but share a centre.
                let centre = |l: &Line| (l.x0 + l.x1) / 2.0;
                let indent_ok = indent_ok || (line.x0 > prev.x0 + size && (centre(&line) - centre(prev)).abs() <= size * 0.6);
                bullet.is_some() || size_changed || too_far || prev_short || style_changed || !indent_ok || toc
            }
        };
        let bullet_x = bullet.map(|i| line.glyphs[i].x0);
        if new_para {
            paras.push(ParaDraft { lines: vec![line], bullet_x });
        } else {
            paras.last_mut().unwrap().lines.push(line);
        }
    }
    paras
}

fn runs_for(draft: &ParaDraft, fonts: &[String], skip_bullet: bool) -> Vec<Run> {
    let mut chars: Vec<(char, Glyph)> = Vec::new();
    for (li, line) in draft.lines.iter().enumerate() {
        let mut glyphs: &[Glyph] = &line.glyphs;
        if li == 0 && skip_bullet {
            if let Some(i) = bullet_glyph(line) {
                glyphs = &glyphs[i + 1..];
                while glyphs.first().is_some_and(Glyph::is_space) {
                    glyphs = &glyphs[1..];
                }
            }
        }
        if li > 0 && !glyphs.is_empty() {
            // Join wrapped lines: drop a line-end hyphen before a lowercase
            // continuation, otherwise put a space.
            let ends_hyphen = chars.last().is_some_and(|(c, _)| *c == '-' || *c == '\u{AD}');
            let next_lower = glyphs.iter().find(|g| !g.is_space()).is_some_and(|g| g.c.is_lowercase());
            if ends_hyphen && next_lower {
                chars.pop();
            } else if !chars.last().is_some_and(|(c, _)| c.is_whitespace()) {
                if let Some((_, g)) = chars.last() {
                    let g = g.clone();
                    chars.push((' ', g));
                }
            }
        }
        for (c, i) in assemble(glyphs, line.size) {
            chars.push((c, glyphs[i].clone()));
        }
    }
    while chars.last().is_some_and(|(c, _)| c.is_whitespace()) {
        chars.pop();
    }

    let mut runs: Vec<Run> = Vec::new();
    for (c, g) in chars {
        let color = if g.color.iter().all(|&v| v < 48) { None } else { Some(g.color) };
        let font = fonts.get(g.font).filter(|f| !f.is_empty()).cloned();
        let size = (g.size * 2.0).round() / 2.0;
        match runs.last_mut() {
            Some(r)
                if (c.is_whitespace() && g.vert == 0 && r.vert == 0)
                    || (r.font == font && r.size == size && r.bold == g.bold && r.italic == g.italic && r.color == color && r.vert == g.vert) =>
            {
                r.text.push(c)
            }
            _ => runs.push(Run { text: c.to_string(), font, size, bold: g.bold, italic: g.italic, color, vert: g.vert }),
        }
    }
    runs
}

fn finish_para(draft: &ParaDraft, fonts: &[String], left: f32, right: f32, in_cell: bool) -> Para {
    let size = draft.size();
    let lines = &draft.lines;
    let tol = (size * 0.8).max(4.0);
    let gaps: Vec<(f32, f32)> = lines.iter().map(|l| (l.x0 - left, right - l.x1)).collect();
    let centered = gaps.iter().all(|&(lg, rg)| (lg - rg).abs() <= tol) && gaps.iter().any(|&(lg, _)| lg > size * 1.5);
    let right_aligned = !centered && gaps.iter().all(|&(lg, rg)| rg <= tol && lg > size * 3.0);
    // Justified lines end at the same x to within rounding; ragged-right
    // lines that merely come close still differ by a point or more.
    let ends: Vec<f32> = gaps[..gaps.len() - 1].iter().map(|&(_, rg)| rg).collect();
    let spread = ends.iter().cloned().fold(f32::MIN, f32::max) - ends.iter().cloned().fold(f32::MAX, f32::min);
    let justified = ends.len() >= 2 && ends.iter().all(|&rg| rg <= size * 0.6) && spread <= size * 0.12;
    let align = if centered {
        Align::Center
    } else if right_aligned {
        Align::Right
    } else if justified {
        Align::Justify
    } else {
        Align::Left
    };

    let (mut indent_left, mut indent_first) = (0.0, 0.0);
    if align == Align::Left || align == Align::Justify {
        if let Some(bx) = draft.bullet_x {
            let text_x = lines[0]
                .glyphs
                .iter()
                .skip(bullet_glyph(&lines[0]).map(|i| i + 1).unwrap_or(0))
                .find(|g| !g.is_space())
                .map(|g| g.x0)
                .unwrap_or(bx + size);
            indent_left = text_x - left;
            indent_first = -(text_x - bx);
        } else {
            let body_x = if lines.len() > 1 { lines[1].x0 } else { lines[0].x0 };
            indent_left = body_x - left;
            indent_first = lines[0].x0 - body_x;
        }
        if in_cell {
            indent_left = 0.0;
            indent_first = 0.0;
        }
        if indent_left.abs() < 2.0 {
            indent_left = 0.0;
        }
        if indent_first.abs() < 2.0 {
            indent_first = 0.0;
        }
    }

    let mut runs = runs_for(draft, fonts, draft.bullet_x.is_some());
    let leader_tab = leader_to_tab(&mut runs).then_some(right - left);
    Para {
        runs,
        align: if leader_tab.is_some() { Align::Left } else { align },
        indent_left,
        indent_first,
        space_before: 0.0,
        line: draft.leading(),
        heading: None,
        bullet: draft.bullet_x.map(|_| 0),
        leader_tab,
    }
}

/// Byte range of a dot leader ("Intro ........ 4"): four or more dots,
/// spaced or not, with the spaces around them.
fn leader_span(s: &str) -> Option<(usize, usize)> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let is_dot = |c: char| matches!(c, '.' | '·' | '…');
    let mut i = 0;
    while i < chars.len() {
        if !is_dot(chars[i].1) {
            i += 1;
            continue;
        }
        let (mut j, mut dots) = (i, 0);
        while j < chars.len() && (is_dot(chars[j].1) || chars[j].1 == ' ') {
            dots += is_dot(chars[j].1) as usize;
            j += 1;
        }
        if dots >= 4 {
            let mut start = i;
            while start > 0 && chars[start - 1].1 == ' ' {
                start -= 1;
            }
            return Some((chars[start].0, chars.get(j).map_or(s.len(), |c| c.0)));
        }
        i = j;
    }
    None
}

/// The leader of a contents line, if what follows it is short (a page number).
fn toc_leader(s: &str) -> Option<(usize, usize)> {
    leader_span(s).filter(|&(a, b)| a > 0 && b < s.len() && s[b..].chars().count() <= 8)
}

/// Contents-page lines: replace the dot leader before the page number with
/// a tab (the paragraph gets a dotted right tab stop). True if one was found.
fn leader_to_tab(runs: &mut [Run]) -> bool {
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    if toc_leader(&text).is_none() {
        return false;
    }
    for run in runs.iter_mut().rev() {
        if let Some((a, b)) = leader_span(&run.text) {
            run.text.replace_range(a..b, "\t");
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------- page layout

pub enum Block {
    Para(Para),
    Table(Table),
    Picture { image: usize, w: f32, h: f32, para: Para },
}

pub struct PageLayout {
    pub section: Section,
    /// Blocks of the full-width part above the columns, if the page has two.
    pub head: Vec<Block>,
    pub blocks: Vec<Block>,
    /// Paragraph sizes, for heading detection across the document.
    pub text_chars: usize,
}

/// One unit in reading order before it becomes a block.
enum Unit {
    Para(ParaDraft),
    Table(usize),
    Image(usize),
}

struct Placed {
    unit: Unit,
    top: f32,
    bottom: f32,
    /// 0 full width, 1 left column, 2 right column.
    column: u8,
}

/// Lines (header/footer text) to leave out, by normalised text.
#[derive(Default)]
pub struct Furniture {
    pub drop: std::collections::HashSet<String>,
    /// The repeated lines as Word header and footer paragraphs, page
    /// numbers as fields.
    pub header: Vec<Para>,
    pub footer: Vec<Para>,
    /// Points from the top and bottom edges.
    pub header_dist: Option<f32>,
    pub footer_dist: Option<f32>,
}

fn normalise(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_digit() { '#' } else { c }).collect::<String>().trim().to_lowercase()
}

fn is_page_number(text: &str) -> bool {
    let t = normalise(text);
    let t = t.trim_matches(|c: char| !c.is_alphanumeric() && c != '#');
    let t = t.strip_prefix("page").unwrap_or(t).trim();
    !t.is_empty() && t.chars().all(|c| c == '#' || c == ' ' || c == '/' || c == '|') || t.contains("# of #") && t.len() < 16
}

/// Find text repeated in the top or bottom band of most pages.
/// Where running headers, footers and page numbers sit: the top and bottom
/// 12% of the page (LaTeX puts page numbers well above the bottom edge).
const BAND: f32 = 0.12;

/// Lines in the top and bottom bands of a page, top first, flagged `true`
/// when in the top band.
fn band_lines(p: &PageContent) -> Vec<(bool, Line)> {
    let band = p.height * BAND;
    let mut lines: Vec<(bool, Line)> = build_lines(p.glyphs.clone())
        .into_iter()
        .filter(|l| l.base > p.height - band || l.base < band)
        .map(|l| (l.base > p.height / 2.0, l))
        .collect();
    lines.sort_by(|a, b| b.1.base.total_cmp(&a.1.base));
    lines
}

/// Running headers and footers: band lines that repeat on at least half
/// the pages (page numbers match whatever the number).
pub fn find_furniture(pages: &[PageContent]) -> Furniture {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for p in pages {
        let mut seen = std::collections::HashSet::new();
        for (_, line) in band_lines(p) {
            let key = normalise(&line.text());
            if !key.is_empty() && seen.insert(key.clone()) {
                *counts.entry(key).or_default() += 1;
            }
        }
    }
    let need = (pages.len() / 2).max(2);
    Furniture { drop: counts.into_iter().filter(|(_, n)| *n >= need).map(|(k, _)| k).collect(), ..Default::default() }
}

impl Furniture {
    fn drops(&self, page: &PageContent, line: &Line) -> bool {
        let band = page.height * BAND;
        let in_band = line.base > page.height - band || line.base < band;
        let text = line.text();
        in_band && (self.drop.contains(&normalise(&text)) || is_page_number(&text))
    }

    /// Build the Word header and footer from the repeated lines.
    pub fn place(&mut self, pages: &[PageContent], margins: (f32, f32, f32, f32)) {
        let mut done = std::collections::HashSet::new();
        for p in pages {
            let (left, right) = (margins.0, p.width - margins.1);
            for (top, line) in band_lines(p) {
                let text = line.text();
                let key = normalise(&text);
                if !self.drop.contains(&key) || !done.insert((top, key)) {
                    continue;
                }
                let edge = if top { p.height - line.top() } else { line.base - line.size * 0.25 };
                let dist = if top { &mut self.header_dist } else { &mut self.footer_dist };
                *dist = Some(dist.map_or(edge, |d: f32| d.min(edge)).max(9.0));
                let mut para = finish_para(&ParaDraft { lines: vec![line], bullet_x: None }, &p.fonts, left, right, false);
                para.line = None;
                if is_page_number(&text) {
                    page_number_fields(&mut para);
                }
                if top {
                    self.header.push(para);
                } else {
                    self.footer.push(para);
                }
            }
        }
    }
}

/// "Page 3 of 10" → "Page {PAGE} of {NUMPAGES}".
fn page_number_fields(para: &mut Para) {
    let mut n = 0;
    let mut in_number = false;
    for run in &mut para.runs {
        let mut out = String::new();
        for c in run.text.chars() {
            if c.is_ascii_digit() {
                if !in_number {
                    n += 1;
                    match n {
                        1 => out.push(PAGE),
                        2 => out.push(NUM_PAGES),
                        _ => {}
                    }
                }
                if n > 2 {
                    out.push(c);
                }
                in_number = true;
            } else {
                in_number = false;
                out.push(c);
            }
        }
        run.text = out;
    }
}

/// Text margins shared by all pages: robust minimum of where text starts.
pub fn page_margins(pages: &[PageContent], furniture: &Furniture) -> (f32, f32, f32, f32) {
    let (mut l, mut r, mut t, mut b) = (vec![], vec![], vec![], vec![]);
    for p in pages {
        let lines: Vec<Line> = build_lines(p.glyphs.clone()).into_iter().filter(|l| !furniture.drops(p, l)).collect();
        if lines.len() < 3 {
            continue;
        }
        l.push(lines.iter().map(|x| x.x0).fold(f32::MAX, f32::min));
        r.push(p.width - lines.iter().map(|x| x.x1).fold(f32::MIN, f32::max));
        t.push(p.height - lines.iter().map(Line::top).fold(f32::MIN, f32::max));
        b.push(lines.iter().map(|x| x.base - x.size * 0.25).fold(f32::MAX, f32::min));
    }
    let pick = |v: Vec<f32>, d: f32| median(v).unwrap_or(d).clamp(18.0, 144.0);
    // Pages that end early say nothing about the bottom margin: take the
    // fullest page, less some slack so Word's line metrics don't push the
    // last line onto a page of its own.
    let bottom = b.into_iter().reduce(f32::min).map_or(72.0, |m| m * 0.75).clamp(18.0, 144.0);
    // Likewise the top: the page whose text starts highest; others get the
    // difference as space above their first paragraph.
    let top = t.into_iter().reduce(f32::min).unwrap_or(72.0).clamp(18.0, 144.0);
    (pick(l, 72.0), pick(r, 72.0), top, bottom)
}

/// Find a vertical gutter splitting the page into two text columns.
fn find_gutter(lines: &[Line], width: f32) -> Option<(f32, f32)> {
    if lines.len() < 12 {
        return None;
    }
    let bins = width.ceil() as usize + 1;
    let mut cover = vec![0usize; bins];
    for l in lines {
        let (a, b) = (l.x0.max(0.0) as usize, (l.x1.max(0.0) as usize).min(bins - 1));
        for c in cover.iter_mut().take(b + 1).skip(a) {
            *c += 1;
        }
    }
    let limit = (lines.len() as f32 * 0.06).max(1.0) as usize;
    let (lo, hi) = ((width * 0.3) as usize, (width * 0.7) as usize);
    let mut best: Option<(usize, usize)> = None;
    let mut start: Option<usize> = None;
    for (x, &n) in cover.iter().enumerate().take(hi + 1).skip(lo) {
        if n <= limit {
            start.get_or_insert(x);
        } else if let Some(s) = start.take() {
            if x - s >= 12 && best.is_none_or(|(bs, be)| x - s > be - bs) {
                best = Some((s, x));
            }
        }
    }
    if let Some(s) = start {
        if hi - s >= 12 && best.is_none_or(|(bs, be)| hi - s > be - bs) {
            best = Some((s, hi));
        }
    }
    let (s, e) = best?;
    let (gl, gr) = (s as f32, e as f32);
    let left = lines.iter().filter(|l| l.x1 <= gl + 1.0).count();
    let right = lines.iter().filter(|l| l.x0 >= gr - 1.0).count();
    (left >= 5 && right >= 5).then_some((gl, gr))
}

pub fn layout(page: &PageContent, margins: (f32, f32, f32, f32), furniture: &Furniture) -> PageLayout {
    let (ml, mr, mt, mb) = margins;
    let mut section = Section { page_w: page.width, page_h: page.height, margin_left: ml, margin_right: mr, margin_top: mt, margin_bottom: mb, ..Default::default() };
    if let Some(d) = furniture.header_dist {
        section.header_dist = d.min(mt);
    }
    if let Some(d) = furniture.footer_dist {
        section.footer_dist = d.min(mb);
    }
    let text_left = ml;
    let text_right = page.width - mr;

    // Tables first: their glyphs leave the main flow.
    let grids = find_grids(&page.rules);
    let mut cell_glyphs: Vec<Vec<Glyph>> = vec![vec![]; grids.len()];
    let mut flow: Vec<Glyph> = Vec::new();
    for g in &page.glyphs {
        match grids.iter().position(|grid| grid.contains(g.cx(), g.cy())) {
            Some(i) => cell_glyphs[i].push(g.clone()),
            None => flow.push(g.clone()),
        }
    }

    // A picture covering the page behind real text is a background or scan.
    let text_count = page.glyphs.iter().filter(|g| !g.is_space()).count();
    let images: Vec<usize> = (0..page.images.len())
        .filter(|&i| {
            let im = &page.images[i];
            let area = (im.x1 - im.x0) * (im.y1 - im.y0);
            !(area > page.width * page.height * 0.8 && text_count >= 30)
        })
        .collect();

    let lines: Vec<Line> = build_lines(flow).into_iter().filter(|l| !furniture.drops(page, l)).collect();

    // Two columns? Then cut lines at the gutter and read column by column.
    let gutter = find_gutter(&lines.iter().flat_map(|l| split_line(l, l.size * 1.2)).collect::<Vec<_>>(), page.width);
    let mut placed: Vec<Placed> = Vec::new();
    let mut col_bounds = [(text_left, text_right), (text_left, text_right), (text_left, text_right)];
    let lines_by_col: [Vec<Line>; 3] = match gutter {
        Some((gl, gr)) => {
            col_bounds[1] = (text_left, gl);
            col_bounds[2] = (gr, text_right);
            let mut cols: [Vec<Line>; 3] = [vec![], vec![], vec![]];
            for l in lines {
                for piece in split_line(&l, l.size * 1.2) {
                    let col = if piece.x1 <= gl + 1.0 {
                        1
                    } else if piece.x0 >= gr - 1.0 {
                        2
                    } else {
                        0
                    };
                    cols[col].push(piece);
                }
            }
            // Re-merge pieces of full-width lines that sit on one baseline.
            cols
        }
        None => [lines, vec![], vec![]],
    };
    for (col, lines) in lines_by_col.into_iter().enumerate() {
        let (_, r) = col_bounds[col];
        let mut lines = lines;
        lines.sort_by(|a, b| b.base.total_cmp(&a.base).then(a.x0.total_cmp(&b.x0)));
        // Same-baseline pieces in one column become one line with tabs.
        let mut merged: Vec<Line> = Vec::new();
        for line in lines {
            match merged.last_mut() {
                Some(m) if (m.base - line.base).abs() <= m.size.max(line.size) * 0.35 => {
                    let mut g = m.glyphs.clone();
                    g.extend(line.glyphs);
                    *m = Line::new(g);
                }
                _ => merged.push(line),
            }
        }
        for p in group_paragraphs(merged, r) {
            let (top, bottom) = (p.top(), p.last_base() - p.size() * 0.25);
            placed.push(Placed { unit: Unit::Para(p), top, bottom, column: col as u8 });
        }
    }
    for (i, grid) in grids.iter().enumerate() {
        let column = match gutter {
            Some((gl, _)) if *grid.xs.last().unwrap() <= gl + 1.0 => 1,
            Some((_, gr)) if grid.xs[0] >= gr - 1.0 => 2,
            _ => 0,
        };
        placed.push(Placed { unit: Unit::Table(i), top: grid.ys[0], bottom: *grid.ys.last().unwrap(), column });
    }
    for &i in &images {
        let im = &page.images[i];
        let column = match gutter {
            Some((gl, _)) if im.x1 <= gl + 1.0 => 1,
            Some((_, gr)) if im.x0 >= gr - 1.0 => 2,
            _ => 0,
        };
        placed.push(Placed { unit: Unit::Image(i), top: im.y1, bottom: im.y0, column });
    }

    // Reading order: full-width part above the columns, then left, right,
    // then anything full-width below.
    let columns_top = placed.iter().filter(|p| p.column != 0).map(|p| p.top).fold(f32::MIN, f32::max);
    let rank = |p: &Placed| -> u8 {
        match p.column {
            0 if p.top > columns_top => 0,
            1 => 1,
            2 => 2,
            _ => 3,
        }
    };
    placed.sort_by(|a, b| rank(a).cmp(&rank(b)).then(b.top.total_cmp(&a.top)));

    let mut head = Vec::new();
    let mut blocks = Vec::new();
    // Bottom of the last block in each reading-order group; the groups
    // above and in the columns start at the top margin.
    let text_top = page.height - mt;
    let mut prev_bottom: [Option<f32>; 4] = [Some(text_top), Some(text_top), Some(text_top), None];
    for p in placed {
        let r = rank(&p);
        let (l, rgt) = col_bounds[if r == 1 || r == 2 { r as usize } else { 0 }];
        let gap_above = prev_bottom[r as usize].map_or(0.0, |bottom| (bottom - p.top).max(0.0));
        let block = match p.unit {
            Unit::Para(draft) => {
                let mut para = finish_para(&draft, &page.fonts, l, rgt, false);
                para.space_before = gap_above.min(72.0) * 0.9;
                Block::Para(para)
            }
            Unit::Table(i) => Block::Table(build_table(&grids[i], std::mem::take(&mut cell_glyphs[i]), &page.fonts, l)),
            Unit::Image(i) => {
                let im = &page.images[i];
                let (w, h) = (im.x1 - im.x0, im.y1 - im.y0);
                let mut para = Para { space_before: gap_above.min(72.0), ..Default::default() };
                let centre = (im.x0 + im.x1) / 2.0;
                if (centre - (l + rgt) / 2.0).abs() < 12.0 && w < (rgt - l) * 0.9 {
                    para.align = Align::Center;
                } else {
                    para.indent_left = (im.x0 - l).max(0.0);
                }
                Block::Picture { image: i, w, h, para }
            }
        };
        prev_bottom[r as usize] = Some(p.bottom);
        if gutter.is_some() && r == 0 {
            head.push(block);
        } else {
            blocks.push(block);
        }
    }

    join_split_paragraphs(&mut head);
    join_split_paragraphs(&mut blocks);
    if let Some((gl, gr)) = gutter {
        section.columns = 2;
        section.column_gap = (gr - gl).max(12.0);
    }
    PageLayout { section, head, blocks, text_chars: text_count }
}

fn build_table(grid: &Grid, glyphs: Vec<Glyph>, fonts: &[String], left: f32) -> Table {
    let (cols, rows) = (grid.xs.len() - 1, grid.ys.len() - 1);
    let mut cell_glyphs: Vec<Vec<Vec<Glyph>>> = vec![vec![vec![]; cols]; rows];
    for g in glyphs {
        let (x, y) = (g.cx(), g.cy());
        let col = grid.xs.windows(2).position(|w| x >= w[0] - 1.0 && x <= w[1] + 1.0);
        let row = grid.ys.windows(2).position(|w| y <= w[0] + 1.0 && y >= w[1] - 1.0);
        if let (Some(c), Some(r)) = (col, row) {
            cell_glyphs[r][c].push(g);
        }
    }
    let mut table = Table {
        col_widths: grid.xs.windows(2).map(|w| w[1] - w[0]).collect(),
        rows: Vec::with_capacity(rows),
        indent_left: (grid.xs[0] - left).max(0.0),
    };
    for (r, row_glyphs) in cell_glyphs.into_iter().enumerate() {
        let mut row: Vec<Cell> = Vec::new();
        let mut pending: Vec<Glyph> = Vec::new();
        let mut span = 0u32;
        let mut cell_left = grid.xs[0];
        for (c, glyphs) in row_glyphs.into_iter().enumerate() {
            pending.extend(glyphs);
            span += 1;
            let closes = c + 1 == cols || grid.has_border(c + 1, r);
            if closes {
                let cell_right = grid.xs[c + 1];
                let lines = build_lines(std::mem::take(&mut pending));
                let paras = group_paragraphs(lines, cell_right - 2.0)
                    .iter()
                    .map(|d| finish_para(d, fonts, cell_left + 2.0, cell_right - 2.0, true))
                    .collect();
                row.push(Cell { span, paras });
                span = 0;
                cell_left = cell_right;
            }
        }
        table.rows.push(row);
    }
    table
}

// ---------------------------------------------------------------- document

/// Body text size (most common) and the heading levels by size.
pub struct Typography {
    body: f32,
    heading_sizes: Vec<f32>,
}

pub fn typography(pages: &[PageLayout]) -> Typography {
    let mut weights: HashMap<i32, usize> = HashMap::new();
    let mut each = |p: &Para| {
        let n: usize = p.runs.iter().map(|r| r.text.chars().count()).sum();
        if let Some(r) = p.runs.first() {
            *weights.entry((r.size * 2.0).round() as i32).or_default() += n;
        }
    };
    for page in pages {
        for b in page.head.iter().chain(&page.blocks) {
            if let Block::Para(p) = b {
                each(p);
            }
        }
    }
    let body = weights.iter().max_by_key(|&(s, n)| (*n, -*s)).map(|(s, _)| *s as f32 / 2.0).unwrap_or(11.0);
    let mut sizes: Vec<i32> = Vec::new();
    for page in pages {
        for b in page.head.iter().chain(&page.blocks) {
            if let Block::Para(p) = b {
                if let Some(s) = heading_size(p, body) {
                    let s = s.round() as i32;
                    if !sizes.contains(&s) {
                        sizes.push(s);
                    }
                }
            }
        }
    }
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    Typography { body, heading_sizes: sizes.into_iter().map(|s| s as f32).collect() }
}

fn heading_size(p: &Para, body: f32) -> Option<f32> {
    let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
    let chars = text.chars().count();
    let size = p.runs.iter().map(|r| r.size).fold(0.0, f32::max);
    // Large text that reads as a sentence or a quotation is a pull quote or
    // lead-in, not a heading.
    let quotes: &[char] = &['"', '\'', '“', '”', '‘', '’', '«', '»'];
    let t = text.trim().trim_end_matches(quotes);
    let sentence = t.ends_with(['.', '!', '?']) && t.split_whitespace().count() >= 4;
    let quoted = text.trim_start().starts_with(quotes);
    (p.bullet.is_none() && chars > 0 && chars <= 160 && size >= body * 1.18 && !sentence && !quoted).then_some(size)
}

pub fn apply_headings(page: &mut PageLayout, t: &Typography) {
    for b in page.head.iter_mut().chain(page.blocks.iter_mut()) {
        if let Block::Para(p) = b {
            if let Some(s) = heading_size(p, t.body) {
                let level = t.heading_sizes.iter().position(|&h| (h - s.round()).abs() < 0.5).unwrap_or(2) + 1;
                p.heading = Some(level.min(3) as u8);
            }
        }
    }
}

fn edge_char(p: &Para, last: bool) -> Option<char> {
    if last {
        p.runs.iter().rev().flat_map(|r| r.text.chars().rev()).find(|c| !c.is_whitespace())
    } else {
        p.runs.iter().flat_map(|r| r.text.chars()).find(|c| !c.is_whitespace())
    }
}

/// Whether `next` carries on the sentence `prev` broke off (a paragraph cut
/// by a column or page break).
fn continues(prev: &Para, next: &Para) -> bool {
    if prev.heading.is_some() || next.heading.is_some() || next.bullet.is_some() {
        return false;
    }
    let (Some(end), Some(start), Some(a), Some(b)) = (edge_char(prev, true), edge_char(next, false), prev.runs.last(), next.runs.first())
    else {
        return false;
    };
    (a.size - b.size).abs() < 0.6 && start.is_lowercase() && (end.is_alphanumeric() || matches!(end, ',' | '-' | '\u{AD}'))
}

fn join(prev: &mut Para, next: Para) {
    if let Some(last) = prev.runs.last_mut() {
        let t = last.text.trim_end();
        last.text = match t.strip_suffix(['-', '\u{AD}']) {
            Some(stem) if stem.ends_with(char::is_alphabetic) => stem.to_string(),
            _ => format!("{t} "),
        };
    }
    prev.runs.extend(next.runs);
}

/// Join paragraphs split by a column break within the blocks.
fn join_split_paragraphs(blocks: &mut Vec<Block>) {
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    for b in blocks.drain(..) {
        match (out.last_mut(), b) {
            (Some(Block::Para(prev)), Block::Para(next)) if continues(prev, &next) => join(prev, next),
            (_, b) => out.push(b),
        }
    }
    *blocks = out;
}

/// Join paragraphs that run on from one page to the next.
pub fn join_across_pages(pages: &mut [PageLayout]) {
    for i in 1..pages.len() {
        let (before, after) = pages.split_at_mut(i);
        let prev_page = &mut before[i - 1];
        let next_page = &mut after[0];
        let prev = if prev_page.blocks.is_empty() { prev_page.head.last_mut() } else { prev_page.blocks.last_mut() };
        let next_list = if next_page.head.is_empty() { &mut next_page.blocks } else { &mut next_page.head };
        if let (Some(Block::Para(prev)), Some(Block::Para(next))) = (prev, next_list.first()) {
            if continues(prev, next) {
                let Block::Para(next) = next_list.remove(0) else { unreachable!() };
                join(prev, next);
            }
        }
    }
}

pub fn write_page(builder: &mut Builder, page: &PageLayout, images: &mut [Option<(Vec<u8>, Picture)>]) {
    let mut emit = |builder: &mut Builder, blocks: &[Block]| {
        for b in blocks {
            match b {
                Block::Para(p) => builder.paragraph(p),
                Block::Table(t) => builder.table(t),
                Block::Picture { image, w, h, para } => {
                    if let Some((data, kind)) = images.get_mut(*image).and_then(Option::take) {
                        builder.picture(data, kind, *w, *h, para);
                    }
                }
            }
        }
    };
    if !page.head.is_empty() {
        let mut one = page.section;
        one.columns = 1;
        builder.section(one);
        emit(builder, &page.head);
        builder.end_section(false);
        let mut two = page.section;
        two.start_continuous = true;
        builder.section(two);
    } else {
        builder.section(page.section);
    }
    emit(builder, &page.blocks);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(c: char, x: f32, base: f32, size: f32) -> Glyph {
        Glyph { c, x0: x, x1: x + size * 0.5, base, size, font: 0, bold: false, italic: false, color: [0, 0, 0], vert: 0 }
    }

    fn word(s: &str, x: f32, base: f32, size: f32) -> Vec<Glyph> {
        s.chars().enumerate().map(|(i, c)| g(c, x + i as f32 * size * 0.5, base, size)).collect()
    }

    #[test]
    fn font_names_are_cleaned() {
        assert_eq!(clean_font_name("ABCDEF+TimesNewRomanPS-BoldMT"), "Times New Roman");
        assert_eq!(clean_font_name("LiberationSerif-Italic"), "Liberation Serif");
        assert_eq!(clean_font_name("Calibri,Bold"), "Calibri");
        assert_eq!(clean_font_name("ArialMT"), "Arial");
    }

    #[test]
    fn wrapped_lines_join_and_paragraphs_split() {
        let mut glyphs = Vec::new();
        // Paragraph 1: two full lines and a short last line.
        glyphs.extend(word("aaaa bbbb cccc dddd eeee ffff gggg", 72.0, 700.0, 10.0));
        glyphs.extend(word("hhhh iiii jjjj kkkk llll mmmm nnnn", 72.0, 688.0, 10.0));
        glyphs.extend(word("oooo", 72.0, 676.0, 10.0));
        // Paragraph 2 after a blank line.
        glyphs.extend(word("next paragraph here", 72.0, 652.0, 10.0));
        let lines = build_lines(glyphs);
        assert_eq!(lines.len(), 4);
        let paras = group_paragraphs(lines, 72.0 + 34.0 * 5.0);
        assert_eq!(paras.len(), 2);
        let p = finish_para(&paras[0], &["Arial".into()], 72.0, 72.0 + 34.0 * 5.0, false);
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        assert!(text.contains("gggg hhhh"), "{text}");
        assert!(text.ends_with("oooo"));
    }

    #[test]
    fn bold_runs_survive() {
        let mut glyphs = word("plain ", 72.0, 700.0, 10.0);
        let mut bold = word("strong", 102.0, 700.0, 10.0);
        bold.iter_mut().for_each(|g| g.bold = true);
        glyphs.extend(bold);
        let paras = group_paragraphs(build_lines(glyphs), 300.0);
        let p = finish_para(&paras[0], &["Arial".into()], 72.0, 300.0, false);
        assert!(p.runs.iter().any(|r| r.bold && r.text == "strong"), "{:?}", p.runs);
        assert!(p.runs.iter().any(|r| !r.bold && r.text.starts_with("plain")));
    }

    #[test]
    fn bullets_become_list_items() {
        let mut glyphs = vec![g('•', 72.0, 700.0, 10.0)];
        glyphs.extend(word("first item", 90.0, 700.0, 10.0));
        glyphs.push(g('•', 72.0, 686.0, 10.0));
        glyphs.extend(word("second item", 90.0, 686.0, 10.0));
        let paras = group_paragraphs(build_lines(glyphs), 400.0);
        assert_eq!(paras.len(), 2);
        let p = finish_para(&paras[0], &["Arial".into()], 72.0, 400.0, false);
        assert_eq!(p.bullet, Some(0));
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(text, "first item");
    }

    #[test]
    fn grid_from_rules() {
        let mut rules = Vec::new();
        for y in [700.0, 680.0, 660.0] {
            rules.push(Rule::H { y, x0: 72.0, x1: 372.0 });
        }
        for x in [72.0, 222.0, 372.0] {
            rules.push(Rule::V { x, y0: 660.0, y1: 700.0 });
        }
        let grids = find_grids(&rules);
        assert_eq!(grids.len(), 1);
        assert_eq!(grids[0].xs.len(), 3);
        assert_eq!(grids[0].ys.len(), 3);
        let mut glyphs = word("Region", 76.0, 686.0, 10.0);
        glyphs.extend(word("Q3", 226.0, 686.0, 10.0));
        glyphs.extend(word("North", 76.0, 666.0, 10.0));
        let t = build_table(&grids[0], glyphs, &["Arial".into()], 72.0);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0].len(), 2);
        let first: String = t.rows[0][0].paras[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(first, "Region");
    }

    #[test]
    fn page_numbers_are_furniture() {
        assert!(is_page_number("12"));
        assert!(is_page_number("Page 3"));
        assert!(is_page_number("- 4 -"));
        assert!(!is_page_number("2. Highlights"));
    }
}
