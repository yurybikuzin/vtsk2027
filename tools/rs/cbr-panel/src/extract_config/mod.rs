mod column;
mod config;
mod period_key;
mod periods;
mod series;

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::excel::{write_long_xlsx, LongRowXlsx};
use column::read_column_series;
use config::{Config, Series};
use period_key::period_key;
use periods::resolve_periods;
use series::read_series;

pub fn run(
    config_path: &Path,
    root: &Path,
    out: &Path,
    log: Option<&Path>,
    only: Option<&str>,
) -> Result<()> {
    if let Some(log_path) = log {
        init_log(log_path)?;
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    let s = std::fs::read_to_string(config_path)
        .with_context(|| format!("read config {}", config_path.display()))?;
    let cfg: Config = toml::from_str(&s).context("parse toml")?;

    log::info!("root = {}", root.display());
    log::info!("config = {}", config_path.display());
    log::info!("out = {}", out.display());

    let selected: Vec<&Series> = match only {
        Some(id) => cfg.series.iter().filter(|s| s.series_id == id).collect(),
        None => cfg.series.iter().collect(),
    };
    if selected.is_empty() {
        anyhow::bail!("no series selected");
    }

    let mut all: Vec<LongRowXlsx> = Vec::new();
    for s in selected {
        if s.period_style == "date_row_column" {
            let pairs = match read_column_series(root, s) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("{}: skip: {:#}", s.series_id, e);
                    continue;
                }
            };
            let min_key = s.min_period.as_deref().map(period_key);
            let max_key = s.max_period.as_deref().map(period_key);
            let filtered: Vec<(String, f64, usize)> = pairs
                .into_iter()
                .filter(|(p, _, _)| {
                    let k = period_key(p);
                    min_key.map_or(true, |m| k >= m) && max_key.map_or(true, |m| k <= m)
                })
                .collect();
            if filtered.is_empty() {
                log::warn!("{}: no values after period filter", s.series_id);
                continue;
            }
            let n = filtered.len();
            for (period, value, col) in filtered {
                all.push(LongRowXlsx {
                    series_id: s.series_id.clone(),
                    period,
                    value,
                    unit: s.unit.clone(),
                    source_file: s.path.clone(),
                    source_sheet: s.sheet.clone(),
                    source_row: 0,
                    source_col: (col + 1) as u32,
                });
            }
            log::info!("{}: {} values (column mode)", s.series_id, n);
            continue;
        }

        let values = match read_series(root, s) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("{}: skip: {:#}", s.series_id, e);
                continue;
            }
        };
        let periods = match resolve_periods(root, s) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("{}: skip periods: {:#}", s.series_id, e);
                continue;
            }
        };
        let period_by_col: BTreeMap<usize, String> = periods.into_iter().collect();

        let pairs: Vec<(String, f64, usize)> = values
            .rows
            .iter()
            .filter_map(|r| {
                period_by_col
                    .get(&r.col)
                    .map(|p| (p.clone(), r.value, r.col))
            })
            .collect();

        let min_key = s.min_period.as_deref().map(period_key);
        let max_key = s.max_period.as_deref().map(period_key);
        let filtered: Vec<(String, f64, usize)> = pairs
            .into_iter()
            .filter(|(p, _, _)| {
                let k = period_key(p);
                min_key.map_or(true, |m| k >= m) && max_key.map_or(true, |m| k <= m)
            })
            .collect();
        if filtered.is_empty() {
            log::warn!(
                "{}: no values after period filter (raw {}, min {:?}, max {:?})",
                s.series_id,
                values.rows.len(),
                s.min_period,
                s.max_period
            );
            continue;
        }
        let n = filtered.len();
        for (period, value, col) in filtered {
            all.push(LongRowXlsx {
                series_id: s.series_id.clone(),
                period,
                value,
                unit: s.unit.clone(),
                source_file: s.path.clone(),
                source_sheet: s.sheet.clone(),
                source_row: (values.label_row + 1) as u32,
                source_col: (col + 1) as u32,
            });
        }
        log::info!(
            "{}: {} values (label at row {})",
            s.series_id,
            n,
            values.label_row + 1
        );
    }

    let n = write_long_xlsx(out, &all)
        .map_err(|e| anyhow::anyhow!("write_long_xlsx {}: {}", out.display(), e))?;
    log::info!("wrote {} records to {}", n, out.display());
    Ok(())
}

fn init_log(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    let target = Box::new(file);
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(target))
        .init();
    Ok(())
}

pub(crate) fn resolve_path(root: &Path, rel: &str) -> PathBuf {
    let p = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        root.join(rel)
    };
    std::fs::canonicalize(&p).unwrap_or(p)
}
