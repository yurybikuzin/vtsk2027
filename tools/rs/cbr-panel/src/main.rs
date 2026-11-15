use calamine::open_workbook_auto;
use calamine::Reader;
use cbr_panel::check::{coverage, print_coverage};
use cbr_panel::extract::{extract_rows, ExtractSpec};
use cbr_panel::extract_config;
use cbr_panel::panel::{build, Inputs};
use cbr_panel::quarterly::monthly_to_quarterly;
use clap::{Parser, Subcommand};
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "cbr-panel",
    version,
    about = "Панель и внешние ряды для проекта по депозитам"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Собрать базовую панель из четырёх файлов ЦБ
    Panel {
        #[arg(long)]
        cbr01: PathBuf,
        #[arg(long)]
        cbr02: PathBuf,
        #[arg(long)]
        cbr03: PathBuf,
        #[arg(long)]
        cbr04: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Вытащить длинную таблицу рядов из произвольной книги (xlsx -> long xlsx)
    Extract {
        input: PathBuf,
        #[arg(long)]
        sheet: String,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        section: Option<String>,
        #[arg(long = "row", value_parser = parse_kv)]
        rows: Vec<(String, String)>,
        #[arg(long, default_value = "%")]
        unit: String,
        #[arg(long, default_value_t = true)]
        monthly: bool,
    },
    /// Свернуть месячный длинный xlsx в кварталы (среднее по трём месяцам)
    Quarterly {
        #[arg(long)]
        r#in: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Приклеить квартальные ряды к панели по period (с бэкапом перед перезаписью)
    Merge {
        #[arg(long)]
        panel: PathBuf,
        #[arg(long, value_delimiter = ',')]
        rows: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        min_quarter: Option<String>,
        #[arg(long)]
        max_quarter: Option<String>,
    },
    /// Отчёт по покрытию столбцов панели
    Check {
        #[arg(long)]
        panel: PathBuf,
        #[arg(long, default_value = "2018Q1")]
        first: String,
        #[arg(long, default_value = "2026Q2")]
        last: String,
    },
    /// Длинный csv из config.toml (альтернативные и сверочные ряды)
    ExtractConfig {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        log: Option<PathBuf>,
        #[arg(long)]
        only: Option<String>,
    },

    /// Найти строки листа по подстроке в первом столбце
    Grep {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        sheet: String,
        #[arg(long)]
        pattern: String,
        #[arg(long, default_value_t = false)]
        ignore_case: bool,
    },

    /// Извлечь длинную таблицу из сохранённого HTML (table с классом)
    ExtractHtml {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        table_class: String,
        #[arg(long)]
        series_id: String,
        #[arg(long, default_value = "%")]
        unit: String,
        #[arg(long)]
        value_col: usize,
        #[arg(long)]
        out: PathBuf,
    },
    /// Ключевая ставка через SOAP DailyInfoWebServ (KeyRateXML)
    FetchKeyRate {
        #[arg(long, default_value = "2018-01-01")]
        from: String,
        #[arg(long, default_value = "2026-12-31")]
        to: String,
        #[arg(
            long,
            default_value = "https://www.cbr.ru/DailyInfoWebServ/DailyInfo.asmx"
        )]
        url: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Кривая бескупонной доходности ОФЗ с сайта ЦБ по кускам
    FetchZcyc {
        #[arg(long, default_value = "2018-01-01")]
        from: String,
        #[arg(long, default_value = "2026-12-31")]
        to: String,
        #[arg(long, default_value_t = 6)]
        chunk_months: u32,
        #[arg(long, default_value = "https://www.cbr.ru/hd_base/zcyc_params/")]
        url: String,
        #[arg(long, default_value_t = 4)]
        value_col: usize,
        #[arg(long)]
        out: PathBuf,
    },
    /// MCFTR и IMOEX через MOEX ISS: Vol, R_E, q_DIV по торговым дням
    FetchMoex {
        #[arg(long, default_value = "2018-01-01")]
        from: String,
        #[arg(long, default_value = "2026-12-31")]
        to: String,
        #[arg(long, default_value_t = 60)]
        vol_window: usize,
        #[arg(long, default_value_t = 252)]
        year_window: usize,
        #[arg(long, default_value = "https://iss.moex.com")]
        base_url: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Курс доллара к рублю через XML_dynamic ЦБ
    FetchUsd {
        #[arg(long, default_value = "01.01.2018")]
        from: String,
        #[arg(long, default_value = "31.12.2026")]
        to: String,
        #[arg(long, default_value = "https://www.cbr.ru")]
        url: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Добавить производные столбцы (s, m, d_usd, outlier) в панель
    Enrich {
        #[arg(long)]
        panel: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

fn parse_kv(s: &str) -> Result<(String, String), String> {
    s.split_once(':')
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .ok_or_else(|| "ожидалось id:название".to_string())
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Panel {
            cbr01,
            cbr02,
            cbr03,
            cbr04,
            out,
        } => {
            let built = build(Inputs {
                cbr01: cbr01.to_str().ok_or("cbr01 path not utf-8")?,
                cbr02: cbr02.to_str().ok_or("cbr02 path not utf-8")?,
                cbr03: cbr03.to_str().ok_or("cbr03 path not utf-8")?,
                cbr04: cbr04.to_str().ok_or("cbr04 path not utf-8")?,
            })?;
            write_panel_xlsx(&out, &built.panel, &built.check)?;
            eprintln!("Done: {}", out.display());
        }
        Cmd::Extract {
            input,
            sheet,
            out,
            section,
            rows,
            unit,
            monthly,
        } => {
            let mut wb = open_workbook_auto(&input)?;
            let range = wb.worksheet_range(&sheet)?;
            let src_name = input
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let specs: Vec<ExtractSpec> = rows
                .into_iter()
                .map(|(id, label)| ExtractSpec {
                    id,
                    label,
                    section: section.clone(),
                    monthly,
                    unit: unit.clone(),
                })
                .collect();
            let long = extract_rows(&range, &specs, &src_name, &sheet);
            let n = cbr_panel::excel::write_long_xlsx(&out, &long)?;
            eprintln!("rows written: {} -> {}", n, out.display());
        }
        Cmd::Quarterly { r#in, out } => {
            let rows = cbr_panel::excel::read_long_xlsx(&r#in)?;
            let q = monthly_to_quarterly(&rows);
            let n = cbr_panel::excel::write_quarterly_xlsx(&out, &q)?;
            eprintln!("quarters written: {} -> {}", n, out.display());
        }
        Cmd::Merge {
            panel,
            rows,
            out,
            min_quarter,
            max_quarter,
        } => {
            let backup = cbr_panel::backup::backup_before_overwrite(Path::new(&out), 5)?;
            if let Some(b) = &backup {
                eprintln!("[OK] backup: {}", b.display());
            }
            let panel_table = cbr_panel::merge::read_panel(Path::new(&panel))?;
            let mut ext: BTreeMap<(String, String), f64> = BTreeMap::new();
            for p in &rows {
                for (k, v) in cbr_panel::merge::read_quarterly_xlsx_file(p)? {
                    ext.insert(k, v);
                }
            }
            let merged = cbr_panel::merge::merge_into(&panel_table, &ext);
            let merged = cbr_panel::merge::filter_quarters(
                &merged,
                min_quarter.as_deref(),
                max_quarter.as_deref(),
            );
            cbr_panel::merge::write_enriched(Path::new(&out), &merged)?;
            eprintln!(
                "merged: {} columns, {} quarters -> {}",
                merged.columns.len(),
                merged.data.len(),
                out.display()
            );
        }
        Cmd::Check { panel, first, last } => {
            let panel_table = cbr_panel::merge::read_panel(Path::new(&panel))?;
            let columns: Vec<&str> = panel_table.columns.iter().map(|s| s.as_str()).collect();
            let cov = coverage(&panel_table.data, &columns, &first, &last);
            print_coverage(&cov);
        }
        Cmd::ExtractConfig {
            config,
            root,
            out,
            log,
            only,
        } => {
            extract_config::run(&config, &root, &out, log.as_deref(), only.as_deref())?;
        }
        Cmd::Grep {
            input,
            sheet,
            pattern,
            ignore_case,
        } => {
            cbr_panel::grep::run(&input, &sheet, &pattern, ignore_case)?;
        }
        Cmd::ExtractHtml {
            input,
            table_class,
            series_id,
            unit,
            value_col,
            out,
        } => {
            cbr_panel::extract_html::run(cbr_panel::extract_html::Args {
                input: &input,
                table_class: &table_class,
                series_id: &series_id,
                unit: &unit,
                value_col,
                out: &out,
            })?;
        }
        Cmd::FetchKeyRate { from, to, url, out } => {
            cbr_panel::fetch_key_rate::run(cbr_panel::fetch_key_rate::Args {
                from: &from,
                to: &to,
                url: &url,
                out: &out,
            })?;
        }
        Cmd::FetchZcyc {
            from,
            to,
            chunk_months,
            url,
            value_col,
            out,
        } => {
            cbr_panel::fetch_zcyc::run(cbr_panel::fetch_zcyc::Args {
                from: &from,
                to: &to,
                chunk_months,
                url: &url,
                value_col,
                out: &out,
            })?;
        }
        Cmd::FetchMoex {
            from,
            to,
            vol_window,
            year_window,
            base_url,
            out,
        } => {
            cbr_panel::fetch_moex::run(cbr_panel::fetch_moex::Args {
                from: &from,
                to: &to,
                vol_window,
                year_window,
                base_url: &base_url,
                out: &out,
            })?;
        }
        Cmd::FetchUsd { from, to, url, out } => {
            cbr_panel::fetch_usd::run(cbr_panel::fetch_usd::Args {
                from: &from,
                to: &to,
                url: &url,
                out: &out,
            })?;
        }
        Cmd::Enrich { panel, out } => {
            cbr_panel::enrich::run(cbr_panel::enrich::Args {
                panel: &panel,
                out: &out,
            })?;
        }
    }
    Ok(())
}

fn write_panel_xlsx(
    out_path: &Path,
    panel: &BTreeMap<String, BTreeMap<String, f64>>,
    check: &BTreeMap<String, (f64, f64)>,
) -> Result<(), Box<dyn Error>> {
    let mut wb = Workbook::new();

    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xDDEBF7))
        .set_align(FormatAlign::Center);
    let int_fmt = Format::new().set_num_format("0.0");
    let num_fmt = Format::new().set_num_format("0.000");

    let ws = wb.add_worksheet();
    ws.set_name("Panel")?;

    let headers = [
        "Quarter",
        "A_bln",
        "D_oth_total",
        "D_oth_rub",
        "D_tr",
        "E",
        "B",
        "F",
        "ESC",
        "f_D_oth_total",
        "f_D_oth_rub",
        "f_D_tr",
        "f_E",
        "f_B",
        "f_F",
        "f_ESC",
    ];
    for (i, h) in headers.iter().enumerate() {
        ws.write_with_format(0, i as u16, *h, &header_fmt)?;
    }

    let keys = ["D_oth_total", "D_oth_rub", "D_tr", "E", "B", "F", "ESC"];

    let mut row = 1u32;
    for (q, vals) in panel {
        ws.write(row, 0, q.as_str())?;
        let a = vals.get("A").copied().unwrap_or(0.0);
        ws.write_with_format(row, 1, a, &int_fmt)?;

        for (j, k) in keys.iter().enumerate() {
            if let Some(v) = vals.get(*k) {
                ws.write_with_format(row, (2 + j) as u16, *v, &int_fmt)?;
            }
        }
        for (j, k) in keys.iter().enumerate() {
            if let Some(v) = vals.get(*k) {
                let norm = if a != 0.0 { v / a * 100.0 } else { 0.0 };
                ws.write_with_format(row, (9 + j) as u16, norm, &num_fmt)?;
            }
        }
        row += 1;
    }

    ws.set_column_width(0, 10)?;
    for i in 1..16u16 {
        ws.set_column_width(i, 14)?;
    }

    let ws2 = wb.add_worksheet();
    ws2.set_name("Check")?;
    let check_headers = [
        "Quarter",
        "D_oth_total (quarterly, CBR01)",
        "D_oth_total (monthly sum, CBR03)",
        "Diff",
    ];
    for (i, h) in check_headers.iter().enumerate() {
        ws2.write_with_format(0, i as u16, *h, &header_fmt)?;
    }
    let mut r = 1u32;
    for (q, (vq, vm)) in check {
        ws2.write(r, 0, q.as_str())?;
        ws2.write_with_format(r, 1, *vq, &int_fmt)?;
        ws2.write_with_format(r, 2, *vm, &int_fmt)?;
        ws2.write_with_format(r, 3, vm - vq, &int_fmt)?;
        r += 1;
    }
    for i in 0..4u16 {
        ws2.set_column_width(i, 30)?;
    }

    wb.save(out_path)?;
    Ok(())
}
