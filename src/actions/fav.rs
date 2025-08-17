pub fn add(label: &str, action: &str, args: Option<&str>) -> anyhow::Result<()> {
    crate::plugins::fav::set_fav(crate::plugins::fav::FAV_FILE, label, action, args)?;
    Ok(())
}

pub fn remove(label: &str) -> anyhow::Result<()> {
    crate::plugins::fav::remove_fav(crate::plugins::fav::FAV_FILE, label)?;
    Ok(())
}
