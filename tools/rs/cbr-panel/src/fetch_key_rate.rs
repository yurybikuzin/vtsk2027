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
    let soap_body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <KeyRateXML xmlns="http://web.cbr.ru/">
      <fromDate>{}</fromDate>
      <ToDate>{}</ToDate>
    </KeyRateXML>
  </soap:Body>
</soap:Envelope>"#,
        args.from, args.to
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("Mozilla/5.0 (compatible; cbr-panel/0.1; +https://www.cbr.ru/)")
        .danger_accept_invalid_certs(true)
        .build()?;

    let resp = client
        .post(args.url)
        .header("Content-Type", "text/xml; charset=utf-8")
        .header("SOAPAction", "http://web.cbr.ru/KeyRateXML")
        .body(soap_body)
        .send()
        .with_context(|| format!("POST {}", args.url))?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {}", status.as_u16());
    }

    let text = resp.text().context("read body")?;

    let kr_re = Regex::new(r"(?is)<KR>.*?<DT>(.*?)</DT>.*?<Rate>(.*?)</Rate>.*?</KR>")?;
    let mut rows: Vec<LongRowXlsx> = Vec::new();

    for cap in kr_re.captures_iter(&text) {
        let dt_raw = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let rate_raw = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        let rate = match rate_raw.replace(',', ".").parse::<f64>() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let date_part = dt_raw.split('T').next().unwrap_or(dt_raw);
        let mut parts = date_part.split('-');
        let y = match parts.next().and_then(|s| s.parse::<u32>().ok()) {
            Some(v) => v,
            None => continue,
        };
        let m = match parts.next().and_then(|s| s.parse::<u32>().ok()) {
            Some(v) => v,
            None => continue,
        };
        let d = match parts.next().and_then(|s| s.parse::<u32>().ok()) {
            Some(v) => v,
            None => continue,
        };
        rows.push(LongRowXlsx {
            series_id: "k".to_string(),
            period: format!("{:04}-{:02}-{:02}", y, m, d),
            value: rate,
            unit: "%".to_string(),
            source_file: "DailyInfoWebServ/KeyRateXML".to_string(),
            source_sheet: String::new(),
            source_row: 0,
            source_col: 0,
        });
    }

    if rows.is_empty() {
        anyhow::bail!("no KeyRateXML rows parsed from response");
    }

    rows.sort_by(|a, b| a.period.cmp(&b.period));

    let n = write_long_xlsx(args.out, &rows)
        .map_err(|e| anyhow::anyhow!("write_long_xlsx {}: {}", args.out.display(), e))?;
    eprintln!("k: {} rows -> {}", n, args.out.display());
    Ok(())
}
