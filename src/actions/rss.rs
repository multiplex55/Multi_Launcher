use anyhow::Result;

/// Execute an RSS command routed from the launcher.
///
/// Commands use a colon separated format: `<verb>:<target>`.
/// The `target` may be a feed id, name, group or `all` depending on the
/// verb. All handlers are currently placeholders.
pub fn run(command: &str) -> Result<()> {
    let mut parts = command.splitn(2, ':');
    let verb = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    match verb {
        "refresh" => refresh(target),
        "open" => open(target),
        "mark" => mark(target),
        // `dialog` opens the UI; nothing to do in CLI.
        "dialog" => Ok(()),
        _ => Ok(()),
    }
}

fn refresh(_target: &str) -> Result<()> {
    // Refresh feed(s); accepts id, name, group or `all`.
    Ok(())
}

fn open(_target: &str) -> Result<()> {
    // Open the given feed or group in the default browser.
    Ok(())
}

fn mark(_target: &str) -> Result<()> {
    // Mark items as read/unread for a feed or group.
    Ok(())
}
