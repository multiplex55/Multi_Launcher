pub fn add(text: &str, priority: u8, tags: &[String]) -> anyhow::Result<()> {
    crate::plugins::todo::append_todo(crate::plugins::todo::TODO_FILE, text, priority, tags)?;
    Ok(())
}

pub fn set_priority(idx: usize, priority: u8) -> anyhow::Result<()> {
    crate::plugins::todo::set_priority(crate::plugins::todo::TODO_FILE, idx, priority)?;
    Ok(())
}

pub fn set_tags(idx: usize, tags: &[String]) -> anyhow::Result<()> {
    crate::plugins::todo::set_tags(crate::plugins::todo::TODO_FILE, idx, tags)?;
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

pub fn export() -> anyhow::Result<std::path::PathBuf> {
    use std::fmt::Write as _;

    let list = crate::plugins::todo::load_todos(crate::plugins::todo::TODO_FILE)?;

    let mut content = String::new();
    for entry in list {
        let done = if entry.done { "[x]" } else { "[ ]" };
        write!(content, "{done} {}", entry.text)?;
        if !entry.tags.is_empty() {
            write!(content, " #{}", entry.tags.join(" #"))?;
        }
        if entry.priority > 0 {
            write!(content, " p={}", entry.priority)?;
        }
        writeln!(content)?;
    }

    let path = crate::plugins::tempfile::create_named_file("todo_export", &content)?;
    open::that(&path)?;
    Ok(path)
}
