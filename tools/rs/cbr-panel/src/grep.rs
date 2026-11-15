use anyhow::{Context, Result};
use calamine::{open_workbook_auto, DataType, Reader};
use std::path::Path;

pub fn run(input: &Path, sheet: &str, pattern: &str, ignore_case: bool) -> Result<()> {
    let mut wb = open_workbook_auto(input).with_context(|| format!("open {}", input.display()))?;
    let range = wb
        .worksheet_range(sheet)
        .with_context(|| format!("sheet {}", sheet))?;

    let needle = if ignore_case {
        pattern.to_lowercase()
    } else {
        pattern.to_string()
    };

    let mut hits = 0usize;
    for (row_idx, row) in range.rows().enumerate() {
        let cell = match row.first() {
            Some(c) => c,
            None => continue,
        };
        let text = match cell.as_string() {
            Some(s) => s,
            None => continue,
        };
        let hay = if ignore_case {
            text.to_lowercase()
        } else {
            text.clone()
        };
        if hay.contains(&needle) {
            hits += 1;
            println!("{:>5}  {}", row_idx + 1, text.replace('\n', " "));
        }
    }

    eprintln!(
        "{}: sheet '{}', pattern '{}', hits {}",
        input.display(),
        sheet,
        pattern,
        hits
    );
    Ok(())
}
