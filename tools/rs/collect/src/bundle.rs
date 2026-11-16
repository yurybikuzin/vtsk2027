use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
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

pub fn run(dir: &Path, out_path: &Path) -> Result<()> {
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

    let mut out =
        fs::File::create(out_path).with_context(|| format!("create {}", out_path.display()))?;

    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    writeln!(out, "# Rust, shell, config and build bundle")?;
    writeln!(out, "# root: {}", root.display())?;
    writeln!(out, "# files: {}", files.len())?;
    writeln!(out, "# generated: {}", ts)?;
    writeln!(out)?;

    writeln!(out, "## File index")?;
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap_or(f);
        writeln!(out, "- {}", rel.display())?;
    }
    writeln!(out)?;

    for f in &files {
        let rel = f.strip_prefix(&root).unwrap_or(f);
        let name = f.file_name().and_then(|s| s.to_str()).unwrap_or("");
        writeln!(out, "## {}", rel.display())?;
        if let Some(lang) = lang_for(name) {
            writeln!(out, "```{}", lang)?;
        } else {
            writeln!(out, "```")?;
        }
        let content = fs::read_to_string(f).with_context(|| format!("read {}", f.display()))?;
        out.write_all(content.as_bytes())?;
        if !content.ends_with('\n') {
            writeln!(out)?;
        }
        writeln!(out, "```")?;
        writeln!(out)?;
    }

    eprintln!(
        "Wrote {} files to {} ({} bytes)",
        files.len(),
        out_path.display(),
        fs::metadata(out_path).map(|m| m.len()).unwrap_or(0)
    );

    Ok(())
}
