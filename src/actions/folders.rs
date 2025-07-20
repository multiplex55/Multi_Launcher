pub fn add(path: &str) -> anyhow::Result<()> {
    crate::plugins::folders::append_folder(
        crate::plugins::folders::FOLDERS_FILE,
        path,
    )?;
    Ok(())
}

pub fn remove(path: &str) -> anyhow::Result<()> {
    crate::plugins::folders::remove_folder(
        crate::plugins::folders::FOLDERS_FILE,
        path,
    )?;
    Ok(())
}
