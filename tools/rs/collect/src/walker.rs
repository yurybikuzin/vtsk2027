use anyhow::Result;
use ignore::WalkBuilder;
use std::path::Path;

pub fn build(root: &Path) -> Result<ignore::Walk> {
    let mut builder = WalkBuilder::new(root);
    builder
        .parents(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .ignore(false)
        .hidden(false)
        .require_git(true)
        .add_custom_ignore_filename(".collectignore");
    Ok(builder.build())
}
