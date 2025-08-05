use arboard::Clipboard;

pub fn add(text: &str) -> anyhow::Result<()> {
    crate::plugins::note::append_note(text, text)?;
    Ok(())
}

pub fn remove(i: usize) -> anyhow::Result<()> {
    crate::plugins::note::remove_note(i)?;
    Ok(())
}

pub fn copy(i: usize) -> anyhow::Result<()> {
    if let Some(entry) = crate::plugins::note::load_notes()?.get(i).cloned() {
        let mut cb = Clipboard::new()?;
        cb.set_text(entry.content)?;
    }
    Ok(())
}
