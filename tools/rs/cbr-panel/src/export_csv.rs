use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::merge::read_panel;

pub struct Args<'a> {
    pub panel: &'a Path,
    pub out: &'a Path,
    pub columns: Option<&'a str>,
    pub drop_outliers: bool,
}

fn quarter_key(q: &str) -> (i32, i32) {
    let q = q.trim();
    if q.len() == 6 {
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

fn prev_quarter(q: &str) -> Option<String> {
    let (y, qn) = quarter_key(q);
    if y == 0 {
        return None;
    }
    let (ny, nq) = if qn == 1 { (y - 1, 4) } else { (y, qn - 1) };
    Some(format!("{}Q{}", ny, nq))
}

fn quarter_num(q: &str) -> i32 {
    let (_, qn) = quarter_key(q);
    qn
}

pub fn run(args: Args) -> Result<()> {
    let panel = read_panel(args.panel)
        .map_err(|e| anyhow::anyhow!("read_panel {}: {}", args.panel.display(), e))?;

    let mut columns = panel.columns.clone();
    for c in [
        "s_lag",
        "y1_lag",
        "q_DIV_lag",
        "Vol_lag",
        "m_lag",
        "d_usd_lag",
        "Q2",
        "Q3",
        "Q4",
    ] {
        if !columns.iter().any(|x| x == c) {
            columns.push(c.to_string());
        }
    }

    if let Some(sel) = args.columns {
        let wanted: Vec<String> = sel
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if wanted.is_empty() {
            anyhow::bail!("--columns is empty");
        }
        let mut missing: Vec<String> = Vec::new();
        for w in &wanted {
            if !columns.iter().any(|c| c == w) {
                missing.push(w.clone());
            }
        }
        if !missing.is_empty() {
            anyhow::bail!("unknown columns: {}", missing.join(", "));
        }
        columns = wanted;
    }

    let mut data: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for (q, row) in &panel.data {
        let mut r = row.clone();

        if let Some(prev) = prev_quarter(q) {
            if let Some(prev_row) = panel.data.get(&prev) {
                for (src, dst) in [
                    ("s", "s_lag"),
                    ("y1", "y1_lag"),
                    ("q_DIV", "q_DIV_lag"),
                    ("Vol", "Vol_lag"),
                    ("m", "m_lag"),
                    ("d_usd", "d_usd_lag"),
                ] {
                    if let Some(v) = prev_row.get(src) {
                        r.insert(dst.to_string(), *v);
                    }
                }
            }
        }

        let qn = quarter_num(q);
        r.insert("Q2".to_string(), if qn == 2 { 1.0 } else { 0.0 });
        r.insert("Q3".to_string(), if qn == 3 { 1.0 } else { 0.0 });
        r.insert("Q4".to_string(), if qn == 4 { 1.0 } else { 0.0 });

        data.insert(q.clone(), r);
    }

    let mut f = File::create(args.out).with_context(|| format!("create {}", args.out.display()))?;

    write!(f, "Quarter")?;
    for c in &columns {
        write!(f, ",{}", c)?;
    }
    writeln!(f)?;

    for (q, row) in &data {
        if args.drop_outliers {
            if let Some(v) = row.get("outlier") {
                if *v == 1.0 {
                    continue;
                }
            }
        }
        write!(f, "{}", q)?;
        for c in &columns {
            match row.get(c) {
                Some(v) => write!(f, ",{}", v)?,
                None => write!(f, ",")?,
            }
        }
        writeln!(f)?;
    }

    eprintln!(
        "export-csv: {} rows, {} columns -> {}",
        data.len(),
        columns.len() + 1,
        args.out.display()
    );
    Ok(())
}
