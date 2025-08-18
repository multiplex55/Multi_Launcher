pub fn add(url: &str) -> anyhow::Result<()> {
    crate::plugins::rss::append_feed(crate::plugins::rss::RSS_FILE, url)?;
    Ok(())
}

pub fn remove(url: &str) -> anyhow::Result<()> {
    crate::plugins::rss::remove_feed(crate::plugins::rss::RSS_FILE, url)?;
    Ok(())
}
