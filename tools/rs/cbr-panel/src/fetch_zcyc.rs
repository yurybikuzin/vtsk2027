use anyhow::{Context, Result};
use regex::Regex;
use reqwest::blocking::Client;
use std::path::Path;
use std::time::Duration;

use crate::excel::{write_long_xlsx, LongRowXlsx};

pub struct Args<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub chunk_months: u32,
    pub out: &'a Path,
    pub url: &'a str,
    pub value_col: usize,
}

fn parse_ymd(s: &str) -> Option<(i32, u32, u32)> {
    let s = s.trim();
    if s.len() != 10 {
        return None;
    }
    let y = s[0..4].parse::<i32>().ok()?;
    let m = s[5..7].parse::<u32>().ok()?;
    let d = s[8..10].parse::<u32>().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

fn add_months(y: i32, m: u32, months: i32) -> (i32, u32) {
    let total = (y * 12 + m as i32 - 1) + months;
    let ny = total.div_euclid(12);
    let nm = total.rem_euclid(12) as u32 + 1;
    (ny, nm)
}

fn last_day_of_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn chunks(
    from: (i32, u32, u32),
    to: (i32, u32, u32),
    months: u32,
) -> Vec<((i32, u32, u32), (i32, u32, u32))> {
    let mut out = Vec::new();
    let mut cur_y = from.0;
    let mut cur_m = from.1;
    loop {
        let (end_y, end_m) = add_months(cur_y, cur_m, months as i32 - 1);
        let end_d = last_day_of_month(end_y, end_m);
        let start = (cur_y, cur_m, from.2.max(1));
        let end = if (end_y, end_m) > (to.0, to.1) {
            to
        } else {
            (end_y, end_m, end_d)
        };
        out.push((start, end));
        if (end_y, end_m) >= (to.0, to.1) {
            break;
        }
        let (ny, nm) = add_months(end_y, end_m, 1);
        cur_y = ny;
        cur_m = nm;
        if (cur_y, cur_m) > (to.0, to.1) {
            break;
        }
    }
    out
}

fn fmt_dmy(d: (i32, u32, u32)) -> String {
    format!("{:02}.{:02}.{:04}", d.2, d.1, d.0)
}

pub fn run(args: Args) -> Result<()> {
    let from =
        parse_ymd(args.from).ok_or_else(|| anyhow::anyhow!("invalid --from: {}", args.from))?;
    let to = parse_ymd(args.to).ok_or_else(|| anyhow::anyhow!("invalid --to: {}", args.to))?;
    let months = args.chunk_months.max(1);

    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36")
        .danger_accept_invalid_certs(true)
        .build()?;

    let table_re =
        Regex::new(r#"(?is)<table[^>]*class="[^"]*data spaced[^"]*"[^>]*>(.*?)</table>"#)?;
    let row_re = Regex::new(r"(?is)<tr[^>]*>(.*?)</tr>")?;
    let cell_re = Regex::new(r"(?is)<td[^>]*>(.*?)</td>")?;
    let tag_re = Regex::new(r"(?is)<[^>]+>")?;
    let date_re = Regex::new(r"(\d{2})\.(\d{2})\.(\d{4})")?;
    let ws_re = Regex::new(r"\s+")?;

    let mut all: Vec<LongRowXlsx> = Vec::new();
    let chunks = chunks(from, to, months);
    eprintln!("fetch-zcyc: {} chunks of {} month(s)", chunks.len(), months);

    for (i, (start, end)) in chunks.iter().enumerate() {
        let from_s = fmt_dmy(*start);
        let to_s = fmt_dmy(*end);
        let url = format!(
            "{}?UniDbQuery.Posted=True&UniDbQuery.From={}&UniDbQuery.To={}",
            args.url, from_s, to_s
        );
        eprintln!("[{}/{}] {} .. {}", i + 1, chunks.len(), from_s, to_s);

        let resp = client
            .get(&url)
            .send()
            .with_context(|| format!("GET {}", url))?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("HTTP {} for {}", status.as_u16(), url);
        }
        let html = resp.text().context("read body")?;

        let table_html = match table_re.captures(&html) {
            Some(c) => c.get(1).map(|m| m.as_str()).unwrap_or("").to_string(),
            None => {
                eprintln!("  no table in response, skipping chunk");
                continue;
            }
        };

        let mut added = 0usize;
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
            let value = match cells[args.value_col]
                .trim()
                .replace(',', ".")
                .parse::<f64>()
            {
                Ok(v) => v,
                Err(_) => continue,
            };
            all.push(LongRowXlsx {
                series_id: "y1".to_string(),
                period: format!("{:04}-{:02}-{:02}", y, m, d),
                value,
                unit: "%".to_string(),
                source_file: "zcyc_params".to_string(),
                source_sheet: "data spaced".to_string(),
                source_row: 0,
                source_col: (args.value_col + 1) as u32,
            });
            added += 1;
        }
        eprintln!("  {} rows", added);
    }

    if all.is_empty() {
        anyhow::bail!("no rows parsed from zcyc_params");
    }

    all.sort_by(|a, b| a.period.cmp(&b.period));
    all.dedup_by(|a, b| a.period == b.period);

    let n = write_long_xlsx(args.out, &all)
        .map_err(|e| anyhow::anyhow!("write_long_xlsx {}: {}", args.out.display(), e))?;
    eprintln!("y1: {} rows -> {}", n, args.out.display());
    Ok(())
}
