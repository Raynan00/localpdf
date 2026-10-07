//! Page range parsing: "1-3, 5, 8-" style, 1-based, validated against a page count.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RangeError {
    #[error("enter at least one page, like 1-3, 5")]
    Empty,
    #[error("\"{0}\" isn't a page or range. Use numbers like 2, 4-6 or 8-")]
    Syntax(String),
    #[error("pages start at 1, not 0")]
    Zero,
    #[error("page {page} doesn't exist, the document has {total} page{s}", s = if *.total == 1 { "" } else { "s" })]
    OutOfBounds { page: u32, total: u32 },
    #[error("{start}-{end} runs backwards. Did you mean {end}-{start}?")]
    Reversed { start: u32, end: u32 },
}

/// One comma-separated group, expanded to 1-based page numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageGroup {
    pub label: String,
    pub pages: Vec<u32>,
}

/// Parse `spec` against a document with `total` pages.
///
/// Accepts `5`, `2-4`, `8-` (to the end), `-3` (from the start) and `last`,
/// separated by commas or semicolons. Whitespace is ignored.
pub fn parse(spec: &str, total: u32) -> Result<Vec<PageGroup>, RangeError> {
    let mut groups = Vec::new();
    for raw in spec.split([',', ';']) {
        let token: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
        if token.is_empty() {
            continue;
        }
        let token = token.replace(['–', '—'], "-");
        let (start, end) = match token.split_once('-') {
            None => {
                let p = page_number(&token, total, &token)?;
                (p, p)
            }
            Some((a, b)) => {
                let start = if a.is_empty() { 1 } else { page_number(a, total, &token)? };
                let end = if b.is_empty() { total } else { page_number(b, total, &token)? };
                (start, end)
            }
        };
        if start > end {
            return Err(RangeError::Reversed { start, end });
        }
        let label = if start == end { start.to_string() } else { format!("{start}-{end}") };
        groups.push(PageGroup { label, pages: (start..=end).collect() });
    }
    if groups.is_empty() {
        return Err(RangeError::Empty);
    }
    Ok(groups)
}

fn page_number(s: &str, total: u32, token: &str) -> Result<u32, RangeError> {
    if s.eq_ignore_ascii_case("last") || s.eq_ignore_ascii_case("end") {
        return Ok(total);
    }
    let n: u32 = s.parse().map_err(|_| RangeError::Syntax(token.to_string()))?;
    if n == 0 {
        return Err(RangeError::Zero);
    }
    if n > total {
        return Err(RangeError::OutOfBounds { page: n, total });
    }
    Ok(n)
}

/// All pages of all groups in order, without duplicates.
pub fn flatten(groups: &[PageGroup]) -> Vec<u32> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for p in groups.iter().flat_map(|g| g.pages.iter().copied()) {
        if seen.insert(p) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed() {
        let g = parse("1-3, 5, 8-", 10).unwrap();
        assert_eq!(g.len(), 3);
        assert_eq!(g[0].pages, vec![1, 2, 3]);
        assert_eq!(g[1].pages, vec![5]);
        assert_eq!(g[2].pages, vec![8, 9, 10]);
        assert_eq!(g[2].label, "8-10");
    }

    #[test]
    fn open_start_and_last() {
        let g = parse("-2; last", 4).unwrap();
        assert_eq!(g[0].pages, vec![1, 2]);
        assert_eq!(g[1].pages, vec![4]);
    }

    #[test]
    fn errors() {
        assert_eq!(parse("  , ", 3), Err(RangeError::Empty));
        assert_eq!(parse("0", 3), Err(RangeError::Zero));
        assert_eq!(parse("4", 3), Err(RangeError::OutOfBounds { page: 4, total: 3 }));
        assert_eq!(parse("3-1", 3), Err(RangeError::Reversed { start: 3, end: 1 }));
        assert!(matches!(parse("a-b", 3), Err(RangeError::Syntax(_))));
        assert!(matches!(parse("1-2-3", 3), Err(RangeError::Syntax(_))));
    }

    #[test]
    fn flatten_dedupes() {
        let g = parse("1-3,2,5", 5).unwrap();
        assert_eq!(flatten(&g), vec![1, 2, 3, 5]);
    }
}
