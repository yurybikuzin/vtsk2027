use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::{Workbook, Worksheet};
use std::path::{Path, PathBuf};

enum XValue {
    Blank,
    Text(String),
    Number(f64),
    Bool(bool),
}

fn cell_to_xlsx(v: &Data) -> Option<XValue> {
    match v {
        Data::Empty => Some(XValue::Blank),
        Data::String(s) => Some(XValue::Text(s.clone())),
        Data::Float(f) => Some(XValue::Number(*f)),
        Data::Int(i) => Some(XValue::Number(*i as f64)),
        Data::Bool(b) => Some(XValue::Bool(*b)),
        Data::DateTime(_) | Data::DateTimeIso(_) | Data::DurationIso(_) => {
            Some(XValue::Text(format!("{:?}", v)))
        }
        Data::Error(_) => Some(XValue::Text(format!("{:?}", v))),
    }
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '.' => c,
            ' ' => '_',
            _ if c.is_alphanumeric() => c,
            _ => '_',
        })
        .collect();
    let trimmed = cleaned.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "sheet".to_string()
    } else {
        trimmed
    }
}

fn write_sheet(range: &calamine::Range<Data>, path: &Path) -> Result<()> {
    let mut wb = Workbook::new();
    let ws: &mut Worksheet = wb.add_worksheet();
    ws.set_name(path.file_stem().and_then(|s| s.to_str()).unwrap_or("sheet"))?;

    for r in 0..range.height() {
        for c in 0..range.width() {
            let cell = range.get((r, c)).cloned().unwrap_or(Data::Empty);
            let row = r as u32;
            let col = c as u16;
            match cell_to_xlsx(&cell) {
                Some(XValue::Blank) => {}
                Some(XValue::Text(s)) => {
                    ws.write_string(row, col, &s)?;
                }
                Some(XValue::Number(n)) => {
                    ws.write_number(row, col, n)?;
                }
                Some(XValue::Bool(b)) => {
                    ws.write_boolean(row, col, b)?;
                }
                None => {}
            }
        }
    }
    ws.set_freeze_panes(1, 0).ok();
    wb.save(path)?;
    Ok(())
}

struct Args {
    input: PathBuf,
    out_dir: PathBuf,
    only: Option<String>,
}

fn usage() -> ! {
    eprintln!("usage: excel-split --input <FILE> --out-dir <DIR> [--only <SHEET>]");
    eprintln!();
    eprintln!("  --input     source workbook (.xlsx, .xls, .xlsb, .ods)");
    eprintln!("  --out-dir   destination directory, one .xlsx per sheet");
    eprintln!("  --only      optional sheet name to split only that sheet");
    std::process::exit(2);
}

fn parse_args() -> Result<Args> {
    let mut input: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut only: Option<String> = None;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--input" => {
                input = Some(PathBuf::from(
                    it.next().ok_or_else(|| anyhow!("--input needs a value"))?,
                ));
            }
            "--out-dir" => {
                out_dir = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| anyhow!("--out-dir needs a value"))?,
                ));
            }
            "--only" => {
                only = Some(it.next().ok_or_else(|| anyhow!("--only needs a value"))?);
            }
            "--help" | "-h" => usage(),
            other => return Err(anyhow!("unknown arg: {other}")),
        }
    }

    let Some(input) = input else { usage() };
    let Some(out_dir) = out_dir else { usage() };
    Ok(Args {
        input,
        out_dir,
        only,
    })
}

fn main() -> Result<()> {
    let args = parse_args()?;

    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("create {}", args.out_dir.display()))?;

    let mut wb = open_workbook_auto(&args.input)
        .with_context(|| format!("open {}", args.input.display()))?;
    let names = wb.sheet_names().to_owned();
    println!("sheets: {}", names.join(", "));

    for name in names {
        if let Some(only) = &args.only {
            if &name != only {
                continue;
            }
        }
        let range = wb
            .worksheet_range(&name)
            .with_context(|| format!("sheet {}", name))?;
        if range.height() == 0 || range.width() == 0 {
            println!("skip empty sheet: {}", name);
            continue;
        }
        let file_name = format!("{}.xlsx", safe_name(&name));
        let path = args.out_dir.join(&file_name);
        write_sheet(&range, &path)?;
        println!(
            "{} rows={} cols={} -> {}",
            name,
            range.height(),
            range.width(),
            path.display()
        );
    }
    Ok(())
}
