use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

use crate::merge::{read_panel, write_enriched, PanelTable};

pub struct Args<'a> {
    pub panel: &'a Path,
    pub out: &'a Path,
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

fn is_outlier(q: &str) -> f64 {
    match q {
        "2022Q1" | "2023Q4" | "2024Q4" => 1.0,
        _ => 0.0,
    }
}

pub fn run(args: Args) -> Result<()> {
    let panel = read_panel(args.panel)
        .map_err(|e| anyhow::anyhow!("read_panel {}: {}", args.panel.display(), e))?;

    let mut columns = panel.columns.clone();
    for c in [
        "s",
        "m",
        "d_usd",
        "outlier",
        "f_D_shift",
        "g_E_D",
        "g_B_E",
        "g_F_E",
    ] {
        if !columns.iter().any(|x| x == c) {
            columns.push(c.to_string());
        }
    }

    let mut data: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for (q, row) in &panel.data {
        let mut r = row.clone();

        if let (Some(y1), Some(rd)) = (row.get("y1"), row.get("rD")) {
            r.insert("s".to_string(), y1 - rd);
        }
        if let (Some(rd), Some(k)) = (row.get("rD"), row.get("k")) {
            r.insert("m".to_string(), rd - k);
        }
        if let (Some(oth_rub), Some(tr)) = (row.get("f_D_oth_rub"), row.get("f_D_tr")) {
            r.insert("f_D_shift".to_string(), oth_rub - tr);
        }
        if let (Some(e), Some(d)) = (row.get("f_E"), row.get("f_D_oth_rub")) {
            r.insert("g_E_D".to_string(), e - d);
        }
        if let (Some(b), Some(e)) = (row.get("f_B"), row.get("f_E")) {
            r.insert("g_B_E".to_string(), b - e);
        }
        if let (Some(f), Some(e)) = (row.get("f_F"), row.get("f_E")) {
            r.insert("g_F_E".to_string(), f - e);
        }
        if let (Some(usd_t), Some(prev)) = (row.get("usd"), prev_quarter(q)) {
            if let Some(usd_prev) = panel.data.get(&prev).and_then(|m| m.get("usd")) {
                if *usd_prev > 0.0 && *usd_t > 0.0 {
                    r.insert("d_usd".to_string(), (usd_t / usd_prev).ln());
                }
            }
        }
        r.insert("outlier".to_string(), is_outlier(q));

        data.insert(q.clone(), r);
    }

    let enriched = PanelTable { columns, data };
    let backup = crate::backup::backup_before_overwrite(args.out, 5)
        .map_err(|e| anyhow::anyhow!("backup {}: {}", args.out.display(), e))?;
    if let Some(b) = &backup {
        eprintln!("[OK] backup: {}", b.display());
    }
    write_enriched(args.out, &enriched)
        .map_err(|e| anyhow::anyhow!("write_enriched {}: {}", args.out.display(), e))?;
    eprintln!(
        "enriched: {} columns, {} quarters -> {}",
        enriched.columns.len(),
        enriched.data.len(),
        args.out.display()
    );
    Ok(())
}
