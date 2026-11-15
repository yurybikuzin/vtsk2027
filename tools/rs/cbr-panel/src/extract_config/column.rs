use anyhow::{Context, Result};
use calamine::{open_workbook_auto, Data, DataType, Reader};
use std::path::Path;

use super::config::Series;
use super::resolve_path;

pub fn read_column_series(root: &Path, series: &Series) -> Result<Vec<(String, f64, usize)>> {
    let path = resolve_path(root, &series.path);
    let column = series
        .column
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("column is required for date_row_column"))?;
    let col_idx =
        column_index(column).ok_or_else(|| anyhow::anyhow!("invalid column '{}'", column))?;

    log::info!(
        "series {} -> {} sheet '{}' column {}",
        series.series_id,
        path.display(),
        series.sheet,
        column
    );

    let mut wb = open_workbook_auto(&path).with_context(|| format!("open {}", path.display()))?;
    let range = wb.worksheet_range(&series.sheet)?;

    let mut out = Vec::new();
    for row in 0..range.height() {
        let first = match range.get((row, 0)).and_then(|c| c.as_string()) {
            Some(s) => s,
            None => continue,
        };
        let (y, m) = match parse_month_full_ru(&first) {
            Some(v) => v,
            None => continue,
        };
        let value = match range.get((row, col_idx)).and_then(cell_f64) {
            Some(v) => v,
            None => continue,
        };
        out.push((format!("{:04}-{:02}", y, m), value, col_idx));
    }

    if out.is_empty() {
        anyhow::bail!(
            "no numeric values in column {} of sheet '{}' in {}",
            column,
            series.sheet,
            path.display()
        );
    }
    log::info!("{}: {} rows in column mode", series.series_id, out.len());
    Ok(out)
}

pub fn column_index(column: &str) -> Option<usize> {
    let c = column.trim();
    if c.is_empty() {
        return None;
    }
    let mut idx = 0usize;
    for ch in c.chars() {
        let up = ch.to_ascii_uppercase();
        if !up.is_ascii_alphabetic() {
            return None;
        }
        idx = idx * 26 + (up as usize - 'A' as usize + 1);
    }
    Some(idx - 1)
}

pub fn parse_month_full_ru(s: &str) -> Option<(i32, i32)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut parts = s.split_whitespace();
    let month_word = parts.next()?;
    let year_word = parts.next()?;
    let year: i32 = year_word.parse().ok()?;
    let m = match month_word.to_lowercase().as_str() {
        "январь" | "января" => 1,
        "февраль" | "февраля" => 2,
        "март" | "марта" => 3,
        "апрель" | "апреля" => 4,
        "май" | "мая" => 5,
        "июнь" | "июня" => 6,
        "июль" | "июля" => 7,
        "август" | "августа" => 8,
        "сентябрь" | "сентября" => 9,
        "октябрь" | "октября" => 10,
        "ноябрь" | "ноября" => 11,
        "декабрь" | "декабря" => 12,
        _ => return None,
    };
    if !(1900..=2100).contains(&year) {
        return None;
    }
    Some((year, m))
}

pub fn cell_f64(d: &Data) -> Option<f64> {
    match d {
        Data::Float(v) => Some(*v),
        Data::Int(v) => Some(*v as f64),
        Data::String(s) => s.trim().replace(',', ".").parse::<f64>().ok(),
        _ => None,
    }
}
