use anyhow::{anyhow, Context, Result};
use regex::Regex;
use reqwest::blocking::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
struct Candidate {
    url: String,
    title: String,
    filename: String,
}

#[derive(Debug, Serialize)]
struct ManifestEntry {
    url: String,
    filename: String,
    title: String,
    sha256: String,
    bytes: u64,
    saved_as: String,
}

#[derive(Debug, Serialize)]
struct SkippedEntry {
    url: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct Manifest {
    generated_at: String,
    source_html: String,
    base_url: String,
    out_dir: String,
    entries: Vec<ManifestEntry>,
    skipped: Vec<SkippedEntry>,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Links,
    Section,
}

struct Args {
    html: String,
    out_dir: String,
    base_url: String,
    user_agent: String,
    referer: String,
    section: Option<String>,
    title_filters: Vec<String>,
    mode: Mode,
    dry_run: bool,
}

fn parse_mode(s: &str) -> Result<Mode> {
    match s {
        "links" => Ok(Mode::Links),
        "section" => Ok(Mode::Section),
        other => Err(anyhow!(
            "unknown --mode: {other} (expected links or section)"
        )),
    }
}

fn parse_args() -> Result<Args> {
    let mut html: Option<String> = None;
    let mut out_dir: Option<String> = None;
    let mut base_url = String::from("https://rosstat.gov.ru");
    let mut user_agent = String::from(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36",
    );
    let mut referer = String::from("https://rosstat.gov.ru/folder/13397");
    let mut section: Option<String> = None;
    let mut title_filters: Vec<String> = Vec::new();
    let mut mode = Mode::Links;
    let mut dry_run = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--html" => html = Some(it.next().ok_or_else(|| anyhow!("--html needs a value"))?),
            "--out-dir" => {
                out_dir = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--out-dir needs a value"))?,
                )
            }
            "--base-url" => {
                base_url = it
                    .next()
                    .ok_or_else(|| anyhow!("--base-url needs a value"))?
            }
            "--user-agent" => {
                user_agent = it
                    .next()
                    .ok_or_else(|| anyhow!("--user-agent needs a value"))?
            }
            "--referer" => {
                referer = it
                    .next()
                    .ok_or_else(|| anyhow!("--referer needs a value"))?
            }
            "--section" => {
                section = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--section needs a value"))?,
                )
            }
            "--title-filters" => {
                let raw = it
                    .next()
                    .ok_or_else(|| anyhow!("--title-filters needs a value"))?;
                title_filters = raw
                    .split('|')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "--mode" => {
                let raw = it.next().ok_or_else(|| anyhow!("--mode needs a value"))?;
                mode = parse_mode(&raw)?;
            }
            "--dry-run" | "-n" => dry_run = true,
            "--help" | "-h" => {
                println!(
                    "usage: rosstat-fetch --html <FILE> [--mode links|section] [--out-dir DIR]"
                );
                println!("  --html          saved HTML page (input)");
                println!(
                    "  --mode          links: all .xlsx on page; section: restricted to --section"
                );
                println!("  --section       section header for --mode section");
                println!("  --title-filters 'a|b|c' optional title substring filter");
                println!("  --out-dir       default: directory of --html");
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown arg: {other}")),
        }
    }

    let html = html.ok_or_else(|| anyhow!("--html is required"))?;
    let out_dir = out_dir.unwrap_or_else(|| {
        Path::new(&html)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".".to_string())
    });

    if mode == Mode::Section && section.is_none() {
        return Err(anyhow!("--mode section requires --section"));
    }

    Ok(Args {
        html,
        out_dir,
        base_url,
        user_agent,
        referer,
        section,
        title_filters,
        mode,
        dry_run,
    })
}

fn slice_section<'a>(html: &'a str, marker: &str) -> &'a str {
    let start = match html.find(marker) {
        Some(p) => p,
        None => return html,
    };
    let after = &html[start + marker.len()..];
    let end = after
        .find("<h2")
        .map(|rel| start + marker.len() + rel)
        .unwrap_or(html.len());
    &html[start..end]
}

fn sanitize(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);
    let decoded = base.replace("%20", " ");
    let cleaned: String = decoded
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    cleaned.trim().to_string()
}

fn absolute_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        let scheme = if base.starts_with("https://") {
            "https:"
        } else {
            "http:"
        };
        return format!("{scheme}{href}");
    }
    if href.starts_with('/') {
        return format!("{}{}", base.trim_end_matches('/'), href);
    }
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        href.trim_start_matches('/')
    )
}

fn extract_candidates_links(block: &str, base_url: &str) -> Vec<Candidate> {
    let anchor_re =
        Regex::new(r#"(?is)<a\b[^>]*?href\s*=\s*"([^"]+\.xlsx)"[^>]*>(.*?)</a>"#).unwrap();
    let tag_re = Regex::new(r"(?is)<[^>]+>").unwrap();
    let ws_re = Regex::new(r"\s+").unwrap();

    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for cap in anchor_re.captures_iter(block) {
        let href = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let inner = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let title = ws_re
            .replace_all(&tag_re.replace_all(inner, " "), " ")
            .trim()
            .to_string();
        let abs = absolute_url(base_url, href);
        if seen.insert(abs.clone()) {
            let filename = sanitize(href);
            out.push(Candidate {
                url: abs,
                title,
                filename,
            });
        }
    }
    out
}

fn extract_candidates_section(block: &str, base_url: &str) -> Vec<Candidate> {
    let item_re = Regex::new(r#"(?is)<div\s+class="document-list__item[^"]*"[^>]*>(.*?)</div>\s*(?:</div>|<div\s+class="document-list__item-links")"#).unwrap();
    let href_re = Regex::new(r#"(?is)href\s*=\s*"([^"]+\.xlsx)""#).unwrap();
    let title_re =
        Regex::new(r#"(?is)<div\s+class="document-list__item-title"[^>]*>(.*?)</div>"#).unwrap();
    let tag_re = Regex::new(r"(?is)<[^>]+>").unwrap();
    let ws_re = Regex::new(r"\s+").unwrap();

    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for cap in item_re.captures_iter(block) {
        let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let href = match href_re.captures(inner).and_then(|c| c.get(1)) {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        let title = title_re
            .captures(inner)
            .and_then(|c| c.get(1))
            .map(|m| {
                ws_re
                    .replace_all(&tag_re.replace_all(m.as_str(), " "), " ")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let abs = absolute_url(base_url, &href);
        if !seen.insert(abs.clone()) {
            continue;
        }
        let filename = sanitize(&href);
        out.push(Candidate {
            url: abs,
            title,
            filename,
        });
    }

    if out.is_empty() {
        for c in extract_candidates_links(block, base_url) {
            if seen.insert(c.url.clone()) {
                out.push(c);
            }
        }
    }
    out
}

fn matches_filter(title: &str, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    let t = title.to_lowercase();
    filters.iter().any(|f| t.contains(&f.to_lowercase()))
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = p
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("xlsx")
        .to_string();
    for i in 1..1000 {
        let candidate = dir.join(format!("{stem}.{i}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}.dup.{ext}"))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f =
        fs::File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
    f.write_all(bytes)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

struct OkEntry {
    url: String,
    title: String,
    bytes: u64,
    sha256: String,
    downloaded_now: bool,
}

struct FailEntry {
    url: String,
    reason: String,
}

fn main() -> Result<()> {
    let args = parse_args()?;

    let html = fs::read_to_string(&args.html)
        .with_context(|| format!("cannot read html file {}", args.html))?;

    let block = match (&args.mode, &args.section) {
        (Mode::Section, Some(sec)) => slice_section(&html, sec),
        _ => html.as_str(),
    };

    let mut candidates = match args.mode {
        Mode::Links => extract_candidates_links(block, &args.base_url),
        Mode::Section => extract_candidates_section(block, &args.base_url),
    };

    if candidates.is_empty() {
        return Err(anyhow!(
            "no .xlsx links found in {} (mode {:?})",
            args.html,
            match args.mode {
                Mode::Links => "links",
                Mode::Section => "section",
            }
        ));
    }

    candidates.retain(|c| matches_filter(&c.title, &args.title_filters));
    eprintln!("found {} candidates after title filter", candidates.len());

    if args.dry_run {
        let out_dir = PathBuf::from(&args.out_dir);
        for c in &candidates {
            let target = out_dir.join(&c.filename);
            let mark = if target.exists() {
                " [exists, will skip]"
            } else {
                ""
            };
            eprintln!("url:   {}", c.url);
            eprintln!("file:  {}{}", c.filename, mark);
            eprintln!("title: {}", c.title);
            eprintln!();
        }
        eprintln!(
            "dry-run: {} candidates, nothing downloaded",
            candidates.len()
        );
        return Ok(());
    }

    let out_dir = PathBuf::from(&args.out_dir);
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("cannot create out dir {}", out_dir.display()))?;

    let client = Client::builder()
        .user_agent(&args.user_agent)
        .timeout(Duration::from_secs(120))
        .danger_accept_invalid_certs(true)
        .build()
        .context("failed to build http client")?;

    let mut entries: Vec<ManifestEntry> = Vec::new();
    let mut skipped: Vec<SkippedEntry> = Vec::new();
    let mut ok_list: Vec<OkEntry> = Vec::new();
    let mut fail_list: Vec<FailEntry> = Vec::new();

    for c in candidates {
        let target = out_dir.join(&c.filename);

        if target.exists() {
            let size = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
            eprintln!(
                "fetch {} ... skip (already exists, {} bytes)",
                c.filename, size
            );
            skipped.push(SkippedEntry {
                url: c.url.clone(),
                reason: format!("already exists ({} bytes)", size),
            });
            ok_list.push(OkEntry {
                url: c.url,
                title: c.title,
                bytes: size,
                sha256: String::new(),
                downloaded_now: false,
            });
            continue;
        }

        println!("fetching {}", c.url);

        let resp = client
            .get(&c.url)
            .header("Referer", &args.referer)
            .header(
                "Accept",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/octet-stream,*/*",
            )
            .send()
            .with_context(|| format!("request failed {}", c.url))?;

        let status = resp.status();
        if !status.is_success() {
            skipped.push(SkippedEntry {
                url: c.url.clone(),
                reason: format!("http status {status}"),
            });
            fail_list.push(FailEntry {
                url: c.url,
                reason: format!("HTTP {}", status.as_u16()),
            });
            continue;
        }

        let ctype = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = resp
            .bytes()
            .with_context(|| format!("cannot read body {}", c.url))?;

        if bytes.len() < 2 || &bytes[..2] != b"PK" {
            let reason = format!(
                "not a zip/xlsx (content-type={}, first bytes: {:02x?})",
                ctype,
                &bytes[..bytes.len().min(4)]
            );
            skipped.push(SkippedEntry {
                url: c.url.clone(),
                reason: reason.clone(),
            });
            fail_list.push(FailEntry { url: c.url, reason });
            continue;
        }

        let saved_as = unique_path(&out_dir, &c.filename);
        write_file(&saved_as, &bytes)?;

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha = hex::encode(hasher.finalize());

        entries.push(ManifestEntry {
            url: c.url.clone(),
            filename: c.filename.clone(),
            title: c.title.clone(),
            sha256: sha.clone(),
            bytes: bytes.len() as u64,
            saved_as: saved_as.display().to_string(),
        });
        ok_list.push(OkEntry {
            url: c.url,
            title: c.title,
            bytes: bytes.len() as u64,
            sha256: sha,
            downloaded_now: true,
        });
    }

    let manifest = Manifest {
        generated_at: chrono::Utc::now().to_rfc3339(),
        source_html: args.html.clone(),
        base_url: args.base_url.clone(),
        out_dir: args.out_dir.clone(),
        entries,
        skipped,
    };

    let manifest_path = out_dir.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, json)
        .with_context(|| format!("cannot write manifest {}", manifest_path.display()))?;

    let downloaded = ok_list.iter().filter(|e| e.downloaded_now).count();
    let already = ok_list.len() - downloaded;

    eprintln!();
    eprintln!("==== summary ====");
    eprintln!(
        "ok: {} (downloaded this run: {}, already present: {})",
        ok_list.len(),
        downloaded,
        already
    );
    for e in &ok_list {
        if e.downloaded_now {
            eprintln!(
                "  + {} ({} bytes, sha256 {}, title: {})",
                e.url,
                e.bytes,
                &e.sha256[..12.min(e.sha256.len())],
                e.title
            );
        } else {
            eprintln!(
                "  + {} ({} bytes, already present, title: {})",
                e.url, e.bytes, e.title
            );
        }
    }
    eprintln!("fail: {}", fail_list.len());
    for e in &fail_list {
        eprintln!("  - {} ({})", e.url, e.reason);
    }
    eprintln!(
        "total: {} candidates, {} ok, {} fail",
        ok_list.len() + fail_list.len(),
        ok_list.len(),
        fail_list.len()
    );
    eprintln!("manifest: {}", manifest_path.display());

    Ok(())
}
