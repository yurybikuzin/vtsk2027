use anyhow::{Context, Result};
use std::path::Path;

use crate::walker;

pub fn run(dir: &Path) -> Result<()> {
    let root = dir.canonicalize().context("canonicalize dir")?;
    let walker = walker::build(&root)?;

    let mut entries: Vec<(usize, std::path::PathBuf, bool)> = Vec::new();
    for result in walker {
        let entry = result.context("walk error")?;
        let depth = entry.depth();
        if depth == 0 {
            continue;
        }
        let path = entry.into_path();
        let is_dir = path.is_dir();
        entries.push((depth, path, is_dir));
    }

    let mut is_last: Vec<bool> = vec![true; entries.len()];
    let mut last_at_depth: Vec<Option<usize>> = vec![None; 64];
    for i in (0..entries.len()).rev() {
        let d = entries[i].0;
        if d < last_at_depth.len() {
            if last_at_depth[d].is_none() {
                is_last[i] = true;
                last_at_depth[d] = Some(i);
            } else {
                is_last[i] = false;
            }
        }
    }

    let mut out = String::new();
    out.push_str(&format!("{}/\n", root.display()));

    for (i, (depth, path, is_dir)) in entries.iter().enumerate() {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

        let mut prefix = String::new();
        for d in 1..*depth {
            let ancestor_last = entries[..i]
                .iter()
                .enumerate()
                .rev()
                .find(|(_, (dd, _, _))| *dd == d)
                .map(|(idx, _)| is_last[idx])
                .unwrap_or(true);
            prefix.push_str(if ancestor_last { "    " } else { "│   " });
        }

        let branch = if is_last[i] {
            "└── "
        } else {
            "├── "
        };
        out.push_str(&format!(
            "{}{}{}{}\n",
            prefix,
            branch,
            name,
            if *is_dir { "/" } else { "" }
        ));
    }

    print!("{}", out);
    Ok(())
}
