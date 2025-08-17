pub fn clear() -> anyhow::Result<()> {
    crate::history::clear_history()?;
    Ok(())
}

pub fn launch_index(i: usize) -> anyhow::Result<()> {
    if let Some(entry) = crate::history::get_history().get(i).cloned() {
        crate::launcher::launch_action(&entry.action)?;
    }
    Ok(())
}
