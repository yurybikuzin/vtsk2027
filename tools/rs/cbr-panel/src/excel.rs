use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::Workbook;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

pub fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn excel_serial_to_ym(serial: f64) -> Option<(u32, u32)> {
    let days = serial.floor() as i64;
    let mut remaining = days - 43101;
    let mut year: i32 = 2018;
    while remaining >= if is_leap(year as u32) { 366 } else { 365 } {
        remaining -= if is_leap(year as u32) { 366 } else { 365 };
        year += 1;
    }
    while remaining < 0 {
        year -= 1;
        remaining += if is_leap(year as u32) { 366 } else { 365 };
    }
    if year < 1900 {
        return None;
    }
    let months = [
        31,
        if is_leap(year as u32) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month: i32 = 1;
    for m in months {
        if remaining < m {
            break;
        }
        remaining -= m;
        month += 1;
    }
    Some((year as u32, month as u32))
}

pub fn cell_str(d: &Data) -> Option<String> {
    match d {
        Data::String(s) => Some(s.clone()),
        Data::Float(f) => Some(format!("{}", f)),
        Data::Int(i) => Some(format!("{}", i)),
        Data::Bool(b) => Some(b.to_string()),
        Data::DateTime(x) => {
            let (y, m) = excel_serial_to_ym(x.as_f64())?;
            Some(format!("{:04}-{:02}-01 00:00:00", y, m))
        }
        _ => None,
    }
}

pub fn cell_f64(d: &Data) -> Option<f64> {
    match d {
        Data::Float(f) => Some(*f),
        Data::Int(i) => Some(*i as f64),
        Data::String(s) => s.trim().replace(',', ".").parse::<f64>().ok(),
        _ => None,
    }
}

pub fn parse_q_ru(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() >= 3 {
        if let (Ok(q), Ok(y)) = (parts[0].parse::<u32>(), parts[2].parse::<u32>()) {
            if (1..=4).contains(&q) {
                return Some(format!("{}Q{}", y, q));
            }
        }
    }
    None
}

pub fn parse_q_date(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() < 7 || &s[4..5] != "-" {
        return None;
    }
    let y: u32 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    Some(format!("{}Q{}", y, (m - 1) / 3 + 1))
}

pub fn parse_month_ru(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        return None;
    }
    let y: u32 = parts[1].parse().ok()?;
    let m = match parts[0].to_lowercase().as_str() {
        "янв" => 1u32,
        "фев" => 2,
        "мар" => 3,
        "апр" => 4,
        "май" => 5,
        "июн" => 6,
        "июл" => 7,
        "авг" => 8,
        "сен" => 9,
        "окт" => 10,
        "ноя" => 11,
        "дек" => 12,
        _ => return None,
    };
    Some((y, m))
}

pub fn parse_ym_any(s: &str) -> Option<(u32, u32)> {
    let s = s.trim();
    if s.len() < 7 || &s[4..5] != "-" {
        return None;
    }
    let y: u32 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    if (1..=12).contains(&m) {
        Some((y, m))
    } else {
        None
    }
}

pub fn quarter_from_ym(y: u32, m: u32) -> String {
    format!("{}Q{}", y, (m - 1) / 3 + 1)
}

#[derive(Clone, Debug)]
pub struct LongRowXlsx {
    pub series_id: String,
    pub period: String,
    pub value: f64,
    pub unit: String,
    pub source_file: String,
    pub source_sheet: String,
    pub source_row: u32,
    pub source_col: u32,
}

pub fn write_long_xlsx<P: AsRef<Path>>(
    path: P,
    rows: &[LongRowXlsx],
) -> Result<usize, Box<dyn Error>> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Long")?;
    let headers = [
        "series_id",
        "period",
        "value",
        "unit",
        "source_file",
        "source_sheet",
        "source_row",
        "source_col",
    ];
    for (i, h) in headers.iter().enumerate() {
        ws.write(0, i as u16, *h)?;
    }
    let num = rust_xlsxwriter::Format::new().set_num_format("0.0000");
    for (r, row) in rows.iter().enumerate() {
        let r = (r + 1) as u32;
        ws.write(r, 0, row.series_id.as_str())?;
        ws.write(r, 1, row.period.as_str())?;
        ws.write_with_format(r, 2, row.value, &num)?;
        ws.write(r, 3, row.unit.as_str())?;
        ws.write(r, 4, row.source_file.as_str())?;
        ws.write(r, 5, row.source_sheet.as_str())?;
        ws.write(r, 6, row.source_row)?;
        ws.write(r, 7, row.source_col)?;
    }
    ws.set_column_width(0, 18)?;
    ws.set_column_width(1, 12)?;
    ws.set_column_width(2, 12)?;
    wb.save(path)?;
    Ok(rows.len())
}

pub fn read_long_xlsx<P: AsRef<Path>>(path: P) -> Result<Vec<LongRowXlsx>, Box<dyn Error>> {
    let mut wb = open_workbook_auto(path)?;
    let range = wb.worksheet_range("Long")?;
    let mut out = Vec::new();
    for r in 1..range.height() {
        let get_s = |c: usize| range.get((r, c)).and_then(cell_str).unwrap_or_default();
        let get_f = |c: usize| range.get((r, c)).and_then(cell_f64).unwrap_or(f64::NAN);
        let get_u = |c: usize| range.get((r, c)).and_then(cell_f64).unwrap_or(0.0) as u32;
        let series_id = get_s(0);
        if series_id.trim().is_empty() {
            continue;
        }
        out.push(LongRowXlsx {
            series_id,
            period: get_s(1),
            value: get_f(2),
            unit: get_s(3),
            source_file: get_s(4),
            source_sheet: get_s(5),
            source_row: get_u(6),
            source_col: get_u(7),
        });
    }
    Ok(out)
}

pub fn write_quarterly_xlsx<P: AsRef<Path>>(
    path: P,
    q: &BTreeMap<(String, String), (f64, Option<String>)>,
) -> Result<usize, Box<dyn Error>> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Quarterly")?;
    let headers = ["series_id", "period", "value", "note"];
    for (i, h) in headers.iter().enumerate() {
        ws.write(0, i as u16, *h)?;
    }
    let num = rust_xlsxwriter::Format::new().set_num_format("0.0000");
    let mut r = 1u32;
    for ((sid, period), (v, note)) in q {
        ws.write(r, 0, sid.as_str())?;
        ws.write(r, 1, period.as_str())?;
        if !v.is_nan() {
            ws.write_with_format(r, 2, *v, &num)?;
        }
        if let Some(n) = note {
            ws.write(r, 3, n.as_str())?;
        }
        r += 1;
    }
    ws.set_column_width(0, 18)?;
    ws.set_column_width(1, 10)?;
    ws.set_column_width(2, 12)?;
    ws.set_column_width(3, 14)?;
    wb.save(path)?;
    Ok(q.len())
}

pub fn read_quarterly_xlsx<P: AsRef<Path>>(
    path: P,
) -> Result<BTreeMap<(String, String), f64>, Box<dyn Error>> {
    let mut wb = open_workbook_auto(path)?;
    let range = wb.worksheet_range("Quarterly")?;
    let mut out = BTreeMap::new();
    for r in 1..range.height() {
        let sid = range.get((r, 0)).and_then(cell_str).unwrap_or_default();
        let period = range.get((r, 1)).and_then(cell_str).unwrap_or_default();
        if sid.trim().is_empty() || period.trim().is_empty() {
            continue;
        }
        let note = range.get((r, 3)).and_then(cell_str).unwrap_or_default();
        if !note.trim().is_empty() {
            continue;
        }
        if let Some(v) = range.get((r, 2)).and_then(cell_f64) {
            out.insert((sid, period), v);
        }
    }
    Ok(out)
}
