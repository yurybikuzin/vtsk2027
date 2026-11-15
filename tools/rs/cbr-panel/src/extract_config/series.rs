use anyhow::{Context, Result};
use calamine::{open_workbook_auto, Data, DataType, Reader};
use std::path::Path;

use super::config::Series;
use super::resolve_path;

pub struct Row {
    pub col: usize,
    pub value: f64,
}

pub struct SeriesValues {
    pub label_row: usize,
    pub rows: Vec<Row>,
}

pub fn read_series(root: &Path, series: &Series) -> Result<SeriesValues> {
    let path = resolve_path(root, &series.path);
    let row_label = series
        .row_label
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("row_label is required for non-column styles"))?;
    log::info!("series {} -> {}", series.series_id, path.display());
    let mut wb = open_workbook_auto(&path).with_context(|| format!("open {}", path.display()))?;
    let sheets = wb.sheet_names().to_vec();
    if !sheets.iter().any(|s| s == &series.sheet) {
        anyhow::bail!(
            "sheet '{}' not found in {}, have {:?}",
            series.sheet,
            path.display(),
            sheets
        );
    }
    let range = wb.worksheet_range(&series.sheet)?;

    let (start, end) = match &series.section {
        Some(sec) => section_bounds(&range, sec).with_context(|| {
            format!(
                "section '{}' not found in sheet '{}' of {}",
                sec,
                series.sheet,
                path.display()
            )
        })?,
        None => (0, range.height()),
    };

    let target = row_label.trim().to_lowercase();
    let mut found: Option<(usize, usize)> = None;
    for i in start..end {
        let row = range.rows().nth(i).unwrap();
        for (j, cell) in row.iter().enumerate() {
            if let Some(s) = cell.as_string() {
                if s.trim().to_lowercase() == target {
                    found = Some((i, j));
                    break;
                }
            }
        }
        if found.is_some() {
            break;
        }
    }
    let (label_row, label_col) = found.with_context(|| {
        format!(
            "row_label '{}' not found in section {:?} of sheet '{}' in {}",
            row_label,
            series.section,
            series.sheet,
            path.display()
        )
    })?;

    let mut rows = Vec::new();
    for (col, cell) in range.rows().nth(label_row).unwrap().iter().enumerate() {
        if col <= label_col {
            continue;
        }
        match cell {
            Data::Float(v) => rows.push(Row { col, value: *v }),
            Data::Int(v) => rows.push(Row {
                col,
                value: *v as f64,
            }),
            _ => {}
        }
    }
    if rows.is_empty() {
        anyhow::bail!(
            "no numeric values right of label in {} sheet {}",
            path.display(),
            series.sheet
        );
    }
    Ok(SeriesValues { label_row, rows })
}

fn section_bounds(range: &calamine::Range<Data>, section: &str) -> Option<(usize, usize)> {
    let mut start = None;
    for r in 0..range.height() {
        if let Some(s) = range.get((r, 0)).and_then(|c| c.as_string()) {
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

fn is_section_header(range: &calamine::Range<Data>, r: usize) -> bool {
    if range.get((r, 0)).and_then(|c| c.as_string()).is_none() {
        return false;
    }
    for c in 1..range.width() {
        if range.get((r, c)).and_then(|c| c.as_string()).is_some() {
            return false;
        }
    }
    true
}
