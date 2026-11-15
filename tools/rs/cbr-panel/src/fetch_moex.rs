use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crate::excel::{write_long_xlsx, LongRowXlsx};

pub struct Args<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub out: &'a Path,
    pub vol_window: usize,
    pub year_window: usize,
    pub base_url: &'a str,
}

#[derive(Debug, Deserialize)]
struct HistoryBlock {
    columns: Vec<String>,
    data: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct HistoryResponse {
    history: HistoryBlock,
}

fn fetch_close(
    client: &reqwest::blocking::Client,
    base_url: &str,
    secid: &str,
    from: &str,
    to: &str,
) -> Result<Vec<(String, f64)>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let page = 100usize;
    loop {
        let url = format!(
            "{}/iss/history/engines/stock/markets/index/securities/{}.json?from={}&till={}&iss.meta=off&start={}",
            base_url, secid, from, to, start
        );
        let resp = client
            .get(&url)
            .send()
            .with_context(|| format!("GET {}", url))?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("HTTP {} for {}", status.as_u16(), url);
        }
        let parsed: HistoryResponse = resp.json().context("parse json")?;
        if parsed.history.data.is_empty() {
            break;
        }
        let idx_date = parsed
            .history
            .columns
            .iter()
            .position(|c| c == "TRADEDATE")
            .ok_or_else(|| anyhow::anyhow!("TRADEDATE column missing"))?;
        let idx_close = parsed
            .history
            .columns
            .iter()
            .position(|c| c == "CLOSE")
            .ok_or_else(|| anyhow::anyhow!("CLOSE column missing"))?;
        let n = parsed.history.data.len();
        for row in parsed.history.data {
            let date = match row.get(idx_date).and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let close = match row.get(idx_close).and_then(|v| v.as_f64()) {
                Some(v) => v,
                None => continue,
            };
            out.push((date, close));
        }
        if n < page {
            break;
        }
        start += page;
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    Ok(out)
}

fn log_returns(prices: &[(String, f64)]) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for i in 1..prices.len() {
        let prev = prices[i - 1].1;
        let cur = prices[i].1;
        if prev > 0.0 && cur > 0.0 {
            out.push((prices[i].0.clone(), (cur / prev).ln()));
        }
    }
    out
}

fn rolling_vol(returns: &[(String, f64)], window: usize) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    if returns.len() < window {
        return out;
    }
    let factor = 252.0_f64.sqrt();
    for i in window - 1..returns.len() {
        let slice = &returns[i + 1 - window..=i];
        let mean: f64 = slice.iter().map(|(_, r)| r).sum::<f64>() / window as f64;
        let var: f64 = slice
            .iter()
            .map(|(_, r)| (r - mean) * (r - mean))
            .sum::<f64>()
            / (window as f64 - 1.0);
        let vol = var.sqrt() * factor;
        out.insert(returns[i].0.clone(), vol);
    }
    out
}

fn rolling_return(prices: &[(String, f64)], window: usize) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    if prices.len() <= window {
        return out;
    }
    for i in window..prices.len() {
        let prev = prices[i - window].1;
        let cur = prices[i].1;
        if prev > 0.0 && cur > 0.0 {
            out.insert(prices[i].0.clone(), cur / prev - 1.0);
        }
    }
    out
}

pub fn run(args: Args) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("Mozilla/5.0 (compatible; cbr-panel/0.1)")
        .build()?;

    eprintln!("fetch-moex: MCFTR {}..{}", args.from, args.to);
    let mcftr = fetch_close(&client, args.base_url, "MCFTR", args.from, args.to)?;
    eprintln!("fetch-moex: MCFTR {} rows", mcftr.len());

    eprintln!("fetch-moex: IMOEX {}..{}", args.from, args.to);
    let imoex = fetch_close(&client, args.base_url, "IMOEX", args.from, args.to)?;
    eprintln!("fetch-moex: IMOEX {} rows", imoex.len());

    let mcftr_returns = log_returns(&mcftr);
    let vol = rolling_vol(&mcftr_returns, args.vol_window);
    let re = rolling_return(&mcftr, args.year_window);
    let ri = rolling_return(&imoex, args.year_window);

    let mut rows: Vec<LongRowXlsx> = Vec::new();

    for (date, v) in &vol {
        rows.push(LongRowXlsx {
            series_id: "Vol".to_string(),
            period: date.clone(),
            value: *v,
            unit: "ann.".to_string(),
            source_file: "MOEX ISS".to_string(),
            source_sheet: "MCFTR".to_string(),
            source_row: 0,
            source_col: 0,
        });
    }

    for (date, v) in &re {
        rows.push(LongRowXlsx {
            series_id: "R_E".to_string(),
            period: date.clone(),
            value: *v,
            unit: "ratio".to_string(),
            source_file: "MOEX ISS".to_string(),
            source_sheet: "MCFTR".to_string(),
            source_row: 0,
            source_col: 0,
        });
    }

    for (date, r_m) in &re {
        if let Some(r_i) = ri.get(date) {
            rows.push(LongRowXlsx {
                series_id: "q_DIV".to_string(),
                period: date.clone(),
                value: r_m - r_i,
                unit: "ratio".to_string(),
                source_file: "MOEX ISS".to_string(),
                source_sheet: "MCFTR-IMOEX".to_string(),
                source_row: 0,
                source_col: 0,
            });
        }
    }

    if rows.is_empty() {
        anyhow::bail!("no market rows computed");
    }

    rows.sort_by(|a, b| {
        (a.series_id.clone(), a.period.clone()).cmp(&(b.series_id.clone(), b.period.clone()))
    });

    let n = write_long_xlsx(args.out, &rows)
        .map_err(|e| anyhow::anyhow!("write_long_xlsx {}: {}", args.out.display(), e))?;
    eprintln!(
        "market: {} rows (Vol {}, R_E {}, q_DIV {}) -> {}",
        n,
        rows.iter().filter(|r| r.series_id == "Vol").count(),
        rows.iter().filter(|r| r.series_id == "R_E").count(),
        rows.iter().filter(|r| r.series_id == "q_DIV").count(),
        args.out.display()
    );
    Ok(())
}
