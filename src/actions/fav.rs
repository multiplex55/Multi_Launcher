pub fn remove(label: &str) -> anyhow::Result<()> {
    crate::plugins::fav::remove_fav(crate::plugins::fav::FAV_FILE, label)?;
    Ok(())
}
