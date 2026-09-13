use eframe::egui;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::actions::Action;
use crate::universal_actions::{ActionGroup, ActionSurface, ResolvedActionTarget, UniversalAction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActionSheetKey {
    Escape,
    Up,
    Down,
    Enter,
}

/// Keyboard-first state for the action surface. The resolved target and action
/// list are snapshots: providers are invoked only when the sheet opens.
#[derive(Default)]
pub(crate) struct ActionSheetState {
    pub(crate) target: Option<ResolvedActionTarget>,
    pub(crate) legacy_action: Option<Action>,
    pub(crate) actions: Vec<UniversalAction>,
    pub(crate) filter: String,
    pub(crate) filtered_indices: Vec<usize>,
    pub(crate) selected_index: Option<usize>,
    pub(crate) request_filter_focus: bool,
}

impl ActionSheetState {
    pub(crate) fn is_open(&self) -> bool {
        self.target.is_some()
    }

    pub(crate) fn open(
        &mut self,
        target: ResolvedActionTarget,
        legacy_action: Action,
        actions: Vec<UniversalAction>,
        matcher: &SkimMatcherV2,
    ) {
        self.target = Some(target);
        self.legacy_action = Some(legacy_action);
        self.actions = actions;
        self.filter.clear();
        self.request_filter_focus = true;
        self.rebuild_filter(matcher);
    }

    pub(crate) fn close(&mut self) {
        self.target = None;
        self.legacy_action = None;
        self.actions.clear();
        self.filter.clear();
        self.filtered_indices.clear();
        self.selected_index = None;
        self.request_filter_focus = false;
    }

    pub(crate) fn set_filter(&mut self, filter: String, matcher: &SkimMatcherV2) {
        self.filter = filter;
        self.rebuild_filter(matcher);
    }

    fn rebuild_filter(&mut self, matcher: &SkimMatcherV2) {
        let filter = self.filter.trim().to_lowercase();
        self.filtered_indices = self
            .actions
            .iter()
            .enumerate()
            .filter(|(_, action)| {
                let presentation = action.effective_presentation(ActionSurface::ActionSheet);
                if !presentation.visible {
                    return false;
                }
                if filter.is_empty() {
                    return true;
                }
                [
                    presentation.label.as_str(),
                    presentation.short_label.as_deref().unwrap_or_default(),
                    presentation.description.as_deref().unwrap_or_default(),
                    action.id.as_str(),
                ]
                .into_iter()
                .any(|field| {
                    matcher
                        .fuzzy_match(&field.to_lowercase(), &filter)
                        .is_some()
                })
            })
            .map(|(index, _)| index)
            .collect();
        self.selected_index = self.first_selectable_position();
    }

    fn first_selectable_position(&self) -> Option<usize> {
        self.filtered_indices
            .iter()
            .position(|index| self.actions[*index].is_available())
            .or_else(|| (!self.filtered_indices.is_empty()).then_some(0))
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        if self.filtered_indices.is_empty() {
            self.selected_index = None;
            return;
        }

        let selectable = self
            .filtered_indices
            .iter()
            .enumerate()
            .filter_map(|(position, index)| self.actions[*index].is_available().then_some(position))
            .collect::<Vec<_>>();
        let positions = if selectable.is_empty() {
            (0..self.filtered_indices.len()).collect::<Vec<_>>()
        } else {
            selectable
        };
        let current = self
            .selected_index
            .and_then(|selected| positions.iter().position(|position| *position == selected))
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(positions.len() as isize) as usize;
        self.selected_index = Some(positions[next]);
    }

    pub(crate) fn selected_action(&self) -> Option<UniversalAction> {
        let filtered_position = self.selected_index?;
        let action_index = *self.filtered_indices.get(filtered_position)?;
        self.actions
            .get(action_index)
            .filter(|action| action.is_available())
            .cloned()
    }

    pub(crate) fn select_filtered_position(&mut self, position: usize) {
        if self
            .filtered_indices
            .get(position)
            .and_then(|index| self.actions.get(*index))
            .is_some_and(UniversalAction::is_available)
        {
            self.selected_index = Some(position);
        }
    }

    pub(crate) fn consume_open_shortcut(input: &mut egui::InputState) -> bool {
        let Some(index) = input.events.iter().position(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.mac_cmd
            )
        }) else {
            return false;
        };
        input.events.remove(index);
        true
    }

    /// Consume sheet-owned navigation keys regardless of modifiers, while only
    /// interpreting their unmodified forms. In particular Ctrl+Enter is eaten
    /// without executing or reaching the launcher.
    pub(crate) fn consume_key(input: &mut egui::InputState) -> Option<ActionSheetKey> {
        let mut routed = None;
        input.events.retain(|event| {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            let key = match key {
                egui::Key::Escape => Some(ActionSheetKey::Escape),
                egui::Key::ArrowUp => Some(ActionSheetKey::Up),
                egui::Key::ArrowDown => Some(ActionSheetKey::Down),
                egui::Key::Enter => Some(ActionSheetKey::Enter),
                _ => None,
            };
            let Some(key) = key else {
                return true;
            };
            if routed.is_none() && *modifiers == egui::Modifiers::NONE {
                routed = Some(key);
            }
            false
        });
        routed
    }
}

pub(crate) fn render(
    ctx: &egui::Context,
    state: &mut ActionSheetState,
    matcher: &SkimMatcherV2,
) -> Option<UniversalAction> {
    if !state.is_open() {
        return None;
    }

    let target_label = state
        .legacy_action
        .as_ref()
        .map(|action| action.label.as_str())
        .unwrap_or("Result");
    let title = format!("Actions for \"{target_label}\"");
    let mut open = true;
    let mut selected = None;
    egui::Window::new(title)
        .id(egui::Id::new("universal_action_sheet"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .collapsible(false)
        .resizable(true)
        .default_width(440.0)
        .open(&mut open)
        .show(ctx, |ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.filter)
                    .id(egui::Id::new("universal_action_sheet_filter"))
                    .desired_width(f32::INFINITY)
                    .hint_text("Filter actions"),
            );
            if state.request_filter_focus {
                response.request_focus();
                state.request_filter_focus = false;
            }
            if response.changed() {
                let filter = state.filter.clone();
                state.set_filter(filter, matcher);
            }

            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    let mut previous_group: Option<ActionGroup> = None;
                    let visible = state.filtered_indices.clone();
                    for (position, action_index) in visible.into_iter().enumerate() {
                        let action = state.actions[action_index].clone();
                        let presentation =
                            action.effective_presentation(ActionSurface::ActionSheet);
                        if previous_group.is_some() && previous_group != Some(presentation.group) {
                            ui.separator();
                        }
                        previous_group = Some(presentation.group);

                        let mut label = presentation.label;
                        if let Some(reason) = action.availability.disabled_reason() {
                            label.push_str(&format!(" — {reason}"));
                        }
                        let response = ui.add_enabled(
                            action.is_available(),
                            egui::SelectableLabel::new(
                                state.selected_index == Some(position),
                                label,
                            ),
                        );
                        if response.hovered() && action.is_available() {
                            state.select_filtered_position(position);
                        }
                        if response.clicked() {
                            selected = Some(action);
                        }
                    }
                    if state.filtered_indices.is_empty() {
                        ui.weak("No matching actions");
                    }
                });
        });

    if !open {
        state.close();
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal_actions::{
        ActionAvailability, ActionId, ActionPresentation, ActionSafety, UniversalActionOperation,
    };

    fn legacy() -> Action {
        Action {
            label: "Project".into(),
            desc: "Test".into(),
            action: "project".into(),
            args: None,
        }
    }

    fn target() -> ResolvedActionTarget {
        ResolvedActionTarget {
            target: crate::universal_actions::ActionTarget::Generic { action: legacy() },
            selected_action: legacy(),
            custom_action_index: None,
        }
    }

    fn action(id: &'static str, label: &str, available: bool) -> UniversalAction {
        UniversalAction {
            id: ActionId::from_static(id),
            target: crate::universal_actions::ActionTarget::Generic { action: legacy() },
            presentation: ActionPresentation::new(label),
            availability: if available {
                ActionAvailability::Available
            } else {
                ActionAvailability::Disabled {
                    reason: "Unavailable".into(),
                }
            },
            safety: ActionSafety::Normal,
            operation: UniversalActionOperation::InvokePrimary(legacy()),
        }
    }

    fn state() -> ActionSheetState {
        let mut state = ActionSheetState::default();
        state.open(
            target(),
            legacy(),
            vec![
                action("open.nvim", "Open in Neovim", true),
                action("open.notepad", "Open in Notepad", false),
                action("copy.link", "Copy Link", true),
                action("delete", "Delete", true),
            ],
            &SkimMatcherV2::default(),
        );
        state
    }

    #[test]
    fn open_filter_and_close_own_independent_state() {
        let mut state = state();
        assert!(state.is_open());
        assert_eq!(state.filtered_indices, vec![0, 1, 2, 3]);

        state.set_filter("NEO".into(), &SkimMatcherV2::default());
        assert_eq!(state.filtered_indices, vec![0]);
        assert_eq!(state.selected_action().unwrap().id.as_str(), "open.nvim");

        state.set_filter(String::new(), &SkimMatcherV2::default());
        assert_eq!(state.filtered_indices, vec![0, 1, 2, 3]);
        state.set_filter("no such action".into(), &SkimMatcherV2::default());
        assert!(state.filtered_indices.is_empty());
        assert_eq!(state.selected_index, None);
        state.close();
        assert!(!state.is_open());
        assert!(state.actions.is_empty());
    }

    #[test]
    fn navigation_wraps_and_skips_disabled_actions() {
        let mut state = state();
        assert_eq!(state.selected_index, Some(0));
        state.move_selection(1);
        assert_eq!(state.selected_index, Some(2));
        state.move_selection(1);
        assert_eq!(state.selected_index, Some(3));
        state.move_selection(1);
        assert_eq!(state.selected_index, Some(0));
        state.move_selection(-1);
        assert_eq!(state.selected_index, Some(3));
    }

    #[test]
    fn disabled_only_results_are_visible_but_not_executable() {
        let mut state = ActionSheetState::default();
        state.open(
            target(),
            legacy(),
            vec![action("disabled", "Disabled", false)],
            &SkimMatcherV2::default(),
        );
        assert_eq!(state.selected_index, Some(0));
        assert!(state.selected_action().is_none());
        state.move_selection(1);
        assert_eq!(state.selected_index, Some(0));
    }

    fn key_press(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn open_shortcut_requires_exact_ctrl_only_and_consumes_its_event() {
        let route = |modifiers| {
            let ctx = egui::Context::default();
            ctx.begin_frame(egui::RawInput {
                events: vec![key_press(egui::Key::Enter, modifiers)],
                ..Default::default()
            });
            let routed = ctx.input_mut(ActionSheetState::consume_open_shortcut);
            let enter_remains = ctx.input(|input| input.key_pressed(egui::Key::Enter));
            let _ = ctx.end_frame();
            (routed, enter_remains)
        };

        assert_eq!(route(egui::Modifiers::CTRL), (true, false));
        assert_eq!(
            route(egui::Modifiers::CTRL | egui::Modifiers::SHIFT),
            (false, true)
        );
        assert_eq!(route(egui::Modifiers::ALT), (false, true));
        assert_eq!(route(egui::Modifiers::NONE), (false, true));
    }

    #[test]
    fn open_sheet_consumes_ctrl_enter_without_executing() {
        let ctx = egui::Context::default();
        ctx.begin_frame(egui::RawInput {
            events: vec![key_press(egui::Key::Enter, egui::Modifiers::CTRL)],
            ..Default::default()
        });
        assert_eq!(ctx.input_mut(ActionSheetState::consume_key), None);
        assert!(!ctx.input(|input| input.key_pressed(egui::Key::Enter)));
        let _ = ctx.end_frame();
    }
}
