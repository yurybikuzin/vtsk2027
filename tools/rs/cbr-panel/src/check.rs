use std::collections::BTreeMap;

pub struct Coverage {
    pub column: String,
    pub first: Option<String>,
    pub last: Option<String>,
    pub present: usize,
    pub missing: usize,
    pub flag_early: bool,
    pub flag_late: bool,
}

pub fn coverage(
    panel: &BTreeMap<String, BTreeMap<String, f64>>,
    columns: &[&str],
    expected_first: &str,
    expected_last: &str,
) -> Vec<Coverage> {
    let quarters: Vec<&String> = panel.keys().collect();
    let mut out = Vec::new();
    for col in columns {
        let mut present = 0usize;
        let mut missing = 0usize;
        let mut first = None;
        let mut last = None;
        for q in &quarters {
            let has = panel.get(*q).and_then(|m| m.get(*col)).is_some();
            if has {
                present += 1;
                if first.is_none() {
                    first = Some((*q).clone());
                }
                last = Some((*q).clone());
            } else {
                missing += 1;
            }
        }
        let flag_early = first
            .as_deref()
            .map(|f| f != expected_first)
            .unwrap_or(true);
        let flag_late = last.as_deref().map(|l| l != expected_last).unwrap_or(true);
        out.push(Coverage {
            column: (*col).to_string(),
            first,
            last,
            present,
            missing,
            flag_early,
            flag_late,
        });
    }
    out
}

pub fn print_coverage(cov: &[Coverage]) {
    println!(
        "{:<16} {:>8} {:>8} {:>8} {:>8} {:>6} {:>6}",
        "column", "first", "last", "present", "missing", "early", "late"
    );
    for c in cov {
        println!(
            "{:<16} {:>8} {:>8} {:>8} {:>8} {:>6} {:>6}",
            c.column,
            c.first.clone().unwrap_or_default(),
            c.last.clone().unwrap_or_default(),
            c.present,
            c.missing,
            if c.flag_early { "yes" } else { "no" },
            if c.flag_late { "yes" } else { "no" }
        );
    }
}
