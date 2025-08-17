pub fn remove(alias: &str) -> anyhow::Result<()> {
    crate::plugins::snippets::remove_snippet(crate::plugins::snippets::SNIPPETS_FILE, alias)?;
    Ok(())
}

pub fn add(alias: &str, text: &str) -> anyhow::Result<()> {
    crate::plugins::snippets::append_snippet(crate::plugins::snippets::SNIPPETS_FILE, alias, text)?;
    Ok(())
}
