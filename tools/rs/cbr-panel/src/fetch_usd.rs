use anyhow::{Context, Result};
use regex::Regex;
use reqwest::blocking::Client;
use std::path::Path;
use std::time::Duration;

use crate::excel::{write_long_xlsx, LongRowXlsx};

pub struct Args<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub out: &'a Path,
    pub url: &'a str,
}

pub fn run(args: Args) -> Result<()> {
    let base = args.url.trim_end_matches('/');
    let from = args.from.replace('.', "/");
    let to = args.to.replace('.', "/");
    let url = format!(
        "{}/scripts/XML_dynamic.asp?date_req1={}&date_req2={}&VAL_NM_RQ=R01235",
        base, from, to
    );
    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("Mozilla/5.0 (compatible; cbr-panel/0.1)")
        .danger_accept_invalid_certs(true)
        .build()?;

    let resp = client
        .get(&url)
        .send()
        .with_context(|| format!("GET {}", url))?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {} for {}", status.as_u16(), url);
    }
    let text = resp.text().context("read body")?;

    let rec_re = Regex::new(
        r#"(?is)<Record[^>]*Date="([^"]+)"[^>]*>.*?<Value>([^<]+)</Value>.*?</Record>"#,
    )?;

    let mut rows: Vec<LongRowXlsx> = Vec::new();
    for cap in rec_re.captures_iter(&text) {
        let date_raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let value_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let value = match value_raw.trim().replace(',', ".").parse::<f64>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let parts: Vec<&str> = date_raw.split('.').collect();
        if parts.len() != 3 {
            continue;
        }
        let d = match parts[0].parse::<u32>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let m = match parts[1].parse::<u32>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let y = match parts[2].parse::<u32>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        rows.push(LongRowXlsx {
            series_id: "usd".to_string(),
            period: format!("{:04}-{:02}-{:02}", y, m, d),
            value,
            unit: "rub".to_string(),
            source_file: "cbr XML_dynamic".to_string(),
            source_sheet: "USD".to_string(),
            source_row: 0,
            source_col: 0,
        });
    }

    if rows.is_empty() {
        anyhow::bail!("no USD rows parsed from {}", url);
    }

    rows.sort_by(|a, b| a.period.cmp(&b.period));

    let n = write_long_xlsx(args.out, &rows)
        .map_err(|e| anyhow::anyhow!("write_long_xlsx {}: {}", args.out.display(), e))?;
    eprintln!("usd: {} rows -> {}", n, args.out.display());
    Ok(())
}
