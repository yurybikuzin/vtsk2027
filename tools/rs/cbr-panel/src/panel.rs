use crate::excel::{cell_f64, cell_str, parse_month_ru, parse_q_date, parse_q_ru};
use calamine::{open_workbook_auto, Data, Range, Reader};
use std::collections::BTreeMap;
use std::error::Error;

pub fn find_data_row(range: &Range<Data>, label: &str) -> Option<usize> {
    for row in 0..range.height() {
        if let Some(s) = range.get((row, 0)).and_then(cell_str) {
            if s.trim() == label {
                return Some(row);
            }
        }
    }
    None
}

fn find_header_row<F>(range: &Range<Data>, data_row: usize, parser: F) -> Option<usize>
where
    F: Fn(&str) -> bool,
{
    for hr in (0..data_row).rev() {
        let mut hits = 0;
        for col in 1..range.width() {
            if let Some(s) = range.get((hr, col)).and_then(cell_str) {
                if parser(&s) {
                    hits += 1;
                }
            }
        }
        if hits >= 3 {
            return Some(hr);
        }
    }
    None
}

pub fn extract_quarterly<F>(range: &Range<Data>, label: &str, parse_q: F) -> BTreeMap<String, f64>
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = BTreeMap::new();
    let data_row = match find_data_row(range, label) {
        Some(r) => r,
        None => {
            eprintln!("[WARN] row not found: {}", label);
            return out;
        }
    };
    let header_row = match find_header_row(range, data_row, |s| parse_q(s).is_some()) {
        Some(h) => h,
        None => {
            eprintln!("[WARN] header not found for: {}", label);
            return out;
        }
    };
    for col in 1..range.width() {
        let q = range
            .get((header_row, col))
            .and_then(cell_str)
            .and_then(|s| parse_q(&s));
        if let Some(q) = q {
            if let Some(v) = range.get((data_row, col)).and_then(cell_f64) {
                out.insert(q, v);
            }
        }
    }
    eprintln!("[OK] {} -> {} quarters", label, out.len());
    out
}

pub fn extract_monthly_aggregated(range: &Range<Data>, label: &str) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    let data_row = match find_data_row(range, label) {
        Some(r) => r,
        None => {
            eprintln!("[WARN] monthly row not found: {}", label);
            return out;
        }
    };
    let header_row = match find_header_row(range, data_row, |s| parse_month_ru(s).is_some()) {
        Some(h) => h,
        None => {
            eprintln!("[WARN] monthly header not found: {}", label);
            return out;
        }
    };
    let mut monthly: BTreeMap<(u32, u32), f64> = BTreeMap::new();
    for col in 1..range.width() {
        let ym = range
            .get((header_row, col))
            .and_then(cell_str)
            .and_then(|s| parse_month_ru(&s));
        if let Some(ym) = ym {
            if let Some(v) = range.get((data_row, col)).and_then(cell_f64) {
                *monthly.entry(ym).or_insert(0.0) += v;
            }
        }
    }
    for ((y, m), v) in monthly {
        out.insert(format!("{}Q{}", y, (m - 1) / 3 + 1), v);
    }
    eprintln!("[OK] monthly {} -> {} quarters", label, out.len());
    out
}

pub struct Inputs<'a> {
    pub cbr01: &'a str,
    pub cbr02: &'a str,
    pub cbr03: &'a str,
    pub cbr04: &'a str,
}

pub struct BuiltPanel {
    pub panel: BTreeMap<String, BTreeMap<String, f64>>,
    pub check: BTreeMap<String, (f64, f64)>,
}

pub fn build(inputs: Inputs) -> Result<BuiltPanel, Box<dyn Error>> {
    eprintln!("Reading {}", inputs.cbr01);
    let mut wb1 = open_workbook_auto(inputs.cbr01)?;
    let r1 = wb1.worksheet_range("Операции")?;

    let d_oth_total = extract_quarterly(
        &r1,
        "Другие депозиты в кредитных организациях - всего",
        parse_q_ru,
    );
    let d_oth_rub = extract_quarterly(
        &r1,
        "Другие депозиты в кредитных организациях в рублях",
        parse_q_ru,
    );
    let d_tr = extract_quarterly(
        &r1,
        "Переводные депозиты в кредитных организациях - всего",
        parse_q_ru,
    );
    let e = extract_quarterly(&r1, "Котируемые акции резидентов - всего", parse_q_ru);
    let b = extract_quarterly(&r1, "Долговые ценные бумаги", parse_q_ru);
    let f = extract_quarterly(
        &r1,
        "Паи и акции инвестиционных фондов - резидентов - всего",
        parse_q_ru,
    );
    let esc = extract_quarterly(&r1, "Средства на счетах эскроу", parse_q_ru);

    eprintln!("Reading {}", inputs.cbr02);
    let mut wb2 = open_workbook_auto(inputs.cbr02)?;
    let r2 = wb2.worksheet_range("Балансы")?;
    let a = extract_quarterly(&r2, "АКТИВЫ(2)", parse_q_date);

    eprintln!("Reading {} (check)", inputs.cbr03);
    let mut wb3 = open_workbook_auto(inputs.cbr03)?;
    let r3 = wb3.worksheet_range("Операции")?;
    let d_oth_monthly =
        extract_monthly_aggregated(&r3, "Другие депозиты в кредитных организациях - всего");

    eprintln!("Checking {}", inputs.cbr04);
    let wb4 = open_workbook_auto(inputs.cbr04)?;
    let _ = wb4.sheet_names();

    let mut panel: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for (q, a_val) in &a {
        let mut m: BTreeMap<String, f64> = BTreeMap::new();
        m.insert("A".into(), *a_val);
        for (k, src) in [
            ("D_oth_total", &d_oth_total),
            ("D_oth_rub", &d_oth_rub),
            ("D_tr", &d_tr),
            ("E", &e),
            ("B", &b),
            ("F", &f),
            ("ESC", &esc),
        ] {
            if let Some(v) = src.get(q) {
                m.insert(k.into(), *v);
            }
        }
        panel.insert(q.clone(), m);
    }

    let mut check: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for (q, vq) in &d_oth_total {
        let vm = d_oth_monthly.get(q).copied().unwrap_or(f64::NAN);
        check.insert(q.clone(), (*vq, vm));
    }

    Ok(BuiltPanel { panel, check })
}
