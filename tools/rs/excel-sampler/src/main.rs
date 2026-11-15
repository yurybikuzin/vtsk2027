use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::{ExcelDateTime, Workbook, Worksheet};

const DEFAULT_CHUNK_SIZE: usize = 50;

fn select_row_indices(data_rows: usize, chunk: usize) -> Vec<usize> {
    if data_rows == 0 {
        return Vec::new();
    }

    let chunk = chunk.max(1);
    let start_end = chunk.min(data_rows);
    let mut indices: Vec<usize> = (0..start_end).collect();

    if data_rows <= chunk {
        return indices;
    }

    let end_start = chunk.max(data_rows - chunk);

    if data_rows > 2 * chunk {
        let zone_start = chunk;
        let zone_end = end_start;
        let zone_len = zone_end - zone_start;
        let mid_len = chunk.min(zone_len);
        let mid_start = zone_start + (zone_len - mid_len) / 2;
        let mid_end = mid_start + mid_len;
        indices.extend(mid_start..mid_end);
    }

    indices.extend(end_start..data_rows);
    indices
}

fn write_row(worksheet: &mut Worksheet, row_idx: u32, row: &[Data]) -> Result<()> {
    for (col_idx, cell) in row.iter().enumerate() {
        let col = col_idx as u16;
        match cell {
            Data::Empty => continue,
            Data::String(s) => {
                worksheet.write_string(row_idx, col, s)?;
            }
            Data::Float(f) => {
                worksheet.write_number(row_idx, col, *f)?;
            }
            Data::Int(i) => {
                worksheet.write_number(row_idx, col, *i as f64)?;
            }
            Data::Bool(b) => {
                worksheet.write_boolean(row_idx, col, *b)?;
            }
            Data::DateTime(serial) => {
                let dt = ExcelDateTime::from_serial_datetime(serial.as_f64())?;
                worksheet.write_datetime(row_idx, col, &dt)?;
            }
            Data::DateTimeIso(s) => match ExcelDateTime::parse_from_str(s) {
                Ok(dt) => {
                    worksheet.write_datetime(row_idx, col, &dt)?;
                }
                Err(_) => {
                    worksheet.write_string(row_idx, col, s)?;
                }
            },
            Data::DurationIso(s) => {
                worksheet.write_string(row_idx, col, s)?;
            }
            Data::Error(e) => {
                worksheet.write_string(row_idx, col, &format!("#ERROR: {:?}", e))?;
            }
        }
    }
    Ok(())
}

struct Args {
    input: String,
    output: String,
    chunk_size: usize,
}

fn usage() -> ! {
    eprintln!("usage: excel-sampler --input <FILE> --output <FILE> [--chunk-size N]");
    eprintln!();
    eprintln!("  --input       source workbook (.xlsx, .xls, .xlsb, .ods)");
    eprintln!("  --output      destination sample workbook (.xlsx)");
    eprintln!("  --chunk-size  rows per slice (start, middle, end), default {DEFAULT_CHUNK_SIZE}");
    std::process::exit(2);
}

fn parse_args() -> Result<Args> {
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut chunk_size: usize = DEFAULT_CHUNK_SIZE;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--input" => {
                input = Some(it.next().ok_or_else(|| anyhow!("--input needs a value"))?);
            }
            "--output" => {
                output = Some(it.next().ok_or_else(|| anyhow!("--output needs a value"))?);
            }
            "--chunk-size" => {
                let raw = it
                    .next()
                    .ok_or_else(|| anyhow!("--chunk-size needs a value"))?;
                chunk_size = raw
                    .parse::<usize>()
                    .with_context(|| format!("--chunk-size: not a positive integer: {raw}"))?;
                if chunk_size == 0 {
                    return Err(anyhow!("--chunk-size must be greater than zero"));
                }
            }
            "--help" | "-h" => usage(),
            other => return Err(anyhow!("unknown arg: {other}")),
        }
    }

    let Some(input) = input else { usage() };
    let Some(output) = output else { usage() };
    Ok(Args {
        input,
        output,
        chunk_size,
    })
}

fn main() -> Result<()> {
    let args = parse_args()?;

    let mut workbook =
        open_workbook_auto(&args.input).with_context(|| format!("open {}", args.input))?;
    let sheet_names = workbook.sheet_names().to_owned();

    let mut out_workbook = Workbook::new();

    for sheet_name in sheet_names {
        let range = workbook
            .worksheet_range(&sheet_name)
            .with_context(|| format!("sheet {}", sheet_name))?;

        let mut worksheet = out_workbook.add_worksheet();
        worksheet.set_name(&sheet_name)?;

        let rows: Vec<&[Data]> = range.rows().collect();
        if rows.is_empty() {
            continue;
        }

        write_row(&mut worksheet, 0, rows[0])?;

        let data_rows = &rows[1..];
        let indices = select_row_indices(data_rows.len(), args.chunk_size);

        for (out_idx, &src_idx) in indices.iter().enumerate() {
            write_row(&mut worksheet, (out_idx + 1) as u32, data_rows[src_idx])?;
        }

        eprintln!(
            "{}: {} data rows -> {} sampled (chunk {})",
            sheet_name,
            data_rows.len(),
            indices.len(),
            args.chunk_size
        );
    }

    if let Some(parent) = std::path::Path::new(&args.output).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
    }

    out_workbook.save(&args.output)?;
    eprintln!("saved {}", args.output);
    Ok(())
}
