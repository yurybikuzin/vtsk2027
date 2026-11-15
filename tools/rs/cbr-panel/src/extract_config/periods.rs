use anyhow::{Context, Result};
use calamine::{open_workbook_auto, Data, DataType, Reader};
use chrono::Datelike;
use regex::Regex;
use std::path::Path;

use super::config::Series;
use super::resolve_path;

pub fn resolve_periods(root: &Path, series: &Series) -> Result<Vec<(usize, String)>> {
    let path = resolve_path(root, &series.path);
    let mut wb = open_workbook_auto(&path)?;
    let range = wb.worksheet_range(&series.sheet)?;

    match series.period_style.as_str() {
        "quarter_short" => resolve_quarter_short(&range, series),
        "roman_quarter" => resolve_roman_quarter(&range, series),
        other => resolve_single_cell(&range, series, other),
    }
}

fn resolve_quarter_short(
    range: &calamine::Range<Data>,
    series: &Series,
) -> Result<Vec<(usize, String)>> {
    let q_re = Regex::new(r"^\s*(\d)к\s*$")?;
    let y_re = Regex::new(r"^\s*(\d{4})\s*$")?;

    let mut q_row: Option<usize> = None;
    for (i, row) in range.rows().enumerate() {
        let hits = row
            .iter()
            .filter(|c| {
                c.as_string()
                    .map(|s| q_re.is_match(s.trim()))
                    .unwrap_or(false)
            })
            .count();
        if hits >= 3 {
            q_row = Some(i);
            break;
        }
    }
    let q_row = q_row.context("quarter row not found")?;

    let y_row =
        find_year_row_above(range, q_row, &y_re).context("year row not found above quarter row")?;

    let q_cells = range.rows().nth(q_row).unwrap();
    let years = spread_years(range.rows().nth(y_row).unwrap(), q_cells.len(), &y_re);

    let mut out = Vec::new();
    for (j, cell) in q_cells.iter().enumerate() {
        let q = match cell.as_string() {
            Some(s) => match q_re.captures(s.trim()) {
                Some(c) => c.get(1).unwrap().as_str().parse::<u32>().ok(),
                None => None,
            },
            None => None,
        };
        if let (Some(q), Some(y)) = (q, years[j]) {
            out.push((j, format!("{}Q{}", y, q)));
        }
    }
    if out.is_empty() {
        anyhow::bail!("no periods reconstructed for {}", series.series_id);
    }
    Ok(out)
}

fn resolve_roman_quarter(
    range: &calamine::Range<Data>,
    series: &Series,
) -> Result<Vec<(usize, String)>> {
    let q_re = Regex::new(r"^\s*([IVX]+)\s*кв\.?\s*$")?;
    let y_re = Regex::new(r"^\s*(\d{4})\s*$")?;

    let mut q_row: Option<usize> = None;
    for (i, row) in range.rows().enumerate() {
        let hits = row
            .iter()
            .filter(|c| {
                c.as_string()
                    .map(|s| q_re.is_match(s.trim()))
                    .unwrap_or(false)
            })
            .count();
        if hits >= 3 {
            q_row = Some(i);
            break;
        }
    }
    let q_row = q_row.context("roman quarter row not found")?;

    let y_row = find_year_row_above(range, q_row, &y_re)
        .context("year row not found above roman quarter row")?;

    let q_cells = range.rows().nth(q_row).unwrap();
    let years = spread_years(range.rows().nth(y_row).unwrap(), q_cells.len(), &y_re);

    let mut out = Vec::new();
    for (j, cell) in q_cells.iter().enumerate() {
        let q = match cell.as_string() {
            Some(s) => match q_re.captures(s.trim()) {
                Some(c) => roman_to_q(c.get(1).unwrap().as_str()),
                None => None,
            },
            None => None,
        };
        if let (Some(q), Some(y)) = (q, years[j]) {
            out.push((j, format!("{}Q{}", y, q)));
        }
    }
    if out.is_empty() {
        anyhow::bail!("no periods reconstructed for {}", series.series_id);
    }
    Ok(out)
}

fn resolve_single_cell(
    range: &calamine::Range<Data>,
    series: &Series,
    style: &str,
) -> Result<Vec<(usize, String)>> {
    let pattern = series
        .period_pattern
        .clone()
        .unwrap_or_else(|| default_pattern(style).to_string());
    let re = Regex::new(&pattern)?;

    let is_date_like = style == "date" || style == "month";

    let mut header_row: Option<(usize, Vec<Data>)> = None;
    for (i, row) in range.rows().enumerate() {
        let hits = row
            .iter()
            .filter(|c| {
                let mut ok = c
                    .as_string()
                    .map(|s| re.is_match(s.trim()))
                    .unwrap_or(false);
                if !ok && is_date_like {
                    if let Some(s) = c.as_string() {
                        ok = if style == "month" {
                            parse_ddmmyyyy_month(&s).is_some()
                        } else {
                            parse_ddmmyyyy_period(&s).is_some()
                        };
                    }
                }
                if !ok && is_date_like {
                    if let Data::DateTime(_) = c {
                        ok = true;
                    }
                }
                ok
            })
            .count();
        if hits >= 1 {
            header_row = Some((i, row.to_vec()));
            break;
        }
    }
    let (header_idx, header_row) = match header_row {
        Some(r) => r,
        None => {
            let mut dump = String::new();
            for (i, row) in range.rows().enumerate().take(10) {
                let cells: Vec<String> = row
                    .iter()
                    .enumerate()
                    .filter_map(|(j, c)| c.as_string().map(|s| format!("[{}]={:?}", j, s)))
                    .collect();
                if !cells.is_empty() {
                    dump.push_str(&format!("row {}: {}\n", i, cells.join(" ")));
                }
            }
            anyhow::bail!("header row not found; first rows:\n{}", dump);
        }
    };
    log::info!(
        "{}: header row at {} (1-based {})",
        series.series_id,
        header_idx,
        header_idx + 1
    );

    let mut out = Vec::new();
    for (col, cell) in header_row.iter().enumerate() {
        if let Some(p) = cell_to_period(cell, style, &re) {
            out.push((col, p));
        }
    }
    if out.is_empty() {
        anyhow::bail!("no periods reconstructed for {}", series.series_id);
    }
    Ok(out)
}

fn cell_to_period(cell: &Data, style: &str, re: &Regex) -> Option<String> {
    if let Some(s) = cell.as_string() {
        let s = s.trim();
        if let Some(caps) = re.captures(s) {
            if let Some(p) = convert(style, &caps) {
                return Some(p);
            }
        }
        if style == "date" {
            if let Some(p) = parse_ddmmyyyy_period(s) {
                return Some(p);
            }
        }
        if style == "month" {
            if let Some(p) = parse_ddmmyyyy_month(s) {
                return Some(p);
            }
        }
    }
    if style == "date" {
        if let Data::DateTime(dt) = cell {
            let dt = dt.as_datetime()?;
            let y = dt.year();
            let m = dt.month();
            let q = (m - 1) / 3 + 1;
            return Some(format!("{}Q{}", y, q));
        }
    }
    if style == "month" {
        if let Data::DateTime(dt) = cell {
            let dt = dt.as_datetime()?;
            return Some(format!("{:04}-{:02}", dt.year(), dt.month()));
        }
    }
    None
}

fn parse_ddmmyyyy_period(s: &str) -> Option<String> {
    let s = s.trim_end_matches('*').trim();
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let d: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let y: u32 = parts[2].parse().ok()?;
    if !(1..=31).contains(&d) || !(1..=12).contains(&m) || !(1900..=2100).contains(&y) {
        return None;
    }
    let q = (m - 1) / 3 + 1;
    Some(format!("{}Q{}", y, q))
}

fn parse_ddmmyyyy_month(s: &str) -> Option<String> {
    let s = s.trim_end_matches('*').trim();
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let d: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let y: u32 = parts[2].parse().ok()?;
    if !(1..=31).contains(&d) || !(1..=12).contains(&m) || !(1900..=2100).contains(&y) {
        return None;
    }
    Some(format!("{:04}-{:02}", y, m))
}

fn find_year_row_above(range: &calamine::Range<Data>, q_row: usize, y_re: &Regex) -> Option<usize> {
    if q_row == 0 {
        return None;
    }
    for i in (0..q_row).rev() {
        let hits = range
            .rows()
            .nth(i)
            .unwrap()
            .iter()
            .filter(|c| {
                c.as_string()
                    .map(|s| y_re.is_match(s.trim()))
                    .unwrap_or(false)
            })
            .count();
        if hits >= 1 {
            return Some(i);
        }
    }
    None
}

fn spread_years(y_cells: &[Data], len: usize, y_re: &Regex) -> Vec<Option<u32>> {
    let mut years = vec![None; len];
    let mut cur: Option<u32> = None;
    for (j, cell) in y_cells.iter().enumerate().take(len) {
        if let Some(s) = cell.as_string() {
            if let Some(caps) = y_re.captures(s.trim()) {
                if let Ok(y) = caps.get(1).unwrap().as_str().parse::<u32>() {
                    cur = Some(y);
                }
            }
        }
        years[j] = cur;
    }
    years
}

fn roman_to_q(s: &str) -> Option<u32> {
    match s.trim().to_uppercase().as_str() {
        "I" => Some(1),
        "II" => Some(2),
        "III" => Some(3),
        "IV" => Some(4),
        _ => None,
    }
}

fn default_pattern(style: &str) -> &'static str {
    match style {
        "quarter_ru" => r"^\s*(\d)\s*кв\s*(\d{4})\s*$",
        "roman" => r"^\s*[IVX]+\s*кв\.?\s*(\d{4})\s*$",
        "date" => r"^\s*(\d{4})-(\d{2})-(\d{2})",
        "month" => r"^\s*(\d{4})-(\d{2})-(\d{2})",
        _ => r"^\s*(\d)к\s*(\d{2})\s*$",
    }
}

fn convert(style: &str, caps: &regex::Captures) -> Option<String> {
    match style {
        "quarter_ru" => {
            let q: u32 = caps.get(1)?.as_str().parse().ok()?;
            let y: u32 = caps.get(2)?.as_str().parse().ok()?;
            Some(format!("{}Q{}", y, q))
        }
        "roman" => {
            let y: u32 = caps.get(1)?.as_str().parse().ok()?;
            Some(format!("{}Q?", y))
        }
        "date" => {
            let y: u32 = caps.get(1)?.as_str().parse().ok()?;
            let m: u32 = caps.get(2)?.as_str().parse().ok()?;
            let q = (m - 1) / 3 + 1;
            Some(format!("{}Q{}", y, q))
        }
        "month" => {
            let y: u32 = caps.get(1)?.as_str().parse().ok()?;
            let m: u32 = caps.get(2)?.as_str().parse().ok()?;
            Some(format!("{:04}-{:02}", y, m))
        }
        _ => None,
    }
}
