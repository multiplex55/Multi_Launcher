pub fn add(url: &str) -> anyhow::Result<()> {
    crate::plugins::bookmarks::append_bookmark("bookmarks.json", url)?;
    Ok(())
}

pub fn remove(url: &str) -> anyhow::Result<()> {
    crate::plugins::bookmarks::remove_bookmark("bookmarks.json", url)?;
    Ok(())
}
