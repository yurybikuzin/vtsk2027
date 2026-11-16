use crate::excel::{write_quarterly_xlsx, LongRowXlsx};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::Path;

enum Period {
    Month(u32, u32),
    Day(u32, u32, u32),
}

fn parse_period(s: &str) -> Option<Period> {
    let s = s.trim();
    if s.len() == 7 && &s[4..5] == "-" {
        let y = s[0..4].parse::<u32>().ok()?;
        let m = s[5..7].parse::<u32>().ok()?;
        if (1..=12).contains(&m) {
            return Some(Period::Month(y, m));
        }
        return None;
    }
    if s.len() == 10 && &s[4..5] == "-" && &s[7..8] == "-" {
        let y = s[0..4].parse::<u32>().ok()?;
        let m = s[5..7].parse::<u32>().ok()?;
        let d = s[8..10].parse::<u32>().ok()?;
        if (1..=12).contains(&m) && (1..=31).contains(&d) {
            return Some(Period::Day(y, m, d));
        }
        return None;
    }
    None
}

pub fn monthly_to_quarterly(
    rows: &[LongRowXlsx],
) -> BTreeMap<(String, String), (f64, Option<String>)> {
    let mut monthly: BTreeMap<String, BTreeMap<(u32, u32), f64>> = BTreeMap::new();
    let mut daily_sum: BTreeMap<String, BTreeMap<(u32, u32), (f64, usize)>> = BTreeMap::new();

    for r in rows {
        match parse_period(&r.period) {
            Some(Period::Month(y, m)) => {
                monthly
                    .entry(r.series_id.clone())
                    .or_default()
                    .insert((y, m), r.value);
            }
            Some(Period::Day(y, m, _d)) => {
                let q = (m - 1) / 3 + 1;
                let entry = daily_sum
                    .entry(r.series_id.clone())
                    .or_default()
                    .entry((y, q))
                    .or_insert((0.0, 0usize));
                entry.0 += r.value;
                entry.1 += 1;
            }
            None => {}
        }
    }

    let mut out = BTreeMap::new();

    for (sid, months) in monthly {
        let years: BTreeSet<u32> = months.keys().map(|(y, _)| *y).collect();
        for y in years {
            for q in 1..=4u32 {
                let keys = [(y, q * 3 - 2), (y, q * 3 - 1), (y, q * 3)];
                let mut vals = Vec::with_capacity(3);
                for k in keys {
                    if let Some(v) = months.get(&k) {
                        vals.push(*v);
                    }
                }
                let period = format!("{}Q{}", y, q);
                let entry = if vals.len() == 3 {
                    (vals.iter().sum::<f64>() / 3.0, None)
                } else {
                    (f64::NAN, Some("incomplete".to_string()))
                };
                out.insert((sid.clone(), period), entry);
            }
        }
    }

    for (sid, quarters) in daily_sum {
        for ((y, q), (sum, count)) in quarters {
            if count == 0 {
                continue;
            }
            let period = format!("{}Q{}", y, q);
            out.insert((sid.clone(), period), (sum / count as f64, None));
        }
    }

    out
}

pub fn write_quarterly<P: AsRef<Path>>(
    path: P,
    q: &BTreeMap<(String, String), (f64, Option<String>)>,
) -> Result<usize, Box<dyn Error>> {
    write_quarterly_xlsx(path, q)
}

pub fn append_quarterly<P: AsRef<Path>>(
    path: P,
    q: &BTreeMap<(String, String), (f64, Option<String>)>,
) -> Result<usize, Box<dyn Error>> {
    let path = path.as_ref();
    let mut merged: BTreeMap<(String, String), (f64, Option<String>)> = BTreeMap::new();
    if path.exists() {
        let existing = crate::excel::read_quarterly_xlsx(path)?;
        for (k, v) in existing {
            merged.insert(k, (v, None));
        }
    }
    for (k, v) in q {
        merged.insert(k.clone(), v.clone());
    }
    crate::excel::write_quarterly_xlsx(path, &merged)
}
