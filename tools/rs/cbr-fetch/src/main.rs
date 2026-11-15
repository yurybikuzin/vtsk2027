use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Row {
    code: String,
    group: String,
    name: String,
    url: String,
    format: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    hypotheses: String,
    #[serde(default)]
    verification: String,
    #[serde(default)]
    notes: String,
}

#[derive(Clone, Copy)]
enum Action {
    Save(&'static str),
    Check,
}

fn action_for(format: &str) -> Action {
    match format.trim().to_ascii_lowercase().as_str() {
        "xlsx" => Action::Save("xlsx"),
        "pdf" => Action::Save("pdf"),
        "zip" => Action::Save("zip"),
        "html" => Action::Save("html"),
        _ => Action::Check,
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn filename_from_url(url: &str, fallback: &str) -> String {
    let trimmed = url.split(['?', '#']).next().unwrap_or(url);
    let last = trimmed
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    if last.is_empty() {
        sanitize(fallback)
    } else {
        sanitize(&last)
    }
}

fn target_name(row: &Row, ext: &str) -> String {
    let name = filename_from_url(&row.url, &row.code);
    let name = if Path::new(&name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
    {
        name
    } else {
        format!("{}.{}", name, ext)
    };
    format!("{}_{}", row.code, name)
}

enum Outcome {
    Downloaded { path: PathBuf, bytes: u64 },
    AlreadyPresent { path: PathBuf, bytes: u64 },
    Reachable { status: u16 },
}

fn fetch(
    client: &reqwest::blocking::Client,
    row: &Row,
    action: Action,
    out_dir: &Path,
) -> Result<Outcome> {
    match action {
        Action::Check => {
            let resp = client
                .get(&row.url)
                .send()
                .with_context(|| format!("GET {}", row.url))?;
            let status = resp.status();
            if !status.is_success() {
                bail!("HTTP {}", status.as_u16());
            }
            Ok(Outcome::Reachable {
                status: status.as_u16(),
            })
        }
        Action::Save(ext) => {
            let name = target_name(row, ext);
            let path = out_dir.join(&name);

            if path.exists() {
                let bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                return Ok(Outcome::AlreadyPresent { path, bytes });
            }

            let resp = client
                .get(&row.url)
                .send()
                .with_context(|| format!("GET {}", row.url))?;
            let status = resp.status();
            if !status.is_success() {
                bail!("HTTP {}", status.as_u16());
            }
            let bytes = resp.bytes().context("read body")?;
            if bytes.is_empty() {
                bail!("empty body");
            }
            fs::write(&path, &bytes).with_context(|| format!("write {}", path.display()))?;
            Ok(Outcome::Downloaded {
                path,
                bytes: bytes.len() as u64,
            })
        }
    }
}

struct Args {
    registry: PathBuf,
    out_dir: PathBuf,
}

fn usage() -> ! {
    eprintln!("usage: cbr-fetch --registry <CSV> --out-dir <DIR>");
    eprintln!();
    eprintln!("  --registry  path to CBR_Deposits_Sources_Registry CSV (given data)");
    eprintln!("  --out-dir   directory for downloaded files and .failed report");
    std::process::exit(2);
}

fn parse_args() -> Result<Args> {
    let mut registry: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--registry" => {
                registry =
                    Some(PathBuf::from(it.next().ok_or_else(|| {
                        anyhow::anyhow!("--registry needs a value")
                    })?));
            }
            "--out-dir" => {
                out_dir = Some(PathBuf::from(
                    it.next()
                        .ok_or_else(|| anyhow::anyhow!("--out-dir needs a value"))?,
                ));
            }
            "--help" | "-h" => usage(),
            other => return Err(anyhow::anyhow!("unknown arg: {other}")),
        }
    }

    let Some(registry) = registry else { usage() };
    let Some(out_dir) = out_dir else { usage() };

    Ok(Args { registry, out_dir })
}

struct OkEntry {
    code: String,
    url: String,
    bytes: u64,
    downloaded_now: bool,
}

struct CheckEntry {
    code: String,
    url: String,
    status: u16,
}

struct FailEntry {
    code: String,
    url: String,
    reason: String,
}

fn main() -> Result<()> {
    let args = parse_args()?;

    fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("create {}", args.out_dir.display()))?;

    let fail_path = args.out_dir.join(".failed");

    let raw = fs::read_to_string(&args.registry)
        .with_context(|| format!("read {}", args.registry.display()))?;
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    let mut rdr = csv::ReaderBuilder::new().from_reader(raw.as_bytes());

    let mut rows: Vec<Row> = Vec::new();
    for (i, rec) in rdr.deserialize::<Row>().enumerate() {
        match rec {
            Ok(row) => rows.push(row),
            Err(e) => eprintln!("CSV row {} parse error: {e}", i + 2),
        }
    }

    println!(
        "Loaded {} rows from {}",
        rows.len(),
        args.registry.display()
    );
    println!("Out dir       : {}", args.out_dir.display());

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("Mozilla/5.0 (compatible; cbr-fetch/0.1; +https://www.cbr.ru/)")
        .danger_accept_invalid_certs(true)
        .build()?;

    let mut ok_list: Vec<OkEntry> = Vec::new();
    let mut check_list: Vec<CheckEntry> = Vec::new();
    let mut fail_list: Vec<FailEntry> = Vec::new();

    for (i, row) in rows.iter().enumerate() {
        let action = action_for(&row.format);
        let kind = match action {
            Action::Save(ext) => format!("download .{ext}"),
            Action::Check => "check".to_string(),
        };
        println!(
            "[{}/{}] {} | {} | {} | {}",
            i + 1,
            rows.len(),
            row.code,
            row.name,
            kind,
            row.url
        );

        match fetch(&client, row, action, &args.out_dir) {
            Ok(Outcome::Downloaded { path, bytes }) => {
                println!("    ok: saved {} bytes -> {}", bytes, path.display());
                ok_list.push(OkEntry {
                    code: row.code.clone(),
                    url: row.url.clone(),
                    bytes,
                    downloaded_now: true,
                });
            }
            Ok(Outcome::AlreadyPresent { path, bytes }) => {
                println!(
                    "    skip: already exists ({} bytes) -> {}",
                    bytes,
                    path.display()
                );
                ok_list.push(OkEntry {
                    code: row.code.clone(),
                    url: row.url.clone(),
                    bytes,
                    downloaded_now: false,
                });
            }
            Ok(Outcome::Reachable { status }) => {
                println!("    ok: reachable (HTTP {})", status);
                check_list.push(CheckEntry {
                    code: row.code.clone(),
                    url: row.url.clone(),
                    status,
                });
            }
            Err(e) => {
                println!("    FAIL: {e}");
                fail_list.push(FailEntry {
                    code: row.code.clone(),
                    url: row.url.clone(),
                    reason: format!("{e:#}"),
                });
            }
        }
    }

    if fail_list.is_empty() {
        let _ = fs::remove_file(&fail_path);
    } else {
        let mut f = fs::File::create(&fail_path)
            .with_context(|| format!("create {}", fail_path.display()))?;
        writeln!(f, "code\turl\terror")?;
        for e in &fail_list {
            writeln!(f, "{}\t{}\t{}", e.code, e.url, e.reason)?;
        }
    }

    let downloaded = ok_list.iter().filter(|e| e.downloaded_now).count();
    let already = ok_list.len() - downloaded;

    println!();
    println!("==== summary ====");
    println!(
        "ok: {} (downloaded this run: {}, already present: {})",
        ok_list.len(),
        downloaded,
        already
    );
    for e in &ok_list {
        if e.downloaded_now {
            println!("  + {} {} ({} bytes)", e.code, e.url, e.bytes);
        } else {
            println!(
                "  + {} {} ({} bytes, already present)",
                e.code, e.url, e.bytes
            );
        }
    }
    if !check_list.is_empty() {
        println!("reachable: {}", check_list.len());
        for e in &check_list {
            println!("  . {} {} (HTTP {})", e.code, e.url, e.status);
        }
    }
    println!("fail: {}", fail_list.len());
    for e in &fail_list {
        println!("  - {} {} ({})", e.code, e.url, e.reason);
    }
    println!(
        "total: {} rows, {} ok, {} reachable, {} fail",
        rows.len(),
        ok_list.len(),
        check_list.len(),
        fail_list.len()
    );
    if !fail_list.is_empty() {
        println!("failures: {}", fail_path.display());
    }

    Ok(())
}
