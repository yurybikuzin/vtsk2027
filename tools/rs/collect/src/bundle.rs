use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::walker;

fn want_file(name: &str) -> bool {
    name.ends_with(".rs")
        || name.ends_with(".sh")
        || name.ends_with(".toml")
        || name == "README.md"
        || name == "Makefile"
        || name == ".gitignore"
        || name == ".collectignore"
}

fn lang_for(name: &str) -> Option<&'static str> {
    if name.ends_with(".rs") {
        Some("rust")
    } else if name.ends_with(".sh") {
        Some("bash")
    } else if name.ends_with(".toml") {
        Some("toml")
    } else if name == "README.md" {
        Some("markdown")
    } else if name == "Makefile" {
        Some("makefile")
    } else {
        None
    }
}

pub fn run(dir: &Path, out_path: &Path, max_bytes: usize) -> Result<()> {
    let root = dir.canonicalize().context("canonicalize dir")?;
    let walker = walker::build(&root)?;

    let mut files: Vec<PathBuf> = Vec::new();
    for result in walker {
        let entry = result.context("walk error")?;
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.into_path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if want_file(name) {
            files.push(path);
        }
    }

    files.sort();

    if files.is_empty() {
        anyhow::bail!(
            "No source, config or build files found under {}",
            root.display()
        );
    }

    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let dir_out = out_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = out_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("bundle");
    let ext = out_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("txt");

    let mut part: usize = 0;
    let mut buf = String::new();
    let mut written: Vec<PathBuf> = Vec::new();

    let flush = |buf: &mut String,
                 part: usize,
                 files_in_part: &[PathBuf],
                 written: &mut Vec<PathBuf>|
     -> Result<()> {
        let name = format!("{}.{:03}.{}", stem, part, ext);
        let path = dir_out.join(name);
        let mut body = header(&root, part, files_in_part.len(), &ts);
        body.push_str(buf);
        fs::write(&path, body.as_bytes()).with_context(|| format!("write {}", path.display()))?;
        written.push(path);
        buf.clear();
        Ok(())
    };

    let mut current: Vec<PathBuf> = Vec::new();

    for f in &files {
        let rel = f.strip_prefix(&root).unwrap_or(f);
        let name = f.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let mut piece = String::new();
        piece.push_str(&format!("## {}\n", rel.display()));
        if let Some(lang) = lang_for(name) {
            piece.push_str(&format!("```{}\n", lang));
        } else {
            piece.push_str("```\n");
        }
        let content = fs::read_to_string(f).with_context(|| format!("read {}", f.display()))?;
        piece.push_str(&content);
        if !content.ends_with('\n') {
            piece.push('\n');
        }
        piece.push_str("```\n\n");

        if !current.is_empty() && buf.len() + piece.len() > max_bytes {
            flush(&mut buf, part, &current, &mut written)?;
            part += 1;
            current.clear();
        }

        buf.push_str(&piece);
        current.push(f.clone());
    }

    if !current.is_empty() {
        flush(&mut buf, part, &current, &mut written)?;
    }

    eprintln!(
        "Wrote {} files in {} bundle part(s) to {}",
        files.len(),
        written.len(),
        dir_out.display()
    );
    for w in &written {
        let size = fs::metadata(w).map(|m| m.len()).unwrap_or(0);
        eprintln!("  {} ({} bytes)", w.display(), size);
    }

    Ok(())
}

fn header(root: &Path, part: usize, count: usize, ts: &str) -> String {
    format!(
        "# Rust, shell, config and build bundle\n# root: {}\n# part: {}\n# files: {}\n# generated: {}\n\n",
        root.display(),
        part,
        count,
        ts
    )
}
