use arboard::Clipboard;

pub fn copy_history_result(index: usize) -> anyhow::Result<()> {
    if let Some(entry) = crate::plugins::calc_history::load_history(
        crate::plugins::calc_history::CALC_HISTORY_FILE,
    )
    .unwrap_or_default()
    .get(index)
    .cloned()
    {
        let mut cb = Clipboard::new()?;
        cb.set_text(entry.result)?;
    }
    Ok(())
}
