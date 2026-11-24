//! Panel ordering and the saved toolbar decision shared by both frontends.

use super::Panel;

/// Add an open panel without changing the order of panels already present.
pub fn ensure(stack: &mut Vec<Panel>, panel: Panel) {
    if !stack.contains(&panel) {
        stack.push(panel);
    }
}

/// Put an explicitly focused or newly opened panel at the top.
pub fn promote(stack: &mut Vec<Panel>, panel: Panel) {
    retire(stack, panel);
    stack.push(panel);
}

pub fn retire(stack: &mut Vec<Panel>, panel: Panel) {
    stack.retain(|open_panel| *open_panel != panel);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedToolbarAction {
    Focus,
    Unpin,
}

/// The saved toolbar uses the actual top, including panels outside the root.
pub fn saved_toolbar_action(stack: &[Panel], panel: Panel) -> SavedToolbarAction {
    if stack.last() == Some(&panel) {
        SavedToolbarAction::Unpin
    } else {
        SavedToolbarAction::Focus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_preserves_order_and_promotion_moves_to_top() {
        let mut stack = vec![Panel::FavDialog, Panel::HelpWindow];
        ensure(&mut stack, Panel::FavDialog);
        assert_eq!(stack, [Panel::FavDialog, Panel::HelpWindow]);
        ensure(&mut stack, Panel::HashGeneratorDialog);
        ensure(&mut stack, Panel::HashGeneratorDialog);
        assert_eq!(
            stack,
            [
                Panel::FavDialog,
                Panel::HelpWindow,
                Panel::HashGeneratorDialog
            ]
        );
        promote(&mut stack, Panel::FavDialog);
        assert_eq!(
            stack,
            [
                Panel::HelpWindow,
                Panel::HashGeneratorDialog,
                Panel::FavDialog
            ]
        );
        promote(&mut stack, Panel::FavDialog);
        assert_eq!(
            stack,
            [
                Panel::HelpWindow,
                Panel::HashGeneratorDialog,
                Panel::FavDialog
            ]
        );
    }

    #[test]
    fn retirement_and_toolbar_use_exact_top() {
        let mut stack = vec![Panel::FavDialog, Panel::HelpWindow];
        assert_eq!(
            saved_toolbar_action(&stack, Panel::FavDialog),
            SavedToolbarAction::Focus
        );
        assert_eq!(
            saved_toolbar_action(&stack, Panel::HelpWindow),
            SavedToolbarAction::Unpin
        );
        retire(&mut stack, Panel::HelpWindow);
        retire(&mut stack, Panel::HelpWindow);
        assert_eq!(
            saved_toolbar_action(&stack, Panel::FavDialog),
            SavedToolbarAction::Unpin
        );
        retire(&mut stack, Panel::FavDialog);
        assert_eq!(
            saved_toolbar_action(&stack, Panel::FavDialog),
            SavedToolbarAction::Focus
        );
    }
}
