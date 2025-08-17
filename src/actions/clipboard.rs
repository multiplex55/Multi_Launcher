use arboard::Clipboard;

pub fn clear_history() -> anyhow::Result<()> {
    crate::plugins::clipboard::clear_history_file(crate::plugins::clipboard::CLIPBOARD_FILE)?;
    Ok(())
}

pub fn copy_entry(i: usize) -> anyhow::Result<()> {
    if let Some(entry) =
        crate::plugins::clipboard::load_history(crate::plugins::clipboard::CLIPBOARD_FILE)
            .unwrap_or_default()
            .get(i)
            .cloned()
    {
        let mut cb = Clipboard::new()?;
        cb.set_text(entry)?;
    }
    Ok(())
}

pub fn set_text(text: &str) -> anyhow::Result<()> {
    let mut cb = Clipboard::new()?;
    cb.set_text(text.to_string())?;
    Ok(())
}

pub fn calc_to_clipboard(val: &str) -> anyhow::Result<()> {
    let mut cb = Clipboard::new()?;
    cb.set_text(val.to_string())?;
    Ok(())
}
