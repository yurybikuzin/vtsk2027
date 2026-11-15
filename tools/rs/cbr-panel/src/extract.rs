use crate::excel::{cell_f64, cell_str, parse_ym_any, quarter_from_ym, LongRowXlsx};
use calamine::{Data, Range};

pub struct ExtractSpec {
    pub id: String,
    pub label: String,
    pub section: Option<String>,
    pub monthly: bool,
    pub unit: String,
}

pub fn extract_rows(
    range: &Range<Data>,
    specs: &[ExtractSpec],
    source_file: &str,
    source_sheet: &str,
) -> Vec<LongRowXlsx> {
    let mut out = Vec::new();
    for spec in specs {
        let (start, end) = match &spec.section {
            None => (0, range.height()),
            Some(sec) => match section_bounds(range, sec) {
                Some(b) => b,
                None => {
                    eprintln!("[WARN] section not found: {}", sec);
                    continue;
                }
            },
        };
        let row = match find_row_in(range, start, end, &spec.label) {
            Some(r) => r,
            None => {
                eprintln!(
                    "[WARN] row not found: {} (section {:?})",
                    spec.label, spec.section
                );
                continue;
            }
        };
        let header = match find_header_above(range, row) {
            Some(h) => h,
            None => {
                eprintln!("[WARN] header not found above row {} ({})", row, spec.label);
                continue;
            }
        };
        let mut n = 0usize;
        for col in 1..range.width() {
            let ym = match range
                .get((header, col))
                .and_then(cell_str)
                .as_deref()
                .and_then(parse_ym_any)
            {
                Some(v) => v,
                None => continue,
            };
            let v = match range.get((row, col)).and_then(cell_f64) {
                Some(v) => v,
                None => continue,
            };
            let period = if spec.monthly {
                format!("{:04}-{:02}", ym.0, ym.1)
            } else {
                quarter_from_ym(ym.0, ym.1)
            };
            out.push(LongRowXlsx {
                series_id: spec.id.clone(),
                period,
                value: v,
                unit: spec.unit.clone(),
                source_file: source_file.to_string(),
                source_sheet: source_sheet.to_string(),
                source_row: row as u32,
                source_col: col as u32,
            });
            n += 1;
        }
        eprintln!("[OK] {} -> {} values", spec.id, n);
    }
    out
}

fn find_row_in(range: &Range<Data>, start: usize, end: usize, label: &str) -> Option<usize> {
    for r in start..end {
        if let Some(s) = range.get((r, 0)).and_then(cell_str) {
            if s.trim() == label {
                return Some(r);
            }
        }
    }
    None
}

fn section_bounds(range: &Range<Data>, section: &str) -> Option<(usize, usize)> {
    let mut start = None;
    for r in 0..range.height() {
        if let Some(s) = range.get((r, 0)).and_then(cell_str) {
            let t = s.trim();
            if start.is_none() && t.starts_with(section) {
                start = Some(r + 1);
                continue;
            }
            if start.is_some() && is_section_header(range, r) {
                return Some((start.unwrap(), r));
            }
        }
    }
    start.map(|s| (s, range.height()))
}

fn is_section_header(range: &Range<Data>, r: usize) -> bool {
    if range.get((r, 0)).and_then(cell_str).is_none() {
        return false;
    }
    for c in 1..range.width() {
        if range.get((r, c)).and_then(cell_str).is_some() {
            return false;
        }
    }
    true
}

fn find_header_above(range: &Range<Data>, data_row: usize) -> Option<usize> {
    for hr in (0..data_row).rev() {
        let mut hits = 0;
        for col in 1..range.width() {
            if let Some(s) = range.get((hr, col)).and_then(cell_str) {
                if parse_ym_any(&s).is_some() {
                    hits += 1;
                    if hits >= 3 {
                        return Some(hr);
                    }
                }
            }
        }
    }
    None
}
