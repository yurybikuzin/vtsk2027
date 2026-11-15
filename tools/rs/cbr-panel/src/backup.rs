use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub fn backup_before_overwrite(
    target: &Path,
    keep: usize,
) -> Result<Option<PathBuf>, Box<dyn Error>> {
    if !target.exists() {
        return Ok(None);
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let file_name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("panel.xlsx");
    let stem = file_name.trim_end_matches(".xlsx");
    let backup = target.with_file_name(format!("{}.backup.{}.xlsx", stem, stamp));
    fs::copy(target, &backup)?;
    prune_backups(target, stem, keep)?;
    Ok(Some(backup))
}

fn prune_backups(target: &Path, stem: &str, keep: usize) -> Result<(), Box<dyn Error>> {
    if keep == 0 {
        return Ok(());
    }
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let prefix = format!("{}.backup.", stem);
    let mut found: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".xlsx") {
            found.push(entry.path());
        }
    }
    found.sort();
    while found.len() > keep {
        let old = found.remove(0);
        let _ = fs::remove_file(old);
    }
    Ok(())
}
