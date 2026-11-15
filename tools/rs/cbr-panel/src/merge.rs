use crate::excel::{cell_f64, cell_str};
use calamine::{open_workbook_auto, Reader};
use rust_xlsxwriter::{Format, Workbook};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

pub struct PanelTable {
    pub columns: Vec<String>,
    pub data: BTreeMap<String, BTreeMap<String, f64>>,
}

pub fn read_panel(path: &Path) -> Result<PanelTable, Box<dyn Error>> {
    let mut wb = open_workbook_auto(path)?;
    let range = wb.worksheet_range("Panel")?;
    if range.height() == 0 || range.width() < 2 {
        return Err("лист Panel пуст или в нём меньше двух столбцов".into());
    }
    let mut columns = Vec::new();
    for c in 1..range.width() {
        if let Some(s) = range.get((0, c)).and_then(cell_str) {
            columns.push(s.trim().to_string());
        }
    }
    let mut data: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for r in 1..range.height() {
        let q = match range.get((r, 0)).and_then(cell_str) {
            Some(s) => s.trim().to_string(),
            None => continue,
        };
        if !is_quarter(&q) {
            eprintln!("[WARN] skip non-quarter row: {}", q);
            continue;
        }
        let mut row = BTreeMap::new();
        for (j, name) in columns.iter().enumerate() {
            let c = j + 1;
            if let Some(v) = range.get((r, c)).and_then(cell_f64) {
                row.insert(name.clone(), v);
            }
        }
        data.insert(q, row);
    }
    Ok(PanelTable { columns, data })
}

pub fn read_quarterly_xlsx_file(
    path: &Path,
) -> Result<BTreeMap<(String, String), f64>, Box<dyn Error>> {
    crate::excel::read_quarterly_xlsx(path)
}

pub fn merge_into(panel: &PanelTable, external: &BTreeMap<(String, String), f64>) -> PanelTable {
    let mut columns = panel.columns.clone();
    let mut series: Vec<String> = external.keys().map(|(s, _)| s.clone()).collect();
    series.sort();
    series.dedup();
    for s in &series {
        if !columns.contains(s) {
            columns.push(s.clone());
        }
    }
    let mut data = panel.data.clone();
    for ((sid, period), v) in external {
        data.entry(period.clone())
            .or_default()
            .insert(sid.clone(), *v);
    }
    PanelTable { columns, data }
}

pub fn filter_quarters(
    table: &PanelTable,
    min_quarter: Option<&str>,
    max_quarter: Option<&str>,
) -> PanelTable {
    let min_key = min_quarter.map(quarter_key);
    let max_key = max_quarter.map(quarter_key);
    let data = table
        .data
        .iter()
        .filter(|(q, _)| {
            let k = quarter_key(q);
            min_key.map_or(true, |m| k >= m) && max_key.map_or(true, |m| k <= m)
        })
        .map(|(q, row)| (q.clone(), row.clone()))
        .collect();
    PanelTable {
        columns: table.columns.clone(),
        data,
    }
}

fn quarter_key(q: &str) -> (i32, i32) {
    let q = q.trim();
    if q.len() >= 6 {
        let (y, rest) = q.split_at(4);
        if let (Ok(y), Some(qn)) = (y.parse::<i32>(), rest.strip_prefix('Q')) {
            if let Ok(qn) = qn.parse::<i32>() {
                if (1..=4).contains(&qn) {
                    return (y, qn);
                }
            }
        }
    }
    (0, 0)
}
pub fn write_enriched(path: &Path, table: &PanelTable) -> Result<(), Box<dyn Error>> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Panel")?;
    let num = Format::new().set_num_format("0.0000");
    ws.write(0, 0, "Quarter")?;
    for (i, name) in table.columns.iter().enumerate() {
        ws.write(0, (i + 1) as u16, name.as_str())?;
    }
    let mut r = 1u32;
    for (q, row) in &table.data {
        ws.write(r, 0, q.as_str())?;
        for (i, name) in table.columns.iter().enumerate() {
            if let Some(v) = row.get(name) {
                ws.write_with_format(r, (i + 1) as u16, *v, &num)?;
            }
        }
        r += 1;
    }
    ws.set_column_width(0, 10)?;
    for i in 1..=table.columns.len() as u16 {
        ws.set_column_width(i, 16)?;
    }
    wb.save(path)?;
    Ok(())
}

fn is_quarter(s: &str) -> bool {
    if s.len() != 6 {
        return false;
    }
    let (y, q) = s.split_at(4);
    y.parse::<u32>().is_ok()
        && q.len() == 2
        && q.starts_with('Q')
        && matches!(q.as_bytes()[1], b'1'..=b'4')
}
