use crate::actions::Action;

/// Launch an [`Action`] through the canonical typed command parser and headless executor.
///
/// This compatibility facade preserves the existing public API for legacy macros, favorites,
/// dialog controls, and other non-GUI callers.
pub fn launch_action(action: &Action) -> anyhow::Result<()> {
    let command = crate::commands::parse_action(action)
        .map_err(|_| anyhow::anyhow!("invalid mkmacro action: {}", action.action))?;
    crate::commands::headless::execute(command, action)
}
