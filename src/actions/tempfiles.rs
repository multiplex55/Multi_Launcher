use anyhow::Context;
use std::path::Path;

pub fn new(alias: Option<&str>) -> anyhow::Result<()> {
    let path = if let Some(a) = alias {
        crate::plugins::tempfile::create_named_file(a, "")?
    } else {
        crate::plugins::tempfile::create_file()?
    };
    open::that(&path)?;
    Ok(())
}

pub fn open_dir() -> anyhow::Result<()> {
    let dir = crate::plugins::tempfile::storage_dir();
    std::fs::create_dir_all(&dir)?;
    open::that(dir)?;
    Ok(())
}

pub fn clear() -> anyhow::Result<()> {
    crate::plugins::tempfile::clear_files()?;
    Ok(())
}

pub fn remove(path: &str) -> anyhow::Result<()> {
    crate::plugins::tempfile::remove_file(Path::new(path))?;
    Ok(())
}

pub fn set_alias(path: &str, alias: &str) -> anyhow::Result<()> {
    crate::plugins::tempfile::set_alias(Path::new(path), alias)
        .context("failed to rename tempfile")?;
    Ok(())
}
