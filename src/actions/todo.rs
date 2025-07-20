pub fn add(text: &str, priority: u8, tags: &[String]) -> anyhow::Result<()> {
    crate::plugins::todo::append_todo(
        crate::plugins::todo::TODO_FILE,
        text,
        priority,
        tags,
    )?;
    Ok(())
}

pub fn set_priority(idx: usize, priority: u8) -> anyhow::Result<()> {
    crate::plugins::todo::set_priority(
        crate::plugins::todo::TODO_FILE,
        idx,
        priority,
    )?;
    Ok(())
}

pub fn set_tags(idx: usize, tags: &[String]) -> anyhow::Result<()> {
    crate::plugins::todo::set_tags(
        crate::plugins::todo::TODO_FILE,
        idx,
        tags,
    )?;
    Ok(())
}

pub fn remove(idx: usize) -> anyhow::Result<()> {
    crate::plugins::todo::remove_todo(crate::plugins::todo::TODO_FILE, idx)?;
    Ok(())
}

pub fn mark_done(idx: usize) -> anyhow::Result<()> {
    crate::plugins::todo::mark_done(crate::plugins::todo::TODO_FILE, idx)?;
    Ok(())
}

pub fn clear_done() -> anyhow::Result<()> {
    crate::plugins::todo::clear_done(crate::plugins::todo::TODO_FILE)?;
    Ok(())
}
