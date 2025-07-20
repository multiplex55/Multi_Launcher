use arboard::Clipboard;

pub fn add(text: &str) -> anyhow::Result<()> {
    crate::plugins::notes::append_note(
        crate::plugins::notes::QUICK_NOTES_FILE,
        text,
    )?;
    Ok(())
}

pub fn remove(i: usize) -> anyhow::Result<()> {
    crate::plugins::notes::remove_note(
        crate::plugins::notes::QUICK_NOTES_FILE,
        i,
    )?;
    Ok(())
}

pub fn copy(i: usize) -> anyhow::Result<()> {
    if let Some(entry) = crate::plugins::notes::load_notes(
        crate::plugins::notes::QUICK_NOTES_FILE,
    )?
    .get(i)
    .cloned()
    {
        let mut cb = Clipboard::new()?;
        cb.set_text(entry.text)?;
    }
    Ok(())
}
