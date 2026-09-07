//! Presentation coordination around the shared, atomic editor mutation boundary.
use super::{MkMacroDialog, folding::FoldState};
use crate::mkmacro::{MkStep, editor_mutation as mutation};
use mutation::InsertionAnchor;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct EditorState {
    pub clipboard: Vec<MkStep>,
    pub folds: FoldState,
    pub drag: Option<StepDrag>,
    pub scroll_to: Option<(u64, u64)>,
    pub shortcut_state: ClipboardShortcutState,
}

#[derive(Default)]
pub(super) struct ClipboardShortcutState {
    paste_on_press: bool,
}

pub(super) struct StepDrag {
    pub macro_id: u64,
    pub ids: BTreeSet<u64>,
    pub primary: Option<u64>,
    preview: Option<(u64, InsertionAnchor, Result<(), String>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClipboardCommand {
    Copy,
    Cut,
    Paste,
    Duplicate,
}

/// Egui integrations emit Copy/Cut/Paste events rather than necessarily emitting
/// their key chords. Consume them only after checking every input owner. Eframe
/// 0.27 suppresses V key-down entirely when the OS clipboard has no text; its
/// key-up still arrives. Use that only if no paste was already handled on press.
pub(super) fn clipboard_shortcut(
    input: &mut eframe::egui::InputState,
    table_owns_input: bool,
    state: &mut ClipboardShortcutState,
) -> Option<ClipboardCommand> {
    use eframe::egui::{Event, Key};
    let paste_release = input.events.iter().any(|event| {
        matches!(event,
        Event::Key { key: Key::V, pressed: false, modifiers, .. }
            if (modifiers.ctrl || modifiers.command) && !modifiers.alt && !modifiers.shift)
    });
    state.paste_on_press |= input.events.iter().any(|event| match event {
        Event::Paste(_) => input.modifiers.ctrl || input.modifiers.command,
        Event::Key {
            key: Key::V,
            pressed: true,
            modifiers,
            ..
        } => modifiers.ctrl || modifiers.command,
        _ => false,
    });
    let paste_on_release = paste_release && !state.paste_on_press;
    if input.events.iter().any(|event| {
        matches!(
            event,
            Event::Key {
                key: Key::V,
                pressed: false,
                ..
            }
        )
    }) {
        state.paste_on_press = false;
    }
    if !table_owns_input {
        return None;
    }
    let event_command = input.events.iter().find_map(|event| match event {
        Event::Copy => Some(ClipboardCommand::Copy),
        Event::Cut => Some(ClipboardCommand::Cut),
        Event::Paste(_) => Some(ClipboardCommand::Paste),
        _ => None,
    });
    let command = event_command
        .or_else(|| paste_on_release.then_some(ClipboardCommand::Paste))
        .or_else(|| {
            let modifiers = input.modifiers;
            if !(modifiers.ctrl || modifiers.command) || modifiers.alt || modifiers.shift {
                return None;
            }
            [
                (Key::C, ClipboardCommand::Copy),
                (Key::X, ClipboardCommand::Cut),
                (Key::V, ClipboardCommand::Paste),
                (Key::D, ClipboardCommand::Duplicate),
            ]
            .into_iter()
            .find(|(key, _)| input.key_pressed(*key))
            .map(|(_, command)| command)
        })?;
    input.events.retain(|event| {
        !matches!(event, Event::Copy | Event::Cut | Event::Paste(_))
            && !matches!(event, Event::Key { key: Key::C | Key::X | Key::V | Key::D, modifiers, .. }
            if (modifiers.ctrl || modifiers.command) && !modifiers.alt && !modifiers.shift)
    });
    Some(command)
}

fn paste_anchor(
    steps: &[MkStep],
    primary: Option<u64>,
) -> Result<InsertionAnchor, mutation::MutationError> {
    let Some(primary) = primary else {
        return Ok(InsertionAnchor::End);
    };
    let normalized = mutation::normalize_selection(steps, &BTreeSet::from([primary]))?;
    Ok(normalized
        .ids
        .last()
        .copied()
        .map_or(InsertionAnchor::End, InsertionAnchor::After))
}

pub(super) fn clipboard(d: &mut MkMacroDialog, command: ClipboardCommand) -> anyhow::Result<()> {
    let m = d
        .selected_macro()
        .ok_or_else(|| anyhow::anyhow!("Select a macro"))?;
    let mid = m.id;
    if command != ClipboardCommand::Paste && d.selection.ids.is_empty() {
        anyhow::bail!("Select one or more steps");
    }
    match command {
        ClipboardCommand::Copy => {
            let fragment = mutation::copy_fragment(&m.steps, &d.selection.ids)?;
            d.editor_state.clipboard = fragment;
        }
        ClipboardCommand::Cut => {
            // Prepare both operations before publishing either. Failure retains
            // the previous clipboard, source, selection, and draft revision.
            let fragment = mutation::copy_fragment(&m.steps, &d.selection.ids)?;
            let mut candidate = m.steps.clone();
            let fallback = mutation::delete_selection(&mut candidate, &d.selection.ids)?;
            d.selected_macro_mut()
                .ok_or_else(|| anyhow::anyhow!("Select a macro"))?
                .steps = candidate;
            d.editor_state.clipboard = fragment;
            d.selection.replace(fallback);
            d.mark_dirty();
        }
        ClipboardCommand::Paste | ClipboardCommand::Duplicate => {
            let fragment = if command == ClipboardCommand::Paste {
                if d.editor_state.clipboard.is_empty() {
                    anyhow::bail!("The MkMacro step clipboard is empty");
                }
                d.editor_state.clipboard.clone()
            } else {
                mutation::copy_fragment(&m.steps, &d.selection.ids)?
            };
            let anchor = if command == ClipboardCommand::Duplicate {
                fragment
                    .last()
                    .map_or(InsertionAnchor::End, |step| InsertionAnchor::After(step.id))
            } else {
                paste_anchor(&m.steps, d.selection.primary)?
            };
            let cloned = mutation::insert_fragment(
                &mut d
                    .selected_macro_mut()
                    .ok_or_else(|| anyhow::anyhow!("Select a macro"))?
                    .steps,
                &fragment,
                anchor,
            )?;
            d.selection.replace(cloned.inserted_ids);
            d.mark_dirty();
            reveal_primary(d, mid);
        }
    }
    d.command_error = None;
    Ok(())
}

fn reveal_primary(d: &mut MkMacroDialog, mid: u64) {
    if let Some(id) = d.selection.primary
        && let Some(analysis) = d.cached_structure(mid)
    {
        d.editor_state.folds.expand_ancestors(mid, id, &analysis);
        d.editor_state.scroll_to = Some((mid, id));
    }
}

pub(super) fn begin_drag(d: &mut MkMacroDialog, id: u64) -> anyhow::Result<()> {
    let m = d
        .selected_macro()
        .ok_or_else(|| anyhow::anyhow!("Select a macro"))?;
    let selected = if d.selection.ids.contains(&id) {
        d.selection.ids.clone()
    } else {
        BTreeSet::from([id])
    };
    let ids = mutation::normalize_selection(&m.steps, &selected)?.ids;
    let primary = if d.selection.ids.contains(&id) {
        d.selection.primary
    } else {
        Some(id)
    };
    let drag = StepDrag {
        macro_id: m.id,
        ids: ids.iter().copied().collect(),
        primary,
        preview: None,
    };
    if !d.selection.ids.contains(&id) {
        d.selection.replace([id]);
    }
    d.editor_state.drag = Some(drag);
    Ok(())
}

pub(super) fn preview_drop(d: &mut MkMacroDialog, anchor: InsertionAnchor) -> Result<(), String> {
    let drag = d
        .editor_state
        .drag
        .as_ref()
        .ok_or("No step drag is active")?;
    let revision = d.draft_revision();
    if let Some((r, previous, result)) = &drag.preview
        && *r == revision
        && *previous == anchor
    {
        return result.clone();
    }
    let result = d
        .selected_macro()
        .filter(|m| m.id == drag.macro_id)
        .ok_or_else(|| "The dragged macro is no longer selected".to_owned())
        .and_then(|m| {
            let mut candidate = m.steps.clone();
            mutation::move_to(&mut candidate, &drag.ids, anchor)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
    if let Some(drag) = &mut d.editor_state.drag {
        drag.preview = Some((revision, anchor, result.clone()));
    }
    result
}

pub(super) fn finish_drag(d: &mut MkMacroDialog, anchor: InsertionAnchor) -> anyhow::Result<()> {
    let Some(drag) = d.editor_state.drag.take() else {
        return Ok(());
    };
    let m = d
        .selected_macro_mut()
        .filter(|m| m.id == drag.macro_id)
        .ok_or_else(|| anyhow::anyhow!("The dragged macro is no longer selected"))?;
    let ids = mutation::move_to(&mut m.steps, &drag.ids, anchor)?;
    d.selection.replace(ids);
    if drag.primary.is_some_and(|id| {
        d.selected_macro()
            .is_some_and(|m| m.steps.iter().any(|s| s.id == id))
    }) {
        d.selection.primary = drag.primary;
        d.selection.anchor = drag.primary;
    }
    d.mark_dirty();
    reveal_primary(d, drag.macro_id);
    d.command_error = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkAction, MkCondition, MkMacroStore};
    use eframe::egui::{Event, InputState, Key, Modifiers};

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            action,
            enabled: true,
            breakpoint: true,
            repeat: 2,
            delay_after_ms: 7,
            on_error: Default::default(),
            metadata: Default::default(),
        }
    }
    fn delay(id: u64) -> MkStep {
        step(id, MkAction::Delay(Default::default()))
    }
    fn block() -> Vec<MkStep> {
        vec![
            delay(90),
            step(10, MkAction::If(MkCondition::All { conditions: vec![] })),
            delay(30),
            step(50, MkAction::Else),
            delay(60),
            step(70, MkAction::EndIf),
            delay(80),
        ]
    }
    fn dialog(steps: Vec<MkStep>) -> (tempfile::TempDir, MkMacroDialog) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let mut d = MkMacroDialog::new(std::sync::Arc::new(store));
        d.create_macro();
        d.selected_macro_mut().unwrap().steps = steps;
        d.mark_dirty();
        d.dirty = false;
        (directory, d)
    }

    #[test]
    fn clipboard_copy_cut_paste_cross_macro_is_atomic_and_preserves_metadata() {
        let (_directory, mut d) = dialog(block());
        d.selected_macro_mut().unwrap().steps[1].metadata = crate::mkmacro::MkStepMetadata {
            label: "Branch".into(),
            comment: "Two branches\nkept together".into(),
            accent: crate::mkmacro::MkStepAccent::Purple,
            bookmarked: true,
        };
        d.mark_dirty();
        d.dirty = false;
        let source = d.selected_macro().unwrap().clone();
        d.selection.replace([50, 30]);
        let revision = d.draft_revision();
        clipboard(&mut d, ClipboardCommand::Copy).unwrap();
        assert_eq!(d.selected_macro(), Some(&source));
        assert_eq!(d.draft_revision(), revision);
        assert!(!d.dirty);
        assert_eq!(
            d.editor_state
                .clipboard
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [10, 30, 50, 60, 70]
        );
        clipboard(&mut d, ClipboardCommand::Cut).unwrap();
        assert_eq!(d.draft_revision(), revision + 1);
        assert_eq!(d.selection.primary, Some(80));
        assert_eq!(
            d.selected_macro()
                .unwrap()
                .steps
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [90, 80]
        );
        d.create_macro();
        let destination = d.selected_macro_id.unwrap();
        d.selected_macro_mut().unwrap().steps = block();
        d.mark_dirty();
        d.selection.replace([50]);
        let revision = d.draft_revision();
        clipboard(&mut d, ClipboardCommand::Paste).unwrap();
        let m = d.selected_macro().unwrap();
        assert_eq!(m.steps.len(), 12);
        let inserted = &m.steps[6..11];
        assert_eq!(d.selection.primary, Some(inserted[0].id));
        assert_eq!(d.selection.ids, inserted.iter().map(|s| s.id).collect());
        assert_eq!(
            d.editor_state.scroll_to,
            Some((destination, inserted[0].id))
        );
        assert_eq!(d.draft_revision(), revision + 1);
        for (copy, original) in inserted.iter().zip(&source.steps[1..6]) {
            let mut expected = original.clone();
            expected.id = copy.id;
            assert_ne!(copy.id, original.id);
            assert_eq!(*copy, expected);
        }
        assert!(
            crate::mkmacro::analyze_structure(&m.steps)
                .diagnostics
                .is_empty()
        );
    }

    #[test]
    fn invalid_cut_preserves_source_clipboard_selection_and_revision() {
        let (_directory, mut d) = dialog(vec![delay(1), step(2, MkAction::EndIf)]);
        d.selection.replace([1]);
        clipboard(&mut d, ClipboardCommand::Copy).unwrap();
        d.selection.replace([2]);
        let before = d.draft.clone();
        let revision = d.draft_revision();
        let clipboard_before = d.editor_state.clipboard.clone();
        assert!(clipboard(&mut d, ClipboardCommand::Cut).is_err());
        assert_eq!(d.draft, before);
        assert_eq!(d.draft_revision(), revision);
        assert_eq!(d.editor_state.clipboard, clipboard_before);
        assert_eq!(d.selection.ids, BTreeSet::from([2]));
        assert!(!d.dirty);
    }

    #[test]
    fn macro_switch_resets_selection_drag_and_scroll_but_keeps_clipboard_and_scoped_folds() {
        let (_directory, mut d) = dialog(block());
        let source = d.selected_macro_id.unwrap();
        let structure = d.cached_structure(source).unwrap();
        d.editor_state.folds.toggle(source, 10, &structure);
        d.selection.replace([90]);
        clipboard(&mut d, ClipboardCommand::Copy).unwrap();
        let clipboard_before = d.editor_state.clipboard.clone();
        d.create_macro();
        let target = d.selected_macro_id.unwrap();
        d.selected_macro_mut().unwrap().steps = block();
        d.mark_dirty();
        d.set_selected_macro(Some(source));
        d.selection.replace([90]);
        begin_drag(&mut d, 90).unwrap();
        d.editor_state.scroll_to = Some((source, 90));
        d.set_selected_macro(Some(target));
        assert!(d.selection.ids.is_empty());
        assert_eq!(d.selection.primary, None);
        assert!(d.editor_state.drag.is_none());
        assert_eq!(d.editor_state.scroll_to, None);
        assert_eq!(d.editor_state.clipboard, clipboard_before);
        assert!(d.editor_state.folds.is_collapsed(source, 10));
        assert!(!d.editor_state.folds.is_collapsed(target, 10));
        clipboard(&mut d, ClipboardCommand::Paste).unwrap();
        assert_eq!(
            d.selected_macro().unwrap().steps.last().unwrap().id,
            d.selection.primary.unwrap()
        );
    }

    #[test]
    fn duplicate_uses_same_fragment_without_changing_clipboard() {
        let (_directory, mut d) = dialog(block());
        d.selection.replace([90]);
        clipboard(&mut d, ClipboardCommand::Copy).unwrap();
        let saved_clipboard = d.editor_state.clipboard.clone();
        d.selection.replace([50]);
        clipboard(&mut d, ClipboardCommand::Duplicate).unwrap();
        assert_eq!(d.editor_state.clipboard, saved_clipboard);
        let inserted = &d.selected_macro().unwrap().steps[6..11];
        assert_eq!(d.selection.ids, inserted.iter().map(|s| s.id).collect());
        assert_eq!(d.selection.primary, Some(inserted[0].id));
    }

    #[test]
    fn drag_preview_is_read_only_invalid_drop_is_atomic_and_valid_drop_preserves_primary() {
        let (_directory, mut d) = dialog(block());
        d.selection.replace([90, 50]);
        d.selection.primary = Some(50);
        let before = d.draft.clone();
        let revision = d.draft_revision();
        begin_drag(&mut d, 90).unwrap();
        assert!(preview_drop(&mut d, InsertionAnchor::Before(30)).is_err());
        assert_eq!(d.draft, before);
        assert!(finish_drag(&mut d, InsertionAnchor::Before(30)).is_err());
        assert_eq!(d.draft, before);
        assert_eq!(d.draft_revision(), revision);
        assert!(!d.dirty);
        begin_drag(&mut d, 90).unwrap();
        preview_drop(&mut d, InsertionAnchor::End).unwrap();
        assert_eq!(d.draft, before);
        finish_drag(&mut d, InsertionAnchor::End).unwrap();
        assert_eq!(d.selection.primary, Some(50));
        assert_eq!(
            d.selected_macro()
                .unwrap()
                .steps
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [80, 90, 10, 30, 50, 60, 70]
        );
        assert_eq!(d.selection.ids, BTreeSet::from([90, 10, 30, 50, 60, 70]));
        assert_eq!(d.draft_revision(), revision + 1);
    }

    fn key(key: Key, pressed: bool) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: Modifiers::CTRL,
        }
    }

    #[test]
    fn clipboard_events_and_duplicate_respect_input_ownership_without_os_output() {
        for (event, expected) in [
            (Event::Copy, ClipboardCommand::Copy),
            (Event::Cut, ClipboardCommand::Cut),
            (
                Event::Paste("OS text stays outside the structured clipboard".into()),
                ClipboardCommand::Paste,
            ),
            (key(Key::D, true), ClipboardCommand::Duplicate),
        ] {
            let mut input = InputState::default();
            input.modifiers = Modifiers::CTRL;
            input.events = vec![event];
            let original = input.events.clone();
            let mut state = ClipboardShortcutState::default();
            assert_eq!(clipboard_shortcut(&mut input, false, &mut state), None);
            assert_eq!(input.events, original);
            assert_eq!(
                clipboard_shortcut(&mut input, true, &mut state),
                Some(expected)
            );
            assert!(input.events.is_empty());
        }
    }

    #[test]
    fn empty_os_clipboard_uses_release_but_handled_or_text_owned_paste_never_repeats() {
        let mut state = ClipboardShortcutState::default();
        let mut input = InputState::default();
        input.modifiers = Modifiers::CTRL;
        input.events = vec![key(Key::V, false)];
        assert_eq!(
            clipboard_shortcut(&mut input, true, &mut state),
            Some(ClipboardCommand::Paste)
        );
        for owns_press in [true, false] {
            input.events = vec![Event::Paste("text".into())];
            assert_eq!(
                clipboard_shortcut(&mut input, owns_press, &mut state),
                owns_press.then_some(ClipboardCommand::Paste)
            );
            input.events = vec![key(Key::V, false)];
            assert_eq!(clipboard_shortcut(&mut input, true, &mut state), None);
        }
        input.events = vec![key(Key::V, false)];
        assert_eq!(clipboard_shortcut(&mut input, false, &mut state), None);
        assert_eq!(input.events, [key(Key::V, false)]);
    }
}
