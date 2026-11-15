use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;

use crate::excel::{write_long_xlsx, LongRowXlsx};

pub struct Args<'a> {
    pub input: &'a Path,
    pub table_class: &'a str,
    pub series_id: &'a str,
    pub unit: &'a str,
    pub value_col: usize,
    pub out: &'a Path,
}

pub fn run(args: Args) -> Result<()> {
    let html =
        fs::read_to_string(args.input).with_context(|| format!("read {}", args.input.display()))?;

    let table_re = Regex::new(&format!(
        r#"(?is)<table[^>]*class="[^"]*{}[^"]*"[^>]*>(.*?)</table>"#,
        regex::escape(args.table_class)
    ))?;

    let table_html = match table_re.captures(&html) {
        Some(c) => c.get(1).map(|m| m.as_str()).unwrap_or("").to_string(),
        None => anyhow::bail!(
            "table with class '{}' not found in {}",
            args.table_class,
            args.input.display()
        ),
    };

    let row_re = Regex::new(r"(?is)<tr[^>]*>(.*?)</tr>")?;
    let cell_re = Regex::new(r"(?is)<t[dh][^>]*>(.*?)</t[dh]>")?;
    let tag_re = Regex::new(r"(?is)<[^>]+>")?;
    let date_re = Regex::new(r"(\d{2})\.(\d{2})\.(\d{4})")?;
    let ws_re = Regex::new(r"\s+")?;

    let mut rows: Vec<LongRowXlsx> = Vec::new();
    for cap in row_re.captures_iter(&table_html) {
        let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let cells: Vec<String> = cell_re
            .captures_iter(inner)
            .map(|c| {
                let raw = c.get(1).map(|m| m.as_str()).unwrap_or("");
                ws_re
                    .replace_all(&tag_re.replace_all(raw, " "), " ")
                    .trim()
                    .to_string()
            })
            .collect();
        if cells.len() <= args.value_col {
            continue;
        }
        let caps = match date_re.captures(&cells[0]) {
            Some(c) => c,
            None => continue,
        };
        let d = match caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) {
            Some(v) => v,
            None => continue,
        };
        let m = match caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok()) {
            Some(v) => v,
            None => continue,
        };
        let y = match caps.get(3).and_then(|m| m.as_str().parse::<u32>().ok()) {
            Some(v) => v,
            None => continue,
        };
        let period = format!("{:04}-{:02}-{:02}", y, m, d);
        let value = match cells[args.value_col]
            .trim()
            .replace(',', ".")
            .parse::<f64>()
        {
            Ok(v) => v,
            Err(_) => continue,
        };
        rows.push(LongRowXlsx {
            series_id: args.series_id.to_string(),
            period,
            value,
            unit: args.unit.to_string(),
            source_file: args
                .input
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
            source_sheet: args.table_class.to_string(),
            source_row: 0,
            source_col: (args.value_col + 1) as u32,
        });
    }

    if rows.is_empty() {
        anyhow::bail!(
            "no data rows found in table '{}' of {}",
            args.table_class,
            args.input.display()
        );
    }

    rows.sort_by(|a, b| a.period.cmp(&b.period));

    let n = write_long_xlsx(args.out, &rows)
        .map_err(|e| anyhow::anyhow!("write_long_xlsx {}: {}", args.out.display(), e))?;
    eprintln!(
        "{}: {} rows from {} -> {}",
        args.series_id,
        n,
        args.input.display(),
        args.out.display()
    );
    Ok(())
}
