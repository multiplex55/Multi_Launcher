use super::*;

use crate::radial::acceptance_trace::{
    self, Correlation, RestoreEdge, RootMenuControl, RootResultKind, VisibilitySource,
};

#[derive(Clone, Debug)]
pub(crate) struct DeferredActivation {
    pub(crate) action: Action,
    pub(crate) query_override: Option<String>,
    pub(crate) source: ActivationSource,
}

#[derive(Clone, Debug)]
struct DeferredUniversalAction {
    action: crate::universal_actions::UniversalAction,
    surface: crate::universal_actions::ActionSurface,
    source: ActivationSource,
}

#[derive(Clone, Debug)]
enum ContextMenuChoice {
    Execute(crate::universal_actions::UniversalAction),
    AddToRadial {
        binding: crate::radial::model::ActionBinding,
        label: String,
        source_query: String,
        trace_identity: acceptance_trace::RadialInsertionTraceIdentity,
    },
}

// egui 0.27 does not publish AccessKit's Disabled flag. D08 observes the actual
// widget responses only when opted in, retaining one bounded frame per context.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum D08Control {
    PickerPin,
    PickerTest,
    PickerSaveQuery,
    AddMenu,
    AddAction,
    SaveLiveQuery,
}

#[cfg(test)]
#[derive(Clone)]
struct D08ResponseObservations {
    frame_nr: u64,
    responses: Vec<(D08Control, egui::Id, egui::Rect, bool)>,
    overflow: bool,
}

#[cfg(test)]
pub(crate) fn begin_d08_response_observation(context: &egui::Context) {
    let frame_nr = context.frame_nr();
    context.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("d08-response-observations"),
            D08ResponseObservations {
                frame_nr,
                responses: Vec::new(),
                overflow: false,
            },
        );
    });
}

#[cfg(test)]
pub(crate) fn observe_d08_response(
    context: &egui::Context,
    control: D08Control,
    response: &egui::Response,
) {
    let frame_nr = context.frame_nr();
    context.data_mut(|data| {
        let key = egui::Id::new("d08-response-observations");
        let Some(mut observations) = data.get_temp::<D08ResponseObservations>(key) else {
            return;
        };
        if observations.frame_nr != frame_nr {
            observations.frame_nr = frame_nr;
            observations.responses.clear();
            observations.overflow = false;
        }
        if observations.responses.len() < 32 {
            observations
                .responses
                .push((control, response.id, response.rect, response.enabled()));
        } else {
            observations.overflow = true;
        }
        data.insert_temp(key, observations);
    });
}

#[cfg(test)]
pub(crate) fn d08_response_enabled(
    context: &egui::Context,
    control: D08Control,
    node_id: egui::accesskit::NodeId,
    bounds: egui::accesskit::Rect,
) -> bool {
    let observations = context
        .data(|data| {
            data.get_temp::<D08ResponseObservations>(egui::Id::new("d08-response-observations"))
        })
        .expect("D08 observation was enabled on this retained context");
    assert!(!observations.overflow, "D08 observation must stay bounded");
    assert_eq!(
        observations.frame_nr.checked_add(1),
        Some(context.frame_nr())
    );
    let responses = observations
        .responses
        .iter()
        .filter(|(kind, id, _, _)| *kind == control && id.value() == node_id.0)
        .collect::<Vec<_>>();
    assert_eq!(
        responses.len(),
        1,
        "AccessKit node must own one actual response"
    );
    let (_, _, rect, enabled) = responses[0];
    assert_eq!(
        [bounds.x0, bounds.y0, bounds.x1, bounds.y1],
        [
            f64::from(rect.left()),
            f64::from(rect.top()),
            f64::from(rect.right()),
            f64::from(rect.bottom()),
        ],
        "the disabled/enabled assertion must use this painted node's response"
    );
    *enabled
}

#[derive(Clone, Debug)]
struct DeferredRadialAuthoringAdd {
    binding: crate::radial::model::ActionBinding,
    label: String,
    source_query: String,
    trace_identity: acceptance_trace::RadialInsertionTraceIdentity,
}

fn defer_universal_context_action(
    deferred: &mut Option<DeferredUniversalAction>,
    action: crate::universal_actions::UniversalAction,
) {
    *deferred = Some(DeferredUniversalAction {
        action,
        surface: crate::universal_actions::ActionSurface::ContextMenu,
        source: ActivationSource::Click,
    });
}

pub(crate) fn deferred_activation_from_results(
    results: &[Action],
    idx: usize,
    source: ActivationSource,
) -> Option<DeferredActivation> {
    results.get(idx).cloned().map(|action| DeferredActivation {
        action,
        query_override: None,
        source,
    })
}

fn deferred_activation_unless_radial_add(
    activation: Option<DeferredActivation>,
    radial_add_pending: bool,
) -> Option<DeferredActivation> {
    if radial_add_pending { None } else { activation }
}

fn trace_root_result_pointer(
    ui: &egui::Ui,
    response: &egui::Response,
    kind: RootResultKind,
    index: usize,
    clicked: bool,
) {
    if !acceptance_trace::enabled() {
        return;
    }
    let (pressed, released, pointer_position) = ui.input(|input| {
        (
            input.pointer.any_pressed(),
            input.pointer.any_released(),
            input.pointer.interact_pos(),
        )
    });
    if !(pressed || released) || !(response.hovered() || response.is_pointer_button_down_on()) {
        return;
    }
    let (has_position, pointer_x, pointer_y) = pointer_position
        .map_or((false, i32::MIN, i32::MIN), |position| {
            (true, position.x.round() as i32, position.y.round() as i32)
        });
    acceptance_trace::emit(acceptance_trace::Event::RootResultPointer {
        kind,
        index,
        pressed,
        released,
        hovered: response.hovered(),
        clicked,
        has_position,
        pointer_x,
        pointer_y,
    });
}

fn root_result_kind(action: &str) -> RootResultKind {
    match action {
        "radial edit" => RootResultKind::RadialEdit,
        "radial skins" => RootResultKind::RadialSkins,
        _ => RootResultKind::Other,
    }
}

fn generic_result_tooltip(action: &Action) -> String {
    if action.desc == "Snippet" {
        action.label.clone()
    } else {
        action.action.clone()
    }
}

#[cfg(windows)]
fn trace_root_pointer_moves(ctx: &egui::Context, hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::{Foundation::POINT, Graphics::Gdi::ClientToScreen};

    if !acceptance_trace::enabled() {
        return;
    }
    let pointer_moves = ctx.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::PointerMoved(position) => Some(*position),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    let pixels_per_point = ctx.pixels_per_point();
    for position in pointer_moves {
        let mut screen = POINT {
            x: (position.x * pixels_per_point).round() as i32,
            y: (position.y * pixels_per_point).round() as i32,
        };
        if unsafe { ClientToScreen(hwnd, &mut screen) }.as_bool() {
            acceptance_trace::emit(acceptance_trace::Event::RootPointerMoved {
                screen_x: screen.x,
                screen_y: screen.y,
            });
        }
    }
    let pointer_button_state = ctx.input(|input| {
        (
            input.pointer.any_pressed(),
            input.pointer.any_released(),
            input.pointer.interact_pos(),
        )
    });
    if let (pressed, released, Some(position)) = pointer_button_state
        && (pressed || released)
    {
        let mut screen = POINT {
            x: (position.x * pixels_per_point).round() as i32,
            y: (position.y * pixels_per_point).round() as i32,
        };
        if unsafe { ClientToScreen(hwnd, &mut screen) }.as_bool() {
            acceptance_trace::emit(acceptance_trace::Event::RootPointerButton {
                pressed,
                released,
                screen_x: screen.x,
                screen_y: screen.y,
            });
        }
    }
}

fn render_universal_context_menu(
    ui: &mut egui::Ui,
    target: &crate::universal_actions::ResolvedActionTarget,
    actions: &[crate::universal_actions::UniversalAction],
    source_query: &str,
    primary_add: Result<crate::gui::universal_action_catalog::UniversalActionPickerRow, String>,
) -> Option<ContextMenuChoice> {
    let surface = crate::universal_actions::ActionSurface::ContextMenu;
    let mut previous_group = None;
    let mut selected = None;

    for action in actions {
        let presentation = action.effective_presentation(surface);
        if !presentation.visible {
            continue;
        }
        if previous_group.is_some() && previous_group != Some(presentation.group) {
            ui.separator();
        }
        previous_group = Some(presentation.group);

        let response = ui.add_enabled(
            action.is_available(),
            egui::Button::new(&presentation.label),
        );
        let response = match action.availability.disabled_reason() {
            Some(reason) => response.on_hover_text(reason),
            None => response,
        };
        if response.clicked() {
            selected = Some(ContextMenuChoice::Execute(action.clone()));
        }
    }

    ui.separator();
    let _add_menu = ui.menu_button("Add to radial", |ui| {
        ui.label("Primary action");
        let primary_action_id = match &primary_add {
            Ok(row) => {
                add_radial_action_choice(ui, row, "Add primary", source_query, &mut selected);
                Some(row.action_id.clone())
            }
            Err(reason) => {
                ui.add_enabled(
                    false,
                    egui::Button::new(format!("Primary action unavailable: {reason}")),
                )
                .on_disabled_hover_text(reason);
                None
            }
        };

        ui.separator();
        ui.label("Secondary actions");
        for action in actions {
            if primary_action_id.as_ref() == Some(&action.id) {
                continue;
            }
            let row = crate::gui::universal_action_catalog::UniversalActionAuthoringCatalog::row_for_semantic_action(
                target,
                &action.id,
                source_query,
            );
            match row {
                Some(row) => {
                    add_radial_action_choice(
                        ui,
                        &row,
                        "Add secondary",
                        source_query,
                        &mut selected,
                    );
                }
                None => {
                    ui.add_enabled(
                        false,
                        egui::Button::new(format!(
                            "Secondary action unavailable: {}",
                            action.presentation.label
                        )),
                    )
                    .on_disabled_hover_text("This action has no radial-menu presentation");
                }
            }
        }
        if !source_query.trim().is_empty() {
            ui.separator();
            let response = ui.button(format!("Save live query {:?}", source_query.trim()));
            #[cfg(test)]
            observe_d08_response(ui.ctx(), D08Control::SaveLiveQuery, &response);
            if response.clicked() {
                let binding = crate::radial::model::ActionBinding::LauncherQuery {
                    query: source_query.trim().to_owned(),
                    mode: crate::radial::model::QueryRunMode::OpenLauncher,
                };
                let trace_identity = acceptance_trace::RadialInsertionTraceIdentity {
                    request_id: acceptance_trace::next_radial_insertion_request_id(),
                    source_target_digest: acceptance_trace::private_trace_parts_digest(&[
                        "launcher_query",
                        source_query.trim(),
                    ]),
                    source_action_digest: acceptance_trace::private_trace_parts_digest(&[
                        "launcher_query",
                        "save_query",
                    ]),
                    source_binding_digest:
                        crate::gui::radial_editor::action_editor::trace_binding_digest(Some(
                            &binding,
                        )),
                    source_query_digest: acceptance_trace::private_trace_text_digest(
                        source_query.trim(),
                    ),
                };
                acceptance_trace::emit_radial_insertion_control(
                    trace_identity,
                    acceptance_trace::RadialInsertionControl::SourceAdd,
                    0,
                    0,
                    0,
                    0,
                    0,
                    true,
                    false,
                    true,
                    0,
                    trace_identity.source_binding_digest,
                );
                selected = Some(ContextMenuChoice::AddToRadial {
                    binding,
                    label: source_query.trim().to_owned(),
                    source_query: source_query.trim().to_owned(),
                    trace_identity,
                });
                ui.close_menu();
            }
        } else {
            ui.add_enabled(false, egui::Button::new("Save live query"))
                .on_disabled_hover_text("Enter a query to save it to the radial menu");
        }
    });
    #[cfg(test)]
    observe_d08_response(ui.ctx(), D08Control::AddMenu, &_add_menu.response);

    selected
}

fn add_radial_action_choice(
    ui: &mut egui::Ui,
    row: &crate::gui::universal_action_catalog::UniversalActionPickerRow,
    label_prefix: &str,
    source_query: &str,
    selected: &mut Option<ContextMenuChoice>,
) {
    let unavailable_reason = radial_add_unavailable_reason(row);
    let label = format!("{label_prefix}: {}", row.display_label());
    let response = ui.add_enabled(
        unavailable_reason.is_none(),
        egui::Button::new(unavailable_reason.as_ref().map_or_else(
            || label,
            |reason| format!("Unavailable: {} — {reason}", row.display_label()),
        )),
    );
    #[cfg(test)]
    observe_d08_response(ui.ctx(), D08Control::AddAction, &response);
    let response = match unavailable_reason.as_deref() {
        Some(reason) => response.on_disabled_hover_text(reason),
        None => response,
    };
    if unavailable_reason.is_some() || !response.clicked() {
        return;
    }
    let Some(choice) = radial_add_choice_for_row(row, source_query) else {
        return;
    };
    *selected = Some(choice);
    ui.close_menu();
}

fn radial_add_unavailable_reason(
    row: &crate::gui::universal_action_catalog::UniversalActionPickerRow,
) -> Option<String> {
    row.availability
        .disabled_reason()
        .map(str::to_owned)
        .or_else(|| {
            (!row.presentation.visible)
                .then(|| "This action is not available on the radial menu surface".into())
        })
        .or_else(|| row.assignment().err().map(|reason| reason.reason))
}

fn radial_add_choice_for_row(
    row: &crate::gui::universal_action_catalog::UniversalActionPickerRow,
    source_query: &str,
) -> Option<ContextMenuChoice> {
    if radial_add_unavailable_reason(row).is_some() {
        return None;
    }
    let binding = row.assignment().ok()?;
    let row_identity = crate::gui::radial_editor::action_editor::trace_picker_row_identity(row);
    let trace_identity = acceptance_trace::RadialInsertionTraceIdentity {
        request_id: acceptance_trace::next_radial_insertion_request_id(),
        source_target_digest: row_identity.target,
        source_action_digest: row_identity.action,
        source_binding_digest: row_identity.binding,
        source_query_digest: acceptance_trace::private_trace_text_digest(source_query),
    };
    acceptance_trace::emit_radial_insertion_control(
        trace_identity,
        acceptance_trace::RadialInsertionControl::SourceAdd,
        0,
        0,
        0,
        0,
        0,
        true,
        false,
        true,
        0,
        row_identity.binding,
    );
    Some(ContextMenuChoice::AddToRadial {
        binding,
        label: row.target_title.clone(),
        source_query: source_query.trim().to_owned(),
        trace_identity,
    })
}

fn resolve_primary_radial_add_row(
    app: &LauncherApp,
    selected: &Action,
    query: &str,
) -> Result<crate::gui::universal_action_catalog::UniversalActionPickerRow, String> {
    let (target, action) = app
        .resolve_launcher_result_action(selected, query)
        .map_err(|reason| format!("{} — {reason}", selected.label))?;
    crate::gui::universal_action_catalog::UniversalActionAuthoringCatalog::row_for_semantic_action(
        &target, &action.id, query,
    )
    .ok_or_else(|| {
        format!(
            "{} — the launcher primary action has no stable radial binding",
            selected.label
        )
    })
}

impl LauncherApp {
    pub(crate) fn launcher_query_keyboard_enabled(query_has_focus: bool) -> bool {
        query_has_focus
    }

    pub(crate) fn launcher_action_sheet_shortcut_enabled(
        query_has_focus: bool,
        file_search_open: bool,
        another_panel_open: bool,
    ) -> bool {
        query_has_focus && !file_search_open && !another_panel_open
    }

    fn consume_query_history_shortcut(
        query_has_focus: bool,
        input: &mut egui::InputState,
    ) -> Option<QueryHistoryDirection> {
        if !query_has_focus {
            return None;
        }

        let matching_event = |key| {
            input.events.iter().position(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: event_key,
                        pressed: true,
                        modifiers,
                        ..
                    } if *event_key == key
                        && modifiers.ctrl
                        && !modifiers.alt
                        && !modifiers.shift
                        && !modifiers.mac_cmd
                )
            })
        };
        let (event_index, direction) = if let Some(index) = matching_event(egui::Key::ArrowUp) {
            (index, QueryHistoryDirection::Older)
        } else if let Some(index) = matching_event(egui::Key::ArrowDown) {
            (index, QueryHistoryDirection::Newer)
        } else {
            return None;
        };

        // Remove the exact event whose own modifier snapshot matched. Using
        // `consume_key` here would logically match and consume shifted arrows.
        input.events.remove(event_index);
        Some(direction)
    }

    fn handle_query_text_changed(&mut self) {
        self.autocomplete_index = 0;
        if Self::is_note_search_query(&self.query) {
            self.last_note_search_change = Some(Instant::now());
        } else {
            self.last_note_search_change = None;
            self.search();
        }
    }

    fn apply_history_query(&mut self, query: String) {
        self.query = query;
        self.selected = None;
        self.suggestions.clear();
        self.handle_query_text_changed();
        self.move_cursor_end = true;
        self.focus_input();
    }

    fn navigate_query_history_with(
        &mut self,
        direction: QueryHistoryDirection,
        snapshot: impl FnOnce() -> Vec<String>,
    ) {
        let next_query = match direction {
            QueryHistoryDirection::Older => self.query_history.older(&self.query, snapshot),
            QueryHistoryDirection::Newer => self.query_history.newer(&self.query),
        };
        if let Some(query) = next_query {
            self.apply_history_query(query);
        }
    }

    fn navigate_query_history(&mut self, direction: QueryHistoryDirection) {
        self.navigate_query_history_with(direction, || {
            history::with_history(|entries| {
                entries
                    .iter()
                    .map(|entry| entry.query.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
        });
    }

    pub(crate) fn launcher_enter_activation_enabled(
        query_has_focus: bool,
        file_search_open: bool,
    ) -> bool {
        query_has_focus && !file_search_open
    }

    pub(crate) fn launcher_escape_handling_enabled(file_search_open: bool) -> bool {
        !file_search_open
    }

    pub(crate) fn launcher_query_focus_should_be_requested(
        just_became_visible: bool,
        focus_query: bool,
        file_search_open: bool,
    ) -> bool {
        (just_became_visible || focus_query) && !file_search_open
    }

    fn resolve_universal_actions(
        &self,
        action: &Action,
        pin: crate::universal_actions::PinCapability,
        surface: crate::universal_actions::ActionSurface,
    ) -> (
        crate::universal_actions::ResolvedActionTarget,
        Vec<crate::universal_actions::UniversalAction>,
    ) {
        let custom_len = self.custom_len.min(self.actions.len());
        let resolver_context = crate::universal_actions::ActionTargetResolverContext::new(
            &self.folder_aliases,
            &self.bookmark_aliases,
            &self.actions[..custom_len],
        );
        let resolved =
            crate::universal_actions::ActionTargetResolver.resolve(action, &resolver_context);
        let mut action_context =
            crate::universal_actions::ActionResolutionContext::new(surface, self.query.trim());
        action_context.pin = pin;
        action_context.can_add_favorite = resolved.target.persistent_ref().is_some();
        match &resolved.target {
            crate::universal_actions::ActionTarget::Timer { id } => {
                action_context.timer_paused = crate::plugins::timer::timer_paused(*id);
            }
            crate::universal_actions::ActionTarget::Stopwatch { id } => {
                action_context.stopwatch_paused = crate::plugins::stopwatch::stopwatch_paused(*id);
            }
            _ => {}
        }

        let actions =
            crate::universal_actions::UniversalActionRegistry.resolve(&resolved, &action_context);
        (resolved, actions)
    }

    pub(crate) fn resolve_context_menu_actions(
        &self,
        action: &Action,
        pin: crate::universal_actions::PinCapability,
    ) -> Vec<crate::universal_actions::UniversalAction> {
        self.resolve_universal_actions(
            action,
            pin,
            crate::universal_actions::ActionSurface::ContextMenu,
        )
        .1
    }

    fn pin_capability_for(&self, action: &Action) -> crate::universal_actions::PinCapability {
        match history::load_pins(HISTORY_PINS_FILE) {
            Ok(pins) => crate::universal_actions::PinCapability::Writable {
                is_pinned: pins.iter().any(|pin| pin.matches_action(action)),
            },
            Err(error) => crate::universal_actions::PinCapability::ReadOnly {
                is_pinned: false,
                reason: format!("Pinned results are read-only: {error}"),
            },
        }
    }

    pub(crate) fn open_action_sheet_for_index(&mut self, index: usize) -> bool {
        let Some(action) = self.results.get(index).cloned() else {
            return false;
        };
        let pin = self.pin_capability_for(&action);
        let (target, actions) = self.resolve_universal_actions(
            &action,
            pin,
            crate::universal_actions::ActionSurface::ActionSheet,
        );
        self.action_sheet
            .open(target, action, actions, &self.matcher);
        true
    }

    fn close_action_sheet(&mut self) {
        self.action_sheet.close();
        self.focus_input();
    }

    fn route_action_sheet_keyboard(
        &mut self,
        ctx: &egui::Context,
        query_input_id: egui::Id,
    ) -> Option<(crate::universal_actions::UniversalAction, ActivationSource)> {
        if self.action_sheet.is_open() {
            let key = ctx.input_mut(action_sheet::ActionSheetState::consume_key);
            return self.handle_action_sheet_key(key);
        }

        let query_has_focus = ctx.memory(|memory| memory.has_focus(query_input_id));
        let shortcut_enabled = Self::launcher_action_sheet_shortcut_enabled(
            query_has_focus,
            self.file_search_dialog.open,
            self.any_panel_open(),
        );
        if shortcut_enabled
            && ctx.input_mut(action_sheet::ActionSheetState::consume_open_shortcut)
            && let Some(index) = self.current_actionable_result_index()
        {
            self.open_action_sheet_for_index(index);
        }
        None
    }

    fn route_snippet_prompt_keyboard(&mut self, ctx: &egui::Context) -> bool {
        self.route_snippet_prompt_keyboard_with(ctx, |app| {
            let _ = app.submit_snippet_prompt();
        })
    }

    fn route_snippet_prompt_keyboard_with(
        &mut self,
        ctx: &egui::Context,
        mut submit: impl FnMut(&mut Self),
    ) -> bool {
        if !self.snippet_prompt_dialog.is_open() {
            return false;
        }

        #[derive(Clone, Copy)]
        enum PromptKey {
            Escape(egui::Modifiers),
            CtrlEnter(egui::Modifiers),
            Tab {
                modifiers: egui::Modifiers,
                backwards: bool,
            },
        }

        let key = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    modifiers,
                    ..
                } => Some(PromptKey::Escape(*modifiers)),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.mac_cmd => {
                    Some(PromptKey::CtrlEnter(*modifiers))
                }
                egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } => Some(PromptKey::Tab {
                    modifiers: *modifiers,
                    backwards: modifiers.shift,
                }),
                _ => None,
            })
        });

        match key {
            Some(PromptKey::Escape(modifiers)) => {
                ctx.input_mut(|input| {
                    input.consume_key(modifiers, egui::Key::Escape);
                });
                let _ = self.cancel_snippet_prompt();
            }
            Some(PromptKey::CtrlEnter(modifiers)) => {
                ctx.input_mut(|input| {
                    input.consume_key(modifiers, egui::Key::Enter);
                });
                if !self.snippet_prompt_dialog.is_preview_only() {
                    submit(self);
                }
            }
            Some(PromptKey::Tab {
                modifiers,
                backwards,
            }) => {
                ctx.input_mut(|input| {
                    input.consume_key(modifiers, egui::Key::Tab);
                });
                self.snippet_prompt_dialog.queue_tab_focus(ctx, backwards);
            }
            None => {}
        }

        // The open prompt owns this complete frame, even if Escape or a
        // successful submit closed it above.
        true
    }

    fn consume_prompt_opening_frame_keys(ctx: &egui::Context) {
        ctx.input_mut(|input| {
            let owned_keys = input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if matches!(key, egui::Key::Enter | egui::Key::Tab | egui::Key::Escape) => {
                        Some((*modifiers, *key))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            for (modifiers, key) in owned_keys {
                input.consume_key(modifiers, key);
            }
        });
    }

    pub(crate) fn handle_action_sheet_key(
        &mut self,
        key: Option<action_sheet::ActionSheetKey>,
    ) -> Option<(crate::universal_actions::UniversalAction, ActivationSource)> {
        match key {
            Some(action_sheet::ActionSheetKey::Escape) => self.close_action_sheet(),
            Some(action_sheet::ActionSheetKey::Up) => self.action_sheet.move_selection(-1),
            Some(action_sheet::ActionSheetKey::Down) => self.action_sheet.move_selection(1),
            Some(action_sheet::ActionSheetKey::Enter) => {
                let selected = self
                    .action_sheet
                    .selected_invocation(ActivationSource::Enter);
                if selected.is_some() {
                    self.close_action_sheet();
                }
                return selected;
            }
            None => {}
        }
        None
    }

    fn attach_result_context_menu(
        &mut self,
        action: &Action,
        menu_resp: egui::Response,
        _refresh: &mut bool,
        _set_focus: &mut bool,
        deferred: &mut Option<DeferredUniversalAction>,
        deferred_radial_add: &mut Option<DeferredRadialAuthoringAdd>,
    ) -> egui::Response {
        // egui only invokes this closure while the popup is open. Keep target
        // resolution and pin I/O here so ordinary list/grid rendering remains
        // on the legacy primary-action fast path.
        menu_resp.clone().context_menu(|ui| {
            let pin = self.pin_capability_for(action);
            let (target, actions) = self.resolve_universal_actions(
                action,
                pin,
                crate::universal_actions::ActionSurface::ContextMenu,
            );
            let primary_add = resolve_primary_radial_add_row(self, action, &self.query);

            if let Some(choice) =
                render_universal_context_menu(ui, &target, &actions, &self.query, primary_add)
            {
                match choice {
                    ContextMenuChoice::Execute(action) => {
                        defer_universal_context_action(deferred, action);
                        ui.close_menu();
                    }
                    ContextMenuChoice::AddToRadial {
                        binding,
                        label,
                        source_query,
                        trace_identity,
                    } => {
                        *deferred_radial_add = Some(DeferredRadialAuthoringAdd {
                            binding,
                            label,
                            source_query,
                            trace_identity,
                        });
                        ui.close_menu();
                    }
                }
            }
        });

        menu_resp
    }

    fn execute_deferred_result_action(&mut self, deferred: Option<DeferredUniversalAction>) {
        if let Some(deferred) = deferred {
            self.execute_universal_action(deferred.action, deferred.surface, deferred.source);
        }
    }
}

impl LauncherApp {
    pub(crate) fn poll_clipboard_modify_runtime(&mut self, ctx: &egui::Context) {
        let reload_events = self
            .clipboard_modify_watcher
            .as_mut()
            .map(|watcher| watcher.poll(Instant::now()))
            .unwrap_or_default();
        for event in reload_events {
            self.handle_clipboard_modify_gui_event(event);
            ctx.request_repaint();
        }
        let preview_finished = self.clipboard_modify_dialog.preview.tick();
        if preview_finished {
            ctx.request_repaint();
        }
        let had_immediate = self.clipboard_modify_immediate.has_pending();
        self.drain_clipboard_modify_immediate();
        if preview_finished || (had_immediate && !self.clipboard_modify_immediate.has_pending()) {
            ctx.request_repaint();
        }
        if self.clipboard_modify_dialog.preview.is_active()
            || self.clipboard_modify_immediate.has_pending()
            || self
                .clipboard_modify_watcher
                .as_ref()
                .is_some_and(|watcher| watcher.has_pending_reload())
        {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}

impl LauncherApp {
    /// Route a macro's raw launcher query through the same search and action
    /// activation paths used by the GUI.
    pub fn handle_macro_launcher_command(
        &mut self,
        request: &crate::mkmacro::LauncherCommandRequest,
    ) -> crate::mkmacro::LauncherCommandResponse {
        use crate::mkmacro::{LauncherCommandKind, LauncherCommandResponse};

        let LauncherCommandKind::Query(query) = &request.kind else {
            let LauncherCommandKind::ResolvedLegacy(action) = &request.kind else {
                unreachable!()
            };
            let before = self.launcher_interaction_snapshot();
            self.activate_action(action.clone(), None, ActivationSource::Macro);
            self.restore_for_new_launcher_interaction(&before);
            return LauncherCommandResponse::Activated;
        };

        self.query = query.clone();
        self.last_timer_query =
            self.query.starts_with("timer list") || self.query.starts_with("alarm list");
        self.selected = None;
        self.last_results_valid = false;
        self.search();

        match self.results.as_slice() {
            [] => LauncherCommandResponse::NoResults,
            [action] => {
                let action = action.clone();
                let before = self.launcher_interaction_snapshot();
                self.activate_action(action, None, ActivationSource::Macro);
                self.restore_for_new_launcher_interaction(&before);
                LauncherCommandResponse::Activated
            }
            results => {
                let result_count = results.len();
                self.selected = None;
                self.move_cursor_end = true;
                self.focus_input();
                self.request_launcher_state(Some(true), Some(true));
                LauncherCommandResponse::PresentedForSelection { result_count }
            }
        }
    }

    /// Number of normal note editor panels currently open.
    pub fn open_note_panel_count(&self) -> usize {
        self.note_panels.len()
    }

    fn poll_macro_launcher_commands_from(
        &mut self,
        ctx: &egui::Context,
        broker: &crate::mkmacro::LauncherCommandBroker,
    ) {
        broker.set_repaint(std::sync::Arc::new({
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        }));
        let Some(pending) = broker.take_pending() else {
            return;
        };

        let response = self.handle_macro_launcher_command(&pending.request);
        // A stopped macro may have dropped its receiver after the GUI took the
        // request. `respond` deliberately makes that race harmless.
        let _ = pending.respond(response);
    }

    fn poll_macro_launcher_commands(&mut self, ctx: &egui::Context) {
        let broker = crate::mkmacro::production_launcher_command_broker();
        self.poll_macro_launcher_commands_from(ctx, &broker);
    }

    fn poll_macro_launcher_query(&mut self, ctx: &egui::Context) {
        let broker = crate::mkmacro::production_launcher_query_broker();
        broker.set_repaint(std::sync::Arc::new({
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        }));
        let Some(pending) = broker.take_pending() else {
            return;
        };
        self.query = pending.query.clone();
        self.last_results_valid = false;
        self.search();
        let response = if let Some(action) = self.results.first().cloned() {
            self.activate_action(action, None, ActivationSource::Enter);
            Ok(())
        } else {
            Err(format!("no launcher result for query '{}'", pending.query))
        };
        pending.respond(response);
    }

    fn poll_macro_prompt(&mut self, ctx: &egui::Context) {
        let broker = crate::mkmacro::production_prompt_broker();
        broker.set_repaint(std::sync::Arc::new({
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        }));
        if self.macro_prompt.pending.is_none()
            && let Some(pending) = broker.take_pending()
        {
            self.macro_prompt.text = pending.request.default_value.clone();
            self.macro_prompt.first_frame = true;
            self.macro_prompt.pending = Some(pending);
        }
        let Some(pending) = self.macro_prompt.pending.as_ref() else {
            return;
        };
        let title = pending.request.title.clone();
        let prompt = pending.request.prompt.clone();
        let mut answer = None;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("mkmacro_prompt"),
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([420.0, 150.0])
                .with_always_on_top(),
            |child, _| {
                egui::CentralPanel::default().show(child, |ui| {
                    ui.label(prompt);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.macro_prompt.text)
                            .desired_width(f32::INFINITY)
                            .hint_text("Enter a value"),
                    );
                    if self.macro_prompt.first_frame {
                        response.request_focus();
                        self.macro_prompt.first_frame = false;
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        answer = Some(crate::mkmacro::PromptResponse::Submitted(
                            self.macro_prompt.text.clone(),
                        ));
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        answer = Some(crate::mkmacro::PromptResponse::Cancelled);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            answer = Some(crate::mkmacro::PromptResponse::Submitted(
                                self.macro_prompt.text.clone(),
                            ));
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_text("Cancel stops the running macro")
                            .clicked()
                        {
                            answer = Some(crate::mkmacro::PromptResponse::Cancelled);
                        }
                    });
                });
                if child.input(|i| i.viewport().close_requested()) {
                    answer.get_or_insert(crate::mkmacro::PromptResponse::Cancelled);
                }
            },
        );
        if let Some(answer) = answer {
            if let Some(pending) = self.macro_prompt.pending.take() {
                pending.respond(answer);
            }
            self.macro_prompt.text.clear();
            self.macro_prompt.first_frame = false;
        }
    }
}

impl LauncherApp {
    pub(super) fn render_root_frame(&mut self, ctx: &egui::Context, frame: Option<&eframe::Frame>) {
        use egui::*;

        if acceptance_trace::enabled()
            && ctx.input(|input| {
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        Event::Key {
                            key: Key::F24,
                            pressed: true,
                            ..
                        }
                    )
                })
            })
        {
            acceptance_trace::emit(acceptance_trace::Event::FrontendKey {
                key: acceptance_trace::AcceptanceKey::F24,
                focused: ctx.input(|input| input.viewport().focused).unwrap_or(false),
                foreground_owner: acceptance_trace::foreground_owner(),
            });
        }

        if let Some(frame) = frame {
            self.root_window_bridge.capture_frame(frame);
        }
        let trace_root_hwnd = acceptance_trace::enabled()
            .then(|| frame.and_then(crate::window_manager::get_hwnd))
            .flatten();
        if let Some(hwnd) = trace_root_hwnd {
            crate::window_manager::register_root_hwnd(hwnd);
        }
        #[cfg(windows)]
        if let Some(hwnd) = trace_root_hwnd {
            trace_root_pointer_moves(ctx, hwnd);
        }
        if acceptance_trace::advance_window_sample_frame() {
            if let Some(hwnd) = trace_root_hwnd {
                while let Some(correlation) = acceptance_trace::take_window_sample_request() {
                    crate::window_manager::emit_window_snapshot(hwnd, correlation);
                }
            }
            if acceptance_trace::window_sample_pending() {
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        }

        // Prompt keyboard actions must win before Escape cleanup, action-sheet
        // routing, or launcher-query handling can observe the same event.
        let mut snippet_prompt_owns_input =
            self.snippet_prompt_dialog.is_open() && !self.ocr_surface_visible();
        if snippet_prompt_owns_input {
            self.route_snippet_prompt_keyboard(ctx);
        }

        let plugin_search_generation = self.plugins.search_generation();
        if plugin_search_generation != self.last_plugin_search_generation
            && !self.ocr_defers_launcher_query_refresh()
        {
            self.last_plugin_search_generation = plugin_search_generation;
            self.last_results_valid = false;
            self.search();
        }

        crate::performance::record_frame(
            self.visible_flag.load(Ordering::Relaxed),
            ctx.input(|input| input.viewport().focused).unwrap_or(true),
            self.should_show_dashboard(self.query.as_str()),
        );

        if self
            .mkmacro_dialog
            .action_editor
            .visual_capture
            .as_ref()
            .is_some_and(|w| w.active())
            || self
                .mkmacro_dialog
                .action_editor
                .visual_overlay
                .operation_id()
                .is_some()
        {
            ctx.request_repaint();
        }

        self.poll_crop_screenshot(ctx);
        self.poll_screen_draw_region_picker(ctx);
        self.poll_macro_launcher_query(ctx);
        self.poll_macro_launcher_commands(ctx);
        self.poll_macro_prompt(ctx);
        if !snippet_prompt_owns_input {
            self.macro_parameter_prompt.show(ctx);
        }

        // tracing::debug!("LauncherApp::update called");
        if let Some(hwnd) = frame.and_then(crate::window_manager::get_hwnd) {
            self.launcher_hwnd = Some(hwnd.0 as usize);
        }
        self.cancel_screen_draw_startup_on_escape(ctx);
        self.poll_color_pick(ctx);
        self.poll_ocr_selection(ctx);
        self.poll_screen_draw_capture(ctx);
        self.show_screen_draw_toolbar(ctx);
        self.multi_manager_drain_runtime_events();
        self.poll_clipboard_modify_runtime(ctx);
        let _ = self.multi_manager.start_pending_automatic_reconnect();
        if self
            .multi_manager
            .reconnect_in_progress
            .load(Ordering::Acquire)
        {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        self.multi_manager.maybe_auto_save_bindings();
        if let Some(message) = self.data_recovery_dialog.take_startup_notice()
            && self.enable_toasts
        {
            self.add_toast(Toast {
                text: message.into(),
                kind: ToastKind::Warning,
                options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
            });
        }
        if self.enable_toasts {
            self.toasts.show(ctx);
        }
        let frame_time = Duration::from_secs_f32(ctx.input(|i| i.unstable_dt).max(0.0));
        self.dashboard.update_frame_timing(frame_time);
        if let Some(pending) = self.pending_query.take() {
            self.query = pending;
            self.search();
            self.focus_input();
        }
        if !self.ocr_defers_launcher_query_refresh() {
            self.maybe_run_note_search_debounce();
        }
        if let (Some(t), Some(_)) = (self.error_time, self.error.as_ref())
            && t.elapsed().as_secs_f32() >= 3.0
        {
            self.error = None;
            self.error_time = None;
        }
        if self
            .enabled_capabilities
            .as_ref()
            .and_then(|m| m.get("timer"))
            .map(|c| c.contains(&"completion_dialog".to_string()))
            .unwrap_or(true)
        {
            for msg in crate::plugins::timer::take_finished_messages() {
                self.completion_dialog.open_message(msg);
            }
        }
        for msg in crate::plugins::macros::take_step_messages() {
            if self.enable_toasts {
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: msg.into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        }
        for msg in crate::plugins::browser_tabs::take_cache_messages() {
            if self.enable_toasts {
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: msg.into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        }
        for err in crate::plugins::macros::take_error_messages() {
            tracing::debug!("{err}");
        }

        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        self.handle_dropped_files(dropped);
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.window_size = (rect.width() as i32, rect.height() as i32);
        }
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            self.window_pos = (rect.min.x as i32, rect.min.y as i32);
        }
        let (
            visibility_request,
            (
                should_be_visible,
                do_restore,
                focus_intent,
                visibility_invocation_id,
                reconcile_native_presentation,
            ),
        ) = self.visibility_revision.inspect(|| {
            (
                self.visible_flag.load(Ordering::SeqCst),
                self.restore_flag.swap(false, Ordering::SeqCst),
                self.visibility_revision.focus_intent(),
                self.visibility_revision.invocation_id(),
                self.root_window_bridge
                    .take_presentation_reconcile_request(),
            )
        });
        if reconcile_native_presentation && self.color_pick_owns_root() {
            self.reconcile_color_pick_parking();
        }
        if reconcile_native_presentation && self.ocr_owns_root() {
            self.reconcile_ocr_parking();
        }
        let screen_draw_repark_result = if reconcile_native_presentation && !should_be_visible {
            let controller = &self.screen_draw_controller;
            self.screen_draw_launcher_parking
                .as_mut()
                .map(|transaction| {
                    transaction.repark_after_stale_restore(|| controller.begin_launcher_repark())
                })
        } else {
            None
        };
        let root_ctx = RootViewportCtx::with_window_bridge(ctx, self.root_window_bridge.clone());
        if self.visible_flag.load(Ordering::SeqCst) && self.help_flag.swap(false, Ordering::SeqCst)
        {
            self.help_window.overlay_open = !self.help_window.overlay_open;
        } else {
            // reset any queued toggle when window not visible
            self.help_flag.store(false, Ordering::SeqCst);
        }
        let mut just_became_visible = false;
        let mut native_restore_target = None;
        let mut native_activation_requested = false;
        let color_pick_owns_root = self.color_pick_owns_root();
        let ocr_owns_root = self.ocr_owns_root();
        let _ = self.visibility_revision.with_current(
            visibility_request,
            || self.visible_flag.load(Ordering::SeqCst) == should_be_visible,
            || {
                if color_pick_owns_root || ocr_owns_root { return; }
                just_became_visible = !self.last_visible && should_be_visible;
                let visibility_changed = self.last_visible != should_be_visible;
                if do_restore && should_be_visible {
                    acceptance_trace::emit(acceptance_trace::Event::Restore {
                        edge: RestoreEdge::RestoreFlag,
                        correlation: Correlation::default(),
                    });
                }
                #[cfg(windows)]
                let waiting_for_first_native_identity = should_be_visible
                    && do_restore && !visibility_changed && !reconcile_native_presentation
                    && self.root_window_bridge.activation_target().is_none();
                #[cfg(not(windows))]
                let waiting_for_first_native_identity = false;
                if waiting_for_first_native_identity {
                    // HWND attachment wakes ROOT. Keep this current request
                    // armed without a periodic repaint or a fabricated target.
                    self.restore_flag.store(true, Ordering::SeqCst);
                    return;
                }
                if (do_restore && should_be_visible) || visibility_changed || reconcile_native_presentation {
                    let screen_draw_owns_hidden_geometry = reconcile_native_presentation
                        && !should_be_visible
                        && screen_draw_repark_result.as_ref().is_some_and(Result::is_ok);
                    if !screen_draw_owns_hidden_geometry
                        && (should_be_visible || self.screen_draw_launcher_parking.is_none())
                    {
                        let placement_policy = if visibility_changed {
                            VisiblePlacementPolicy::ApplyConfiguredPlacement
                        } else {
                            VisiblePlacementPolicy::PreserveCurrentGeometry
                        };
                        native_activation_requested = acceptance_trace::with_visibility_trace_link(
                            visibility_request,
                            visibility_invocation_id,
                            || apply_visibility_with_focus_intent(
                                should_be_visible, focus_intent, placement_policy, &root_ctx,
                                self.offscreen_pos, self.follow_mouse, self.static_location_enabled,
                                self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                                self.static_size.map(|(w, h)| (w as f32, h as f32)),
                                (self.window_size.0 as f32, self.window_size.1 as f32),
                            ),
                        ) == crate::visibility::RootActivationDisposition::OrderedNative;
                    }
                    if reconcile_native_presentation
                        && let Some(Err(error)) = screen_draw_repark_result.as_ref()
                    {
                        tracing::warn!(%error, "failed to repark ROOT during stale activation reconciliation; queued ordinary visibility fallback");
                        if !should_be_visible {
                            acceptance_trace::with_visibility_trace_link(
                                visibility_request, visibility_invocation_id,
                                || apply_visibility_with_focus_intent(
                                    false, focus_intent, VisiblePlacementPolicy::PreserveCurrentGeometry,
                                    &root_ctx, self.offscreen_pos, self.follow_mouse, self.static_location_enabled,
                                    self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                                    self.static_size.map(|(w, h)| (w as f32, h as f32)),
                                    (self.window_size.0 as f32, self.window_size.1 as f32),
                                ),
                            );
                            if let Some(transaction) = self.screen_draw_launcher_parking.as_mut() {
                                transaction.retain_restore_point_after_fallback_park();
                            }
                        }
                    }
                    if native_activation_requested {
                        native_restore_target = self.root_window_bridge.activation_target();
                        if native_restore_target.is_none() {
                            self.restore_flag.store(true, Ordering::SeqCst);
                        }
                    }
                    self.last_visible = should_be_visible;
                }
            },
        );
        // Admission takes the same non-reentrant visibility gate. One current
        // presentation (even restore+show+reconcile) submits at most one group.
        if let Some(target) = native_restore_target {
            if !crate::window_manager::restore_launcher_to_current_desktop_ordered(
                target,
                self.visibility_revision.clone(),
                visibility_request,
                self.visible_flag.clone(),
                self.root_window_bridge.clone(),
            ) {
                tracing::warn!(
                    visibility_request,
                    "ROOT native activation was rejected or could not be queued"
                );
            }
        }
        if reconcile_native_presentation && self.visibility_revision.current() != visibility_request
        {
            // An exact Screen Draw repark or viewport command may have raced a
            // newer show. Re-arm reconciliation so the newest frame repairs
            // native geometry instead of trusting last_visible.
            self.root_window_bridge.request_presentation_reconcile();
        }

        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                let mut apps_menu_state = (false, false, false);
                let mut file_menu_body_entered = false;
                let mut apps_menu_body_entered = false;
                let file_menu = ui.menu_button("File", |ui| {
                    file_menu_body_entered = true;
                    acceptance_trace::trace_root_menu_body(
                        crate::radial::acceptance_trace::RootMenuControl::File,
                        true,
                    );
                    let apps_menu = ui.menu_button("Apps", |ui| {
                        apps_menu_body_entered = true;
                        acceptance_trace::trace_root_menu_body(
                            crate::radial::acceptance_trace::RootMenuControl::Apps,
                            true,
                        );
                        if ui.button("Edit Apps").clicked() {
                            self.show_editor = !self.show_editor;
                        }
                        if ui.button("Edit Plugins").clicked() {
                            self.show_plugins = !self.show_plugins;
                        }
                        if ui.button("Edit Radial Menus").clicked() {
                            self.focus_panel(crate::gui::Panel::RadialEditor);
                        }
                    });
                    apps_menu_state = (
                        apps_menu.response.hovered(),
                        apps_menu.response.clicked(),
                        apps_menu.inner.is_some(),
                    );
                    if ui.button("Close Application").clicked() {
                        // eframe's `on_exit` is the single shutdown boundary. It
                        // tears down Screen Draw before flushing preferences.
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                if !file_menu_body_entered {
                    acceptance_trace::trace_root_menu_body(
                        crate::radial::acceptance_trace::RootMenuControl::File,
                        false,
                    );
                }
                if !apps_menu_body_entered {
                    acceptance_trace::trace_root_menu_body(
                        crate::radial::acceptance_trace::RootMenuControl::Apps,
                        false,
                    );
                }
                acceptance_trace::trace_root_menu_interaction(
                    RootMenuControl::Apps,
                    apps_menu_state.0,
                    apps_menu_state.1,
                    apps_menu_state.2,
                );
                acceptance_trace::trace_root_menu_interaction(
                    RootMenuControl::File,
                    file_menu.response.hovered(),
                    file_menu.response.clicked(),
                    file_menu.inner.is_some(),
                );
                ui.menu_button("Settings", |ui| {
                    if ui.button("Edit Settings").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Command List").clicked() {
                        self.help_window.open = true;
                    }
                    if ui.button("Linking Guide (todo/note/cal)").clicked() {
                        self.help_window.open = true;
                        self.help_window.filter = "todo note cal @note: @todo:".into();
                    }
                    if ui.button("Quick Help Overlay").clicked() {
                        self.help_window.overlay_open = true;
                    }
                    if ui.button("Open Toast Log").clicked() {
                        if std::fs::OpenOptions::new()
                            .create(true)
                            .write(true)
                            .open(TOAST_LOG_FILE)
                            .is_err()
                        {
                            self.report_error_message("launcher", "Failed to create log");
                        } else if let Err(e) = open::that(TOAST_LOG_FILE) {
                            self.report_error_message(
                                "launcher",
                                format!("Failed to open log: {e}"),
                            );
                        }
                    }
                    if ui.button("View Toast Log").clicked() {
                        self.toast_log_dialog.open();
                    }
                });
                for panel in self.pinned_panels.clone() {
                    let label = format!("{:?}", panel);
                    if ui.button(label).clicked() {
                        if self.panel_stack.last() == Some(&panel) {
                            self.toggle_pin(panel);
                        } else {
                            self.focus_panel(panel);
                        }
                    }
                }
            });
        });

        self.process_watch_events();
        self.flush_background_query_refresh();
        self.start_next_authoring_provider_search();
        self.show_radial_placement_failure(ctx);

        let trimmed = self.query.trim().to_string();
        let use_dashboard = self.should_show_dashboard(trimmed.as_str());
        if !self.ocr_defers_launcher_query_refresh() {
            self.maybe_refresh_timer_list();
            self.maybe_refresh_stopwatch_list();
            if trimmed.eq_ignore_ascii_case("net")
                && self.last_net_update.elapsed().as_secs_f32() >= self.net_refresh
            {
                self.search();
                self.last_net_update = Instant::now();
            }
        }

        // The Action Sheet owns keyboard input before the launcher query and
        // navigation paths see it. Opening is also resolved before rendering,
        // so the filter can take focus in the same frame as Ctrl+Enter.
        let query_input_id = egui::Id::new("query_input");
        // Latch ownership for this entire frame, including a dismissal frame,
        // so input cannot fall through to the underlying query after Close.
        let ocr_blocks_launcher_input = self.ocr_surface_visible();
        if !ocr_blocks_launcher_input
            && !snippet_prompt_owns_input
            && self.snippet_prompt_dialog.is_open()
        {
            // A prompt opened by an earlier asynchronous or macro action owns
            // the rest of this frame, but the key that opened it must not also
            // be treated as a submit.
            snippet_prompt_owns_input = true;
            Self::consume_prompt_opening_frame_keys(ctx);
        }
        self.route_ocr_surface_dismissal(ctx);
        let mut selected_sheet_action = if ocr_blocks_launcher_input || snippet_prompt_owns_input {
            None
        } else {
            self.route_action_sheet_keyboard(ctx, query_input_id)
        };

        if self.action_sheet.is_open() && !ocr_blocks_launcher_input && !snippet_prompt_owns_input {
            if let Some(action) = action_sheet::render(ctx, &mut self.action_sheet, &self.matcher) {
                self.close_action_sheet();
                selected_sheet_action = Some(action);
            } else if !self.action_sheet.is_open() {
                self.focus_input();
            }
        }
        if let Some((action, source)) = selected_sheet_action {
            // Closing first prevents primary and UI-intent execution from
            // competing with the sheet's focus or modal state.
            self.execute_universal_action(
                action,
                crate::universal_actions::ActionSurface::ActionSheet,
                source,
            );
        }
        if !ocr_blocks_launcher_input
            && !snippet_prompt_owns_input
            && self.snippet_prompt_dialog.is_open()
        {
            snippet_prompt_owns_input = true;
            Self::consume_prompt_opening_frame_keys(ctx);
        }
        let action_sheet_blocks_launcher_input =
            self.action_sheet.is_open() || snippet_prompt_owns_input;

        let mut deferred_universal_action = None;
        let mut deferred_radial_authoring_add = None;
        CentralPanel::default().show(ctx, |ui| {
            if ocr_blocks_launcher_input {
                ui.heading("Screen Region OCR");
                return;
            }
            let mut deferred_activation: Option<DeferredActivation> = None;
            ui.heading("🚀 Multi Lnchr");
            if self.should_render_inline_error()
                && let Some(err) = &self.error
            {
                ui.colored_label(Color32::RED, err);
            }

            scale_ui(ui, self.query_scale, |ui| {
                let input_id = query_input_id;

                let query_owned_focus = ui.ctx().memory(|memory| memory.has_focus(input_id));
                let numpad_navigation = ui.ctx().input_mut(|input| {
                    consume_physical_numpad_navigation(
                        query_owned_focus,
                        input,
                        &NativeNumpadKeyStateProbe,
                    )
                });

                let mut query_output = egui::TextEdit::singleline(&mut self.query)
                    .id(input_id)
                    .interactive(!action_sheet_blocks_launcher_input)
                    .desired_width(f32::INFINITY)
                    .show(ui);
                if self.move_cursor_end
                    && !action_sheet_blocks_launcher_input
                    && query_output.response.enabled()
                    && query_output.response.has_focus()
                {
                    query_output
                        .state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(self.query.chars().count()),
                        )));
                    query_output.state.clone().store(ui.ctx(), input_id);
                    self.move_cursor_end = false;
                }
                let query_response = query_output.response;
                if Self::launcher_query_focus_should_be_requested(
                    just_became_visible,
                    self.focus_query,
                    self.file_search_dialog.open,
                ) && !action_sheet_blocks_launcher_input
                {
                    query_response.request_focus();
                    self.focus_query = false;
                }
                let query_has_focus = query_response.has_focus();

                self.query_history.synchronize(&self.query);
                if query_response.changed() {
                    self.handle_query_text_changed();
                }

                let history_direction = if action_sheet_blocks_launcher_input {
                    None
                } else {
                    ctx.input_mut(|input| {
                        Self::consume_query_history_shortcut(query_has_focus, input)
                    })
                };
                if let Some(direction) = history_direction {
                    self.navigate_query_history(direction);
                }

                if !action_sheet_blocks_launcher_input {
                    for direction in numpad_navigation {
                        self.handle_key(direction.navigation_key());
                    }
                }

                if self.query_autocomplete && !use_dashboard && !self.suggestions.is_empty() {
                    ui.vertical(|ui| {
                        for s in &self.suggestions {
                            ui.colored_label(Color32::GRAY, s);
                        }
                    });
                }

                if !action_sheet_blocks_launcher_input
                    && Self::launcher_escape_handling_enabled(self.file_search_dialog.open)
                    && ctx.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    if self.any_panel_open() {
                        if self.close_front_dialog() {
                            ctx.input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            });
                        }
                    } else {
                        self.request_launcher_visibility(false);
                    }
                }

                if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::W))
                    && self.any_panel_open()
                    && self.close_front_dialog()
                {
                    ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W));
                }

                if !action_sheet_blocks_launcher_input
                    && history_direction.is_none()
                    && Self::launcher_query_keyboard_enabled(query_has_focus)
                {
                    for key in [
                        egui::Key::ArrowDown,
                        egui::Key::ArrowUp,
                        egui::Key::PageDown,
                        egui::Key::PageUp,
                        egui::Key::ArrowLeft,
                        egui::Key::ArrowRight,
                    ] {
                        if ctx.input(|i| i.key_pressed(key)) {
                            self.handle_key(key);
                        }
                    }
                }

                let tab = ctx.input(|i| i.key_pressed(egui::Key::Tab));
                let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                let mut accepted_suggestion = false;
                if !action_sheet_blocks_launcher_input
                    && Self::launcher_query_keyboard_enabled(query_has_focus)
                    && (tab || (enter && self.selected.is_none()))
                {
                    accepted_suggestion = self.accept_suggestion(tab);
                }
                if accepted_suggestion {
                    ctx.input_mut(|i| {
                        if tab {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                        }
                        if enter {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                        }
                    });
                }

                let mut launch_idx: Option<usize> = None;
                if !action_sheet_blocks_launcher_input
                    && !accepted_suggestion
                    && enter
                    && Self::launcher_enter_activation_enabled(
                        query_has_focus,
                        self.file_search_dialog.open,
                    )
                    && !self.bookmark_alias_dialog.open
                    && !self.tempfile_alias_dialog.open
                    && !self.tempfile_dialog.open
                    && !self.shell_cmd_dialog.open
                    && !self.notes_dialog.open
                    && !self.todo_dialog.open
                    && !self.todo_view_dialog.open
                    && self.note_panels.is_empty()
                    && self.image_panels.is_empty()
                {
                    launch_idx = self.handle_key(egui::Key::Enter);
                }

                if let Some(i) = launch_idx {
                    deferred_activation =
                        deferred_activation_from_results(&self.results, i, ActivationSource::Enter);
                }
            });

            if use_dashboard {
                if !self.suggestions.is_empty() {
                    self.autocomplete_index = 0;
                    self.suggestions.clear();
                }
                let dashboard_visible = self.visible_flag.load(Ordering::SeqCst);
                let dashboard_focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
                let has_diagnostics_widget = self.has_diagnostics_widget();
                let show_diagnostics_widget =
                    self.show_dashboard_diagnostics || has_diagnostics_widget;
                let diagnostics = if self.show_dashboard_diagnostics || has_diagnostics_widget {
                    Some(self.dashboard.diagnostics_snapshot())
                } else {
                    None
                };
                let repaint_demand = self.dashboard.repaint_demand(
                    dashboard_visible,
                    dashboard_focused,
                    self.reduce_dashboard_work_when_unfocused,
                    show_diagnostics_widget,
                );
                let dash_ctx = DashboardContext {
                    actions: &self.actions,
                    actions_by_id: &self.actions_by_id,
                    usage: &self.usage,
                    plugins: &self.plugins,
                    enabled_plugins: self.enabled_plugins.as_ref(),
                    default_location: self.dashboard_default_location.as_deref(),
                    data_cache: &self.dashboard_data_cache,
                    actions_version: crate::actions::actions_version(),
                    fav_version: crate::plugins::fav::fav_version(),
                    notes_version: crate::plugins::note::note_version(),
                    todo_version: crate::plugins::todo::todo_version(),
                    calendar_version: crate::plugins::calendar::calendar_version(),
                    clipboard_version: crate::plugins::clipboard::clipboard_version(),
                    snippets_version: crate::plugins::snippets::snippets_version(),
                    dashboard_visible,
                    dashboard_focused,
                    reduce_dashboard_work_when_unfocused: self.reduce_dashboard_work_when_unfocused,
                    diagnostics,
                    show_diagnostics_widget,
                };
                if let Some(interval) =
                    crate::dashboard::repaint_interval(crate::dashboard::RepaintPolicyInput {
                        launcher_visible: dashboard_visible,
                        dashboard_active: true,
                        viewport_focused: dashboard_focused,
                        reduce_when_unfocused: self.reduce_dashboard_work_when_unfocused,
                        demand: repaint_demand,
                    })
                {
                    crate::performance::record_dashboard_repaint_request();
                    ctx.request_repaint_after(interval);
                }
                if crate::dashboard::dashboard_should_render(dashboard_visible, true)
                    && let Some(action) = self.dashboard.ui(ui, &dash_ctx, WidgetActivation::Click)
                {
                    self.activate_action(
                        action.action,
                        action.query_override,
                        ActivationSource::Dashboard,
                    );
                }
            } else {
                let area_height = ui.available_height();
                ScrollArea::vertical()
                    .max_height(area_height)
                    .show(ui, |ui| {
                        scale_ui(ui, self.list_scale, |ui| {
                            let mut refresh = false;
                            let mut set_focus = false;
                            let show_full = self
                                .enabled_capabilities
                                .as_ref()
                                .and_then(|m| m.get("folders"))
                                .map(|caps| caps.contains(&"show_full_path".to_string()))
                                .unwrap_or(false);
                            if self.resolved_grid_layout {
                                let cols = self.query_results_layout.cols.max(1);
                                let col_width = ((ui.available_width()
                                    - ((cols.saturating_sub(1)) as f32 * 8.0))
                                    / cols as f32)
                                    .max(160.0);
                                egui::Grid::new("query_results_grid")
                                    .num_columns(cols)
                                    .spacing([8.0, 6.0])
                                    .show(ui, |ui| {
                                        for idx in 0..self.results.len() {
                                            let action = self.results[idx].clone();
                                            let text = format!("{}\n{}", action.label, action.desc);
                                            let resp = ui.add_sized(
                                                [col_width, 44.0],
                                                egui::SelectableLabel::new(
                                                    self.selected == Some(idx),
                                                    text,
                                                ),
                                            );
                                            let menu_resp = self.attach_result_context_menu(
                                                &action,
                                                resp,
                                                &mut refresh,
                                                &mut set_focus,
                                                &mut deferred_universal_action,
                                                &mut deferred_radial_authoring_add,
                                            );
                                            trace_root_result_pointer(
                                                ui,
                                                &menu_resp,
                                                root_result_kind(&action.action),
                                                idx,
                                                menu_resp.clicked(),
                                            );
                                            if self.selected == Some(idx) {
                                                menu_resp.scroll_to_me(Some(egui::Align::Center));
                                            }
                                            if menu_resp.clicked() {
                                                self.selected = Some(idx);
                                                deferred_activation = Some(DeferredActivation {
                                                    action,
                                                    query_override: None,
                                                    source: ActivationSource::Click,
                                                });
                                            }
                                            if (idx + 1) % cols == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            } else {
                                for idx in 0..self.results.len() {
                                    let a = self.results[idx].clone();
                                    let aliased =
                                        self.folder_aliases.get(&a.action).and_then(|v| v.as_ref());
                                    let show_path = show_full || aliased.is_none();
                                    let text = if show_path {
                                        format!("{} : {}", a.label, a.desc)
                                    } else {
                                        a.label.clone()
                                    };
                                    let resp = ui.add_sized(
                                        [ui.available_width(), 0.0],
                                        egui::SelectableLabel::new(
                                            self.selected == Some(idx),
                                            text,
                                        ),
                                    );
                                    let tooltip = if a.desc == "Timer"
                                        && a.action.starts_with("timer:show:")
                                    {
                                        if let Ok(id) = a.action[11..].parse::<u64>() {
                                            if let Some(ts) =
                                                crate::plugins::timer::timer_start_ts(id)
                                            {
                                                format!(
                                                    "Started {}",
                                                    crate::plugins::timer::format_ts(ts)
                                                )
                                            } else {
                                                a.action.clone()
                                            }
                                        } else {
                                            a.action.clone()
                                        }
                                    } else {
                                        generic_result_tooltip(&a)
                                    };
                                    let menu_resp = self.attach_result_context_menu(
                                        &a,
                                        resp.on_hover_text(tooltip),
                                        &mut refresh,
                                        &mut set_focus,
                                        &mut deferred_universal_action,
                                        &mut deferred_radial_authoring_add,
                                    );
                                    trace_root_result_pointer(
                                        ui,
                                        &menu_resp,
                                        root_result_kind(&a.action),
                                        idx,
                                        menu_resp.clicked(),
                                    );
                                    if self.selected == Some(idx) {
                                        menu_resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    if menu_resp.clicked() {
                                        self.selected = Some(idx);
                                        deferred_activation = Some(DeferredActivation {
                                            action: a.clone(),
                                            query_override: None,
                                            source: ActivationSource::Click,
                                        });
                                    }
                                }
                            }
                            if refresh {
                                self.last_results_valid = false;
                                self.search();
                            }
                            if set_focus {
                                self.focus_input();
                            } else if self.visible_flag.load(Ordering::SeqCst)
                                && !self.any_panel_open()
                            {
                                self.focus_input();
                            }
                        });
                    });
            }
            if let Some(deferred) = deferred_activation_unless_radial_add(
                deferred_activation.take(),
                deferred_radial_authoring_add.is_some(),
            ) {
                self.activate_action(deferred.action, deferred.query_override, deferred.source);
            }
        });
        self.execute_deferred_result_action(deferred_universal_action);
        if !ocr_blocks_launcher_input
            && !snippet_prompt_owns_input
            && self.snippet_prompt_dialog.is_open()
        {
            Self::consume_prompt_opening_frame_keys(ctx);
        }
        if let Some(add) = deferred_radial_authoring_add {
            if let Ok(mut editor) = self.radial_editor.lock() {
                editor.add_action_to_radial(
                    add.binding,
                    add.label,
                    add.source_query,
                    add.trace_identity,
                );
            }
            ctx.request_repaint();
            ctx.request_repaint_of(crate::gui::radial_editor::radial_designer_viewport_id());
        }
        // Preserve other dialogs' state while OCR owns this transient surface;
        // their editors must not compete for focus or consume OCR keyboard input.
        if ocr_blocks_launcher_input {
            self.enforce_pinned();
            self.update_panel_stack();
            self.show_ocr_surface(ctx);
            return;
        }
        let show_editor = self.show_editor;
        if show_editor {
            let mut editor = std::mem::take(&mut self.editor);
            editor.ui(ctx, self);
            self.editor = editor;
        }
        let show_settings = self.show_settings;
        if show_settings {
            let mut ed = std::mem::take(&mut self.settings_editor);
            ed.ui(ctx, self);
            self.settings_editor = ed;
        }
        let show_plugin = self.show_plugins;
        if show_plugin {
            let mut ed = std::mem::take(&mut self.plugin_editor);
            ed.ui(ctx, self);
            self.plugin_editor = ed;
        }
        crate::gui::radial_editor::RadialEditorState::show_deferred(&self.radial_editor, ctx, self);
        let file_dialog_pending = self
            .radial_editor
            .lock()
            .map(|editor| editor.intent_bridge().has_pending_file_dialog())
            .unwrap_or(false);
        if file_dialog_pending {
            crate::gui::radial_editor::RadialEditorState::process_pending_file_dialog(
                &self.radial_editor,
            );
        }
        self.process_radial_designer_intents();
        let designer_preferences = self
            .radial_editor
            .lock()
            .ok()
            .and_then(|mut editor| editor.take_preferences_for_persist());
        if let Some(preferences) = designer_preferences {
            let settings_path = self.settings_path.clone();
            if let Err(error) = crate::settings::Settings::update(&settings_path, |settings| {
                settings.radial_designer = preferences;
                Ok(())
            }) {
                self.report_error_message("radial.designer.preferences", error.to_string());
            }
        }
        if self.show_dashboard_editor && !self.dashboard_editor.open {
            let registry = self.dashboard.registry().clone();
            self.dashboard_editor.open(&self.dashboard_path, &registry);
        }
        if self.show_dashboard_editor {
            let registry = self.dashboard.registry().clone();
            let mut dlg = std::mem::take(&mut self.dashboard_editor);
            let plugin_infos = self.plugins.plugin_infos();
            let plugin_commands = self.plugins.commands();
            let dashboard_snapshot = self.dashboard_data_cache.snapshot();
            let settings_ctx = WidgetSettingsContext {
                plugins: Some(&self.plugins),
                plugin_infos: Some(&plugin_infos),
                plugin_commands: Some(&plugin_commands),
                actions: Some(self.actions.as_slice()),
                favorites: Some(dashboard_snapshot.favorites.as_ref()),
                usage: Some(&self.usage),
                default_location: self.dashboard_default_location.as_deref(),
                enabled_plugins: self.enabled_plugins.as_ref(),
            };
            let reload = dlg.ui(
                ctx,
                &registry,
                settings_ctx,
                self.require_confirm_destructive,
            );
            self.show_dashboard_editor = dlg.open;
            self.dashboard_editor = dlg;
            if reload {
                self.dashboard.reload();
            }
        }

        let mut mm_dlg = std::mem::take(&mut self.multi_manager_dialog);
        mm_dlg.ui(ctx, self);
        self.multi_manager_dialog = mm_dlg;
        let mut mm_settings_dlg = std::mem::take(&mut self.multi_manager_settings_dialog);
        mm_settings_dlg.ui(ctx, self);
        self.multi_manager_settings_dialog = mm_settings_dlg;
        let mut dlg = std::mem::take(&mut self.alias_dialog);
        dlg.ui(ctx, self);
        self.alias_dialog = dlg;
        let mut bm_dlg = std::mem::take(&mut self.bookmark_alias_dialog);
        bm_dlg.ui(ctx, self);
        self.bookmark_alias_dialog = bm_dlg;
        let mut tf_dlg = std::mem::take(&mut self.tempfile_alias_dialog);
        tf_dlg.ui(ctx, self);
        self.tempfile_alias_dialog = tf_dlg;
        let mut create_tf = std::mem::take(&mut self.tempfile_dialog);
        create_tf.ui(ctx, self);
        self.tempfile_dialog = create_tf;
        let mut add_bm_dlg = std::mem::take(&mut self.add_bookmark_dialog);
        add_bm_dlg.ui(ctx, self);
        self.add_bookmark_dialog = add_bm_dlg;
        let mut help = std::mem::take(&mut self.help_window);
        help.ui(ctx, self);
        self.help_window = help;
        let mut timer_dlg = std::mem::take(&mut self.timer_dialog);
        timer_dlg.ui(ctx, self);
        self.timer_dialog = timer_dlg;
        let mut comp = std::mem::take(&mut self.completion_dialog);
        comp.ui(ctx);
        self.completion_dialog = comp;
        let mut shell_dlg = std::mem::take(&mut self.shell_cmd_dialog);
        shell_dlg.ui(ctx, self);
        self.shell_cmd_dialog = shell_dlg;
        let preview_active =
            self.snippet_prompt_dialog.is_open() && self.snippet_prompt_dialog.is_preview_only();
        if !preview_active {
            let mut snip_dlg = std::mem::take(&mut self.snippet_dialog);
            let preview_draft = snip_dlg.ui(ctx, self);
            self.snippet_dialog = snip_dlg;
            if let Some(draft) = preview_draft {
                match self.begin_snippet_preview(draft) {
                    Ok(()) => {
                        snippet_prompt_owns_input |= self.snippet_prompt_dialog.is_open();
                        if snippet_prompt_owns_input {
                            Self::consume_prompt_opening_frame_keys(ctx);
                        }
                    }
                    Err(error) => self.snippet_dialog.report_preview_start_failure(error),
                }
            }
        }
        let mut macro_dlg = std::mem::take(&mut self.macro_dialog);
        macro_dlg.ui(ctx, self);
        self.macro_dialog = macro_dlg;
        self.mkmacro_dialog.ui(ctx);
        while let Some(notice) = self.mkmacro_dialog.take_ui_notice() {
            if self.enable_toasts {
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: notice.into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        }
        self.crop_dialog.show(ctx);
        let mut mg_dlg = std::mem::take(&mut self.mouse_gestures_dialog);
        mg_dlg.ui(ctx, self);
        self.mouse_gestures_dialog = mg_dlg;
        let mut mg_settings_dlg = std::mem::take(&mut self.mouse_gesture_settings_dialog);
        mg_settings_dlg.ui(ctx, self);
        self.mouse_gesture_settings_dialog = mg_settings_dlg;
        let mut theme_state = std::mem::take(&mut self.theme_settings_dialog);
        let mut theme_open = self.theme_settings_dialog_open;
        crate::gui::theme_settings_dialog::ui(ctx, self, &mut theme_open, &mut theme_state);
        self.theme_settings_dialog_open = theme_open;
        self.theme_settings_dialog = theme_state;
        let mut fav_dlg = std::mem::take(&mut self.fav_dialog);
        fav_dlg.ui(ctx, self);
        self.fav_dialog = fav_dlg;
        let file_search_was_open = self.file_search_dialog.open;
        let file_search_commands = self
            .file_search_dialog
            .ui(ctx, &mut self.file_search_coordinator);
        for command in file_search_commands {
            self.handle_file_search_ui_command(command);
        }
        if file_search_was_open && !self.file_search_dialog.open {
            self.save_file_search_ui_preferences_if_dirty();
        }
        self.file_search_dialog
            .preview_dialog
            .ui(ctx, &self.file_search_dialog.settings);
        self.diff_dialog.ui(ctx);
        let mut notes_dlg = std::mem::take(&mut self.notes_dialog);
        notes_dlg.ui(ctx, self);
        self.notes_dialog = notes_dlg;
        let mut graph_dlg = std::mem::take(&mut self.note_graph_dialog);
        let dashboard_snapshot = self.dashboard_data_cache.snapshot();
        graph_dlg.ui(
            ctx,
            self,
            dashboard_snapshot,
            crate::plugins::note::note_version(),
        );
        self.note_graph_dialog = graph_dlg;
        let mut assets_dlg = std::mem::take(&mut self.unused_assets_dialog);
        assets_dlg.ui(ctx, self);
        self.unused_assets_dialog = assets_dlg;
        let mut note_close_frame = self
            .radial_query_observation
            .has_note_close_request()
            .then(|| super::query_observation::NoteCloseRenderFrame::new(ctx));
        let mut i = 0;
        while i < self.note_panels.len() {
            let mut panel = self.note_panels.remove(i);
            let mut discard = None;
            panel.ui_with_note_close_observation(
                ctx,
                self,
                note_close_frame.as_ref().map(|_| &mut discard),
            );
            if let Some(observed) = &mut note_close_frame {
                observed.observe_panel(&panel, discard);
            }
            if panel.open {
                self.note_panels.insert(i, panel);
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.image_panels.len() {
            let mut panel = self.image_panels.remove(i);
            panel.ui(ctx);
            if panel.open {
                self.image_panels.insert(i, panel);
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.screenshot_editors.len() {
            let mut editor = self.screenshot_editors.remove(i);
            editor.ui(ctx, self);
            if editor.open {
                self.screenshot_editors.insert(i, editor);
                i += 1;
            }
        }
        let mut todo_dlg = std::mem::take(&mut self.todo_dialog);
        todo_dlg.ui(ctx, self);
        self.todo_dialog = todo_dlg;
        let mut todo_view = std::mem::take(&mut self.todo_view_dialog);
        todo_view.ui(ctx, self);
        self.todo_view_dialog = todo_view;
        let mut cb_dlg = std::mem::take(&mut self.clipboard_dialog);
        cb_dlg.ui(ctx, self);
        self.clipboard_dialog = cb_dlg;
        let mut cm_dlg = std::mem::take(&mut self.clipboard_modify_dialog);
        cm_dlg.ui(
            ctx,
            &crate::clipboard_modify::runtime::clipboard_service(),
            self.clipboard_modify_runtime.catalog_snapshot(),
            Some(&self.clipboard_modify_runtime.store),
        );
        self.clipboard_modify_dialog = cm_dlg;
        self.json_utility_dialog.show(ctx);
        self.regex_tester_dialog.show(ctx);
        self.qr_dialog.show(ctx);
        let mut conv_panel = std::mem::take(&mut self.convert_panel);
        conv_panel.ui(ctx, self);
        self.convert_panel = conv_panel;
        let mut vol_dlg = std::mem::take(&mut self.volume_dialog);
        vol_dlg.ui(ctx, self);
        self.volume_dialog = vol_dlg;
        let mut bright_dlg = std::mem::take(&mut self.brightness_dialog);
        bright_dlg.ui(ctx, self);
        self.brightness_dialog = bright_dlg;
        let mut cpu_dlg = std::mem::take(&mut self.cpu_list_dialog);
        cpu_dlg.ui(ctx, self);
        self.cpu_list_dialog = cpu_dlg;
        let (data_actions, data_notices) = self.data_recovery_dialog.ui(ctx);
        for notice in data_notices {
            if notice.error {
                self.report_error_message("data", notice.message);
            } else if self.enable_toasts {
                self.add_toast(Toast {
                    text: notice.message.into(),
                    kind: ToastKind::Success,
                    options: ToastOptions::default()
                        .duration_in_seconds(self.toast_duration as f64),
                });
            }
        }
        for action in data_actions {
            match action {
                DataRecoveryUiAction::OpenPath(path) => {
                    if let Err(error) = open::that(&path) {
                        self.report_error_message(
                            "data",
                            format!("Failed to open {}: {error}", path.display()),
                        );
                    }
                }
                DataRecoveryUiAction::Confirm(intent) => {
                    self.queue_data_recovery_confirmation(intent);
                }
            }
        }
        let mut toast_dlg = std::mem::take(&mut self.toast_log_dialog);
        toast_dlg.ui(ctx, self);
        self.toast_log_dialog = toast_dlg;
        let mut calendar_popover = std::mem::take(&mut self.calendar_popover);
        calendar_popover.ui(ctx, self);
        self.calendar_popover = calendar_popover;
        let mut calendar_editor = std::mem::take(&mut self.calendar_event_editor);
        calendar_editor.ui(ctx, self);
        self.calendar_event_editor = calendar_editor;
        let mut calendar_details = std::mem::take(&mut self.calendar_event_details);
        calendar_details.ui(ctx, self);
        self.calendar_event_details = calendar_details;
        match self.confirm_modal.ui(ctx) {
            ConfirmationResult::Confirmed => {
                if self.pending_data_recovery.is_some() {
                    self.resolve_data_recovery_confirmation(true);
                } else {
                    self.resolve_pending_confirmation(true);
                }
            }
            ConfirmationResult::Cancelled => {
                if self.pending_data_recovery.is_some() {
                    self.resolve_data_recovery_confirmation(false);
                } else {
                    self.resolve_pending_confirmation(false);
                }
            }
            ConfirmationResult::None => {}
        }
        match self.snippet_prompt_dialog.show(ctx) {
            Some(super::snippet_prompt_dialog::SnippetPromptUiAction::Submit) => {
                let _ = self.submit_snippet_prompt();
            }
            Some(super::snippet_prompt_dialog::SnippetPromptUiAction::Cancel) => {
                let _ = self.cancel_snippet_prompt();
            }
            None => {}
        }
        self.enforce_pinned();
        self.update_panel_stack();
        if !self.dashboard_initial_refresh_queued {
            self.dashboard_initial_refresh_queued = true;
            self.dashboard_data_cache
                .request_refresh(DashboardRefreshRequest::All);
        }
        let note_close_snapshot = note_close_frame.map(|frame| frame.snapshot(&self.note_panels));
        self.poll_radial_query_observation(ctx, note_close_snapshot);
        self.show_ocr_surface(ctx);
    }
}

// Keep the normal close writer independently exercisable against an isolated
// settings path without running the unrelated native/global shutdown owners.
fn persist_launcher_close_preferences(
    settings_path: &str,
    window_size: (i32, i32),
    pinned_panels: Vec<Panel>,
    dialog_size: (f32, f32),
) -> anyhow::Result<Settings> {
    Settings::update(settings_path, |settings| {
        settings.window_size = Some(window_size);
        settings.pinned_panels = pinned_panels;
        // Persist only the explicitly approved, non-sensitive Clipboard
        // Modify UI geometry. Runtime source/preview/undo data is held only
        // by the dialog/service and is never serialized into Settings.
        let mut clipboard_modify_preferences: ClipboardModifyPluginSettings = settings
            .plugin_settings
            .get("clipboard_modify")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        let hide_launcher_after_apply = clipboard_modify_preferences.hide_launcher_after_apply;
        clipboard_modify_preferences.dialog_width = dialog_size.0;
        clipboard_modify_preferences.dialog_height = dialog_size.1;
        debug_assert_eq!(
            clipboard_modify_preferences.hide_launcher_after_apply, hide_launcher_after_apply,
            "persisting dialog geometry must not reset the live visibility preference"
        );
        let value = serde_json::to_value(clipboard_modify_preferences)?;
        settings
            .plugin_settings
            .insert("clipboard_modify".into(), value);
        Ok(())
    })
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.render_root_frame(ctx, Some(frame));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown_color_pick();
        self.shutdown_ocr_selection();
        self.root_window_bridge.clear();
        self.close_screen_draw_for_exit();
        self.macro_parameter_prompt.shutdown();
        self.snippet_prompt_dialog.shutdown();
        self.data_recovery_dialog.shutdown();
        self.clipboard_modify_dialog.cleanup_after_close();
        self.clipboard_modify_immediate.cancel_pending();
        self.clipboard_modify_events.clear();
        self.clipboard_modify_watcher = None;
        self.multi_manager.shutdown();
        let multi_manager_save_on_exit = self.multi_manager_save_on_exit();
        if multi_manager_save_on_exit && let Err(err) = self.multi_manager.save() {
            self.report_error("multi_manager.save_on_exit", err);
        }
        if let Err(err) = self.multi_manager.flush_bindings_if_dirty() {
            self.report_error("multi_manager.bindings.save_on_exit", err);
        }
        self.unregister_all_hotkeys();
        self.request_launcher_visibility(false);
        self.last_visible = false;
        self.save_file_search_ui_preferences_if_dirty();
        let window_size = self.window_size;
        let pinned_panels = self.pinned_panels.clone();
        let dialog_width = self.clipboard_modify_dialog.persisted_window_size.x;
        let dialog_height = self.clipboard_modify_dialog.persisted_window_size.y;
        let _ = persist_launcher_close_preferences(
            &self.settings_path,
            window_size,
            pinned_panels,
            (dialog_width, dialog_height),
        );
        let _ = usage::save_usage(USAGE_FILE, &self.usage);
    }
}

impl LauncherApp {
    pub(super) fn recover_screen_draw(
        &mut self,
        intent: crate::screen_draw::ScreenDrawRecoveryIntent,
    ) {
        use crate::screen_draw::{ScreenDrawRecoveryKind, ScreenDrawRestoreCause, ScreenDrawState};
        let cause = match intent.kind {
            ScreenDrawRecoveryKind::LauncherToggle => ScreenDrawRestoreCause::LauncherRecovery,
            ScreenDrawRecoveryKind::Emergency => ScreenDrawRestoreCause::EmergencyRecovery,
        };
        let scope = match self.screen_draw_restore_scope() {
            Ok(scope) => scope,
            Err(error) => {
                self.report_error_message("screen_draw.restore", error);
                return;
            }
        };
        let key = super::screen_draw_restore::RestoreKey {
            scope,
            cause,
            intent: Some(intent),
        };
        if intent.activity_epoch() != self.screen_draw_recovery_bridge.activity_epoch()
            && (self.screen_draw_recovery_bridge.is_active()
                || !self
                    .screen_draw_restore_publication
                    .previously_admitted(intent))
        {
            self.trace_screen_draw_restore_decision(
                key,
                self.visibility_revision.current(),
                crate::screen_draw::ScreenDrawRestoreOutcome::StaleIntent,
            );
            return;
        }
        if let Some(outcome) = self.screen_draw_restore_publication.decision(key) {
            self.trace_screen_draw_restore_decision(
                key,
                self.visibility_revision.current(),
                outcome,
            );
            return;
        }

        let state = self.screen_draw_controller.state().clone();
        let mut transition_error = None;
        match state {
            ScreenDrawState::AwaitingLauncherParking { generation }
            | ScreenDrawState::Capturing { generation } => {
                if let Err(error) = self
                    .screen_draw_controller
                    .cancel_capture_startup(generation)
                {
                    transition_error = Some(error.to_string());
                }
            }
            ScreenDrawState::AwaitingNativeTeardown { .. } => {
                // Dropping PendingNewCapture before discarding the worker
                // receiver quarantines a late SessionClosed completion.
                self.cancel_screen_draw_region_picker();
                self.screen_draw_controller.close();
            }
            ScreenDrawState::Drawing { .. } => match intent.kind {
                ScreenDrawRecoveryKind::LauncherToggle => {
                    if let Err(error) = self.screen_draw_controller.enter_ghost() {
                        transition_error = Some(error.to_string());
                        // A lost native channel cannot prove input was disarmed.
                        self.screen_draw_controller.close();
                    }
                }
                ScreenDrawRecoveryKind::Emergency => {
                    self.screen_draw_controller.reconcile_emergency_pause();
                }
            },
            ScreenDrawState::SelectingRegion { generation } => {
                self.cancel_screen_draw_region_picker();
                if let Err(error) = self
                    .screen_draw_controller
                    .cancel_region_selection(generation, None)
                {
                    transition_error = Some(error.to_string());
                }
                if intent.kind == ScreenDrawRecoveryKind::Emergency {
                    self.screen_draw_controller.reconcile_emergency_pause();
                }
            }
            ScreenDrawState::Ghost { .. }
            | ScreenDrawState::Finish { .. }
            | ScreenDrawState::DisplayChanged { .. } => {
                if intent.kind == ScreenDrawRecoveryKind::Emergency {
                    self.screen_draw_controller.reconcile_emergency_pause();
                }
            }
            ScreenDrawState::NoSession | ScreenDrawState::Failed { .. } => {}
        }

        if let Some(error) = transition_error {
            self.report_error_message("screen_draw.recovery", error);
        }
        if let Err(error) = self.restore_screen_draw_launcher_exact(cause, Some(intent)) {
            tracing::error!(error = %error, "failed to restore Screen Draw launcher during recovery");
            self.report_error_message("screen_draw.restore", error);
        }
    }

    fn cancel_screen_draw_startup_on_escape(&mut self, ctx: &egui::Context) {
        let Some(generation) = self
            .screen_draw_controller
            .state()
            .generation()
            .filter(|_| {
                matches!(
                    self.screen_draw_controller.state(),
                    crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { .. }
                        | crate::screen_draw::ScreenDrawState::Capturing { .. }
                )
            })
        else {
            return;
        };
        let escape = ctx.input_mut(|input| {
            let modifiers = input.modifiers;
            input.consume_key(modifiers, egui::Key::Escape)
        });
        if escape
            && let Ok(poll) = self
                .screen_draw_controller
                .cancel_capture_startup(generation)
        {
            self.apply_screen_draw_capture_poll(ctx, poll);
        }
    }

    fn poll_screen_draw_capture(&mut self, ctx: &egui::Context) {
        let repaint_ctx = ctx.clone();
        let poll = self.screen_draw_controller.poll_capture(
            self.launcher_hwnd,
            Arc::new(move || repaint_ctx.request_repaint()),
        );
        self.apply_screen_draw_capture_poll(ctx, poll);
    }

    fn apply_screen_draw_capture_poll(
        &mut self,
        ctx: &egui::Context,
        poll: crate::screen_draw::ScreenDrawCapturePoll,
    ) {
        if let Some(request) = poll.park_launcher
            && let Err(error) = self.apply_screen_draw_parking_request(request)
        {
            let recovery = self
                .screen_draw_controller
                .fail_capture_startup(request.generation, error.clone());
            match recovery {
                Ok(recovery) => self.apply_screen_draw_capture_poll(ctx, recovery),
                Err(_) => self.report_error_message("screen_draw", error),
            }
            return;
        }
        if poll.restore_launcher {
            if let Err(error) = self.restore_screen_draw_launcher_exact(
                crate::screen_draw::ScreenDrawRestoreCause::CapturePoll,
                None,
            ) {
                tracing::error!(error = %error, "failed to restore Screen Draw launcher geometry");
                self.report_error_message("screen_draw.restore", error);
            }
        }
        if poll.capture_completed
            && let Some(transaction) = self.screen_draw_launcher_parking.as_mut()
        {
            transaction.commit_hidden();
        }
        if poll.session_completed {
            if let Err(error) = self.screen_draw_restore_publication.advance() {
                self.report_error_message("screen_draw.restore", error);
            }
            if let Some(transaction) = self.screen_draw_launcher_parking.as_mut() {
                transaction.commit_hidden();
            }
            self.screen_draw_launcher_parking = None;
            self.request_launcher_state(Some(false), Some(false));
            self.last_visible = false;
        }
        if let Some(delay) = poll.repoll_after {
            ctx.request_repaint_after(delay);
        }
        if let Some(ready) = poll.region_picker_ready {
            self.begin_screen_draw_region_picker(ready);
        }
        if let Some(handoff) = poll.editor_handoff {
            debug_assert!(matches!(
                self.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::NoSession
            ));
            self.open_screenshot_editor(handoff.image, false, crate::gui::MarkupTool::Pen);
        }
        if let Some(diagnostic) = poll.diagnostic {
            tracing::error!(error = %diagnostic, "Screen Draw operation failed");
            self.report_error_message("screen_draw", diagnostic);
        }
    }

    fn apply_screen_draw_parking_request(
        &mut self,
        request: crate::screen_draw::ScreenDrawParkingRequest,
    ) -> Result<(), String> {
        self.ensure_color_pick_does_not_own_root()?;
        if self.screen_draw_controller.state().generation() != Some(request.generation)
            || !matches!(
                self.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { .. }
            )
        {
            return Ok(());
        }

        if let Some(transaction) = self.screen_draw_launcher_parking.as_ref() {
            match transaction.verify() {
                Ok(true) => {
                    self.request_launcher_state(Some(false), Some(false));
                    self.last_visible = false;
                    self.egui_ctx.request_repaint();
                    return Ok(());
                }
                Ok(false) => {}
                Err(error) => {
                    return Err(format!(
                        "failed to verify existing Screen Draw launcher parking: {error}"
                    ));
                }
            }
        }
        if self.screen_draw_launcher_parking.is_some() {
            self.restore_screen_draw_launcher_exact(
                crate::screen_draw::ScreenDrawRestoreCause::ParkingReconciliation,
                None,
            )?;
        }

        let hwnd = self
            .launcher_hwnd
            .ok_or_else(|| "launcher HWND is unavailable for Screen Draw parking".to_string())?;
        self.screen_draw_controller.begin_launcher_repark()?;
        let transaction = crate::launcher_parking::LauncherParkingTransaction::begin(
            request.generation,
            hwnd,
            request.virtual_desktop,
        )?;
        self.screen_draw_restore_publication.advance()?;
        self.screen_draw_launcher_parking = Some(transaction);
        // Synchronize both logical visibility values so the ordinary visibility
        // path cannot apply configured off-screen placement after native parking.
        self.request_launcher_state(Some(false), Some(false));
        self.last_visible = false;
        // Verification occurs on the next event-loop turn, after SetWindowPos.
        self.egui_ctx.request_repaint();
        Ok(())
    }

    fn screen_draw_restore_scope(
        &mut self,
    ) -> Result<super::screen_draw_restore::RestoreScope, String> {
        let generation = self
            .screen_draw_controller
            .state()
            .generation()
            .or_else(|| {
                self.screen_draw_launcher_parking
                    .as_ref()
                    .map(|transaction| transaction.generation())
            });
        let cycle = self
            .screen_draw_launcher_parking
            .as_ref()
            .map(|transaction| transaction.cycle());
        self.screen_draw_restore_publication
            .scope(generation, cycle)
    }

    fn trace_screen_draw_restore_decision(
        &self,
        key: super::screen_draw_restore::RestoreKey,
        starting_revision: u64,
        outcome: crate::screen_draw::ScreenDrawRestoreOutcome,
    ) {
        acceptance_trace::emit(acceptance_trace::Event::ScreenDrawRestoreDecision {
            cause: key.cause,
            intent_id: key.intent.map_or(0, |intent| intent.id()),
            admission_serial: key
                .intent
                .and_then(|intent| intent.admission)
                .map_or(0, |admission| admission.serial),
            activity_epoch: key.intent.map_or(0, |intent| intent.activity_epoch()),
            generation: key.scope.generation,
            lifecycle: key.scope.lifecycle,
            parking_cycle: key.scope.parking_cycle,
            starting_revision,
            current_revision: self.visibility_revision.current(),
            outcome,
        });
    }

    pub(super) fn restore_screen_draw_launcher_exact(
        &mut self,
        cause: crate::screen_draw::ScreenDrawRestoreCause,
        intent: Option<crate::screen_draw::ScreenDrawRecoveryIntent>,
    ) -> Result<(), String> {
        let key = super::screen_draw_restore::RestoreKey {
            scope: self.screen_draw_restore_scope()?,
            cause,
            intent,
        };
        let starting_revision = self.visibility_revision.current();
        if let Some(outcome) = self.screen_draw_restore_publication.decision(key) {
            self.trace_screen_draw_restore_decision(key, starting_revision, outcome);
            return Ok(());
        }
        let result = self.publish_screen_draw_launcher_restore();
        if matches!(
            result,
            Ok(crate::screen_draw::ScreenDrawRestoreOutcome::Published)
        ) {
            self.screen_draw_restore_publication.complete(key);
        }
        self.trace_screen_draw_restore_decision(
            key,
            starting_revision,
            result
                .as_ref()
                .copied()
                .unwrap_or(crate::screen_draw::ScreenDrawRestoreOutcome::Error),
        );
        result.map(|_| ())
    }

    fn publish_screen_draw_launcher_restore(
        &mut self,
    ) -> Result<crate::screen_draw::ScreenDrawRestoreOutcome, String> {
        let restored_exact_geometry = self.screen_draw_launcher_parking.is_some();
        let (
            observed_revision,
            (launcher_was_logically_hidden, observed_focus_intent, observed_invocation_id),
        ) = self.visibility_revision.inspect(|| {
            (
                !self.visible_flag.load(Ordering::SeqCst),
                self.visibility_revision.focus_intent(),
                self.visibility_revision.invocation_id(),
            )
        });
        if let Some(transaction) = self.screen_draw_launcher_parking.as_mut() {
            transaction.restore()?;
        }
        let retain_for_resume = matches!(
            self.screen_draw_controller.state(),
            crate::screen_draw::ScreenDrawState::Ghost { .. }
                | crate::screen_draw::ScreenDrawState::Finish { .. }
        );

        // Exact geometry restoration is a native call and must not hold the
        // visibility gate. Commit the logical show only if no newer request
        // arrived while SetWindowPos was in flight.
        let Some((request_revision, ())) = self
            .visibility_revision
            .request_if_current_with_focus_intent_and_invocation(
                observed_revision,
                observed_focus_intent,
                observed_invocation_id,
                || {
                    self.visible_flag.store(true, Ordering::SeqCst);
                    self.restore_flag.store(false, Ordering::SeqCst);
                },
            )
        else {
            let (
                current_revision,
                (should_be_visible, current_focus_intent, current_invocation_id),
            ) = self.visibility_revision.inspect(|| {
                (
                    self.visible_flag.load(Ordering::SeqCst),
                    self.visibility_revision.focus_intent(),
                    self.visibility_revision.invocation_id(),
                )
            });

            let mut repark_error = None;
            if retain_for_resume && !should_be_visible {
                let controller = &self.screen_draw_controller;
                if let Some(transaction) = self.screen_draw_launcher_parking.as_mut() {
                    if let Err(error) = transaction
                        .repark_after_stale_restore(|| controller.begin_launcher_repark())
                    {
                        repark_error = Some(error);
                    }
                }
            } else if !retain_for_resume {
                self.screen_draw_launcher_parking = None;
            }

            // Keep the next frame armed if another request races this
            // reconciliation. A successful current request will set the
            // baseline to its desired state below.
            self.last_visible = !should_be_visible;
            let root_ctx = RootViewportCtx::with_window_bridge(
                &self.egui_ctx,
                self.root_window_bridge.clone(),
            );
            let mut activation = crate::visibility::RootActivationDisposition::PresentationOnly;
            let mut activation_target = None;
            let reconciled = self.visibility_revision.with_current(
                current_revision,
                || self.visible_flag.load(Ordering::SeqCst) == should_be_visible,
                || {
                    acceptance_trace::with_visibility_trace_link(
                        current_revision,
                        current_invocation_id,
                        || {
                            if !retain_for_resume || should_be_visible || repark_error.is_some() {
                                activation = crate::visibility::apply_visibility_with_focus_intent(
                                    should_be_visible,
                                    current_focus_intent,
                                    if restored_exact_geometry {
                                        crate::visibility::VisiblePlacementPolicy::PreserveCurrentGeometry
                                    } else {
                                        crate::visibility::VisiblePlacementPolicy::ApplyConfiguredPlacement
                                    },
                                    &root_ctx,
                                    self.offscreen_pos,
                                    self.follow_mouse,
                                    self.static_location_enabled,
                                    self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                                    self.static_size.map(|(w, h)| (w as f32, h as f32)),
                                    (self.window_size.0 as f32, self.window_size.1 as f32),
                                );
                                if activation == crate::visibility::RootActivationDisposition::OrderedNative {
                                    activation_target = self.root_window_bridge.activation_target();
                                }
                                if repark_error.is_some()
                                    && !should_be_visible
                                    && let Some(transaction) = self.screen_draw_launcher_parking.as_mut()
                                {
                                    transaction.retain_restore_point_after_fallback_park();
                                }
                            }
                        }
                    )
                },
            );
            if reconciled.is_some() {
                self.last_visible = should_be_visible;
            } else {
                let (_, latest_visibility) = self
                    .visibility_revision
                    .inspect(|| self.visible_flag.load(Ordering::SeqCst));
                self.last_visible = !latest_visibility;
            }
            if reconciled.is_some()
                && activation == crate::visibility::RootActivationDisposition::OrderedNative
            {
                let target = activation_target
                    .ok_or("reconciled ROOT native activation target is unavailable")?;
                if !crate::window_manager::restore_launcher_to_current_desktop_ordered(
                    target,
                    self.visibility_revision.clone(),
                    current_revision,
                    self.visible_flag.clone(),
                    self.root_window_bridge.clone(),
                ) {
                    return Err(
                        "reconciled ROOT native activation could not be admitted or queued".into(),
                    );
                }
            }
            if let Some(error) = repark_error {
                return Err(format!(
                    "failed to repark ROOT after stale Screen Draw restore; queued current visibility fallback: {error}"
                ));
            }
            return Ok(if reconciled.is_some() {
                crate::screen_draw::ScreenDrawRestoreOutcome::Reconciled
            } else {
                crate::screen_draw::ScreenDrawRestoreOutcome::Superseded
            });
        };

        if !retain_for_resume {
            self.screen_draw_launcher_parking = None;
        }
        self.last_visible = true;
        let mut native_restore_target = None;
        let mut native_activation_requested = false;
        let root_ctx =
            RootViewportCtx::with_window_bridge(&self.egui_ctx, self.root_window_bridge.clone());
        let published = self.visibility_revision.with_current(
            request_revision,
            || self.visible_flag.load(Ordering::SeqCst),
            || {
                acceptance_trace::with_visibility_trace_link(
                    request_revision,
                    observed_invocation_id,
                    || {
                        acceptance_trace::emit(acceptance_trace::Event::DesiredVisibility {
                            visible: true,
                            revision: request_revision,
                            source: VisibilitySource::ScreenDrawRestore,
                            invocation_id: observed_invocation_id,
                        });
                        acceptance_trace::emit(
                            acceptance_trace::Event::ScreenDrawRestoreFocusIntent {
                                revision: request_revision,
                                invocation_id: observed_invocation_id,
                                focus_intent: observed_focus_intent,
                            },
                        );
                        if !restored_exact_geometry && launcher_was_logically_hidden {
                            // A completed Screen Draw session discards its exact
                            // snapshot while the native window is parked. Move ROOT
                            // onscreen before applying optional configured placement.
                            root_ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                                egui::pos2(0.0, 0.0),
                            ));
                        }
                        native_activation_requested = crate::visibility::apply_visibility_with_focus_intent(
                            true,
                            observed_focus_intent,
                            if restored_exact_geometry {
                                crate::visibility::VisiblePlacementPolicy::PreserveCurrentGeometry
                            } else {
                                crate::visibility::VisiblePlacementPolicy::ApplyConfiguredPlacement
                            },
                            &root_ctx,
                            self.offscreen_pos,
                            self.follow_mouse,
                            self.static_location_enabled,
                            self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                            self.static_size.map(|(w, h)| (w as f32, h as f32)),
                            (self.window_size.0 as f32, self.window_size.1 as f32),
                        ) == crate::visibility::RootActivationDisposition::OrderedNative;
                        if native_activation_requested && let Some(hwnd) = self.launcher_hwnd {
                            native_restore_target = Some(crate::visibility::RootActivationTarget {
                                hwnd,
                                generation: self.root_window_bridge.identity().1,
                            });
                        }
                    },
                )
            },
        );
        if published.is_none() {
            return Ok(crate::screen_draw::ScreenDrawRestoreOutcome::Superseded);
        }
        if native_activation_requested {
            let target = native_restore_target
                .ok_or("ROOT native activation target is not yet available")?;
            if !crate::window_manager::restore_launcher_to_current_desktop_ordered(
                target,
                self.visibility_revision.clone(),
                request_revision,
                self.visible_flag.clone(),
                self.root_window_bridge.clone(),
            ) {
                return Err("ROOT native restoration could not be admitted or queued".into());
            }
        }
        Ok(crate::screen_draw::ScreenDrawRestoreOutcome::Published)
    }

    fn multi_manager_save_on_exit(&self) -> bool {
        self.multi_manager_settings.save_on_exit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gui::numpad_navigation::{
            LauncherNumpadNavigation, NumpadKeyStateProbe, PhysicalDigitKeyState,
            PhysicalNumpadKey, consume_physical_numpad_navigation,
        },
        mkmacro::{LauncherCommandBroker, LauncherCommandResponse, RunControl},
        plugin::{Plugin, PluginManager},
        settings::Settings,
    };
    use eframe::egui;
    use std::{
        cell::Cell,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    static MACRO_ACTIVATION_TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn generic_result_tooltips_hide_snippet_bodies_and_keep_clipboard_actions() {
        let snippet = Action {
            label: "sig".into(),
            desc: "Snippet".into(),
            action: "clipboard:private λ\nbody".into(),
            args: None,
        };
        assert_eq!(generic_result_tooltip(&snippet), "sig");

        let clipboard_history = Action {
            label: "Clipboard entry".into(),
            desc: "Clipboard".into(),
            action: "clipboard:copy:3".into(),
            args: None,
        };
        assert_eq!(
            generic_result_tooltip(&clipboard_history),
            "clipboard:copy:3"
        );
    }

    #[test]
    fn controlled_normal_close_preferences_preserve_prepared_settings_and_unrelated_values() {
        fn protected_value(bytes: &[u8]) -> serde_json::Value {
            let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            if let Some(serde_json::Value::Array(plugins)) = value.get_mut("enabled_plugins") {
                plugins.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
            }
            value
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let radial_path = directory.path().join("radial.json");
        let mut settings = Settings::default();
        settings.window_size = Some((900, 650));
        settings.radial.enabled = true;
        settings.pinned_panels.clear();
        settings.enabled_plugins = Some(
            ["a-plugin", "z-plugin", "clipboard_modify"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        );
        settings.enabled_capabilities = Some(std::collections::HashMap::from([(
            "a-plugin".into(),
            vec!["second".into(), "first".into()],
        )]));
        let mut clipboard = ClipboardModifyPluginSettings::default();
        clipboard.hide_launcher_after_apply = false;
        clipboard.template_filter = "protected authored filter".into();
        settings.plugin_settings.insert(
            "clipboard_modify".into(),
            serde_json::to_value(&clipboard).unwrap(),
        );
        settings.plugin_settings.insert(
            "z-plugin".into(),
            serde_json::json!({
                "nested":{"z":2,"a":1}, "ordered":["second","first"]
            }),
        );
        settings.radial.default_submenu_presentation =
            crate::radial::model::SubmenuPresentation::SameCenter;
        settings.save(path.to_str().unwrap()).unwrap();
        let mut document = crate::radial::model::RadialDocument::starter();
        for menu in &mut document.menus {
            menu.submenu_presentation = crate::radial::model::SubmenuPresentation::SameCenter;
        }
        std::fs::write(&radial_path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        let startup = crate::startup::load_startup_settings(&path);
        assert!(startup.diagnostic.is_none());
        let store = crate::radial::store::RadialStore::at_path(&radial_path, document).unwrap();
        store.reload().unwrap();
        let migrated =
            crate::radial::submenu_migration::startup_migrate(&store, &path, true).unwrap();
        assert!(migrated.settings.radial_submenu_migration.is_some());
        let baseline = std::fs::read(&path).unwrap();
        let radial = std::fs::read(&radial_path).unwrap();
        for (name, bytes) in [
            ("actions.json", b"protected authored actions".as_slice()),
            (
                "query-marker-ledger.txt",
                b"protected marker ledger".as_slice(),
            ),
            ("note.md", b"protected authored note".as_slice()),
        ] {
            std::fs::write(directory.path().join(name), bytes).unwrap();
        }
        let unchanged = ["actions.json", "query-marker-ledger.txt", "note.md"]
            .map(|name| std::fs::read(directory.path().join(name)).unwrap());
        for _ in 0..2 {
            let actual = persist_launcher_close_preferences(
                path.to_str().unwrap(),
                (900, 650),
                Vec::new(),
                (900.0, 640.0),
            )
            .unwrap();
            let after = std::fs::read(&path).unwrap();
            // The real reload/save may reorder maps; compare every raw JSON key
            // and value while normalizing only the actual plugin set.
            assert_eq!(protected_value(&after), protected_value(&baseline));
            assert_eq!(actual.window_size, Some((900, 650)));
            assert!(actual.pinned_panels.is_empty());
            let actual_clipboard: ClipboardModifyPluginSettings =
                serde_json::from_value(actual.plugin_settings["clipboard_modify"].clone()).unwrap();
            assert_eq!(actual_clipboard, clipboard);
            assert!(!actual_clipboard.hide_launcher_after_apply);
            assert_eq!(std::fs::read(&radial_path).unwrap(), radial);
            assert_eq!(
                ["actions.json", "query-marker-ledger.txt", "note.md"]
                    .map(|name| std::fs::read(directory.path().join(name)).unwrap()),
                unchanged
            );
            let restarted = crate::startup::load_startup_settings(&path);
            assert!(restarted.diagnostic.is_none());
            assert_eq!(
                std::fs::read(&path).unwrap(),
                after,
                "normal settings restart adds no post-close defaults"
            );
        }
    }

    #[test]
    fn choosing_add_to_radial_suppresses_the_parent_result_activation() {
        let activation = DeferredActivation {
            action: Action {
                label: "Close".into(),
                desc: "Selected window".into(),
                action: "window:close".into(),
                args: None,
            },
            query_override: None,
            source: ActivationSource::Click,
        };
        let suppressed = deferred_activation_unless_radial_add(Some(activation.clone()), true);
        assert!(suppressed.is_none());

        let ordinary_click = deferred_activation_unless_radial_add(Some(activation.clone()), false);
        assert_eq!(
            ordinary_click.as_ref().map(|activation| &activation.action),
            Some(&activation.action)
        );
    }

    #[test]
    fn radial_add_primary_uses_launcher_resolution_and_keeps_secondary_choices_clear() {
        let context = egui::Context::default();
        let app = new_app(&context);
        let query = "macro add primary fixture";
        let selected = Action {
            label: "Automation Fixture".into(),
            desc: "Mouse/keyboard macro".into(),
            action: "mkmacro:run:74".into(),
            args: None,
        };

        let primary = resolve_primary_radial_add_row(&app, &selected, query)
            .expect("launcher primary is a stable macro action");
        let ranked_rows =
            app.authoring_catalog_for_ranked_actions(std::slice::from_ref(&selected), query);
        assert_eq!(
            ranked_rows.rows().first().map(|row| &row.action_id),
            Some(&crate::universal_actions::action_ids::RESULT_EXECUTE),
            "the registry's first row is the generic Execute action"
        );
        assert_eq!(
            primary.action_id,
            crate::universal_actions::action_ids::MKMACRO_RUN,
            "Add primary must use the resolved launcher action"
        );
        assert_ne!(
            primary.action_id,
            ranked_rows.rows()[0].action_id,
            "the fixture distinguishes launcher primary from registry row zero"
        );
        assert!(primary.assignment().is_ok());

        let (target, actions) = app.resolve_universal_actions(
            &selected,
            crate::universal_actions::PinCapability::Writable { is_pinned: false },
            crate::universal_actions::ActionSurface::ContextMenu,
        );
        let secondary_ids = actions
            .iter()
            .filter(|action| action.id != primary.action_id)
            .filter_map(|action| {
                crate::gui::universal_action_catalog::UniversalActionAuthoringCatalog::row_for_semantic_action(
                    &target,
                    &action.id,
                    query,
                )
            })
            .map(|row| row.action_id)
            .collect::<Vec<_>>();
        assert!(
            secondary_ids.contains(&crate::universal_actions::action_ids::MKMACRO_EDIT),
            "the macro editor remains an explicit secondary Add choice"
        );
        assert!(!secondary_ids.contains(&primary.action_id));

        let choice = radial_add_choice_for_row(&primary, query)
            .expect("the selected primary row becomes an Add choice");
        assert!(matches!(
            choice,
            ContextMenuChoice::AddToRadial { binding, source_query, .. }
                if binding == primary.assignment().unwrap() && source_query == query
        ));

        let mut disabled_primary = primary;
        let disabled_reason = "the live macro is no longer available";
        disabled_primary.availability = crate::universal_actions::ActionAvailability::Disabled {
            reason: disabled_reason.into(),
        };
        assert_eq!(
            radial_add_unavailable_reason(&disabled_primary).as_deref(),
            Some(disabled_reason)
        );
        assert!(
            radial_add_choice_for_row(&disabled_primary, query).is_none(),
            "a disabled primary cannot be converted into another Add choice"
        );
    }

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn new_prompt_app(ctx: &egui::Context) -> (LauncherApp, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let actions_path = directory.path().join("actions.json");
        let settings_path = directory.path().join("settings.json");
        let app = LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            actions_path.display().to_string(),
            settings_path.display().to_string(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        (app, directory)
    }

    struct StartupCaptureBackend {
        capture_calls: AtomicUsize,
        completed: std::sync::mpsc::Sender<()>,
    }

    impl crate::screen_draw::capture::DesktopCaptureBackend for StartupCaptureBackend {
        fn virtual_desktop(&self) -> Result<crate::mkmacro::screen::ScreenRect, String> {
            Ok(crate::mkmacro::screen::ScreenRect::new(
                -1920, -1080, 5760, 3240,
            ))
        }

        fn capture_desktop(
            &self,
            cancelled: &dyn Fn() -> bool,
        ) -> Result<crate::mkmacro::screen::CapturedRegion, String> {
            assert!(!cancelled());
            self.capture_calls.fetch_add(1, Ordering::SeqCst);
            let _ = self.completed.send(());
            Ok(crate::mkmacro::screen::CapturedRegion {
                image: image::RgbaImage::from_pixel(4, 3, image::Rgba([1, 2, 3, 255])),
                origin: (-1920, -1080),
            })
        }
    }

    struct ParkedLauncherProbe;

    impl crate::screen_draw::capture::LauncherVisibilityProbe for ParkedLauncherProbe {
        fn launcher_is_capture_parked(
            &self,
            launcher_hwnd: Option<usize>,
            virtual_desktop: crate::mkmacro::screen::ScreenRect,
        ) -> Result<bool, String> {
            assert_eq!(launcher_hwnd, Some(42));
            assert_eq!(
                virtual_desktop,
                crate::mkmacro::screen::ScreenRect::new(-1920, -1080, 5760, 3240)
            );
            Ok(true)
        }
    }

    struct StartupNativeFactory {
        spawns: AtomicUsize,
        command_receivers:
            Mutex<Vec<std::sync::mpsc::Receiver<crate::screen_draw::NativeSessionCommand>>>,
    }

    impl crate::screen_draw::native_runtime::NativeSessionFactory for StartupNativeFactory {
        fn spawn(
            &self,
            config: crate::screen_draw::native_runtime::NativeSessionConfig,
        ) -> Result<crate::screen_draw::NativeSessionHandle, String> {
            assert_eq!(config.snapshot.capture().origin, (-1920, -1080));
            self.spawns.fetch_add(1, Ordering::SeqCst);
            let (handle, commands, events) =
                crate::screen_draw::NativeSessionHandle::test_stub_with_events();
            events
                .send(crate::screen_draw::NativeSessionEvent::SessionStarted(
                    crate::screen_draw::NativeRuntimeState {
                        mode: crate::screen_draw::ScreenDrawMode::Drawing,
                        tool: config.tool,
                        color: config.color,
                        thickness: config.thickness,
                        annotations_visible: true,
                        background: config.settings.default_background,
                    },
                ))
                .expect(
                    "startup fixture publishes its configured runtime through the worker queue",
                );
            self.command_receivers.lock().unwrap().push(commands);
            Ok(handle)
        }
    }

    #[test]
    fn screen_draw_start_keeps_root_alive_across_parking_capture_native_and_toolbar() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let (capture_completed, capture_completed_rx) = std::sync::mpsc::channel();
        let capture = Arc::new(StartupCaptureBackend {
            capture_calls: AtomicUsize::new(0),
            completed: capture_completed,
        });
        let native = Arc::new(StartupNativeFactory {
            spawns: AtomicUsize::new(0),
            command_receivers: Mutex::new(Vec::new()),
        });
        app.screen_draw_controller =
            crate::screen_draw::ScreenDrawController::with_test_dependencies(
                capture.clone(),
                Arc::new(ParkedLauncherProbe),
                native.clone(),
            );
        app.screen_draw_controller
            .set_recovery_bridge(Arc::clone(&app.screen_draw_recovery_bridge));
        app.launcher_hwnd = Some(42);
        install_owned_root_for_test(&mut app);
        app.static_location_enabled = true;
        app.follow_mouse = true;

        assert!(app.start_or_focus_screen_draw().unwrap());
        let generation = app.screen_draw_controller.state().generation().unwrap();
        let original = crate::launcher_parking::LauncherWindowRect {
            left: 1371,
            top: 611,
            right: 1834,
            bottom: 898,
        };
        let desktop = crate::mkmacro::screen::ScreenRect::new(-1920, -1080, 5760, 3240);
        let (transaction, observer) =
            crate::launcher_parking::launcher_parking_test_fixture(generation, original, desktop);
        app.screen_draw_launcher_parking = Some(transaction);

        // Reproduce the old invocation-bearing owner before normal parking.
        // The normal production parking request must clear it; observation
        // must neither copy that old trace owner nor advance restore history.
        while app.visibility_revision.current() < 114 {
            app.visibility_revision
                .request_with_focus_intent_and_invocation(
                    crate::visibility::RootFocusIntent::ActivateRoot,
                    Some(124),
                    || app.visible_flag.store(true, Ordering::SeqCst),
                );
        }
        assert_eq!(app.visibility_revision.current(), 114);
        assert_eq!(app.visibility_revision.invocation_id(), Some(124));

        ctx.begin_frame(egui::RawInput::default());
        app.poll_screen_draw_capture(&ctx);
        assert!(matches!(
            app.screen_draw_controller.state(),
            crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { .. }
        ));
        assert!(!app.visible_flag.load(Ordering::SeqCst));

        app.poll_screen_draw_capture(&ctx);
        assert!(matches!(
            app.screen_draw_controller.state(),
            crate::screen_draw::ScreenDrawState::Capturing { .. }
        ));
        assert!(!app.screen_draw_controller.toolbar_open());
        capture_completed_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("capture worker completes without blocking the GUI thread");

        for _ in 0..100_000 {
            app.poll_screen_draw_capture(&ctx);
            if matches!(
                app.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::Drawing { .. }
            ) {
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Drawing { generation }
        );
        // Capture completion precedes the native worker's queued startup event.
        assert!(app.screen_draw_controller.runtime_state().is_none());
        app.poll_screen_draw_capture(&ctx);
        assert_eq!(
            app.screen_draw_controller
                .runtime_state()
                .expect("the normal poll consumes the worker's startup event")
                .mode,
            crate::screen_draw::ScreenDrawMode::Drawing
        );
        assert!(app.screen_draw_controller.toolbar_open());
        assert_eq!(capture.capture_calls.load(Ordering::SeqCst), 1);
        assert_eq!(native.spawns.load(Ordering::SeqCst), 1);
        assert_eq!(app.visibility_revision.current(), 115);
        assert_eq!(app.visibility_revision.invocation_id(), None);
        let publication_before = format!("{:?}", app.screen_draw_restore_publication);
        let current =
            super::super::screen_draw_toolbar::retained_current_launcher_observation_for_test(
                &mut app,
                crate::screen_draw::window_layers::ToolbarObservationIdentity {
                    hwnd: 51,
                    process_id: std::process::id(),
                    client_size: [264, 700],
                },
            )
            .expect("real retained Drawing Responses publish their actual current owner");
        assert_eq!(current.launcher.visibility_revision, 115);
        assert_eq!(current.launcher.invocation_id, None);
        assert!(!current.launcher.visible);
        assert_eq!(current.launcher.root.unwrap().hwnd, 42);
        assert_eq!(
            current.launcher.root.unwrap().process_id,
            std::process::id()
        );
        assert_eq!(
            current.launcher.root.unwrap().generation,
            app.root_window_bridge.identity().1
        );
        let parking = current.launcher.parking.unwrap();
        assert_eq!(parking.hwnd, 42);
        assert_eq!(parking.generation, generation.get());
        assert_eq!(parking.cycle, 1);
        assert_eq!(
            parking.state,
            crate::radial::acceptance_trace::ScreenDrawParkingState::Committed
        );
        assert!(current.has_visible_controls_for_mode(
            crate::radial::acceptance_trace::ScreenDrawToolbarMode::Drawing
        ));
        assert_eq!(
            format!("{:?}", app.screen_draw_restore_publication),
            publication_before
        );
        assert_eq!(app.visibility_revision.current(), 115);
        app.show_screen_draw_toolbar(&ctx);
        let output = ctx.end_frame();

        assert!(app.screen_draw_toolbar.was_open);
        assert!(observer.restored_rects().is_empty());
        let root_commands = &output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport remains in the frame")
            .commands;
        assert!(!root_commands.iter().any(|command| matches!(
            command,
            egui::ViewportCommand::Visible(false) | egui::ViewportCommand::Minimized(true)
        )));
    }

    #[test]
    fn screen_draw_capture_failure_restores_launcher_and_reports_diagnostic() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            app.visible_flag.store(false, Ordering::SeqCst);
            app.restore_flag.store(false, Ordering::SeqCst);
            app.show_inline_errors = true;
            let original = crate::launcher_parking::LauncherWindowRect {
                left: 1200,
                top: 700,
                right: 1663,
                bottom: 987,
            };
            let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
                crate::screen_draw::ScreenDrawGeneration::from_raw(1),
                original,
                crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
            );
            app.screen_draw_launcher_parking = Some(transaction);

            app.apply_screen_draw_capture_poll(
                &ctx,
                crate::screen_draw::ScreenDrawCapturePoll {
                    restore_launcher: true,
                    diagnostic: Some("fixture capture failure".into()),
                    ..Default::default()
                },
            );

            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(app.last_visible);
            assert_eq!(observer.restored_rects(), [original]);
            assert!(!app.restore_flag.load(Ordering::SeqCst));
            assert_eq!(app.error.as_deref(), Some("fixture capture failure"));
            assert!(!app.screen_draw_controller.toolbar_open());
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn screen_draw_editor_handoff_restores_launcher_after_session_teardown() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            app.visible_flag.store(false, Ordering::SeqCst);
            app.restore_flag.store(false, Ordering::SeqCst);
            let image = image::RgbaImage::from_pixel(2, 1, image::Rgba([4, 5, 6, 255]));
            let original = crate::launcher_parking::LauncherWindowRect {
                left: -800,
                top: 250,
                right: -337,
                bottom: 537,
            };
            let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
                crate::screen_draw::ScreenDrawGeneration::from_raw(1),
                original,
                crate::mkmacro::screen::ScreenRect::new(-1920, 0, 3840, 1080),
            );
            app.screen_draw_launcher_parking = Some(transaction);

            app.apply_screen_draw_capture_poll(
                &ctx,
                crate::screen_draw::ScreenDrawCapturePoll {
                    restore_launcher: true,
                    editor_handoff: Some(crate::screen_draw::ScreenDrawEditorHandoff {
                        generation: crate::screen_draw::ScreenDrawGeneration::from_raw(1),
                        image,
                    }),
                    ..Default::default()
                },
            );

            assert!(matches!(
                app.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::NoSession
            ));
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(app.last_visible);
            assert_eq!(observer.restored_rects(), [original]);
            assert!(!app.restore_flag.load(Ordering::SeqCst));
            assert_eq!(app.screenshot_editors.len(), 1);
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn successful_screen_draw_completion_discards_snapshot_without_restoring_launcher() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let generation = app.screen_draw_controller.request_start().unwrap();
        let original = crate::launcher_parking::LauncherWindowRect {
            left: 20,
            top: 30,
            right: 420,
            bottom: 250,
        };
        let (mut transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
            generation,
            original,
            crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
        );
        transaction.commit_hidden();
        app.screen_draw_launcher_parking = Some(transaction);
        app.screen_draw_controller.close();

        app.apply_screen_draw_capture_poll(
            &ctx,
            crate::screen_draw::ScreenDrawCapturePoll {
                session_completed: true,
                ..Default::default()
            },
        );

        assert!(observer.restored_rects().is_empty());
        assert!(app.screen_draw_launcher_parking.is_none());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.last_visible);
        assert!(!app.restore_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn idle_toolbar_close_after_completion_uses_configured_show_from_parked_geometry() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            let generation = app.screen_draw_controller.request_start().unwrap();
            app.static_location_enabled = true;
            app.static_pos = Some((640, 360));
            app.static_size = Some((520, 300));
            let original = crate::launcher_parking::LauncherWindowRect {
                left: 20,
                top: 30,
                right: 420,
                bottom: 250,
            };
            let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
                generation,
                original,
                crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
            );
            let parked = observer.current_rect();
            app.screen_draw_launcher_parking = Some(transaction);
            app.screen_draw_controller.close();
            app.apply_screen_draw_capture_poll(
                &ctx,
                crate::screen_draw::ScreenDrawCapturePoll {
                    session_completed: true,
                    ..Default::default()
                },
            );
            assert_eq!(observer.current_rect(), parked);

            ctx.begin_frame(egui::RawInput::default());
            app.close_screen_draw_session().unwrap();
            let output = ctx.end_frame();

            let commands = &output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .unwrap()
                .commands;
            assert!(commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::OuterPosition(position)
                    if *position == egui::pos2(640.0, 360.0)
            )));
            assert!(commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::InnerSize(size)
                    if *size == egui::vec2(520.0, 300.0)
            )));
            #[cfg(not(windows))]
            assert!(commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Visible(true) | egui::ViewportCommand::Minimized(false)
            )));
            #[cfg(windows)]
            assert!(!commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Focus
                    | egui::ViewportCommand::Visible(true)
                    | egui::ViewportCommand::Minimized(false)
            )));
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(app.last_visible);
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn new_capture_after_completion_snapshots_repositioned_onscreen_geometry() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            let generation = app.screen_draw_controller.request_start().unwrap();
            app.static_location_enabled = true;
            app.static_pos = Some((300, 240));
            app.static_size = Some((480, 280));
            let original = crate::launcher_parking::LauncherWindowRect {
                left: 20,
                top: 30,
                right: 420,
                bottom: 250,
            };
            let desktop = crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080);
            let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
                generation, original, desktop,
            );
            app.screen_draw_launcher_parking = Some(transaction);
            app.screen_draw_controller.close();
            app.apply_screen_draw_capture_poll(
                &ctx,
                crate::screen_draw::ScreenDrawCapturePoll {
                    session_completed: true,
                    ..Default::default()
                },
            );

            ctx.begin_frame(egui::RawInput::default());
            app.request_new_screen_draw_capture().unwrap();
            let output = ctx.end_frame();
            let commands = &output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .unwrap()
                .commands;
            assert!(commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::OuterPosition(position)
                    if *position == egui::pos2(300.0, 240.0)
            )));

            // Model the event-loop settle turn applying the configured placement
            // before the following capture poll snapshots and parks the HWND.
            let onscreen = crate::launcher_parking::LauncherWindowRect {
                left: 300,
                top: 240,
                right: 780,
                bottom: 520,
            };
            observer.set_current_rect(onscreen);
            let next_generation = app.screen_draw_controller.state().generation().unwrap();
            let transaction = observer.begin_transaction(next_generation, desktop);
            assert_eq!(transaction.original_snapshot().rect(), onscreen);
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn idle_toolbar_close_after_completion_has_onscreen_fallback_without_placement_settings() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            app.follow_mouse = false;
            app.static_location_enabled = false;
            let generation = app.screen_draw_controller.request_start().unwrap();
            let original = crate::launcher_parking::LauncherWindowRect {
                left: 20,
                top: 30,
                right: 420,
                bottom: 250,
            };
            let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
                generation,
                original,
                crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
            );
            let parked = observer.current_rect();
            app.screen_draw_launcher_parking = Some(transaction);
            app.screen_draw_controller.close();
            app.apply_screen_draw_capture_poll(
                &ctx,
                crate::screen_draw::ScreenDrawCapturePoll {
                    session_completed: true,
                    ..Default::default()
                },
            );
            assert_eq!(observer.current_rect(), parked);

            ctx.begin_frame(egui::RawInput::default());
            app.close_screen_draw_session().unwrap();
            let output = ctx.end_frame();

            let commands = &output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .unwrap()
                .commands;
            assert!(commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::OuterPosition(position)
                    if *position == egui::pos2(0.0, 0.0)
            )));
            #[cfg(not(windows))]
            assert!(commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Visible(true) | egui::ViewportCommand::Minimized(false)
            )));
            #[cfg(windows)]
            assert!(!commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Focus
                    | egui::ViewportCommand::Visible(true)
                    | egui::ViewportCommand::Minimized(false)
            )));
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(app.last_visible);
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn new_capture_after_completion_has_onscreen_snapshot_without_placement_settings() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            app.follow_mouse = false;
            app.static_location_enabled = false;
            let generation = app.screen_draw_controller.request_start().unwrap();
            let original = crate::launcher_parking::LauncherWindowRect {
                left: 20,
                top: 30,
                right: 420,
                bottom: 250,
            };
            let desktop = crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080);
            let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
                generation, original, desktop,
            );
            app.screen_draw_launcher_parking = Some(transaction);
            app.screen_draw_controller.close();
            app.apply_screen_draw_capture_poll(
                &ctx,
                crate::screen_draw::ScreenDrawCapturePoll {
                    session_completed: true,
                    ..Default::default()
                },
            );

            ctx.begin_frame(egui::RawInput::default());
            app.request_new_screen_draw_capture().unwrap();
            let output = ctx.end_frame();
            let commands = &output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .unwrap()
                .commands;
            assert!(commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::OuterPosition(position)
                    if *position == egui::pos2(0.0, 0.0)
            )));

            let onscreen = crate::launcher_parking::LauncherWindowRect {
                left: 0,
                top: 0,
                right: 400,
                bottom: 220,
            };
            observer.set_current_rect(onscreen);
            let next_generation = app.screen_draw_controller.state().generation().unwrap();
            let transaction = observer.begin_transaction(next_generation, desktop);
            assert_eq!(transaction.original_snapshot().rect(), onscreen);
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn screen_draw_exact_restore_ignores_configured_position_and_size() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            ctx.begin_frame(egui::RawInput::default());
            let _ = ctx.end_frame();
            app.static_location_enabled = true;
            app.static_pos = Some((900, 700));
            app.static_size = Some((800, 600));
            app.visible_flag.store(false, Ordering::SeqCst);
            app.last_visible = false;
            app.restore_flag.store(true, Ordering::SeqCst);
            let original = crate::launcher_parking::LauncherWindowRect {
                left: 21,
                top: 34,
                right: 421,
                bottom: 234,
            };
            let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
                crate::screen_draw::ScreenDrawGeneration::from_raw(11),
                original,
                crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
            );
            app.screen_draw_launcher_parking = Some(transaction);

            ctx.begin_frame(egui::RawInput::default());
            app.restore_screen_draw_launcher_exact(
                crate::screen_draw::ScreenDrawRestoreCause::SessionClose,
                None,
            )
            .unwrap();
            let output = ctx.end_frame();

            assert_eq!(observer.restored_rects(), [original]);
            assert!(app.screen_draw_launcher_parking.is_none());
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(app.last_visible);
            assert!(!app.restore_flag.load(Ordering::SeqCst));
            let commands = &output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .unwrap()
                .commands;
            assert!(!commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::OuterPosition(_) | egui::ViewportCommand::InnerSize(_)
            )));
            assert!(!commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Visible(false) | egui::ViewportCommand::Minimized(true)
            )));
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn normal_ui_handoff_preserves_moved_root_and_next_explicit_show_applies_placement() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        install_owned_root_for_test(&mut app);
        app.follow_mouse = false;
        app.static_location_enabled = true;
        app.static_pos = Some((240, 180));
        app.static_size = Some((900, 650));
        app.visible_flag.store(true, Ordering::SeqCst);
        app.last_visible = true;
        app.restore_flag.store(false, Ordering::SeqCst);
        let moved = egui::Rect::from_min_size(egui::pos2(420.0, 300.0), egui::vec2(700.0, 500.0));
        let input_at_moved_root = || {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, moved.size())),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .outer_rect = Some(moved);
            input
        };
        ctx.begin_frame(input_at_moved_root());
        let _ = ctx.end_frame();
        assert!(!app.mkmacro_dialog.open);

        app.activate_action(
            Action {
                label: "Open real macro editor".into(),
                desc: "UI handoff placement proof".into(),
                action: "mkmacro:dialog".into(),
                args: None,
            },
            None,
            ActivationSource::Click,
        );
        assert!(app.mkmacro_dialog.open);
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
        let restore_revision = app.visibility_revision.current();
        let (restored, queued) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.run(input_at_moved_root(), |root| {
                    app.render_root_frame(root, None)
                })
            });
        let commands = &restored.viewport_output[&egui::ViewportId::ROOT].commands;
        #[cfg(windows)]
        {
            assert_eq!(queued, [(restore_revision, 42)]);
            assert_root_activation_requests(&restored, &queued);
        }
        #[cfg(not(windows))]
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Visible(true)))
        );
        assert!(!commands.iter().any(|command| matches!(
            command,
            egui::ViewportCommand::OuterPosition(_)
                | egui::ViewportCommand::InnerSize(_)
                | egui::ViewportCommand::Visible(false)
                | egui::ViewportCommand::Minimized(true)
        )));
        assert!(app.last_visible);
        assert!(!app.restore_flag.load(Ordering::SeqCst));
        assert_eq!(app.visibility_revision.current(), restore_revision);

        app.request_launcher_state(Some(false), Some(false));
        let hidden = ctx.run(input_at_moved_root(), |root| {
            app.render_root_frame(root, None)
        });
        assert!(!app.last_visible);
        assert!(hidden.viewport_output[&egui::ViewportId::ROOT].commands.iter().any(|command| matches!(
            command, egui::ViewportCommand::OuterPosition(position) if *position == egui::pos2(app.offscreen_pos.0, app.offscreen_pos.1)
        )));
        app.request_launcher_state(Some(true), Some(false));
        let show_revision = app.visibility_revision.current();
        let (shown, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.run(input_at_moved_root(), |root| {
                app.render_root_frame(root, None)
            })
        });
        #[cfg(windows)]
        {
            assert_eq!(queued, [(show_revision, 42)]);
            assert_root_activation_requests(&shown, &queued);
        }
        let commands = &shown.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.iter().any(|command| matches!(
            command, egui::ViewportCommand::OuterPosition(position) if *position == egui::pos2(240.0, 180.0)
        )));
        assert!(commands.iter().any(|command| matches!(
            command, egui::ViewportCommand::InnerSize(size) if *size == egui::vec2(900.0, 650.0)
        )));
        assert!(app.last_visible);
        assert!(app.mkmacro_dialog.open);
        assert!(app.visibility_revision.current() > restore_revision);
    }

    #[test]
    fn screen_draw_restore_error_does_not_publish_logical_visibility() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.last_visible = false;
        app.restore_flag.store(true, Ordering::SeqCst);
        let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
            crate::screen_draw::ScreenDrawGeneration::from_raw(12),
            crate::launcher_parking::LauncherWindowRect {
                left: 21,
                top: 34,
                right: 421,
                bottom: 234,
            },
            crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
        );
        app.screen_draw_launcher_parking = Some(transaction);
        observer.fail_next_restore();
        let revision = app.visibility_revision.current();

        ctx.begin_frame(egui::RawInput::default());
        let result = app.restore_screen_draw_launcher_exact(
            crate::screen_draw::ScreenDrawRestoreCause::SessionClose,
            None,
        );
        let _ = ctx.end_frame();

        assert!(result.is_err());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
        assert_eq!(app.visibility_revision.current(), revision);
        assert!(app.screen_draw_launcher_parking.is_some());
        assert!(observer.restored_rects().is_empty());
    }

    #[test]
    fn newer_show_after_screen_draw_hide_request_keeps_restore_state() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.visible_flag.store(true, Ordering::SeqCst);
        app.last_visible = true;
        app.restore_flag.store(true, Ordering::SeqCst);

        app.request_launcher_state(Some(false), Some(false));
        app.last_visible = false;
        app.request_launcher_state(Some(true), Some(true));

        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn newer_hide_during_screen_draw_restore_reconciles_exact_geometry() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.visible_flag.store(true, Ordering::SeqCst);
        app.last_visible = true;
        app.restore_flag.store(true, Ordering::SeqCst);
        let original = crate::launcher_parking::LauncherWindowRect {
            left: 21,
            top: 34,
            right: 421,
            bottom: 234,
        };
        let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
            crate::screen_draw::ScreenDrawGeneration::from_raw(13),
            original,
            crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
        );
        app.screen_draw_launcher_parking = Some(transaction);
        let revision = app.visibility_revision.clone();
        let visible = Arc::clone(&app.visible_flag);
        let restore = Arc::clone(&app.restore_flag);
        observer.before_next_restore(move || {
            revision.request(|| {
                visible.store(false, Ordering::SeqCst);
                restore.store(false, Ordering::SeqCst);
            });
        });

        ctx.begin_frame(egui::RawInput::default());
        app.restore_screen_draw_launcher_exact(
            crate::screen_draw::ScreenDrawRestoreCause::SessionClose,
            None,
        )
        .unwrap();
        let output = ctx.end_frame();

        assert_eq!(observer.restored_rects(), [original]);
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.last_visible);
        assert!(app.screen_draw_launcher_parking.is_none());
        let commands = &output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .unwrap()
            .commands;
        assert!(!commands.iter().any(|command| matches!(
            command,
            egui::ViewportCommand::Visible(true) | egui::ViewportCommand::Focus
        )));
    }

    #[test]
    fn newer_preserve_foreground_show_survives_blocked_screen_draw_restore() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, _commands) = drawing_app_with_resume_fixture(&ctx);
            app.screen_draw_controller.enter_ghost().unwrap();
            let (observer, _) = install_recovery_parking(&mut app, generation);
            let revision = app.visibility_revision.clone();
            let visible = Arc::clone(&app.visible_flag);
            observer.before_next_restore(move || {
                revision.request_with_focus_intent_and_invocation(
                    RootFocusIntent::PreserveForeground,
                    Some(88),
                    || visible.store(true, Ordering::SeqCst),
                );
            });

            ctx.begin_frame(egui::RawInput::default());
            app.restore_screen_draw_launcher_exact(
                crate::screen_draw::ScreenDrawRestoreCause::SessionClose,
                None,
            )
            .unwrap();
            let output = ctx.end_frame();

            assert!(app.visible_flag.load(Ordering::SeqCst));
            let (revision, (focus_intent, invocation_id)) = app.visibility_revision.inspect(|| {
                (
                    app.visibility_revision.focus_intent(),
                    app.visibility_revision.invocation_id(),
                )
            });
            assert!(revision > 0);
            assert_eq!(focus_intent, RootFocusIntent::PreserveForeground);
            assert_eq!(invocation_id, Some(88));
            let commands = &output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .unwrap()
                .commands;
            assert!(!commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Focus | egui::ViewportCommand::Minimized(false)
            )));
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 0);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn newer_show_during_stale_screen_draw_repark_arms_next_frame_reconciliation() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, _commands) = drawing_app_with_resume_fixture(&ctx);
            app.screen_draw_controller.enter_ghost().unwrap();
            let (observer, _) = install_recovery_parking(&mut app, generation);
            app.visible_flag.store(true, Ordering::SeqCst);
            app.last_visible = true;

            let hide_revision = app.visibility_revision.clone();
            let hide_visible = Arc::clone(&app.visible_flag);
            observer.before_next_restore(move || {
                hide_revision.request(|| hide_visible.store(false, Ordering::SeqCst));
            });
            let show_revision = app.visibility_revision.clone();
            let show_visible = Arc::clone(&app.visible_flag);
            observer.before_next_park(move || {
                show_revision.request(|| show_visible.store(true, Ordering::SeqCst));
            });

            ctx.begin_frame(egui::RawInput::default());
            app.restore_screen_draw_launcher_exact(
                crate::screen_draw::ScreenDrawRestoreCause::SessionClose,
                None,
            )
            .unwrap();
            let _ = ctx.end_frame();

            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(
                !app.last_visible,
                "the newer show must differ from last_visible so the next frame reapplies it"
            );
            assert_eq!(
                app.screen_draw_launcher_parking.as_ref().unwrap().state(),
                crate::launcher_parking::LauncherParkingState::Active
            );
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 0);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn failed_stale_screen_draw_repark_queues_guarded_hidden_fallback() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, _commands) = drawing_app_with_resume_fixture(&ctx);
            app.screen_draw_controller.enter_ghost().unwrap();
            let (observer, _) = install_recovery_parking(&mut app, generation);
            app.visible_flag.store(true, Ordering::SeqCst);
            app.last_visible = true;
            observer.fail_next_park();
            let hide_revision = app.visibility_revision.clone();
            let hide_visible = Arc::clone(&app.visible_flag);
            observer.before_next_restore(move || {
                hide_revision.request(|| hide_visible.store(false, Ordering::SeqCst));
            });

            ctx.begin_frame(egui::RawInput::default());
            let result = app.restore_screen_draw_launcher_exact(
                crate::screen_draw::ScreenDrawRestoreCause::SessionClose,
                None,
            );
            let output = ctx.end_frame();

            assert!(result.is_err());
            assert!(!app.visible_flag.load(Ordering::SeqCst));
            assert!(!app.last_visible);
            let commands = &output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .unwrap()
                .commands;
            assert!(
                commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::OuterPosition(_)))
            );
            assert!(!commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Visible(true) | egui::ViewportCommand::Focus
            )));
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 0);
            assert_root_activation_evidence(&scheduled);
        }
    }

    fn install_recovery_parking(
        app: &mut LauncherApp,
        generation: crate::screen_draw::ScreenDrawGeneration,
    ) -> (
        crate::launcher_parking::LauncherParkingTestObserver,
        crate::launcher_parking::LauncherWindowRect,
    ) {
        let original = crate::launcher_parking::LauncherWindowRect {
            left: 31,
            top: 47,
            right: 431,
            bottom: 267,
        };
        let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
            generation,
            original,
            crate::mkmacro::screen::ScreenRect::new(-1920, 0, 3840, 1080),
        );
        app.screen_draw_launcher_parking = Some(transaction);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.last_visible = false;
        install_owned_root_for_test(app);
        (observer, original)
    }

    fn hook_recovery_notice(
        bridge: &crate::screen_draw::ScreenDrawRecoveryBridge,
    ) -> crate::hotkey::launcher_invocation::ServiceNotice {
        use crate::hotkey::launcher_invocation::{
            InvocationConfig, KeyEvent, KeyTransition, LauncherInvocationAdapter, PriorityOwner,
        };
        let mut adapter = LauncherInvocationAdapter::new(InvocationConfig {
            launcher_enabled: true,
            hotkey: crate::hotkey::parse_hotkey("Shift+Alt+Win+End").unwrap(),
            threshold_ms: 350,
            generation: 4,
            context_token: 9,
            menu_id: crate::radial::model::MenuId::new("starter"),
            interaction: crate::radial::model::InteractionMode::StickyClick,
            accept_external_injected: true,
            item_inputs: Vec::new(),
        })
        .unwrap();
        let edge = |vk, transition, at| KeyEvent {
            vk,
            transition,
            at,
            provenance: crate::radial::invocation::InputProvenance::Physical,
        };
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(edge(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let owner = PriorityOwner::ScreenDrawRecovery(bridge.active_lifetime().unwrap());
        let down = adapter.process(edge(0x23, KeyTransition::Down, 1), owner);
        assert!(down.consume && down.recovery.is_some() && down.intents.is_empty());
        let repeat = adapter.process(edge(0x23, KeyTransition::Repeat, 2), owner);
        assert!(repeat.consume && repeat.recovery.is_none());
        let release = adapter.process(edge(0x23, KeyTransition::Up, 3), owner);
        assert!(release.consume && release.recovery.is_none());
        down.into()
    }

    fn root_commands(output: &egui::FullOutput) -> &[egui::ViewportCommand] {
        output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(&[], |viewport| viewport.commands.as_slice())
    }

    fn install_owned_root_for_test(app: &mut LauncherApp) {
        #[cfg(windows)]
        assert!(
            !unsafe {
                windows::Win32::UI::WindowsAndMessaging::IsWindow(windows::Win32::Foundation::HWND(
                    42 as *mut _,
                ))
            }
            .as_bool(),
            "the isolated ROOT fixture must not name a live OS window"
        );
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(42);
        app.root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
    }

    fn assert_root_activation_requests(output: &egui::FullOutput, queued: &[(u64, usize)]) {
        #[cfg(windows)]
        {
            assert!(
                !root_commands(output).iter().any(|command| matches!(
                    command,
                    egui::ViewportCommand::Focus
                        | egui::ViewportCommand::Visible(true)
                        | egui::ViewportCommand::Minimized(false)
                )),
                "Windows ROOT must not enter the framework activation path"
            );
            assert_root_activation_evidence(queued);
        }
        #[cfg(not(windows))]
        for command in [
            egui::ViewportCommand::Visible(true),
            egui::ViewportCommand::Minimized(false),
            egui::ViewportCommand::Focus,
        ] {
            assert_eq!(
                root_commands(output)
                    .iter()
                    .filter(|actual| **actual == command)
                    .count(),
                queued.len()
            );
        }
    }

    #[cfg(windows)]
    fn assert_root_activation_evidence(queued: &[(u64, usize)]) {
        let events = acceptance_trace::take_root_activation_test_events();
        let focus = events
            .iter()
            .filter_map(|event| match event {
                acceptance_trace::Event::RootCommand {
                    command: crate::radial::acceptance_trace::RootCommandKind::Focus,
                    correlation,
                } => Some(*correlation),
                _ => None,
            })
            .collect::<Vec<_>>();
        let native = events
            .iter()
            .filter_map(|event| match event {
                acceptance_trace::Event::NativeActivation {
                    edge: crate::radial::acceptance_trace::NativeActivationEdge::RestoreRequested,
                    hwnd,
                    correlation,
                } => Some((*hwnd as usize, *correlation)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            native
                .iter()
                .map(|(hwnd, correlation)| (correlation.visibility_revision, *hwnd))
                .collect::<Vec<_>>(),
            queued
        );
        assert_eq!(focus.len(), queued.len());
        for (focus, (_, native)) in focus.iter().zip(&native) {
            assert!(focus.request_id > 0 && focus.request_id < native.request_id);
            assert_eq!(focus.visibility_revision, native.visibility_revision);
            assert_eq!(focus.invocation_id, native.invocation_id);
            assert!(!focus.terminal && !native.terminal);
        }
        assert!(
            !events.iter().any(|event| matches!(
                event,
                acceptance_trace::Event::NativeActivation {
                    edge: crate::radial::acceptance_trace::NativeActivationEdge::RestoreCompleted,
                    ..
                }
            )),
            "queue admission is not OS completion"
        );
    }

    #[cfg(windows)]
    #[test]
    fn ordinary_root_show_without_restore_and_cofiring_reconcile_submit_once() {
        for restore in [false, true] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            app.last_visible = false;
            app.static_location_enabled = true;
            app.static_pos = Some((240, 180));
            app.static_size = Some((900, 650));
            let request_revision = app.request_launcher_state(Some(true), Some(restore));
            if restore {
                app.root_window_bridge.request_presentation_reconcile();
            }
            let (output, queued) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.run(egui::RawInput::default(), |root| {
                        app.render_root_frame(root, None)
                    })
                });
            assert_eq!(queued, [(request_revision, 42)]);
            assert_root_activation_requests(&output, &queued);
            assert!(app.visible_flag.load(Ordering::SeqCst) && app.last_visible);
            assert!(!app.restore_flag.load(Ordering::SeqCst));
            assert!(
                root_commands(&output).contains(&egui::ViewportCommand::OuterPosition(egui::pos2(
                    240.0, 180.0
                )))
            );
            assert!(
                root_commands(&output)
                    .contains(&egui::ViewportCommand::InnerSize(egui::vec2(900.0, 650.0)))
            );
            let (next, queued) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.run(egui::RawInput::default(), |root| {
                        app.render_root_frame(root, None)
                    })
                });
            assert!(queued.is_empty());
            assert_root_activation_requests(&next, &queued);
        }
    }

    #[cfg(windows)]
    #[test]
    fn actual_startup_defers_activation_until_native_identity_and_newer_hide_invalidates_it() {
        for hide_before_attachment in [false, true] {
            let ctx = egui::Context::default();
            let mut created = None;
            let (initial, queued) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.run(egui::RawInput::default(), |root| {
                        let mut app = LauncherApp::new(
                            root,
                            Arc::new(Vec::new()),
                            0,
                            PluginManager::new(),
                            "actions.json".into(),
                            "settings.json".into(),
                            Settings::default(),
                            None,
                            None,
                            None,
                            None,
                            Arc::new(AtomicBool::new(true)),
                            Arc::new(AtomicBool::new(false)),
                            Arc::new(AtomicBool::new(false)),
                        );
                        app.render_root_frame(root, None);
                        created = Some(app);
                    })
                });
            let mut app = created.unwrap();
            assert!(queued.is_empty());
            assert_root_activation_requests(&initial, &queued);
            assert_eq!(app.root_window_bridge.identity().0, 0);
            assert!(app.restore_flag.load(Ordering::SeqCst));
            if hide_before_attachment {
                app.request_launcher_visibility(false);
            }
            install_owned_root_for_test(&mut app);
            let revision = app.visibility_revision.current();
            let (qualified, queued) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.run(egui::RawInput::default(), |root| {
                        app.render_root_frame(root, None)
                    })
                });
            assert_eq!(
                queued,
                if hide_before_attachment {
                    Vec::new()
                } else {
                    vec![(revision, 42)]
                }
            );
            assert_root_activation_requests(&qualified, &queued);
            assert_eq!(app.last_visible, !hide_before_attachment);
            assert_eq!(
                app.visible_flag.load(Ordering::SeqCst),
                !hide_before_attachment
            );
            assert!(!app.restore_flag.load(Ordering::SeqCst));
        }
    }

    #[cfg(windows)]
    #[test]
    fn legacy_queued_context_and_toggle_routes_share_one_native_submission_per_revision() {
        use crate::visibility::{
            VisibilityToggleBatch, handle_visibility_toggle_batch_ordered,
            handle_visibility_trigger_with_owner_ordered,
        };
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        install_owned_root_for_test(&mut app);
        let contexts = Arc::new(Mutex::new(None::<RootViewportCtx>));
        let trigger =
            crate::hotkey::HotkeyTrigger::new(crate::hotkey::parse_hotkey("End").unwrap());
        let mut queued_visibility = None;
        *trigger.open.lock().unwrap() = true;
        assert!(handle_visibility_trigger_with_owner_ordered(
            &trigger,
            &app.visible_flag,
            &app.restore_flag,
            &contexts,
            &mut queued_visibility,
            app.offscreen_pos,
            false,
            false,
            None,
            None,
            (900.0, 650.0),
            &app.visibility_revision,
            |_| {}
        ));
        assert_eq!(queued_visibility, Some(true));
        let initial_revision = app.visibility_revision.current();
        let (output, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let output = ctx.run(egui::RawInput::default(), |root| {
                app.render_root_frame(root, None)
            });
            *contexts.lock().unwrap() = Some(RootViewportCtx::with_window_bridge(
                &ctx,
                app.root_window_bridge.clone(),
            ));
            handle_visibility_trigger_with_owner_ordered(
                &trigger,
                &app.visible_flag,
                &app.restore_flag,
                &contexts,
                &mut queued_visibility,
                app.offscreen_pos,
                false,
                false,
                None,
                None,
                (900.0, 650.0),
                &app.visibility_revision,
                |_| {},
            );
            ctx.run(egui::RawInput::default(), |root| {
                app.render_root_frame(root, None)
            });
            output
        });
        assert_eq!(queued, [(initial_revision, 42)]);
        assert!(queued_visibility.is_none());
        assert_root_activation_requests(&output, &queued);
        let (shown, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let mut batch = VisibilityToggleBatch::default();
            batch.record_toggle_ordered_with_invocation(
                &app.visibility_revision,
                &app.visible_flag,
                Some(71),
            );
            handle_visibility_toggle_batch_ordered(
                &batch,
                &app.visibility_revision,
                &app.restore_flag,
                &contexts,
                &mut queued_visibility,
                app.offscreen_pos,
                false,
                false,
                None,
                None,
                (900.0, 650.0),
            );
            ctx.run(egui::RawInput::default(), |root| {
                app.render_root_frame(root, None)
            });
            assert!(!app.last_visible);
            let mut batch = VisibilityToggleBatch::default();
            batch.record_toggle_ordered_with_invocation(
                &app.visibility_revision,
                &app.visible_flag,
                Some(72),
            );
            handle_visibility_toggle_batch_ordered(
                &batch,
                &app.visibility_revision,
                &app.restore_flag,
                &contexts,
                &mut queued_visibility,
                app.offscreen_pos,
                false,
                false,
                None,
                None,
                (900.0, 650.0),
            );
            ctx.run(egui::RawInput::default(), |root| {
                app.render_root_frame(root, None)
            })
        });
        assert_eq!(queued, [(initial_revision + 2, 42)]);
        assert_root_activation_requests(&shown, &queued);
        assert_eq!(app.visibility_revision.invocation_id(), Some(72));
        assert!(app.visible_flag.load(Ordering::SeqCst) && app.last_visible);
    }

    #[cfg(windows)]
    #[test]
    fn preserve_foreground_root_frame_has_safe_show_but_no_native_or_framework_focus() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        install_owned_root_for_test(&mut app);
        app.query = "protected dirty query".into();
        app.visibility_revision.request_with_focus_intent(
            RootFocusIntent::PreserveForeground,
            || {
                app.visible_flag.store(true, Ordering::SeqCst);
                app.restore_flag.store(true, Ordering::SeqCst);
            },
        );
        let (output, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.run(egui::RawInput::default(), |root| {
                app.render_root_frame(root, None)
            })
        });
        assert!(queued.is_empty());
        let events = acceptance_trace::take_root_activation_test_events();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    acceptance_trace::Event::RootCommand {
                        command: crate::radial::acceptance_trace::RootCommandKind::Show,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(!events.iter().any(|event| matches!(
            event,
            acceptance_trace::Event::NativeActivation { .. }
                | acceptance_trace::Event::RootCommand {
                    command: crate::radial::acceptance_trace::RootCommandKind::Focus,
                    ..
                }
        )));
        assert!(!root_commands(&output).iter().any(|command| matches!(
            command,
            egui::ViewportCommand::Focus
                | egui::ViewportCommand::Visible(true)
                | egui::ViewportCommand::Minimized(false)
        )));
        assert_eq!(app.query, "protected dirty query");
        assert_eq!(
            app.visibility_revision.focus_intent(),
            RootFocusIntent::PreserveForeground
        );
        assert!(app.visible_flag.load(Ordering::SeqCst));
    }

    #[cfg(windows)]
    #[test]
    fn actual_hook_held_recovery_cycle_has_one_shared_restore_and_a_fresh_cycle_remains_usable() {
        use crate::hotkey::launcher_invocation::{
            InvocationConfig, KeyEvent, KeyTransition, LauncherInvocationAdapter, PriorityOwner,
            ServiceNotice,
        };
        use crate::radial::invocation::InputProvenance;
        let ctx = egui::Context::default();
        let (mut app, generation, commands) = drawing_app_with_resume_fixture(&ctx);
        let (observer, original) = install_recovery_parking(&mut app, generation);
        let capture = Arc::clone(
            app.screen_draw_controller
                .session_snapshot()
                .unwrap()
                .capture(),
        );
        let lifetime = app.screen_draw_recovery_bridge.active_lifetime().unwrap();
        let mut adapter = LauncherInvocationAdapter::new(InvocationConfig {
            launcher_enabled: true,
            hotkey: crate::hotkey::parse_hotkey("Shift+Alt+Win+End").unwrap(),
            threshold_ms: 350,
            generation: 4,
            context_token: 9,
            menu_id: crate::radial::model::MenuId::new("starter"),
            interaction: crate::radial::model::InteractionMode::StickyClick,
            accept_external_injected: true,
            item_inputs: Vec::new(),
        })
        .unwrap();
        let edge = |vk, transition, at, provenance| KeyEvent {
            vk,
            transition,
            at,
            provenance,
        };
        for vk in [0xA0, 0xA4, 0x5B] {
            let outcome = adapter.process(
                edge(vk, KeyTransition::Down, 0, InputProvenance::Physical),
                PriorityOwner::Launcher,
            );
            assert!(outcome.recovery.is_none() && outcome.intents.is_empty() && !outcome.consume);
        }
        let owner = PriorityOwner::ScreenDrawRecovery(lifetime);
        let down = adapter.process(
            edge(0x23, KeyTransition::Down, 1, InputProvenance::Physical),
            owner,
        );
        assert!(down.consume && down.intents.is_empty());
        let first_admission = down.recovery.unwrap();
        assert_eq!(first_admission.serial, 1);
        assert_eq!(first_admission.lifetime, lifetime);
        for (vk, transition, provenance, consumed) in [
            (0xA0, KeyTransition::Down, InputProvenance::Physical, false),
            (
                0xA4,
                KeyTransition::Repeat,
                InputProvenance::Physical,
                false,
            ),
            (0xA4, KeyTransition::Up, InputProvenance::Physical, false),
            (
                0xA4,
                KeyTransition::Down,
                InputProvenance::ExternalInjected,
                false,
            ),
            (
                0x23,
                KeyTransition::Up,
                InputProvenance::ExternalInjected,
                false,
            ),
            (0x23, KeyTransition::Repeat, InputProvenance::Physical, true),
        ] {
            let outcome = adapter.process(edge(vk, transition, 2, provenance), owner);
            assert_eq!(outcome.consume, consumed);
            assert!(outcome.recovery.is_none() && outcome.intents.is_empty());
        }
        let mut notice = ServiceNotice::from(down);
        let first = notice
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        assert!(
            notice
                .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
                .is_none()
        );
        let revision = app.visibility_revision.current();
        let (output, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.run(egui::RawInput::default(), |root| {
                for _ in 0..3 {
                    app.event_tx
                        .send(WatchEvent::ScreenDrawRecover(first))
                        .unwrap();
                }
                app.process_watch_events();
                assert!(matches!(
                    commands.try_iter().collect::<Vec<_>>().as_slice(),
                    [crate::screen_draw::NativeSessionCommand::Ghost]
                ));
                app.render_root_frame(root, None);
            })
        });
        assert_eq!(queued, [(revision + 1, 42)]);
        assert_root_activation_requests(&output, &queued);
        assert_eq!(app.visibility_revision.current(), revision + 1);
        assert_eq!(observer.restored_rects(), [original]);
        assert_eq!(observer.current_rect(), original);
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Ghost { generation }
        );
        assert!(matches!(
            commands.try_iter().collect::<Vec<_>>().as_slice(),
            [crate::screen_draw::NativeSessionCommand::SetToolbarWindow(
                None
            )]
        ));
        let released = adapter.process(
            edge(0x23, KeyTransition::Up, 3, InputProvenance::Physical),
            owner,
        );
        assert!(released.consume && released.recovery.is_none() && released.intents.is_empty());
        for transition in [KeyTransition::Up, KeyTransition::Down] {
            for vk in [0xA0, 0xA4, 0x5B] {
                let outcome =
                    adapter.process(edge(vk, transition, 4, InputProvenance::Physical), owner);
                assert!(
                    !outcome.consume && outcome.recovery.is_none() && outcome.intents.is_empty()
                );
            }
        }
        let later_owner = PriorityOwner::ScreenDrawRecovery(
            app.screen_draw_recovery_bridge.active_lifetime().unwrap(),
        );
        let later = adapter.process(
            edge(0x23, KeyTransition::Down, 5, InputProvenance::Physical),
            later_owner,
        );
        assert!(later.consume && later.intents.is_empty());
        assert_eq!(later.recovery.unwrap().serial, 2);
        let mut notice = ServiceNotice::from(later);
        let second = notice
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        assert_ne!(first.id(), second.id());
        let (output, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.run(egui::RawInput::default(), |root| {
                for intent in [second, first, second] {
                    app.event_tx
                        .send(WatchEvent::ScreenDrawRecover(intent))
                        .unwrap();
                }
                app.process_watch_events();
                assert!(commands.try_recv().is_err());
                app.render_root_frame(root, None);
            })
        });
        assert_eq!(queued, [(revision + 2, 42)]);
        assert_root_activation_requests(&output, &queued);
        assert_eq!(app.visibility_revision.current(), revision + 2);
        assert_eq!(observer.current_rect(), original);
        assert_eq!(observer.restored_rects(), [original]);
        assert!(commands.try_recv().is_err());
        assert!(Arc::ptr_eq(
            &capture,
            app.screen_draw_controller
                .session_snapshot()
                .unwrap()
                .capture()
        ));
        let release = adapter.process(
            edge(0x23, KeyTransition::Up, 6, InputProvenance::Physical),
            later_owner,
        );
        assert!(release.consume && release.recovery.is_none() && release.intents.is_empty());
    }

    #[test]
    fn simulated_root_wake_preserves_shared_restore_publication_and_strict_queue_cardinality() {
        let ctx = egui::Context::default();
        let (mut app, generation, commands) = drawing_app_with_resume_fixture(&ctx);
        let (observer, original) = install_recovery_parking(&mut app, generation);
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(42);
        let qualified = app
            .root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        let bridge_clone = app.root_window_bridge.clone();
        let intent = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let revision = app.visibility_revision.current();
        acceptance_trace::take_screen_draw_restore_test_events();

        let (output, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.begin_frame(egui::RawInput::default());
            for _ in 0..2 {
                app.event_tx
                    .send(WatchEvent::ScreenDrawRecover(intent))
                    .unwrap();
            }
            app.process_watch_events();
            ctx.end_frame()
        });
        assert_eq!(app.root_window_bridge.identity(), qualified);
        assert_eq!(bridge_clone.identity(), qualified);
        assert_eq!(
            bridge_clone.take_simulated_wake_observations_for_test(),
            [qualified]
        );
        assert!(
            app.root_window_bridge
                .take_simulated_wake_observations_for_test()
                .is_empty()
        );
        assert_eq!(app.visibility_revision.current(), revision + 1);
        assert_eq!(queued, [(revision + 1, 42)]);
        assert_root_activation_requests(&output, &queued);
        assert!(app.visible_flag.load(Ordering::SeqCst) && app.last_visible);
        assert!(!app.restore_flag.load(Ordering::SeqCst));
        assert_eq!(observer.current_rect(), original);
        assert_eq!(observer.restored_rects(), [original]);
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Ghost { generation }
        );
        assert!(matches!(
            commands.try_iter().collect::<Vec<_>>().as_slice(),
            [crate::screen_draw::NativeSessionCommand::Ghost]
        ));
        let outcomes = acceptance_trace::take_screen_draw_restore_test_events()
            .into_iter()
            .filter_map(|event| match event {
                acceptance_trace::Event::ScreenDrawRestoreDecision { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes,
            [
                crate::screen_draw::ScreenDrawRestoreOutcome::Published,
                crate::screen_draw::ScreenDrawRestoreOutcome::Duplicate
            ]
        );
        // These are simulated wake and real queue-admission receipts. The
        // fixture does not observe native OS restoration completion.
    }

    #[test]
    fn queued_hook_admission_and_undelivered_intent_from_closed_a_have_zero_effects_on_b() {
        let ctx = egui::Context::default();
        let (mut app, a, _) = drawing_app_with_resume_fixture(&ctx);
        install_recovery_parking(&mut app, a);
        let mut queued = hook_recovery_notice(&app.screen_draw_recovery_bridge);
        let mut minted_notice = queued.clone();
        let undelivered = minted_notice
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        ctx.begin_frame(egui::RawInput::default());
        let close_revision = app.visibility_revision.current();
        let (_, closed) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            app.close_screen_draw_session().unwrap()
        });
        assert_eq!(closed, [(close_revision + 1, 42)]);
        let _ = ctx.end_frame();
        let b = app.screen_draw_controller.request_start().unwrap();
        assert_ne!(a, b);
        app.screen_draw_controller.launcher_parked(b).unwrap();
        app.screen_draw_controller.capture_succeeded(b).unwrap();
        app.screen_draw_controller.install_test_session_snapshot(
            b,
            crate::mkmacro::screen::CapturedRegion {
                image: image::RgbaImage::new(8, 6),
                origin: (-4, -3),
            },
        );
        let (commands, _) = app.screen_draw_controller.install_test_native_worker();
        let capture = Arc::clone(
            app.screen_draw_controller
                .session_snapshot()
                .unwrap()
                .capture(),
        );
        let (observer, original) = install_recovery_parking(&mut app, b);
        let parked = observer.current_rect();
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(42);
        app.root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        let revision = app.visibility_revision.current();

        // This is the production main notice consumer, followed by the real
        // queued GUI reducer for an intent already minted before A closed.
        assert!(
            queued
                .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
                .is_none()
        );
        assert!(queued.recovery.is_none());
        let (output, scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.begin_frame(egui::RawInput::default());
                app.event_tx
                    .send(WatchEvent::ScreenDrawRecover(undelivered))
                    .unwrap();
                app.process_watch_events();
                ctx.end_frame()
            });
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Drawing { generation: b }
        );
        assert_eq!(app.visibility_revision.current(), revision);
        assert_eq!(observer.current_rect(), parked);
        assert!(observer.restored_rects().is_empty());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(root_commands(&output).is_empty() && scheduled.is_empty());
        assert!(commands.try_recv().is_err());
        assert!(Arc::ptr_eq(
            &capture,
            app.screen_draw_controller
                .session_snapshot()
                .unwrap()
                .capture()
        ));

        let fresh = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let (output, scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.begin_frame(egui::RawInput::default());
                for _ in 0..2 {
                    app.event_tx
                        .send(WatchEvent::ScreenDrawRecover(fresh))
                        .unwrap();
                }
                app.process_watch_events();
                ctx.end_frame()
            });
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Ghost { generation: b }
        );
        assert_eq!(observer.restored_rects(), [original]);
        assert_eq!(scheduled, [(revision + 1, 42)]);
        assert_eq!(app.visibility_revision.current(), revision + 1);
        assert_root_activation_requests(&output, &scheduled);
        assert!(matches!(
            commands.try_iter().collect::<Vec<_>>().as_slice(),
            [crate::screen_draw::NativeSessionCommand::Ghost]
        ));
        assert!(Arc::ptr_eq(
            &capture,
            app.screen_draw_controller
                .session_snapshot()
                .unwrap()
                .capture()
        ));
    }

    #[test]
    fn first_undelivered_recovery_is_rejected_after_active_typed_new_capture() {
        use crate::commands::{ScreenDrawCommand, ScreenDrawCommandHost};
        let ctx = egui::Context::default();
        let (mut app, a, commands) = drawing_app_with_resume_fixture(&ctx);
        let (observer, original) = install_recovery_parking(&mut app, a);
        let mut queued = hook_recovery_notice(&app.screen_draw_recovery_bridge);
        let old = queued
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(42);
        app.root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        ctx.begin_frame(egui::RawInput::default());
        let initial_revision = app.visibility_revision.current();
        let (_, initial_scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ScreenDrawCommandHost::execute_screen_draw_command(
                    &mut app,
                    ScreenDrawCommand::NewCapture,
                )
                .unwrap();
            });
        assert_eq!(initial_scheduled, [(initial_revision + 1, 42)]);
        let _ = ctx.end_frame();
        let b = app.screen_draw_controller.state().generation().unwrap();
        assert_ne!(a, b);
        assert!(app.screen_draw_recovery_bridge.is_active());
        assert!(app.screen_draw_recovery_bridge.activity_epoch() > old.activity_epoch());
        assert!(matches!(
            app.screen_draw_controller.state(),
            crate::screen_draw::ScreenDrawState::AwaitingNativeTeardown { .. }
        ));
        let issued = commands.try_iter().collect::<Vec<_>>();
        assert!(
            !issued.is_empty()
                && issued.iter().all(|command| matches!(
                    command,
                    crate::screen_draw::NativeSessionCommand::Shutdown
                ))
        );
        let state = app.screen_draw_controller.state().clone();
        let revision = app.visibility_revision.current();
        let restored = observer.restored_rects();
        assert_eq!(restored, [original]);
        let (output, scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.begin_frame(egui::RawInput::default());
                app.event_tx
                    .send(WatchEvent::ScreenDrawRecover(old))
                    .unwrap();
                app.process_watch_events();
                ctx.end_frame()
            });
        assert_eq!(app.screen_draw_controller.state(), &state);
        assert_eq!(observer.current_rect(), original);
        assert_eq!(observer.restored_rects(), restored);
        assert_eq!(app.visibility_revision.current(), revision);
        assert!(root_commands(&output).is_empty() && scheduled.is_empty());
        assert!(commands.try_recv().is_err());
        let fresh = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let (_, scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.begin_frame(egui::RawInput::default());
            app.event_tx
                .send(WatchEvent::ScreenDrawRecover(fresh))
                .unwrap();
            app.process_watch_events();
            ctx.end_frame()
        });
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::NoSession
        );
        assert_eq!(scheduled, [(revision + 1, 42)]);
        assert_eq!(app.visibility_revision.current(), revision + 1);
    }

    #[test]
    fn undelivered_ghost_recovery_cannot_cancel_fresh_resume_parking() {
        let ctx = egui::Context::default();
        let (mut app, generation, commands) = drawing_app_with_resume_fixture(&ctx);
        let (observer, _) = install_recovery_parking(&mut app, generation);
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(42);
        app.root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        ctx.begin_frame(egui::RawInput::default());
        let first = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let initial_revision = app.visibility_revision.current();
        let (_, initial_scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                app.recover_screen_draw(first);
            });
        assert_eq!(initial_scheduled, [(initial_revision + 1, 42)]);
        let old = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let moved = crate::launcher_parking::LauncherWindowRect {
            left: 600,
            top: 320,
            right: 1050,
            bottom: 610,
        };
        observer.set_current_rect(moved);
        app.resume_screen_draw().unwrap();
        let _ = ctx.end_frame();
        let _ = commands.try_iter().collect::<Vec<_>>();
        let capture = Arc::clone(
            app.screen_draw_controller
                .session_snapshot()
                .unwrap()
                .capture(),
        );
        let revision = app.visibility_revision.current();
        let parked = observer.current_rect();
        let restored = observer.restored_rects();
        let (output, scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.begin_frame(egui::RawInput::default());
                app.event_tx
                    .send(WatchEvent::ScreenDrawRecover(old))
                    .unwrap();
                app.process_watch_events();
                ctx.end_frame()
            });
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Drawing { generation }
        );
        assert_eq!(app.visibility_revision.current(), revision);
        assert_eq!(observer.current_rect(), parked);
        assert_eq!(observer.restored_rects(), restored);
        assert_eq!(
            app.screen_draw_launcher_parking
                .as_ref()
                .unwrap()
                .original_snapshot()
                .rect(),
            moved
        );
        assert!(root_commands(&output).is_empty() && scheduled.is_empty());
        assert!(commands.try_recv().is_err());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        let fresh = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let (_, scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.begin_frame(egui::RawInput::default());
            app.recover_screen_draw(fresh);
            ctx.end_frame()
        });
        assert_eq!(scheduled, [(revision + 1, 42)]);
        assert_eq!(observer.current_rect(), moved);
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Ghost { generation }
        );
        assert!(matches!(
            commands.try_iter().collect::<Vec<_>>().as_slice(),
            [crate::screen_draw::NativeSessionCommand::Ghost]
        ));
        assert!(Arc::ptr_eq(
            &capture,
            app.screen_draw_controller
                .session_snapshot()
                .unwrap()
                .capture()
        ));
    }

    #[test]
    fn independent_idle_toolbar_open_close_cycles_each_restore_hidden_root_once() {
        use crate::commands::{ScreenDrawCommand, ScreenDrawCommandHost};
        for configured in [false, true] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            app.launcher_hwnd = Some(42);
            app.root_window_bridge.set_identity_for_test(42);
            app.root_window_bridge
                .qualify_simulated_wake_for_test()
                .unwrap();
            app.follow_mouse = false;
            app.static_location_enabled = configured;
            app.static_pos = configured.then_some((640, 360));
            app.static_size = configured.then_some((520, 300));
            app.query = "unchanged live query".into();
            let _ = app
                .query_history
                .older(&app.query, || ["older query".to_string()]);
            let history = app.query_history.acceptance_digest();
            let document = app
                .radial_editor
                .lock()
                .unwrap()
                .acceptance_observation()
                .unwrap();
            let generation = app.screen_draw_controller.state().generation();
            for _ in 0..2 {
                ctx.begin_frame(egui::RawInput::default());
                ScreenDrawCommandHost::execute_screen_draw_command(
                    &mut app,
                    ScreenDrawCommand::OpenToolbar,
                )
                .unwrap();
                assert!(app.screen_draw_controller.toolbar_open());
                app.request_launcher_visibility(false);
                let _ = ctx.end_frame();
                let revision = app.visibility_revision.current();
                let (output, scheduled) =
                    crate::window_manager::with_launcher_restore_queue_for_test(|| {
                        ctx.begin_frame(egui::RawInput::default());
                        ScreenDrawCommandHost::execute_screen_draw_command(
                            &mut app,
                            ScreenDrawCommand::Close,
                        )
                        .unwrap();
                        ScreenDrawCommandHost::execute_screen_draw_command(
                            &mut app,
                            ScreenDrawCommand::Close,
                        )
                        .unwrap();
                        ctx.end_frame()
                    });
                assert_eq!(app.visibility_revision.current(), revision + 1);
                assert_eq!(scheduled, [(revision + 1, 42)]);
                assert!(app.visible_flag.load(Ordering::SeqCst) && app.last_visible);
                assert!(!app.screen_draw_controller.toolbar_open());
                let commands = root_commands(&output);
                assert_root_activation_requests(&output, &scheduled);
                let expected = if configured {
                    egui::pos2(640.0, 360.0)
                } else {
                    egui::pos2(0.0, 0.0)
                };
                assert!(commands.iter().any(|command| matches!(command, egui::ViewportCommand::OuterPosition(position) if *position == expected)));
                if configured {
                    assert!(commands.iter().any(|command| matches!(command, egui::ViewportCommand::InnerSize(size) if *size == egui::vec2(520.0, 300.0))));
                }
                assert_eq!(app.query, "unchanged live query");
                assert_eq!(app.query_history.acceptance_digest(), history);
                assert_eq!(
                    app.radial_editor
                        .lock()
                        .unwrap()
                        .acceptance_observation()
                        .unwrap(),
                    document
                );
                assert_eq!(app.screen_draw_controller.state().generation(), generation);
            }
            app.request_launcher_visibility(false);
            let hidden_revision = app.visibility_revision.current();
            let (output, scheduled) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.begin_frame(egui::RawInput::default());
                    ScreenDrawCommandHost::execute_screen_draw_command(
                        &mut app,
                        ScreenDrawCommand::Close,
                    )
                    .unwrap();
                    ctx.end_frame()
                });
            assert_eq!(app.visibility_revision.current(), hidden_revision);
            assert!(!app.visible_flag.load(Ordering::SeqCst));
            assert!(root_commands(&output).is_empty() && scheduled.is_empty());
        }
    }

    #[test]
    fn failed_capture_retry_keeps_main_staged_lifetime_for_queued_and_minted_recovery() {
        for minted_before_start in [false, true] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            let (capture_completed, _capture_completed_rx) = std::sync::mpsc::channel();
            let capture = Arc::new(StartupCaptureBackend {
                capture_calls: AtomicUsize::new(0),
                completed: capture_completed,
            });
            let native = Arc::new(StartupNativeFactory {
                spawns: AtomicUsize::new(0),
                command_receivers: Mutex::new(Vec::new()),
            });
            app.screen_draw_controller =
                crate::screen_draw::ScreenDrawController::with_test_dependencies(
                    capture.clone(),
                    Arc::new(ParkedLauncherProbe),
                    native.clone(),
                );
            app.screen_draw_controller
                .set_recovery_bridge(Arc::clone(&app.screen_draw_recovery_bridge));
            let a = app.screen_draw_controller.request_start().unwrap();
            app.screen_draw_controller.launcher_parked(a).unwrap();
            app.screen_draw_controller
                .capture_failed(a, "actual capture failure fixture")
                .unwrap();
            assert!(
                matches!(app.screen_draw_controller.state(), crate::screen_draw::ScreenDrawState::Failed { generation, .. } if *generation == a)
            );
            assert!(!app.screen_draw_recovery_bridge.is_active());
            let trigger = crate::hotkey::HotkeyTrigger::new(
                crate::hotkey::parse_hotkey("Ctrl+Shift+D").unwrap(),
            );
            *trigger.open.lock().unwrap() = true;
            // Main uses this exact consumed-edge owner before queueing Start.
            assert!(
                app.screen_draw_recovery_bridge
                    .stage_start_if_triggered(&trigger)
            );
            let staged_lifetime = app.screen_draw_recovery_bridge.active_lifetime().unwrap();
            assert!(
                !app.screen_draw_recovery_bridge
                    .stage_start_if_triggered(&trigger)
            );
            assert_eq!(
                app.screen_draw_recovery_bridge.active_lifetime(),
                Some(staged_lifetime)
            );
            let mut queued = hook_recovery_notice(&app.screen_draw_recovery_bridge);
            let admission = queued.recovery.unwrap();
            let minted = minted_before_start.then(|| {
                queued
                    .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
                    .unwrap()
            });
            ctx.begin_frame(egui::RawInput::default());
            app.event_tx.send(WatchEvent::ScreenDrawStart).unwrap();
            app.process_watch_events();
            let _ = ctx.end_frame();
            let b = app.screen_draw_controller.state().generation().unwrap();
            assert_ne!(a, b);
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { generation: b }
            );
            assert_eq!(
                app.screen_draw_recovery_bridge.active_lifetime(),
                Some(staged_lifetime)
            );
            let (observer, original) = install_recovery_parking(&mut app, b);
            app.launcher_hwnd = Some(42);
            app.root_window_bridge.set_identity_for_test(42);
            app.root_window_bridge
                .qualify_simulated_wake_for_test()
                .unwrap();
            let intent = minted
                .or_else(|| queued.take_screen_draw_recovery(&app.screen_draw_recovery_bridge))
                .unwrap();
            assert_eq!(intent.admission, Some(admission));
            assert_eq!(intent.activity_epoch(), staged_lifetime.get());
            let revision = app.visibility_revision.current();
            let (output, scheduled) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.begin_frame(egui::RawInput::default());
                    for _ in 0..2 {
                        app.event_tx
                            .send(WatchEvent::ScreenDrawRecover(intent))
                            .unwrap();
                    }
                    app.process_watch_events();
                    app.poll_screen_draw_capture(&ctx);
                    ctx.end_frame()
                });
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::NoSession
            );
            assert!(!app.screen_draw_recovery_bridge.is_active());
            assert!(app.screen_draw_controller.session_snapshot().is_none());
            assert!(!app.screen_draw_controller.toolbar_open());
            assert_eq!(observer.restored_rects(), [original]);
            assert!(app.screen_draw_launcher_parking.is_none());
            assert_eq!(app.visibility_revision.current(), revision + 1);
            assert_eq!(scheduled, [(revision + 1, 42)]);
            assert_root_activation_requests(&output, &scheduled);
            assert_eq!(capture.capture_calls.load(Ordering::SeqCst), 0);
            assert_eq!(native.spawns.load(Ordering::SeqCst), 0);
            assert!(native.command_receivers.lock().unwrap().is_empty());
        }
    }

    fn screen_draw_root_reconcile_input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            ..Default::default()
        }
    }

    #[test]
    fn root_presentation_repark_retires_old_requests_but_safe_noop_keeps_current_lifetime() {
        for finish in [false, true] {
            let ctx = egui::Context::default();
            ctx.set_embed_viewports(false);
            let (mut app, generation, _retired_commands) = drawing_app_with_resume_fixture(&ctx);
            let (commands, _native_events) =
                app.screen_draw_controller.install_test_native_worker();
            let (observer, original) = install_recovery_parking(&mut app, generation);
            app.launcher_hwnd = Some(42);
            app.root_window_bridge.set_identity_for_test(42);
            app.root_window_bridge
                .qualify_simulated_wake_for_test()
                .unwrap();
            let initial_revision = app.visibility_revision.current();
            let (_, initial_scheduled) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.begin_frame(egui::RawInput::default());
                    let first = hook_recovery_notice(&app.screen_draw_recovery_bridge)
                        .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
                        .unwrap();
                    app.recover_screen_draw(first);
                    if finish {
                        app.screen_draw_controller.finish().unwrap();
                    }
                    ctx.end_frame()
                });
            assert_eq!(initial_scheduled, [(initial_revision + 1, 42)]);
            let mut queued = hook_recovery_notice(&app.screen_draw_recovery_bridge);
            let old = queued
                .clone()
                .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
                .unwrap();
            let old_emergency = app
                .screen_draw_recovery_bridge
                .admit(crate::screen_draw::ScreenDrawRecoveryKind::Emergency, None)
                .unwrap();
            let cycle = app.screen_draw_launcher_parking.as_ref().unwrap().cycle();
            assert_eq!(observer.current_rect(), original);
            app.request_launcher_visibility(false);
            let hidden_revision = app.visibility_revision.current();
            app.root_window_bridge.request_presentation_reconcile();
            let (output, scheduled) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.run(screen_draw_root_reconcile_input(), |root| {
                        app.render_root_frame(root, None)
                    })
                });
            let epoch = app.screen_draw_recovery_bridge.activity_epoch();
            assert!(epoch > old.activity_epoch());
            assert_eq!(
                app.screen_draw_launcher_parking.as_ref().unwrap().cycle(),
                cycle + 1
            );
            assert!(
                app.screen_draw_launcher_parking
                    .as_ref()
                    .unwrap()
                    .verify()
                    .unwrap()
            );
            let parked = observer.current_rect();
            assert_ne!(parked, original);
            assert!(!app.visible_flag.load(Ordering::SeqCst) && !app.last_visible);
            assert_eq!(app.visibility_revision.current(), hidden_revision);
            assert!(scheduled.is_empty());
            assert!(!root_commands(&output).iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Visible(true)
                    | egui::ViewportCommand::Minimized(false)
                    | egui::ViewportCommand::Focus
            )));
            let _ = commands.try_iter().collect::<Vec<_>>();
            let state = app.screen_draw_controller.state().clone();
            let capture = Arc::clone(
                app.screen_draw_controller
                    .session_snapshot()
                    .unwrap()
                    .capture(),
            );
            let mut fresh_notice = hook_recovery_notice(&app.screen_draw_recovery_bridge);
            app.root_window_bridge.request_presentation_reconcile();
            let (_, scheduled) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.run(screen_draw_root_reconcile_input(), |root| {
                        app.render_root_frame(root, None)
                    })
                });
            assert_eq!(
                app.screen_draw_recovery_bridge.activity_epoch(),
                epoch,
                "an already capture-safe reconcile has no fresh park operation"
            );
            assert_eq!(
                app.screen_draw_launcher_parking.as_ref().unwrap().cycle(),
                cycle + 1
            );
            assert_eq!(observer.current_rect(), parked);
            assert_eq!(app.visibility_revision.current(), hidden_revision);
            assert!(scheduled.is_empty());
            let _ = commands.try_iter().collect::<Vec<_>>();
            assert!(
                queued
                    .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
                    .is_none()
            );
            assert!(
                !app.screen_draw_recovery_bridge
                    .emergency_pause(old_emergency)
                    .unwrap()
            );
            let (output, scheduled) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.begin_frame(egui::RawInput::default());
                    app.event_tx
                        .send(WatchEvent::ScreenDrawRecover(old))
                        .unwrap();
                    app.process_watch_events();
                    ctx.end_frame()
                });
            assert_eq!(app.screen_draw_controller.state(), &state);
            assert_eq!(app.visibility_revision.current(), hidden_revision);
            assert_eq!(observer.current_rect(), parked);
            assert_eq!(observer.restored_rects(), [original]);
            assert!(!app.visible_flag.load(Ordering::SeqCst));
            assert!(root_commands(&output).is_empty() && scheduled.is_empty());
            assert!(commands.try_recv().is_err());
            assert!(Arc::ptr_eq(
                &capture,
                app.screen_draw_controller
                    .session_snapshot()
                    .unwrap()
                    .capture()
            ));
            let emergency = app
                .screen_draw_recovery_bridge
                .admit(crate::screen_draw::ScreenDrawRecoveryKind::Emergency, None)
                .unwrap();
            assert!(
                app.screen_draw_recovery_bridge
                    .emergency_pause(emergency)
                    .unwrap()
            );
            assert!(matches!(
                commands.try_recv(),
                Ok(crate::screen_draw::NativeSessionCommand::EmergencyPause)
            ));
            assert!(commands.try_recv().is_err());
            let fresh = fresh_notice
                .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
                .unwrap();
            let (output, scheduled) =
                crate::window_manager::with_launcher_restore_queue_for_test(|| {
                    ctx.begin_frame(egui::RawInput::default());
                    app.event_tx
                        .send(WatchEvent::ScreenDrawRecover(fresh))
                        .unwrap();
                    app.event_tx
                        .send(WatchEvent::ScreenDrawRecover(fresh))
                        .unwrap();
                    app.process_watch_events();
                    ctx.end_frame()
                });
            assert_eq!(app.screen_draw_controller.state(), &state);
            assert_eq!(app.visibility_revision.current(), hidden_revision + 1);
            assert_eq!(observer.current_rect(), original);
            assert_eq!(observer.restored_rects(), [original, original]);
            assert_eq!(scheduled, [(hidden_revision + 1, 42)]);
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert_root_activation_requests(&output, &scheduled);
            assert!(commands.try_recv().is_err());
            assert!(Arc::ptr_eq(
                &capture,
                app.screen_draw_controller
                    .session_snapshot()
                    .unwrap()
                    .capture()
            ));
        }
    }

    #[test]
    fn root_presentation_failed_repark_keeps_fallback_snapshot_and_retires_old_intent() {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let (mut app, generation, _retired_commands) = drawing_app_with_resume_fixture(&ctx);
        let (commands, _native_events) = app.screen_draw_controller.install_test_native_worker();
        let (observer, original) = install_recovery_parking(&mut app, generation);
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(42);
        app.root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        let initial_revision = app.visibility_revision.current();
        let (_, initial_scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.begin_frame(egui::RawInput::default());
                let first = hook_recovery_notice(&app.screen_draw_recovery_bridge)
                    .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
                    .unwrap();
                app.recover_screen_draw(first);
                ctx.end_frame()
            });
        assert_eq!(initial_scheduled, [(initial_revision + 1, 42)]);
        let old = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let cycle = app.screen_draw_launcher_parking.as_ref().unwrap().cycle();
        app.request_launcher_visibility(false);
        let revision = app.visibility_revision.current();
        observer.fail_next_park();
        app.root_window_bridge.request_presentation_reconcile();
        let (output, scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.run(screen_draw_root_reconcile_input(), |root| {
                    app.render_root_frame(root, None)
                })
            });
        assert!(app.screen_draw_recovery_bridge.activity_epoch() > old.activity_epoch());
        assert_eq!(
            app.screen_draw_launcher_parking.as_ref().unwrap().cycle(),
            cycle + 1
        );
        assert_eq!(
            app.screen_draw_launcher_parking
                .as_ref()
                .unwrap()
                .original_snapshot()
                .rect(),
            original
        );
        assert_eq!(
            app.screen_draw_launcher_parking.as_ref().unwrap().state(),
            crate::launcher_parking::LauncherParkingState::Active
        );
        assert_eq!(
            observer.current_rect(),
            original,
            "the failed native park is not fabricated as completed"
        );
        assert_eq!(app.visibility_revision.current(), revision);
        assert!(!app.visible_flag.load(Ordering::SeqCst) && !app.last_visible);
        assert!(scheduled.is_empty());
        assert!(root_commands(&output).iter().any(|command| matches!(command, egui::ViewportCommand::OuterPosition(position) if *position == egui::pos2(app.offscreen_pos.0, app.offscreen_pos.1))));
        let _ = commands.try_iter().collect::<Vec<_>>();
        let state = app.screen_draw_controller.state().clone();
        let restored = observer.restored_rects();
        let (output, scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.begin_frame(egui::RawInput::default());
                app.event_tx
                    .send(WatchEvent::ScreenDrawRecover(old))
                    .unwrap();
                app.process_watch_events();
                ctx.end_frame()
            });
        assert_eq!(app.screen_draw_controller.state(), &state);
        assert_eq!(app.visibility_revision.current(), revision);
        assert_eq!(observer.current_rect(), original);
        assert_eq!(observer.restored_rects(), restored);
        assert!(root_commands(&output).is_empty() && scheduled.is_empty());
        assert!(commands.try_recv().is_err());
        let fresh = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let (_, scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.begin_frame(egui::RawInput::default());
            app.recover_screen_draw(fresh);
            ctx.end_frame()
        });
        assert_eq!(scheduled, [(revision + 1, 42)]);
        assert_eq!(app.visibility_revision.current(), revision + 1);
        assert_eq!(observer.current_rect(), original);
        assert_eq!(observer.restored_rects(), [original, original]);
        assert_eq!(app.screen_draw_controller.state(), &state);
    }

    #[test]
    fn screen_draw_recovery_cancels_both_capture_startup_states_and_restores_exactly() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            for recovery in [
                crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                crate::screen_draw::ScreenDrawRecoveryKind::Emergency,
            ] {
                for advance_to_capturing in [false, true] {
                    let ctx = egui::Context::default();
                    let mut app = new_app(&ctx);
                    let generation = app.screen_draw_controller.request_start().unwrap();
                    if advance_to_capturing {
                        app.screen_draw_controller
                            .launcher_parked(generation)
                            .unwrap();
                    }
                    let (observer, original) = install_recovery_parking(&mut app, generation);

                    ctx.begin_frame(egui::RawInput::default());
                    let intent = app
                        .screen_draw_recovery_bridge
                        .admit(recovery, None)
                        .unwrap();
                    app.recover_screen_draw(intent);
                    let _ = ctx.end_frame();

                    assert_eq!(
                        app.screen_draw_controller.state(),
                        &crate::screen_draw::ScreenDrawState::NoSession
                    );
                    assert!(!app.screen_draw_recovery_bridge.is_active());
                    assert_eq!(observer.restored_rects(), [original]);
                    assert!(app.screen_draw_launcher_parking.is_none());
                    assert!(app.visible_flag.load(Ordering::SeqCst));
                }
            }
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 4);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn screen_draw_recovery_pauses_drawing_but_preserves_safe_session_states() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            let generation = app.screen_draw_controller.request_start().unwrap();
            app.screen_draw_controller
                .launcher_parked(generation)
                .unwrap();
            app.screen_draw_controller
                .capture_succeeded(generation)
                .unwrap();
            let (observer, original) = install_recovery_parking(&mut app, generation);

            ctx.begin_frame(egui::RawInput::default());
            let intent = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            app.recover_screen_draw(intent);
            let _ = ctx.end_frame();

            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::Ghost { generation }
            );
            assert!(app.screen_draw_controller.toolbar_open());
            assert!(app.screen_draw_recovery_bridge.is_active());
            assert_eq!(observer.restored_rects(), [original]);

            for expected in [
                crate::screen_draw::ScreenDrawState::Ghost { generation },
                crate::screen_draw::ScreenDrawState::Finish { generation },
                crate::screen_draw::ScreenDrawState::DisplayChanged { generation },
            ] {
                match expected {
                    crate::screen_draw::ScreenDrawState::Ghost { .. } => {}
                    crate::screen_draw::ScreenDrawState::Finish { .. } => {
                        app.screen_draw_controller.finish().unwrap();
                    }
                    crate::screen_draw::ScreenDrawState::DisplayChanged { .. } => {
                        app.screen_draw_controller
                            .note_display_changed(generation)
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
                let intent = app
                    .screen_draw_recovery_bridge
                    .admit(
                        crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                        None,
                    )
                    .unwrap();
                app.recover_screen_draw(intent);
                assert_eq!(app.screen_draw_controller.state(), &expected);
                assert!(app.screen_draw_recovery_bridge.is_active());
            }
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 4);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn screen_draw_recovery_cancels_picker_and_quarantines_its_operation() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            install_owned_root_for_test(&mut app);
            let fixture = crate::gui::mkmacro_dialog::visual_capture_workflow::SharedVisualOverlayController::test_fixture();
            app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
            let generation = app.screen_draw_controller.request_start().unwrap();
            app.screen_draw_controller
                .launcher_parked(generation)
                .unwrap();
            app.screen_draw_controller
                .capture_succeeded(generation)
                .unwrap();
            app.screen_draw_controller.finish().unwrap();
            app.screen_draw_controller
                .begin_region_selection(
                    crate::screen_draw::ExportBackground::Transparent,
                    crate::screen_draw::ExportDestination::Clipboard,
                )
                .unwrap();
            app.begin_screen_draw_region_picker(crate::screen_draw::ScreenDrawRegionPickerReady {
                generation,
                bounds: crate::mkmacro::screen::ScreenRect::new(-50, 10, 200, 100),
            });
            let operation_id = app.screen_draw_region_operation.unwrap().operation_id;
            fixture.observer.wait_for_commands(1);

            let intent = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            app.recover_screen_draw(intent);

            assert!(app.screen_draw_region_operation.is_none());
            assert!(
                fixture
                    .controller
                    .screen_draw_discard_pending_for_test(operation_id)
            );
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::Finish { generation }
            );
            assert!(app.screen_draw_controller.toolbar_open());
            assert!(!app.screen_draw_controller.export_in_flight());
            fixture.observer.wait_for_commands(2);
            fixture.observer.confirm_rectangle(
                operation_id,
                crate::mkmacro::variables::MkPoint { x: -20, y: 20 },
                crate::mkmacro::variables::MkPoint { x: 20, y: 50 },
            );
            std::thread::yield_now();
            assert!(fixture.controller.poll().is_empty());
            assert!(
                fixture
                    .controller
                    .poll_rectangle_event(operation_id)
                    .is_none()
            );
            app.poll_screen_draw_region_picker(&ctx);
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::Finish { generation }
            );
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn screen_draw_recovery_cancels_native_teardown_and_restores_exactly() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            let first = app.screen_draw_controller.request_start().unwrap();
            app.screen_draw_controller.launcher_parked(first).unwrap();
            app.screen_draw_controller.capture_succeeded(first).unwrap();
            let (commands, late_events) = app.screen_draw_controller.install_test_native_worker();
            let replacement = app.screen_draw_controller.request_new_capture().unwrap();
            let (observer, original) = install_recovery_parking(&mut app, replacement);

            ctx.begin_frame(egui::RawInput::default());
            let intent = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            app.recover_screen_draw(intent);
            let _ = ctx.end_frame();

            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::NoSession
            );
            assert!(!app.screen_draw_recovery_bridge.is_active());
            assert_eq!(observer.restored_rects(), [original]);
            let commands = commands.try_iter().collect::<Vec<_>>();
            assert!(!commands.is_empty());
            assert!(commands.iter().all(|command| matches!(
                command,
                crate::screen_draw::NativeSessionCommand::Shutdown
            )));
            let _ = late_events.send(crate::screen_draw::NativeSessionEvent::SessionClosed);
            app.poll_screen_draw_capture(&ctx);
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::NoSession
            );
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn repeated_emergency_recovery_is_idempotent_and_shutdown_clears_activity() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let generation = app.screen_draw_controller.request_start().unwrap();
        app.screen_draw_controller
            .launcher_parked(generation)
            .unwrap();
        app.screen_draw_controller
            .capture_succeeded(generation)
            .unwrap();
        let (observer, original) = install_recovery_parking(&mut app, generation);
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(42);
        app.root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        let intent = app
            .screen_draw_recovery_bridge
            .admit(crate::screen_draw::ScreenDrawRecoveryKind::Emergency, None)
            .unwrap();
        let revision = app.visibility_revision.current();
        acceptance_trace::take_screen_draw_restore_test_events();
        let (output, scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.begin_frame(egui::RawInput::default());
                app.event_tx
                    .send(WatchEvent::ScreenDrawEmergency(intent))
                    .unwrap();
                app.event_tx
                    .send(WatchEvent::ScreenDrawEmergency(intent))
                    .unwrap();
                app.process_watch_events();
                ctx.end_frame()
            });
        assert_eq!(app.visibility_revision.current(), revision + 1);
        assert_eq!(scheduled, [(revision + 1, 42)]);
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert_root_activation_requests(&output, &scheduled);
        let events = acceptance_trace::take_screen_draw_restore_test_events();
        let decisions = events
            .iter()
            .filter_map(|event| match event {
                acceptance_trace::Event::ScreenDrawRestoreDecision {
                    intent_id, outcome, ..
                } => Some((*intent_id, *outcome)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            decisions,
            [
                (
                    intent.id(),
                    crate::screen_draw::ScreenDrawRestoreOutcome::Published
                ),
                (
                    intent.id(),
                    crate::screen_draw::ScreenDrawRestoreOutcome::Duplicate
                )
            ]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, acceptance_trace::Event::DesiredVisibility { .. }))
                .count(),
            1
        );
        assert_eq!(observer.restored_rects(), [original]);
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Ghost { generation }
        );
        assert!(app.screen_draw_controller.toolbar_open());
        assert!(app.screen_draw_recovery_bridge.is_active());
        app.close_screen_draw_for_exit();
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::NoSession
        );
        assert!(!app.screen_draw_recovery_bridge.is_active());
    }

    #[test]
    fn screen_draw_distinct_admitted_recoveries_publish_once_each_in_the_same_ghost_session() {
        let ctx = egui::Context::default();
        let (mut app, generation, commands) = drawing_app_with_resume_fixture(&ctx);
        install_recovery_parking(&mut app, generation);
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(42);
        app.root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        let first = app
            .screen_draw_recovery_bridge
            .admit(
                crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                Some(crate::screen_draw::ScreenDrawRecoveryAdmission {
                    serial: 1,
                    lifetime: app.screen_draw_recovery_bridge.active_lifetime().unwrap(),
                    kind: crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                }),
            )
            .unwrap();
        let second = app
            .screen_draw_recovery_bridge
            .admit(
                crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                Some(crate::screen_draw::ScreenDrawRecoveryAdmission {
                    serial: 2,
                    lifetime: app.screen_draw_recovery_bridge.active_lifetime().unwrap(),
                    kind: crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                }),
            )
            .unwrap();
        assert_ne!(first.id(), second.id());
        let revision = app.visibility_revision.current();
        let (output, scheduled) =
            crate::window_manager::with_launcher_restore_queue_for_test(|| {
                ctx.begin_frame(egui::RawInput::default());
                for intent in [first, first, second, second, first] {
                    app.event_tx
                        .send(WatchEvent::ScreenDrawRecover(intent))
                        .unwrap();
                    app.process_watch_events();
                }
                ctx.end_frame()
            });
        assert_eq!(app.visibility_revision.current(), revision + 2);
        assert_eq!(scheduled, [(revision + 1, 42), (revision + 2, 42)]);
        assert_root_activation_requests(&output, &scheduled);
        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::Ghost { generation }
        );
        assert!(matches!(
            commands.try_iter().collect::<Vec<_>>().as_slice(),
            [crate::screen_draw::NativeSessionCommand::Ghost]
        ));
    }

    #[test]
    fn screen_draw_failed_geometry_recovery_can_complete_the_same_intent_on_a_later_attempt() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, _commands) = drawing_app_with_resume_fixture(&ctx);
            let (observer, original) = install_recovery_parking(&mut app, generation);
            observer.fail_next_restore();
            let intent = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            let revision = app.visibility_revision.current();
            ctx.begin_frame(egui::RawInput::default());
            app.recover_screen_draw(intent);
            assert_eq!(app.visibility_revision.current(), revision);
            assert!(!app.visible_flag.load(Ordering::SeqCst));
            assert!(observer.restored_rects().is_empty());
            app.recover_screen_draw(intent);
            app.recover_screen_draw(intent);
            let output = ctx.end_frame();
            assert_eq!(app.visibility_revision.current(), revision + 1);
            assert_eq!(observer.restored_rects(), [original]);
            #[cfg(not(windows))]
            assert_eq!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .filter(|command| matches!(command, egui::ViewportCommand::Focus))
                    .count(),
                1
            );
            #[cfg(windows)]
            assert!(!root_commands(&output).iter().any(|command| matches!(
                command,
                egui::ViewportCommand::Focus
                    | egui::ViewportCommand::Visible(true)
                    | egui::ViewportCommand::Minimized(false)
            )));
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn screen_draw_rejected_native_admission_does_not_complete_the_recovery_intent() {
        let ctx = egui::Context::default();
        let (mut app, generation, _commands) = drawing_app_with_resume_fixture(&ctx);
        install_recovery_parking(&mut app, generation);
        app.launcher_hwnd = Some(42);
        app.root_window_bridge.set_identity_for_test(43);
        let mismatched_wake_identity = app
            .root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        let intent = app
            .screen_draw_recovery_bridge
            .admit(
                crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                None,
            )
            .unwrap();
        let revision = app.visibility_revision.current();
        acceptance_trace::take_screen_draw_restore_test_events();
        let (_, scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.begin_frame(egui::RawInput::default());
            app.recover_screen_draw(intent);
            assert_eq!(app.visibility_revision.current(), revision + 1);
            assert_eq!(app.root_window_bridge.identity(), mismatched_wake_identity);
            assert_eq!(
                app.root_window_bridge
                    .take_simulated_wake_observations_for_test(),
                [mismatched_wake_identity]
            );
            app.root_window_bridge.set_identity_for_test(42);
            let retry_wake_identity = app
                .root_window_bridge
                .qualify_simulated_wake_for_test()
                .unwrap();
            assert_eq!(retry_wake_identity.0, 42);
            assert!(retry_wake_identity.1 > mismatched_wake_identity.1);
            app.recover_screen_draw(intent);
            assert_eq!(app.root_window_bridge.identity(), retry_wake_identity);
            assert_eq!(
                app.root_window_bridge
                    .take_simulated_wake_observations_for_test(),
                [retry_wake_identity]
            );
            app.recover_screen_draw(intent);
            assert!(
                app.root_window_bridge
                    .take_simulated_wake_observations_for_test()
                    .is_empty()
            );
            ctx.end_frame()
        });
        assert_eq!(app.visibility_revision.current(), revision + 2);
        assert_eq!(
            scheduled,
            [(revision + 2, 42)],
            "only the admitted publication may schedule native activation"
        );
        let outcomes = acceptance_trace::take_screen_draw_restore_test_events()
            .into_iter()
            .filter_map(|event| match event {
                acceptance_trace::Event::ScreenDrawRestoreDecision { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes,
            [
                crate::screen_draw::ScreenDrawRestoreOutcome::Error,
                crate::screen_draw::ScreenDrawRestoreOutcome::Published,
                crate::screen_draw::ScreenDrawRestoreOutcome::Duplicate
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn screen_draw_missing_native_identity_keeps_failed_intent_retryable() {
        let ctx = egui::Context::default();
        let (mut app, generation, _) = drawing_app_with_resume_fixture(&ctx);
        let (observer, original) = install_recovery_parking(&mut app, generation);
        app.launcher_hwnd = None;
        app.root_window_bridge.clear();
        let intent = hook_recovery_notice(&app.screen_draw_recovery_bridge)
            .take_screen_draw_recovery(&app.screen_draw_recovery_bridge)
            .unwrap();
        let revision = app.visibility_revision.current();
        acceptance_trace::take_screen_draw_restore_test_events();
        let (output, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.run(egui::RawInput::default(), |_| {
                app.recover_screen_draw(intent)
            })
        });
        assert!(queued.is_empty());
        assert_root_activation_requests(&output, &queued);
        assert_eq!(app.visibility_revision.current(), revision + 1);
        assert_eq!(observer.current_rect(), original);
        assert!(acceptance_trace::take_screen_draw_restore_test_events().iter().any(|event| matches!(event,
            acceptance_trace::Event::ScreenDrawRestoreDecision { intent_id, outcome: crate::screen_draw::ScreenDrawRestoreOutcome::Error, .. }
                if *intent_id == intent.id())));
        install_owned_root_for_test(&mut app);
        let (output, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            ctx.run(egui::RawInput::default(), |_| {
                app.recover_screen_draw(intent);
                app.recover_screen_draw(intent);
            })
        });
        assert_eq!(queued, [(revision + 2, 42)]);
        assert_root_activation_requests(&output, &queued);
        assert_eq!(app.visibility_revision.current(), revision + 2);
        assert_eq!(observer.current_rect(), original);
        let decisions = acceptance_trace::take_screen_draw_restore_test_events()
            .into_iter()
            .filter_map(|event| match event {
                acceptance_trace::Event::ScreenDrawRestoreDecision {
                    intent_id, outcome, ..
                } if intent_id == intent.id() => Some(outcome),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            decisions,
            [
                crate::screen_draw::ScreenDrawRestoreOutcome::Published,
                crate::screen_draw::ScreenDrawRestoreOutcome::Duplicate
            ]
        );
    }

    #[test]
    fn screen_draw_completed_recovery_does_not_override_a_newer_hide_or_a_new_session() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, _commands) = drawing_app_with_resume_fixture(&ctx);
            install_recovery_parking(&mut app, generation);
            let intent = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            ctx.begin_frame(egui::RawInput::default());
            app.recover_screen_draw(intent);
            app.request_launcher_visibility(false);
            let hidden_revision = app.visibility_revision.current();
            app.recover_screen_draw(intent);
            assert_eq!(app.visibility_revision.current(), hidden_revision);
            assert!(!app.visible_flag.load(Ordering::SeqCst));
            app.screen_draw_controller.close();
            let next = app.screen_draw_controller.request_start().unwrap();
            assert_ne!(generation, next);
            app.recover_screen_draw(intent);
            let _ = ctx.end_frame();
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { generation: next }
            );
            assert_eq!(app.visibility_revision.current(), hidden_revision);
            assert!(!app.visible_flag.load(Ordering::SeqCst));
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn screen_draw_stale_recovery_reconciles_then_allows_a_fresh_admitted_recovery() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, _commands) = drawing_app_with_resume_fixture(&ctx);
            let (observer, original) = install_recovery_parking(&mut app, generation);
            let first = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            let revision = app.visibility_revision.clone();
            let visible = app.visible_flag.clone();
            observer.before_next_restore(move || {
                revision.request(|| visible.store(false, Ordering::SeqCst));
            });
            acceptance_trace::take_screen_draw_restore_test_events();
            ctx.begin_frame(egui::RawInput::default());
            app.recover_screen_draw(first);
            let after_hide = app.visibility_revision.current();
            assert!(!app.visible_flag.load(Ordering::SeqCst));
            assert!(
                app.screen_draw_launcher_parking
                    .as_ref()
                    .unwrap()
                    .verify()
                    .unwrap()
            );
            app.recover_screen_draw(first);
            assert_eq!(app.visibility_revision.current(), after_hide);
            let fresh = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            app.recover_screen_draw(fresh);
            let _ = ctx.end_frame();
            assert_eq!(app.visibility_revision.current(), after_hide + 1);
            assert_eq!(observer.restored_rects(), [original, original]);
            assert_eq!(observer.current_rect(), original);
            let outcomes = acceptance_trace::take_screen_draw_restore_test_events()
                .into_iter()
                .filter_map(|event| match event {
                    acceptance_trace::Event::ScreenDrawRestoreDecision { outcome, .. } => {
                        Some(outcome)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                outcomes,
                [
                    crate::screen_draw::ScreenDrawRestoreOutcome::Reconciled,
                    crate::screen_draw::ScreenDrawRestoreOutcome::StaleIntent,
                    crate::screen_draw::ScreenDrawRestoreOutcome::Published
                ]
            );
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 1);
            assert_root_activation_evidence(&scheduled);
        }
    }

    fn drawing_app_with_resume_fixture(
        ctx: &egui::Context,
    ) -> (
        LauncherApp,
        crate::screen_draw::ScreenDrawGeneration,
        std::sync::mpsc::Receiver<crate::screen_draw::NativeSessionCommand>,
    ) {
        let mut app = new_app(ctx);
        let generation = app.screen_draw_controller.request_start().unwrap();
        app.screen_draw_controller
            .launcher_parked(generation)
            .unwrap();
        app.screen_draw_controller
            .capture_succeeded(generation)
            .unwrap();
        app.screen_draw_controller.install_test_session_snapshot(
            generation,
            crate::mkmacro::screen::CapturedRegion {
                image: image::RgbaImage::new(8, 6),
                origin: (-4, -3),
            },
        );
        let (commands, _) = app.screen_draw_controller.install_test_native_worker();
        (app, generation, commands)
    }

    #[test]
    fn resume_reparks_from_current_geometry_and_next_recovery_restores_it() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, commands) = drawing_app_with_resume_fixture(&ctx);
            let retained_capture = Arc::clone(
                app.screen_draw_controller
                    .session_snapshot()
                    .unwrap()
                    .capture(),
            );
            let (observer, original) = install_recovery_parking(&mut app, generation);
            app.screen_draw_launcher_parking
                .as_mut()
                .unwrap()
                .commit_hidden();
            let intent = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            app.recover_screen_draw(intent);
            let original_intent = intent;
            let prior_cycle = app.screen_draw_launcher_parking.as_ref().unwrap().cycle();
            let moved = crate::launcher_parking::LauncherWindowRect {
                left: 410,
                top: 260,
                right: 910,
                bottom: 560,
            };
            observer.set_current_rect(moved);

            app.resume_screen_draw().unwrap();
            assert!(app.screen_draw_launcher_parking.as_ref().unwrap().cycle() > prior_cycle);
            let resumed_revision = app.visibility_revision.current();
            app.recover_screen_draw(original_intent);
            assert_eq!(app.visibility_revision.current(), resumed_revision);
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::Drawing { generation }
            );
            assert!(!app.visible_flag.load(Ordering::SeqCst));

            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::Drawing { generation }
            );
            assert!(!app.visible_flag.load(Ordering::SeqCst));
            assert_eq!(
                app.screen_draw_launcher_parking
                    .as_ref()
                    .unwrap()
                    .original_snapshot()
                    .rect(),
                moved
            );
            assert!(Arc::ptr_eq(
                &retained_capture,
                app.screen_draw_controller
                    .session_snapshot()
                    .unwrap()
                    .capture()
            ));

            let intent = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            app.recover_screen_draw(intent);
            assert_eq!(observer.restored_rects(), [original, moved]);
            assert_eq!(observer.current_rect(), moved);
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::Ghost { generation }
            );
            let commands = commands.try_iter().collect::<Vec<_>>();
            assert!(matches!(
                commands.as_slice(),
                [
                    crate::screen_draw::NativeSessionCommand::Ghost,
                    crate::screen_draw::NativeSessionCommand::SetToolbarWindow(None),
                    crate::screen_draw::NativeSessionCommand::Resume,
                    crate::screen_draw::NativeSessionCommand::Ghost
                ]
            ));
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 2);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn resume_parking_failure_keeps_safe_mode_and_current_launcher_geometry() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, _commands) = drawing_app_with_resume_fixture(&ctx);
            let (observer, original) = install_recovery_parking(&mut app, generation);
            app.screen_draw_launcher_parking
                .as_mut()
                .unwrap()
                .commit_hidden();
            let intent = app
                .screen_draw_recovery_bridge
                .admit(
                    crate::screen_draw::ScreenDrawRecoveryKind::LauncherToggle,
                    None,
                )
                .unwrap();
            app.recover_screen_draw(intent);
            let moved = crate::launcher_parking::LauncherWindowRect {
                left: 600,
                top: 320,
                right: 1050,
                bottom: 610,
            };
            observer.set_current_rect(moved);
            observer.fail_next_park();

            let error = app.resume_screen_draw().unwrap_err();

            assert!(error.contains("fixture launcher park failed"), "{error}");
            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::Ghost { generation }
            );
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert_eq!(observer.restored_rects(), [original]);
            assert_eq!(observer.current_rect(), moved);
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 2);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn resume_reuses_generation_bound_parking_when_launcher_is_already_safe() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            let ctx = egui::Context::default();
            let (mut app, generation, commands) = drawing_app_with_resume_fixture(&ctx);
            let (observer, _) = install_recovery_parking(&mut app, generation);
            app.screen_draw_launcher_parking
                .as_mut()
                .unwrap()
                .commit_hidden();
            app.screen_draw_controller.enter_ghost().unwrap();

            app.resume_screen_draw().unwrap();

            assert_eq!(
                app.screen_draw_controller.state(),
                &crate::screen_draw::ScreenDrawState::Drawing { generation }
            );
            assert!(observer.restored_rects().is_empty());
            assert!(matches!(
                commands.try_iter().collect::<Vec<_>>().as_slice(),
                [
                    crate::screen_draw::NativeSessionCommand::Ghost,
                    crate::screen_draw::NativeSessionCommand::SetToolbarWindow(None),
                    crate::screen_draw::NativeSessionCommand::Resume
                ]
            ));
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 0);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn root_escape_cancels_pre_capture_and_restores_without_generic_placement() {
        let ((), scheduled) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            for capturing in [false, true] {
                let ctx = egui::Context::default();
                let mut app = new_app(&ctx);
                ctx.begin_frame(egui::RawInput::default());
                let _ = ctx.end_frame();
                assert!(app.start_or_focus_screen_draw().unwrap());
                let generation = app.screen_draw_controller.state().generation().unwrap();
                assert!(!app.start_or_focus_screen_draw().unwrap());
                assert!(!app.screen_draw_controller.toolbar_open());
                if capturing {
                    app.screen_draw_controller
                        .launcher_parked(generation)
                        .unwrap();
                }
                let (observer, original) = install_recovery_parking(&mut app, generation);

                ctx.begin_frame(egui::RawInput {
                    events: vec![key_press(egui::Key::Escape, egui::Modifiers::NONE)],
                    ..Default::default()
                });
                app.cancel_screen_draw_startup_on_escape(&ctx);
                let output = ctx.end_frame();

                assert_eq!(
                    app.screen_draw_controller.state(),
                    &crate::screen_draw::ScreenDrawState::NoSession
                );
                assert_eq!(observer.restored_rects(), [original]);
                assert!(app.screen_draw_launcher_parking.is_none());
                assert!(app.visible_flag.load(Ordering::SeqCst));
                assert!(app.last_visible);
                assert!(!app.restore_flag.load(Ordering::SeqCst));
                let commands = &output
                    .viewport_output
                    .get(&egui::ViewportId::ROOT)
                    .unwrap()
                    .commands;
                assert!(!commands.iter().any(|command| matches!(
                    command,
                    egui::ViewportCommand::Visible(false) | egui::ViewportCommand::Minimized(true)
                )));
            }
        });
        #[cfg(windows)]
        {
            assert_eq!(scheduled.len(), 2);
            assert_root_activation_evidence(&scheduled);
        }
    }

    #[test]
    fn typed_parking_effect_synchronizes_visibility_without_configured_hide() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        ctx.begin_frame(egui::RawInput::default());
        let _ = ctx.end_frame();
        let generation = app.screen_draw_controller.request_start().unwrap();
        let original = crate::launcher_parking::LauncherWindowRect {
            left: 25,
            top: 40,
            right: 425,
            bottom: 240,
        };
        let (transaction, observer) = crate::launcher_parking::launcher_parking_test_fixture(
            generation,
            original,
            crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
        );
        app.screen_draw_launcher_parking = Some(transaction);
        app.visible_flag.store(true, Ordering::SeqCst);
        app.last_visible = true;
        app.restore_flag.store(true, Ordering::SeqCst);

        ctx.begin_frame(egui::RawInput::default());
        app.apply_screen_draw_capture_poll(
            &ctx,
            crate::screen_draw::ScreenDrawCapturePoll {
                park_launcher: Some(crate::screen_draw::ScreenDrawParkingRequest {
                    generation,
                    virtual_desktop: crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
                }),
                ..Default::default()
            },
        );
        let output = ctx.end_frame();

        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.last_visible);
        assert!(!app.restore_flag.load(Ordering::SeqCst));
        assert!(app.screen_draw_launcher_parking.is_some());
        assert!(observer.restored_rects().is_empty());
        let commands = &output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .unwrap()
            .commands;
        assert!(!commands.iter().any(|command| matches!(
            command,
            egui::ViewportCommand::Visible(false) | egui::ViewportCommand::Minimized(true)
        )));
    }

    #[test]
    fn stale_parking_effect_cannot_mutate_replacement_generation() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let first = app.screen_draw_controller.request_start().unwrap();
        let second = app.screen_draw_controller.request_new_capture().unwrap();
        assert_ne!(first, second);

        app.apply_screen_draw_capture_poll(
            &ctx,
            crate::screen_draw::ScreenDrawCapturePoll {
                park_launcher: Some(crate::screen_draw::ScreenDrawParkingRequest {
                    generation: first,
                    virtual_desktop: crate::mkmacro::screen::ScreenRect::new(0, 0, 1920, 1080),
                }),
                ..Default::default()
            },
        );

        assert_eq!(
            app.screen_draw_controller.state(),
            &crate::screen_draw::ScreenDrawState::AwaitingLauncherParking { generation: second }
        );
        assert!(app.screen_draw_launcher_parking.is_none());
    }

    fn two_results() -> Vec<Action> {
        (0..2)
            .map(|index| Action {
                label: format!("Result {index}"),
                desc: "Test".into(),
                action: format!("test:{index}"),
                args: None,
            })
            .collect()
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

    fn prompted_test_entry() -> crate::plugins::snippets::SnippetEntry {
        let mut message = crate::plugins::snippets::SnippetFieldDefinition::new("message");
        message.required = false;
        message.input_kind = crate::plugins::snippets::SnippetInputKind::Multiline;
        crate::plugins::snippets::SnippetEntry {
            alias: "reply".into(),
            text: "Hello {{name}}\n{{message}}".into(),
            hide_contents: false,
            prompt_for_fields: true,
            fields: vec![
                crate::plugins::snippets::SnippetFieldDefinition::new("name"),
                message,
            ],
        }
    }

    fn request_test_prompt(app: &mut LauncherApp, entry: crate::plugins::snippets::SnippetEntry) {
        let prepared = match crate::plugins::snippets::prepare_snippet_run(&entry).unwrap() {
            crate::plugins::snippets::SnippetRunMode::Prompted(prepared) => prepared,
            crate::plugins::snippets::SnippetRunMode::Plain => unreachable!(),
        };
        crate::commands::HeadlessCommandHost::request_snippet_prompt(
            app,
            crate::commands::SnippetPromptIntent {
                alias: entry.alias.clone(),
                entry_snapshot: entry,
                prepared,
                safe_action: Action {
                    label: "reply".into(),
                    desc: "Snippet".into(),
                    action: crate::plugins::snippets::snippet_run_action("reply"),
                    args: None,
                },
                source: ActivationSource::Dashboard,
                history_query: "cs reply".into(),
                root_policy: crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState,
            },
        )
        .unwrap();
    }

    fn prompt_raw_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(720.0, 520.0),
            )),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn render_prompt_test_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(prompt_raw_input(events), |ctx| {
            app.render_root_frame(ctx, None);
        });
    }

    fn render_prompt_test_frame_at_size(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        events: Vec<egui::Event>,
        size: egui::Vec2,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| {
                app.render_root_frame(ctx, None);
            },
        );
    }

    #[test]
    fn snippet_prompt_focus_tab_multiline_and_escape_are_owned_by_the_dialog() {
        let ctx = egui::Context::default();
        let (mut app, _test_directory) = new_prompt_app(&ctx);
        app.test_skip_history_persistence = true;
        app.visible_flag.store(true, Ordering::SeqCst);
        app.last_visible = true;
        app.query = "keep this query".into();
        app.results = vec![Action {
            label: "Help".into(),
            desc: "Command".into(),
            action: "help:show".into(),
            args: None,
        }];
        assert!(app.open_action_sheet_for_index(0));
        request_test_prompt(&mut app, prompted_test_entry());
        let generation = app.snippet_prompt_dialog.session().unwrap().generation;
        let name_id = egui::Id::new(("snippet_prompt_field", generation, "name"));

        render_prompt_test_frame(&ctx, &mut app, Vec::new());
        assert!(ctx.memory(|memory| memory.has_focus(name_id)));

        // Ctrl+Enter reaches the central validator before the form has a value.
        // That failure must leave the dialog, value and focus in place.
        render_prompt_test_frame(
            &ctx,
            &mut app,
            vec![key_press(egui::Key::Enter, egui::Modifiers::CTRL)],
        );
        assert!(app.snippet_prompt_dialog.is_open());
        assert_eq!(
            app.snippet_prompt_dialog.feedback(),
            Some(super::super::snippet_prompt_dialog::SnippetPromptError::RequiredValueEmpty)
        );
        assert!(ctx.memory(|memory| memory.has_focus(name_id)));
        app.snippet_prompt_dialog.set_value("name", "Ada".into());

        let mut tab_fell_through = true;
        let _ = ctx.run(
            prompt_raw_input(vec![key_press(egui::Key::Tab, egui::Modifiers::NONE)]),
            |ctx| {
                app.render_root_frame(ctx, None);
                tab_fell_through = ctx.input(|input| input.key_pressed(egui::Key::Tab));
            },
        );
        let message_id = egui::Id::new(("snippet_prompt_field", generation, "message"));
        assert!(!tab_fell_through);
        assert!(ctx.memory(|memory| memory.has_focus(message_id)));

        // Plain Enter remains text input in the multiline field.
        render_prompt_test_frame(
            &ctx,
            &mut app,
            vec![key_press(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert!(
            app.snippet_prompt_dialog
                .session()
                .and_then(|session| session.values.get("message"))
                .is_some_and(|value| value.contains('\n'))
        );

        render_prompt_test_frame(
            &ctx,
            &mut app,
            vec![key_press(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(app.snippet_prompt_dialog.session().is_none());
        assert!(app.action_sheet.is_open());
        assert_eq!(app.query, "keep this query");
        assert!(app.test_activation_trace.is_empty());
    }

    #[test]
    fn snippet_prompt_ctrl_enter_submits_once_and_preview_consumes_without_copy() {
        let ctx = egui::Context::default();
        let (mut app, _test_directory) = new_prompt_app(&ctx);
        app.test_skip_history_persistence = true;
        let entry = prompted_test_entry();
        request_test_prompt(&mut app, entry.clone());
        app.snippet_prompt_dialog.set_value("name", "Ada".into());
        app.results = vec![Action {
            label: "Help".into(),
            desc: "Command".into(),
            action: "help:show".into(),
            args: None,
        }];
        assert!(app.open_action_sheet_for_index(0));

        let mut copied = Vec::new();
        let mut submit_result = None;
        let _ = ctx.run(
            prompt_raw_input(vec![key_press(egui::Key::Enter, egui::Modifiers::CTRL)]),
            |ctx| {
                assert!(app.route_snippet_prompt_keyboard_with(ctx, |app| {
                    submit_result = Some(app.submit_snippet_prompt_with_test_backends(
                        |alias| (alias == entry.alias).then(|| entry.clone()).ok_or(()),
                        |text| {
                            copied.push(text.to_owned());
                            Ok(())
                        },
                    ));
                }));
                assert!(!ctx.input(|input| input.key_pressed(egui::Key::Enter)));
            },
        );
        assert_eq!(copied, ["Hello Ada\n"]);
        assert!(submit_result.unwrap().is_ok());
        assert!(app.snippet_prompt_dialog.session().is_none());
        assert!(app.action_sheet.is_open());

        let (mut preview_app, _preview_directory) = new_prompt_app(&ctx);
        preview_app.test_skip_history_persistence = true;
        preview_app
            .snippet_prompt_dialog
            .begin_preview(entry)
            .unwrap();
        preview_app.focus_panel(super::super::Panel::SnippetPromptDialog);
        let mut submissions = 0;
        let _ = ctx.run(
            prompt_raw_input(vec![key_press(egui::Key::Enter, egui::Modifiers::CTRL)]),
            |ctx| {
                assert!(preview_app.route_snippet_prompt_keyboard_with(ctx, |_| {
                    submissions += 1;
                }));
                assert!(!ctx.input(|input| input.key_pressed(egui::Key::Enter)));
            },
        );
        assert_eq!(submissions, 0);
        assert!(preview_app.snippet_prompt_dialog.is_open());
        assert!(preview_app.snippet_prompt_dialog.is_preview_only());
    }

    #[test]
    fn many_prompt_fields_and_feedback_keep_keyboard_and_footer_reachable_on_small_root() {
        let ctx = egui::Context::default();
        let (mut app, _test_directory) = new_prompt_app(&ctx);
        app.test_skip_history_persistence = true;
        app.visible_flag.store(true, Ordering::SeqCst);
        app.last_visible = true;

        let fields = (0..12)
            .map(|index| {
                let mut field =
                    crate::plugins::snippets::SnippetFieldDefinition::new(format!("field_{index}"));
                field.label = format!(
                    "Long display label for the recipient or ticket field number {index} that wraps"
                );
                field.default_value = format!("value {index}");
                field
            })
            .collect::<Vec<_>>();
        let text = fields
            .iter()
            .map(|field| format!("{{{{{}}}}}", field.name))
            .collect::<Vec<_>>()
            .join("\n");
        let entry = crate::plugins::snippets::SnippetEntry {
            alias: "reply".into(),
            text,
            hide_contents: false,
            prompt_for_fields: true,
            fields,
        };
        request_test_prompt(&mut app, entry);
        let feedback = app
            .snippet_prompt_dialog
            .submit_with(|_| Err(()), |_| panic!("stale form must not copy"))
            .unwrap_err();
        assert_eq!(
            feedback,
            super::super::snippet_prompt_dialog::SnippetPromptError::StaleTemplate
        );

        let screen_size = egui::vec2(400.0, 220.0);
        render_prompt_test_frame_at_size(&ctx, &mut app, Vec::new(), screen_size);
        // egui learns a new window's outer size during its first frame. Assert
        // against the same settled bounds users get on the following frame.
        render_prompt_test_frame_at_size(&ctx, &mut app, Vec::new(), screen_size);
        let layout = app
            .snippet_prompt_dialog
            .last_layout
            .as_ref()
            .expect("the production prompt should report its rendered widgets");
        assert_eq!(layout.field_ids.len(), 12);
        assert!(
            layout.body_content_size.y > layout.body_rect.height(),
            "many fields should use the production body scroll area"
        );
        assert!(
            layout.window_rect.min.x >= 0.0
                && layout.window_rect.min.y >= 0.0
                && layout.window_rect.max.x <= screen_size.x
                && layout.window_rect.max.y <= screen_size.y,
            "prompt window should stay inside the 400x220 root: {:?}",
            layout.window_rect
        );
        let copy = layout.copy.expect("execute mode must render Copy");
        assert!(
            layout.window_rect.contains_rect(copy.rect),
            "Copy response {:?} extends past prompt window {:?}",
            copy.rect,
            layout.window_rect
        );
        assert!(layout.window_rect.contains_rect(layout.cancel.rect));
        assert!(layout.body_rect.max.y <= layout.cancel.rect.min.y);
        let field_ids = layout.field_ids.clone();
        assert!(ctx.memory(|memory| memory.has_focus(field_ids[0])));

        for index in 1..field_ids.len() {
            render_prompt_test_frame_at_size(
                &ctx,
                &mut app,
                vec![key_press(egui::Key::Tab, egui::Modifiers::NONE)],
                screen_size,
            );
            assert!(
                ctx.memory(|memory| memory.has_focus(field_ids[index])),
                "Tab should focus field {index}"
            );
        }

        // egui animates scroll_to_me over 100–300 ms. Advance the deterministic
        // test clock by one second so the final focused field reaches its target.
        let settled_time = ctx.input(|input| input.time + 1.0);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size)),
                focused: true,
                time: Some(settled_time),
                ..Default::default()
            },
            |ctx| app.render_root_frame(ctx, None),
        );
        let layout = app.snippet_prompt_dialog.last_layout.as_ref().unwrap();
        assert!(
            layout.body_scroll_offset.y > 0.0,
            "focusing later fields should scroll the production form; offset={:?}, body={:?}, content={:?}, focused={:?}",
            layout.body_scroll_offset,
            layout.body_rect,
            layout.body_content_size,
            ctx.memory(|memory| field_ids.iter().position(|id| memory.has_focus(*id)))
        );
        render_prompt_test_frame_at_size(
            &ctx,
            &mut app,
            vec![key_press(egui::Key::Tab, egui::Modifiers::NONE)],
            screen_size,
        );
        let layout = app.snippet_prompt_dialog.last_layout.as_ref().unwrap();
        let copy = layout.copy.expect("Copy remains rendered after scrolling");
        assert!(ctx.memory(|memory| memory.has_focus(copy.id)));
        assert!(layout.window_rect.contains_rect(copy.rect));
        assert!(layout.window_rect.contains_rect(layout.cancel.rect));

        render_prompt_test_frame_at_size(
            &ctx,
            &mut app,
            vec![key_press(egui::Key::Tab, egui::Modifiers::NONE)],
            screen_size,
        );
        let layout = app.snippet_prompt_dialog.last_layout.as_ref().unwrap();
        assert!(ctx.memory(|memory| memory.has_focus(layout.cancel.id)));
        assert!(layout.window_rect.contains_rect(layout.cancel.rect));
    }

    fn focus_launcher_query(ctx: &egui::Context, app: &mut LauncherApp) {
        let query_id = egui::Id::new("query_input");
        ctx.begin_frame(egui::RawInput::default());
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(egui::TextEdit::singleline(&mut app.query).id(query_id))
                .request_focus();
        });
        let _ = ctx.end_frame();
        assert!(ctx.memory(|memory| memory.has_focus(query_id)));
    }

    fn actual_query_input(focused: bool) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            focused,
            ..Default::default()
        }
    }

    fn prepare_actual_query_frame(ctx: &egui::Context, app: &mut LauncherApp) {
        app.visible_flag.store(true, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        app.last_visible = true;
        app.focus_query = false;
        app.follow_mouse = false;
        focus_launcher_query(ctx, app);
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None)
        });
    }

    #[test]
    fn l08_actual_root_note_observation_refuses_coexistence_even_with_fixture_editor_and_foreign_prompt()
     {
        use super::super::query_observation::{
            NoteCloseFixture, NoteCloseObservationRequest, NoteCloseObservationResponse,
            NoteCloseObservationStatus, QueryObservationMailbox,
        };
        for mode in [
            crate::settings::NoteViewMode::Edit,
            crate::settings::NoteViewMode::Preview,
        ] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut app = new_app(&ctx);
            prepare_actual_query_frame(&ctx, &mut app);
            app.root_window_bridge.set_identity_for_test(42);
            app.note_save_on_close = false;
            app.note_confirm_discard_unsaved_changes = true;
            let directory = tempfile::tempdir().unwrap();
            let base = directory.path().join("observation");
            app.radial_query_observation =
                QueryObservationMailbox::isolated_note_close_test(base.clone());
            let make = |slug: &str, title: &str| {
                crate::gui::note_panel::NotePanel::from_note(crate::plugins::note::Note {
                    title: title.into(),
                    path: std::path::PathBuf::new(),
                    content: "# radial acceptance q11".into(),
                    tags: Vec::new(),
                    links: Vec::new(),
                    slug: slug.into(),
                    alias: None,
                    aliases: Vec::new(),
                    entity_refs: Vec::new(),
                })
            };
            let mut fixture = make("radial-acceptance-q11", "Q11 fixture");
            fixture.test_set_view_mode(mode);
            let mut foreign = make("other-note", "Other open note");
            foreign.test_set_view_mode(crate::settings::NoteViewMode::Edit);
            foreign.replace_content_from_mutation(
                "# radial acceptance q11\nother unsaved note".into(),
                1.0,
            );
            foreign.request_close(&mut app);
            app.note_panels.extend([fixture, foreign]);
            app.update_panel_stack();
            for _ in 0..4 {
                let _ = ctx.run(actual_query_input(true), |root| {
                    app.render_root_frame(root, None)
                });
            }
            let query = app.query.clone();
            let history = app.query_history.acceptance_digest();
            let focus = ctx.memory(|memory| memory.focused());
            let request = NoteCloseObservationRequest {
                schema_version: 1,
                fixture: NoteCloseFixture::Q11,
                request_id: 1,
                run_nonce: [71, 73],
                expected_hwnd: 42,
                expected_pid: std::process::id(),
                expected_generation: Some(app.root_window_bridge.identity().1),
                after_frame_ordinal: 0,
            };
            std::fs::write(
                format!("{}.note-close.request.json", base.display()),
                serde_json::to_vec(&request).unwrap(),
            )
            .unwrap();
            let output = ctx.run(actual_query_input(true), |root| {
                app.render_root_frame(root, None)
            });
            let bytes =
                std::fs::read(format!("{}.note-close.response.json", base.display())).unwrap();
            assert!(bytes.len() <= 64 * 1024);
            let response: NoteCloseObservationResponse = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(response.status, NoteCloseObservationStatus::Captured);
            assert_eq!(response.run_nonce, request.run_nonce);
            assert_eq!(
                response.root.as_ref().unwrap().process_id,
                std::process::id()
            );
            assert_eq!(response.root.as_ref().unwrap().hwnd, 42);
            assert!(response.observed_frame_ordinal > 0);
            let snapshot = response.snapshot.unwrap();
            assert_eq!(snapshot.client_size, [900, 650]);
            assert_eq!(snapshot.open_note_count, 2);
            assert!(
                snapshot.sole_note.is_none(),
                "an actual foreign prompt cannot be associated by fixture presence"
            );
            assert!(
                output
                    .platform_output
                    .accesskit_update
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, node)| node.role() == egui::accesskit::Role::Button
                        && node.name() == Some("Discard Changes"))
            );
            assert_eq!(app.note_panels.len(), 2);
            assert!(!app.note_panels[0].has_unsaved_changes());
            assert!(app.note_panels[1].has_unsaved_changes());
            assert!(app.note_panels[1].note_close_observation().pending_discard);
            assert_eq!(
                app.note_panels[1].note_content(),
                "# radial acceptance q11\nother unsaved note"
            );
            assert_eq!(app.query, query);
            assert_eq!(app.query_history.acceptance_digest(), history);
            assert_eq!(ctx.memory(|memory| memory.focused()), focus);
        }
    }

    #[test]
    fn l08_actual_root_note_observation_publishes_current_sole_fixture_discard_response_without_policy_mutation()
     {
        use super::super::query_observation::{
            NoteCloseFixture, NoteCloseObservationRequest, NoteCloseObservationResponse,
            NoteCloseObservationStatus, QueryObservationMailbox,
        };
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = new_app(&ctx);
        prepare_actual_query_frame(&ctx, &mut app);
        app.root_window_bridge.set_identity_for_test(42);
        app.note_save_on_close = false;
        app.note_confirm_discard_unsaved_changes = true;
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("observation");
        app.radial_query_observation =
            QueryObservationMailbox::isolated_note_close_test(base.clone());
        let mut fixture =
            crate::gui::note_panel::NotePanel::from_note(crate::plugins::note::Note {
                title: "Q11 fixture".into(),
                path: std::path::PathBuf::new(),
                content: "# radial acceptance q11".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "radial-acceptance-q11".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            });
        fixture.test_set_view_mode(crate::settings::NoteViewMode::Edit);
        fixture.replace_content_from_mutation(
            "# radial acceptance q11\nowned unsaved text".into(),
            1.0,
        );
        fixture.request_close(&mut app);
        app.note_panels.push(fixture);
        app.update_panel_stack();
        for _ in 0..4 {
            let _ = ctx.run(actual_query_input(true), |root| {
                app.render_root_frame(root, None)
            });
        }
        let before = app.note_panels[0].note_close_observation();
        let query = app.query.clone();
        let history = app.query_history.acceptance_digest();
        let mut previous_frame = 0;
        let (_, generation) = app.root_window_bridge.identity();
        for request_id in [1, 2] {
            let request = NoteCloseObservationRequest {
                schema_version: 1,
                fixture: NoteCloseFixture::Q11,
                request_id,
                run_nonce: [71, 73],
                expected_hwnd: 42,
                expected_pid: std::process::id(),
                expected_generation: Some(generation),
                after_frame_ordinal: previous_frame,
            };
            std::fs::write(
                format!("{}.note-close.request.json", base.display()),
                serde_json::to_vec(&request).unwrap(),
            )
            .unwrap();
            let output = ctx.run(actual_query_input(true), |root| {
                app.render_root_frame(root, None)
            });
            let response: NoteCloseObservationResponse = serde_json::from_slice(
                &std::fs::read(format!("{}.note-close.response.json", base.display())).unwrap(),
            )
            .unwrap();
            assert_eq!(response.status, NoteCloseObservationStatus::Captured);
            assert_eq!(response.request_id, request_id);
            assert_eq!(response.run_nonce, request.run_nonce);
            let root = response.root.unwrap();
            assert_eq!(
                (root.hwnd, root.process_id, root.generation),
                (42, std::process::id(), generation)
            );
            assert!(response.observed_frame_ordinal > previous_frame);
            previous_frame = response.observed_frame_ordinal;
            let snapshot = response.snapshot.unwrap();
            assert_eq!(snapshot.open_note_count, 1);
            let sole = snapshot.sole_note.unwrap();
            assert!(sole.fixture_slug && sole.fixture_marker && sole.pending_discard);
            assert_eq!(sole.slug_digest, before.slug_digest);
            let widget = sole.rendered_discard.unwrap();
            assert!(widget.enabled && widget.visible && widget.fully_visible);
            assert_eq!(widget.owner_slug_digest, sole.slug_digest);
            let update = output.platform_output.accesskit_update.unwrap();
            let (_, node) = update
                .nodes
                .iter()
                .find(|(id, _)| *id == egui::accesskit::NodeId(widget.widget_id))
                .unwrap();
            assert_eq!(node.role(), egui::accesskit::Role::Button);
            assert_eq!(node.name(), Some("Discard Changes"));
            let bounds = node.bounds().unwrap();
            assert_eq!(
                widget.bounds,
                [
                    bounds.x0.floor() as i32,
                    bounds.y0.floor() as i32,
                    bounds.x1.ceil() as i32,
                    bounds.y1.ceil() as i32
                ]
            );
            assert_eq!(app.note_panels[0].note_close_observation(), before);
            assert!(app.note_panels[0].open && app.note_panels[0].has_unsaved_changes());
            assert_eq!(
                app.note_panels[0].note_content(),
                "# radial acceptance q11\nowned unsaved text"
            );
            assert_eq!(app.query, query);
            assert_eq!(app.query_history.acceptance_digest(), history);
            assert!(!app.note_save_on_close && app.note_confirm_discard_unsaved_changes);
        }
    }

    #[test]
    fn l08_actual_root_note_focus_escape_and_discard_render_preserve_clean_policy() {
        for dirty in [false, true] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut app = new_app(&ctx);
            prepare_actual_query_frame(&ctx, &mut app);
            app.note_save_on_close = false;
            app.note_confirm_discard_unsaved_changes = true;
            let mut panel =
                crate::gui::note_panel::NotePanel::from_note(crate::plugins::note::Note {
                    title: "L08 owned note".into(),
                    path: std::path::PathBuf::new(),
                    content: "# radial acceptance q11".into(),
                    tags: Vec::new(),
                    links: Vec::new(),
                    slug: "l08-owned-note".into(),
                    alias: None,
                    aliases: Vec::new(),
                    entity_refs: Vec::new(),
                });
            panel.test_set_view_mode(crate::settings::NoteViewMode::Edit);
            if dirty {
                panel.replace_content_from_mutation(
                    "# radial acceptance q11\nunsaved owned edit".into(),
                    1.0,
                );
            }
            assert_eq!(panel.has_unsaved_changes(), dirty);
            app.note_panels.push(panel);
            app.update_panel_stack();
            let mut output = None;
            for _ in 0..4 {
                output = Some(ctx.run(actual_query_input(true), |root| {
                    app.render_root_frame(root, None)
                }));
            }
            let editor = ctx
                .memory(|memory| memory.focused())
                .expect("actual note editor acquired focus");
            assert_ne!(editor, egui::Id::new("query_input"));
            assert!(egui::TextEdit::load_state(&ctx, editor).is_some());
            let editor_node = egui::accesskit::NodeId(editor.value());
            let update = output
                .as_ref()
                .unwrap()
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap();
            assert!(update.nodes.iter().any(|(id, node)| *id == editor_node
                && node.role() == egui::accesskit::Role::MultilineTextInput));
            ctx.memory_mut(|memory| memory.surrender_focus(editor));
            assert!(!ctx.memory(|memory| memory.has_focus(editor)));
            // AccessKit queues SetFocus as an input request. Only the actual
            // next ROOT/TextEdit frame processes it; submission is not an ack.
            let mut focus_input = actual_query_input(true);
            focus_input.events.push(egui::Event::AccessKitActionRequest(
                egui::accesskit::ActionRequest {
                    action: egui::accesskit::Action::Focus,
                    target: editor_node,
                    data: None,
                },
            ));
            assert!(!ctx.memory(|memory| memory.has_focus(editor)));
            let _ = ctx.run(focus_input, |root| app.render_root_frame(root, None));
            assert!(ctx.memory(|memory| memory.has_focus(editor)));
            assert_eq!(app.note_panels.len(), 1);
            let mut escape = actual_query_input(true);
            for pressed in [true, false] {
                escape.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: Some(egui::Key::Escape),
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = ctx.run(escape, |root| app.render_root_frame(root, None));
            if dirty {
                assert_eq!(
                    app.note_panels.len(),
                    1,
                    "Escape requests the real confirmation without discarding"
                );
                assert!(app.note_panels[0].has_unsaved_changes());
                for _ in 0..3 {
                    output = ctx.run(actual_query_input(true), |root| {
                        app.render_root_frame(root, None)
                    });
                }
                let discard_point = |output: &egui::FullOutput| {
                    let update = output.platform_output.accesskit_update.as_ref().unwrap();
                    let matches = update
                        .nodes
                        .iter()
                        .filter(|(_, node)| {
                            node.role() == egui::accesskit::Role::Button
                                && node.name() == Some("Discard Changes")
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(matches.len(), 1);
                    let bounds = matches[0].1.bounds().unwrap();
                    assert!(bounds.x1 > bounds.x0 && bounds.y1 > bounds.y0);
                    egui::pos2(
                        ((bounds.x0 + bounds.x1) * 0.5) as f32,
                        ((bounds.y0 + bounds.y1) * 0.5) as f32,
                    )
                };
                let point = discard_point(&output);
                let mut input = actual_query_input(true);
                input.events.push(egui::Event::PointerMoved(point));
                output = ctx.run(input, |root| app.render_root_frame(root, None));
                for pressed in [true, false] {
                    let point = discard_point(&output);
                    let mut input = actual_query_input(true);
                    input.events.push(egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                    output = ctx.run(input, |root| app.render_root_frame(root, None));
                }
            }
            let _ = ctx.run(actual_query_input(true), |root| {
                app.render_root_frame(root, None)
            });
            assert!(app.note_panels.is_empty());
            assert!(!app.is_panel_open(Panel::NotePanel));
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(!app.note_save_on_close);
            assert!(app.note_confirm_discard_unsaved_changes);
            assert!(app.test_last_activation.is_none());
        }
    }

    fn set_query_cursor(ctx: &egui::Context, first: usize, last: usize) {
        let id = egui::Id::new("query_input");
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(first),
                egui::text::CCursor::new(last),
            )));
        state.store(ctx, id);
    }

    fn assert_query_cursor_end(ctx: &egui::Context, query: &str) {
        let range = egui::TextEdit::load_state(ctx, egui::Id::new("query_input"))
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(range.primary.index, query.chars().count());
        assert_eq!(range.secondary.index, query.chars().count());
    }

    #[test]
    fn actual_root_query_caret_moves_to_character_end_after_text_edit_stores_its_output() {
        for query in ["ascii query", "\u{03b2}\u{1f642}\u{00e9}", ""] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            app.query = query.into();
            prepare_actual_query_frame(&ctx, &mut app);
            set_query_cursor(&ctx, 0, query.chars().count().min(1));
            app.move_cursor_end = true;
            let revision = app.visibility_revision.current();
            let output = ctx.run(actual_query_input(true), |root| {
                app.render_root_frame(root, None)
            });
            assert!(!app.move_cursor_end);
            assert_query_cursor_end(&ctx, query);
            assert_eq!(app.query, query);
            assert_eq!(app.visibility_revision.current(), revision);
            assert!(
                !output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Focus))
            );
            // A consumed request must not override the user's next selection.
            set_query_cursor(&ctx, 0, 0);
            let _ = ctx.run(actual_query_input(true), |root| {
                app.render_root_frame(root, None)
            });
            assert_eq!(
                egui::TextEdit::load_state(&ctx, egui::Id::new("query_input"))
                    .unwrap()
                    .cursor
                    .char_range()
                    .unwrap()
                    .primary
                    .index,
                0
            );
        }
        assert!(!include_str!("../window_manager.rs").contains("pub fn send_end_key"));
        assert!(!include_str!("../window_manager.rs").contains("VK_END"));
    }

    #[test]
    fn actual_root_query_caret_request_survives_unfocused_and_action_sheet_disabled_frames() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "pending caret".into();
        prepare_actual_query_frame(&ctx, &mut app);
        set_query_cursor(&ctx, 0, 0);
        app.move_cursor_end = true;
        let _ = ctx.run(actual_query_input(false), |root| {
            app.render_root_frame(root, None)
        });
        assert!(app.move_cursor_end);
        assert_eq!(
            egui::TextEdit::load_state(&ctx, egui::Id::new("query_input"))
                .unwrap()
                .cursor
                .char_range()
                .unwrap()
                .primary
                .index,
            0
        );
        app.results = two_results();
        assert!(app.open_action_sheet_for_index(0));
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None)
        });
        assert!(app.action_sheet.is_open());
        assert!(app.move_cursor_end);
        app.close_action_sheet();
        focus_launcher_query(&ctx, &mut app);
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None)
        });
        assert!(!app.move_cursor_end);
        assert_query_cursor_end(&ctx, &app.query);
    }

    #[test]
    fn actual_root_history_and_autocomplete_mutations_after_render_keep_the_next_caret_request() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "draft".into();
        prepare_actual_query_frame(&ctx, &mut app);
        set_query_cursor(&ctx, 1, 1);
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None);
            app.navigate_query_history_with(QueryHistoryDirection::Older, || {
                vec!["recalled \u{03b2}".into()]
            });
        });
        assert_eq!(app.query, "recalled \u{03b2}");
        assert!(app.move_cursor_end && app.focus_query);
        assert_eq!(app.selected, None);
        assert_eq!(app.autocomplete_index, 0);
        assert_eq!(app.last_search_query, app.query);
        assert!(app.last_results_valid);
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None)
        });
        assert_query_cursor_end(&ctx, &app.query);
        assert!(!app.move_cursor_end);
        app.query_autocomplete = true;
        app.suggestions = vec!["completed \u{1f642}".into()];
        app.autocomplete_index = 0;
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None);
            assert!(app.accept_suggestion(true));
        });
        assert_eq!(app.query, "completed \u{1f642}");
        assert!(app.move_cursor_end);
        assert_eq!(app.last_search_query, app.query);
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None)
        });
        assert_query_cursor_end(&ctx, &app.query);
        assert!(!app.move_cursor_end);
    }

    #[test]
    fn actual_root_command_completion_after_render_moves_the_new_query_caret_on_the_next_frame() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "old".into();
        prepare_actual_query_frame(&ctx, &mut app);
        set_query_cursor(&ctx, 0, 1);
        let invocation = crate::commands::CommandInvocation {
            command: crate::commands::Command::VirtualDesktop(
                crate::commands::VirtualDesktopCommand::Create,
            ),
            original_action: two_results().remove(0),
            query_override: None,
            source: ActivationSource::Enter,
        };
        let revision = app.visibility_revision.current();
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None);
            app.apply_command_outcome(
                crate::commands::CommandOutcome {
                    query: crate::commands::QueryPolicy::Set("completion \u{03b2}\u{1f642}".into()),
                    focus: true,
                    move_cursor_end: true,
                    ..Default::default()
                },
                &invocation,
            );
        });
        assert_eq!(app.query, "completion \u{03b2}\u{1f642}");
        assert!(app.move_cursor_end && app.focus_query);
        assert_eq!(app.visibility_revision.current(), revision);
        assert!(app.test_recorded_history_queries.is_empty());
        let _ = ctx.run(actual_query_input(true), |root| {
            app.render_root_frame(root, None)
        });
        assert_query_cursor_end(&ctx, &app.query);
        assert!(!app.move_cursor_end);
        assert_eq!(app.visibility_revision.current(), revision);
        assert!(app.test_recorded_history_queries.is_empty());
    }

    struct FakeNumpadProbe {
        down: Option<PhysicalNumpadKey>,
        top_row_down: bool,
        calls: Cell<usize>,
    }

    impl NumpadKeyStateProbe for FakeNumpadProbe {
        fn state(&self, key: PhysicalNumpadKey) -> PhysicalDigitKeyState {
            self.calls.set(self.calls.get() + 1);
            PhysicalDigitKeyState {
                numpad_down: self.down == Some(key),
                top_row_down: self.top_row_down,
            }
        }
    }

    fn render_query_text_edit_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        request_focus: bool,
        probe: &impl NumpadKeyStateProbe,
    ) -> (bool, Vec<LauncherNumpadNavigation>) {
        let mut focused = false;
        let mut routed = Vec::new();
        egui::CentralPanel::default().show(ctx, |ui| {
            let input_id = egui::Id::new("numpad_routing_query_input");
            let query_owned_focus = ui.ctx().memory(|memory| memory.has_focus(input_id));
            routed = ui.ctx().input_mut(|input| {
                consume_physical_numpad_navigation(query_owned_focus, input, probe)
            });

            let response = ui.add(
                egui::TextEdit::singleline(&mut app.query)
                    .id(input_id)
                    .desired_width(f32::INFINITY),
            );
            if request_focus {
                response.request_focus();
            }
            focused = response.has_focus();
            for direction in routed.iter().copied() {
                app.handle_key(direction.navigation_key());
            }
        });
        (focused, routed)
    }

    fn run_two_frame_query_routing(
        key: egui::Key,
        digit: &str,
        down: Option<PhysicalNumpadKey>,
        top_row_down: bool,
        selected: usize,
    ) -> (LauncherApp, Vec<LauncherNumpadNavigation>, usize) {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "note".into();
        app.results = (0..8)
            .map(|index| Action {
                label: format!("Result {index}"),
                desc: "Test".into(),
                action: format!("test:{index}"),
                args: None,
            })
            .collect();
        app.resolved_grid_layout = true;
        app.query_results_layout.cols = 3;
        app.selected = Some(selected);
        let probe = FakeNumpadProbe {
            down,
            top_row_down,
            calls: Cell::new(0),
        };

        ctx.begin_frame(egui::RawInput::default());
        let (focused, routed) = render_query_text_edit_frame(&ctx, &mut app, true, &probe);
        assert!(focused);
        assert!(routed.is_empty());
        let _ = ctx.end_frame();

        let input_id = egui::Id::new("numpad_routing_query_input");
        ctx.data_mut(|data| {
            let state = data
                .get_persisted_mut_or_default::<egui::widgets::text_edit::TextEditState>(input_id);
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(app.query.chars().count()),
                )));
        });

        ctx.begin_frame(egui::RawInput {
            events: vec![
                key_press(key, egui::Modifiers::NONE),
                egui::Event::Text(digit.into()),
            ],
            ..Default::default()
        });
        let (focused, routed) = render_query_text_edit_frame(&ctx, &mut app, false, &probe);
        assert!(focused);
        let _ = ctx.end_frame();
        (app, routed, probe.calls.get())
    }

    #[test]
    fn focused_query_text_edit_routes_all_physical_numpad_directions_without_typing() {
        for (key, digit, physical, selected, expected_selected, expected_direction) in [
            (
                egui::Key::Num8,
                "8",
                PhysicalNumpadKey::Num8,
                4,
                1,
                LauncherNumpadNavigation::Up,
            ),
            (
                egui::Key::Num2,
                "2",
                PhysicalNumpadKey::Num2,
                1,
                4,
                LauncherNumpadNavigation::Down,
            ),
            (
                egui::Key::Num4,
                "4",
                PhysicalNumpadKey::Num4,
                4,
                3,
                LauncherNumpadNavigation::Left,
            ),
            (
                egui::Key::Num6,
                "6",
                PhysicalNumpadKey::Num6,
                4,
                5,
                LauncherNumpadNavigation::Right,
            ),
        ] {
            let (app, routed, calls) =
                run_two_frame_query_routing(key, digit, Some(physical), false, selected);
            assert_eq!(routed, vec![expected_direction]);
            assert_eq!(calls, 1);
            assert_eq!(app.query, "note");
            assert_eq!(app.selected, Some(expected_selected));
        }
    }

    #[test]
    fn focused_query_text_edit_preserves_all_top_row_navigation_digits_as_text() {
        for (key, digit) in [
            (egui::Key::Num8, "8"),
            (egui::Key::Num2, "2"),
            (egui::Key::Num4, "4"),
            (egui::Key::Num6, "6"),
        ] {
            let (app, routed, calls) = run_two_frame_query_routing(key, digit, None, false, 4);
            assert_eq!(app.query, format!("note{digit}"));
            assert_eq!(app.selected, Some(4));
            assert!(routed.is_empty());
            assert_eq!(calls, 1);
        }
    }

    #[test]
    fn focused_query_preserves_ambiguous_digit_when_top_row_and_keypad_are_both_down() {
        let (app, routed, calls) = run_two_frame_query_routing(
            egui::Key::Num8,
            "8",
            Some(PhysicalNumpadKey::Num8),
            true,
            4,
        );
        assert_eq!(app.query, "note8");
        assert_eq!(app.selected, Some(4));
        assert!(routed.is_empty());
        assert_eq!(calls, 1);
    }

    #[test]
    fn numpad_routing_leaves_query_history_shortcuts_available() {
        let ctx = egui::Context::default();
        ctx.begin_frame(egui::RawInput {
            events: vec![key_press(egui::Key::ArrowUp, egui::Modifiers::CTRL)],
            ..Default::default()
        });
        let probe = FakeNumpadProbe {
            down: Some(PhysicalNumpadKey::Num8),
            top_row_down: false,
            calls: Cell::new(0),
        };

        assert_eq!(
            ctx.input_mut(|input| consume_physical_numpad_navigation(true, input, &probe)),
            Vec::new()
        );
        assert_eq!(probe.calls.get(), 0);
        assert_eq!(
            ctx.input_mut(|input| LauncherApp::consume_query_history_shortcut(true, input)),
            Some(QueryHistoryDirection::Older)
        );
        let _ = ctx.end_frame();
    }

    #[test]
    fn query_history_shortcuts_use_event_modifiers_and_require_exact_ctrl_only_focus() {
        let route = |query_has_focus, frame_modifiers, events| {
            let ctx = egui::Context::default();
            ctx.begin_frame(egui::RawInput {
                modifiers: frame_modifiers,
                events,
                ..Default::default()
            });
            let direction = ctx.input_mut(|input| {
                LauncherApp::consume_query_history_shortcut(query_has_focus, input)
            });
            let arrows_remain = ctx.input(|input| {
                input.key_pressed(egui::Key::ArrowUp) || input.key_pressed(egui::Key::ArrowDown)
            });
            let _ = ctx.end_frame();
            (direction, arrows_remain)
        };

        assert_eq!(
            route(
                true,
                egui::Modifiers::SHIFT,
                vec![key_press(egui::Key::ArrowUp, egui::Modifiers::CTRL)],
            ),
            (Some(QueryHistoryDirection::Older), false)
        );
        assert_eq!(
            route(
                true,
                egui::Modifiers::NONE,
                vec![key_press(egui::Key::ArrowDown, egui::Modifiers::CTRL)],
            ),
            (Some(QueryHistoryDirection::Newer), false)
        );
        assert_eq!(
            route(
                true,
                egui::Modifiers::CTRL,
                vec![key_press(
                    egui::Key::ArrowUp,
                    egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                )],
            ),
            (None, true)
        );
        assert_eq!(
            route(
                false,
                egui::Modifiers::CTRL,
                vec![key_press(egui::Key::ArrowUp, egui::Modifiers::CTRL)],
            ),
            (None, true)
        );
        assert_eq!(
            route(
                true,
                egui::Modifiers::CTRL,
                vec![key_press(egui::Key::ArrowUp, egui::Modifiers::NONE)],
            ),
            (None, true)
        );
        assert_eq!(
            route(
                true,
                egui::Modifiers::NONE,
                vec![key_press(
                    egui::Key::ArrowUp,
                    egui::Modifiers {
                        command: true,
                        ..egui::Modifiers::CTRL
                    },
                )],
            ),
            (Some(QueryHistoryDirection::Older), false)
        );
        assert_eq!(
            route(
                true,
                egui::Modifiers::CTRL,
                vec![
                    key_press(
                        egui::Key::ArrowUp,
                        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                    ),
                    key_press(egui::Key::ArrowDown, egui::Modifiers::CTRL),
                ],
            ),
            (Some(QueryHistoryDirection::Newer), true)
        );
    }

    #[test]
    fn consumed_query_history_arrow_cannot_reach_result_navigation() {
        let ctx = egui::Context::default();
        ctx.begin_frame(egui::RawInput {
            events: vec![key_press(egui::Key::ArrowUp, egui::Modifiers::CTRL)],
            ..Default::default()
        });
        let mut app = new_app(&ctx);
        app.results = two_results();
        app.selected = Some(1);

        assert_eq!(
            ctx.input_mut(|input| LauncherApp::consume_query_history_shortcut(true, input)),
            Some(QueryHistoryDirection::Older)
        );
        if ctx.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
            app.handle_key(egui::Key::ArrowUp);
        }

        assert_eq!(app.selected, Some(1));
        let _ = ctx.end_frame();
    }

    #[test]
    fn query_history_recall_reuses_search_selection_autocomplete_focus_and_cursor_paths() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "draft".into();
        app.results = two_results();
        app.selected = Some(1);
        app.suggestions = vec!["stale suggestion".into()];
        app.autocomplete_index = 3;

        app.navigate_query_history_with(QueryHistoryDirection::Older, || {
            vec!["Recalled Query".into()]
        });

        assert_eq!(app.query, "Recalled Query");
        assert_eq!(app.selected, None);
        assert_eq!(app.autocomplete_index, 0);
        assert!(app.suggestions.is_empty());
        assert_eq!(app.last_search_query, "Recalled Query");
        assert!(app.last_results_valid);
        assert!(app.focus_query);
        assert!(app.move_cursor_end);
    }

    #[test]
    fn query_history_note_search_recall_uses_existing_debounce() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "draft".into();
        app.last_search_query = "before recall".into();
        app.last_results_valid = true;

        app.navigate_query_history_with(QueryHistoryDirection::Older, || {
            vec!["note search project road map".into()]
        });

        assert_eq!(app.query, "note search project road map");
        assert_eq!(app.last_search_query, "before recall");
        assert!(app.last_note_search_change.is_some());
        assert!(app.focus_query);
        assert!(app.move_cursor_end);
    }

    #[test]
    fn query_history_action_activation_resets_snapshot_before_next_traversal() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "draft".into();
        app.navigate_query_history_with(QueryHistoryDirection::Older, || {
            vec!["old snapshot".into()]
        });

        app.activate_action(
            Action {
                label: "Invalid".into(),
                desc: "Test".into(),
                action: "mkmacro:future".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );
        assert_eq!(app.query, "old snapshot");
        app.navigate_query_history_with(QueryHistoryDirection::Older, || {
            vec!["newly recorded".into()]
        });

        assert_eq!(app.query, "newly recorded");
    }

    #[test]
    fn shutdown_uses_committed_multi_manager_policy_after_settings_corruption() {
        let directory = tempfile::tempdir().unwrap();
        let settings_path = directory.path().join("settings.json");
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.settings_path = settings_path.to_string_lossy().into_owned();
        app.multi_manager_settings.save_on_exit = false;
        std::fs::write(&settings_path, "corrupt after startup").unwrap();

        assert!(!app.multi_manager_save_on_exit());
    }

    struct FakePlugin;

    impl Plugin for FakePlugin {
        fn search(&self, query: &str) -> Vec<Action> {
            let action = |label: &str, action: &str| Action {
                label: label.into(),
                desc: "Fake".into(),
                action: action.into(),
                args: None,
            };
            match query {
                "one" => vec![action("Help", "help:show")],
                "rewrite" => vec![action("Rewrite", "query:rewritten")],
                "recursive" => vec![action("Recursive", "queryexec:one")],
                "destroy" => vec![action("Clear history", "history:clear")],
                "delete alpha" => vec![action("Delete alpha", "note:remove:alpha")],
                "bookmarks" => vec![action("Bookmarks", "query:bm list")],
                "bm list" => vec![action("Example", "https://example.test")],
                "modify clipboard" => vec![action(
                    "Modify clipboard",
                    "clipboard_modify:open:templates",
                )],
                "open help" => vec![action("Help", "help:show")],
                "many" => vec![action("First", "help:show"), action("Second", "help:show")],
                "open alpha" => vec![Action {
                    label: "alpha".into(),
                    desc: "Note".into(),
                    action: "note:open:alpha".into(),
                    args: None,
                }],
                "dynamic universal" => vec![Action {
                    label: "Dynamic Universal".into(),
                    desc: "Third-party result".into(),
                    action: "third_party:opaque:command".into(),
                    args: Some("payload".into()),
                }],
                "note list" => ["alpha", "beta", "gamma"]
                    .into_iter()
                    .map(|slug| Action {
                        label: slug.into(),
                        desc: "Note".into(),
                        action: format!("note:open:{slug}"),
                        args: None,
                    })
                    .collect(),
                _ => Vec::new(),
            }
        }

        fn name(&self) -> &str {
            "fake"
        }
        fn description(&self) -> &str {
            "deterministic launcher test plugin"
        }
        fn capabilities(&self) -> &[&str] {
            &[]
        }
        fn always_search(&self) -> bool {
            true
        }
    }

    fn app_with_fake(ctx: &egui::Context) -> LauncherApp {
        let mut app = new_app(ctx);
        app.plugins.register(Box::new(FakePlugin));
        app
    }

    fn query_request(id: u64, query: &str) -> crate::mkmacro::LauncherCommandRequest {
        crate::mkmacro::LauncherCommandRequest {
            id,
            kind: crate::mkmacro::LauncherCommandKind::Query(query.into()),
        }
    }

    #[test]
    fn unmatched_launcher_queries_are_not_executable_commands() {
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);

        let response =
            app.handle_macro_launcher_command(&query_request(1, "definitely-missing-command-xyz"));

        assert_eq!(response, LauncherCommandResponse::NoResults);
        assert!(app.results.is_empty());
        assert_eq!(app.query, "definitely-missing-command-xyz");
        assert_eq!(
            app.test_last_activation, None,
            "an unmatched query must not reach activate_action or its process-launch path"
        );
    }

    #[test]
    fn single_macro_query_result_activates_normal_note_ui_without_process_launch() {
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);

        let response = app.handle_macro_launcher_command(&query_request(2, "open alpha"));

        assert_eq!(response, LauncherCommandResponse::Activated);
        assert_eq!(
            app.test_last_activation
                .as_ref()
                .map(|(action, source)| (action.action.as_str(), *source)),
            Some(("note:open:alpha", ActivationSource::Macro))
        );
        assert_eq!(
            app.note_panels.len(),
            1,
            "the normal note action opened a note panel"
        );
        assert!(
            app.is_panel_open(Panel::NotePanel),
            "normal note activation must update the Launcher panel state"
        );
    }

    #[test]
    fn ambiguous_macro_query_preserves_order_and_presents_launcher_for_selection() {
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        app.query = "different value".into();

        let response = app.handle_macro_launcher_command(&query_request(3, "note list"));

        assert_eq!(
            response,
            LauncherCommandResponse::PresentedForSelection { result_count: 3 }
        );
        assert_eq!(app.test_last_activation, None);
        assert_eq!(app.query, "note list");
        assert_eq!(
            app.results
                .iter()
                .map(|result| result.label.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta", "gamma"]
        );
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
        assert!(app.focus_query);
        assert!(
            LauncherApp::launcher_query_focus_should_be_requested(false, app.focus_query, false),
            "the next rendered frame must request search-input focus"
        );
    }

    #[test]
    fn macro_query_result_count_boundary_only_activates_exactly_one() {
        let ctx = egui::Context::default();
        for (query, expected_count, expected_response) in [
            (
                "definitely-missing-command-xyz",
                0,
                LauncherCommandResponse::NoResults,
            ),
            ("open alpha", 1, LauncherCommandResponse::Activated),
            (
                "note list",
                3,
                LauncherCommandResponse::PresentedForSelection { result_count: 3 },
            ),
        ] {
            let mut app = app_with_fake(&ctx);
            assert_eq!(
                app.handle_macro_launcher_command(&query_request(10, query)),
                expected_response
            );
            assert_eq!(app.results.len(), expected_count);
            assert_eq!(app.test_last_activation.is_some(), expected_count == 1);
        }
    }

    #[test]
    fn macro_launcher_resolution_uses_search_and_normal_activation() {
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);

        for (query, expected) in [
            ("missing raw query", LauncherCommandResponse::NoResults),
            ("one", LauncherCommandResponse::Activated),
        ] {
            let response =
                app.handle_macro_launcher_command(&crate::mkmacro::LauncherCommandRequest {
                    id: 1,
                    kind: crate::mkmacro::LauncherCommandKind::Query(query.into()),
                });
            assert_eq!(response, expected);
            assert_eq!(app.query, query);
        }
        assert!(
            app.help_window.open,
            "the one result used normal activation"
        );

        app.help_window.open = false;
        let response = app.handle_macro_launcher_command(&crate::mkmacro::LauncherCommandRequest {
            id: 2,
            kind: crate::mkmacro::LauncherCommandKind::Query("rewrite".into()),
        });
        assert_eq!(response, LauncherCommandResponse::Activated);
        assert_eq!(app.query, "rewritten");

        let response = app.handle_macro_launcher_command(&crate::mkmacro::LauncherCommandRequest {
            id: 3,
            kind: crate::mkmacro::LauncherCommandKind::Query("recursive".into()),
        });
        assert_eq!(response, LauncherCommandResponse::Activated);
        assert_eq!(app.query, "one");
        assert!(
            app.help_window.open,
            "queryexec recursively activated its result"
        );
    }

    #[test]
    fn migrated_legacy_action_activates_gui_owned_dialog_as_macro() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let response = app.handle_macro_launcher_command(&crate::mkmacro::LauncherCommandRequest {
            id: 7,
            kind: crate::mkmacro::LauncherCommandKind::ResolvedLegacy(Action {
                label: "Help".into(),
                desc: "GUI dialog".into(),
                action: "help:show".into(),
                args: None,
            }),
        });
        assert_eq!(response, LauncherCommandResponse::Activated);
        assert_eq!(
            app.test_last_activation
                .as_ref()
                .map(|(action, source)| (action.action.as_str(), *source)),
            Some(("help:show", ActivationSource::Macro)),
            "ResolvedLegacy dispatch must call activate_action"
        );
        assert!(
            app.help_window.open,
            "GUI-owned dialog was opened during dispatch"
        );

        app.require_confirm_destructive = true;
        let response = app.handle_macro_launcher_command(&crate::mkmacro::LauncherCommandRequest {
            id: 8,
            kind: crate::mkmacro::LauncherCommandKind::ResolvedLegacy(Action {
                label: "Clear".into(),
                desc: String::new(),
                action: "history:clear".into(),
                args: None,
            }),
        });
        assert_eq!(response, LauncherCommandResponse::Activated);
        assert_eq!(
            app.pending_confirm
                .as_ref()
                .map(|pending| pending.invocation.source),
            Some(ActivationSource::Macro)
        );
    }

    #[test]
    fn macro_launcher_destructive_and_ambiguous_results_remain_interactive() {
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);
        app.require_confirm_destructive = true;

        let response = app.handle_macro_launcher_command(&crate::mkmacro::LauncherCommandRequest {
            id: 1,
            kind: crate::mkmacro::LauncherCommandKind::Query("destroy".into()),
        });
        assert_eq!(response, LauncherCommandResponse::Activated);
        assert_eq!(
            app.pending_confirm
                .as_ref()
                .map(|pending| pending.invocation.source),
            Some(ActivationSource::Macro)
        );

        app.selected = Some(1);
        let response = app.handle_macro_launcher_command(&crate::mkmacro::LauncherCommandRequest {
            id: 2,
            kind: crate::mkmacro::LauncherCommandKind::Query("many".into()),
        });
        assert_eq!(
            response,
            LauncherCommandResponse::PresentedForSelection { result_count: 2 }
        );
        assert_eq!(app.query, "many");
        assert_eq!(app.results.len(), 2);
        assert_eq!(app.selected, None);
        assert!(app.move_cursor_end && app.focus_query);
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
        assert!(
            !app.help_window.open,
            "ambiguous results were not activated"
        );
    }

    #[test]
    fn single_macro_destructive_result_uses_confirmation_before_exactly_one_execution() {
        let _lock = MACRO_ACTIVATION_TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);
        app.require_confirm_destructive = true;
        let executions = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&executions);
        set_execute_action_hook(Some(Box::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })));

        assert_eq!(
            app.handle_macro_launcher_command(&query_request(20, "destroy")),
            LauncherCommandResponse::Activated
        );
        assert!(app.confirm_modal.is_open());
        assert_eq!(app.confirm_modal.source_copy(), Some("Triggered by macro"));
        assert_eq!(
            app.pending_confirm
                .as_ref()
                .map(|pending| pending.invocation.source),
            Some(ActivationSource::Macro)
        );
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert_eq!(app.test_activation_trace.len(), 1);
        app.resolve_pending_confirmation(true);
        assert_eq!(executions.load(Ordering::SeqCst), 1);

        assert_eq!(
            app.handle_macro_launcher_command(&query_request(21, "destroy")),
            LauncherCommandResponse::Activated
        );
        app.resolve_pending_confirmation(false);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        set_execute_action_hook(None);
    }

    #[test]
    fn single_macro_query_action_runs_follow_up_search_without_literal_launch() {
        let _lock = MACRO_ACTIVATION_TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);
        let launches = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&launches);
        set_execute_action_hook(Some(Box::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })));

        assert_eq!(
            app.handle_macro_launcher_command(&query_request(22, "bookmarks")),
            LauncherCommandResponse::Activated
        );
        assert_eq!(app.query, "bm list");
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.results[0].action, "https://example.test");
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(launches.load(Ordering::SeqCst), 0);
        assert_eq!(
            app.test_activation_trace
                .iter()
                .map(|(action, source)| (action.action.as_str(), *source))
                .collect::<Vec<_>>(),
            [("query:bm list", ActivationSource::Macro)]
        );
        set_execute_action_hook(None);
    }

    #[test]
    fn single_macro_clipboard_modify_result_uses_coordinator_not_launcher() {
        let _lock = MACRO_ACTIVATION_TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);
        let launches = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&launches);
        set_execute_action_hook(Some(Box::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })));

        assert_eq!(
            app.handle_macro_launcher_command(&query_request(23, "modify clipboard")),
            LauncherCommandResponse::Activated
        );
        assert!(app.clipboard_modify_dialog.open);
        assert_eq!(
            app.clipboard_modify_dialog.section,
            ClipboardModifyDialogSection::Templates
        );
        assert_eq!(launches.load(Ordering::SeqCst), 0);
        assert_eq!(
            app.test_activation_trace
                .last()
                .map(|(action, source)| (action.action.as_str(), *source)),
            Some(("clipboard_modify:open:templates", ActivationSource::Macro))
        );
        set_execute_action_hook(None);
    }

    #[test]
    fn single_macro_panel_result_restores_hidden_launcher_and_focus_without_launch() {
        let _lock = MACRO_ACTIVATION_TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);
        app.visible_flag.store(false, Ordering::SeqCst);
        app.restore_flag.store(false, Ordering::SeqCst);
        let launches = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&launches);
        set_execute_action_hook(Some(Box::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })));

        assert_eq!(
            app.handle_macro_launcher_command(&query_request(24, "open help")),
            LauncherCommandResponse::Activated
        );
        assert!(app.help_window.open);
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.restore_flag.load(Ordering::SeqCst));
        assert_eq!(launches.load(Ordering::SeqCst), 0);
        assert_eq!(
            app.test_activation_trace
                .last()
                .map(|(action, source)| (action.action.as_str(), *source)),
            Some(("help:show", ActivationSource::Macro))
        );
        set_execute_action_hook(None);
    }

    fn wait_for_repaint(wakes: &AtomicUsize) {
        for _ in 0..100 {
            if wakes.load(Ordering::SeqCst) != 0 {
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("worker submission did not wake egui");
    }

    #[test]
    fn macro_launcher_poll_consumes_one_request_and_mutates_query_on_gui_dispatch() {
        let ctx = egui::Context::default();
        let broker = Arc::new(LauncherCommandBroker::default());
        let mut app = new_app(&ctx);
        app.command_cache = two_results();
        app.query = "before polling".into();

        // Consume repaint requests made while constructing the app. An immediate
        // egui repaint intentionally remains outstanding for another frame, so
        // one frame is not sufficient to put the context back to sleep. If a
        // repaint is already pending, another `request_repaint` is coalesced and
        // does not invoke the integration callback.
        for _ in 0..8 {
            let _ = ctx.run(egui::RawInput::default(), |_| {});
            if !ctx.has_requested_repaint() {
                break;
            }
        }
        assert!(!ctx.has_requested_repaint());
        let wakes = Arc::new(AtomicUsize::new(0));
        let callback_wakes = Arc::clone(&wakes);
        ctx.set_request_repaint_callback(move |_| {
            callback_wakes.fetch_add(1, Ordering::SeqCst);
        });

        // The empty poll installs the wakeup callback but changes no GUI state.
        app.poll_macro_launcher_commands_from(&ctx, &broker);
        assert_eq!(app.query, "before polling");

        let submit_broker = Arc::clone(&broker);
        let submitter =
            thread::spawn(move || submit_broker.submit_query("", &RunControl::default()));
        wait_for_repaint(&wakes);
        assert_eq!(app.query, "before polling");

        app.poll_macro_launcher_commands_from(&ctx, &broker);
        assert_eq!(app.query, "");
        assert!(broker.take_pending().is_none());
        assert_eq!(
            submitter.join().unwrap().unwrap(),
            LauncherCommandResponse::PresentedForSelection { result_count: 2 }
        );

        // A second poll cannot respond to, or otherwise process, the request again.
        app.poll_macro_launcher_commands_from(&ctx, &broker);
        assert_eq!(app.query, "");
    }

    #[test]
    fn empty_macro_launcher_broker_is_a_no_op() {
        let ctx = egui::Context::default();
        let broker = LauncherCommandBroker::default();
        let mut app = new_app(&ctx);
        app.query = "unchanged".into();

        app.poll_macro_launcher_commands_from(&ctx, &broker);

        assert_eq!(app.query, "unchanged");
        assert!(app.results.is_empty());
    }

    #[test]
    fn disconnected_macro_launcher_waiter_does_not_panic_gui_dispatch() {
        let ctx = egui::Context::default();
        let broker = Arc::new(LauncherCommandBroker::default());
        let control = Arc::new(RunControl::default());
        let submit_broker = Arc::clone(&broker);
        let submit_control = Arc::clone(&control);
        let submitter =
            thread::spawn(move || submit_broker.submit_query("missing", &submit_control));
        let pending = loop {
            if let Some(pending) = broker.take_pending() {
                break pending;
            }
            thread::yield_now();
        };
        control.stop();
        assert!(submitter.join().unwrap().is_err());

        let mut app = new_app(&ctx);
        let response = app.handle_macro_launcher_command(&pending.request);
        assert_eq!(response, LauncherCommandResponse::NoResults);
        assert!(!pending.respond(response));
    }

    #[test]
    fn d08_main_context_menu_disables_ephemeral_add_and_clicks_typed_live_query_alternative() {
        use crate::gui::universal_action_catalog::{
            UniversalActionAuthoringCatalog, UniversalActionCatalogSnapshot, retained_window_target,
        };
        use crate::radial::context::{InvocationContext, WindowIdentity};
        use crate::universal_actions::{
            ActionResolutionContext, ActionSurface, UniversalActionRegistry, action_ids,
        };
        let context = egui::Context::default();
        context.enable_accesskit();
        begin_d08_response_observation(&context);
        let app = new_app(&context);
        let before_usage = app.usage.clone();
        let before_activation = app.test_activation_trace.clone();
        let before_history =
            serde_json::to_vec(&crate::history::with_history(Clone::clone).unwrap()).unwrap();
        let query = "D08 live window";
        let window = WindowIdentity {
            hwnd: 91,
            pid: 7,
            title: query.into(),
            process_name: Some("editor.exe".into()),
            process_path: Some("C:\\Apps\\editor.exe".into()),
            class_name: Some("EditorWindow".into()),
        };
        let target = retained_window_target(&window);
        let catalog = UniversalActionCatalogSnapshot {
            entries: vec![target.clone()],
            recent_entries: Vec::new(),
            dashboard: Arc::new(crate::dashboard::DashboardDataSnapshot::default()),
        };
        let authoring =
            UniversalActionAuthoringCatalog::build(&catalog, &InvocationContext::empty(1), query);
        let primary = authoring
            .rows()
            .iter()
            .find(|row| {
                row.action_id == action_ids::WINDOW_ACTIVATE
                    && row.target_command == "window:switch:91"
            })
            .unwrap()
            .clone();
        let reason = primary.assignment().unwrap_err().reason;
        assert!(primary.binding.is_none());
        let actions = UniversalActionRegistry.resolve(
            &target,
            &ActionResolutionContext::new(ActionSurface::ContextMenu, query),
        );
        let frame = |events: Vec<egui::Event>| {
            let mut choice = None;
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        choice = render_universal_context_menu(
                            ui,
                            &target,
                            &actions,
                            query,
                            Ok(primary.clone()),
                        );
                    });
                },
            );
            (output, choice)
        };
        let button = |output: &egui::FullOutput, prefix: &str, control: D08Control| {
            let update = output.platform_output.accesskit_update.as_ref().unwrap();
            let (node_id, node) = update
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == egui::accesskit::Role::Button
                        && node.name().is_some_and(|name| name.starts_with(prefix))
                })
                .unwrap();
            let b = node.bounds().unwrap();
            (
                egui::pos2(((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32),
                !d08_response_enabled(&context, control, *node_id, b),
            )
        };
        let pointer = |point, pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let (output, choice) = frame(Vec::new());
        assert!(choice.is_none());
        let (point, disabled) = button(&output, "Add to radial", D08Control::AddMenu);
        assert!(!disabled);
        for event in [
            egui::Event::PointerMoved(point),
            pointer(point, true),
            pointer(point, false),
        ] {
            let (_, choice) = frame(vec![event]);
            assert!(choice.is_none());
        }
        let mut latest = frame(Vec::new()).0;
        assert!(format!("{:?}", latest.shapes).contains(&reason));
        let (point, disabled) = button(&latest, "Unavailable:", D08Control::AddAction);
        assert!(disabled);
        for event in [
            egui::Event::PointerMoved(point),
            pointer(point, true),
            pointer(point, false),
        ] {
            let (output, choice) = frame(vec![event]);
            assert!(choice.is_none(), "disabled Add emitted {choice:?}");
            latest = output;
        }
        let (point, disabled) = button(&latest, "Save live query", D08Control::SaveLiveQuery);
        assert!(!disabled);
        let mut choices = Vec::new();
        for event in [
            egui::Event::PointerMoved(point),
            pointer(point, true),
            pointer(point, false),
        ] {
            let (_, choice) = frame(vec![event]);
            choices.extend(choice);
        }
        assert_eq!(choices.len(), 1);
        let ContextMenuChoice::AddToRadial {
            binding,
            label,
            source_query,
            trace_identity,
        } = &choices[0]
        else {
            panic!("live query alternative emitted {:?}", choices[0]);
        };
        assert_eq!(
            binding,
            &crate::radial::model::ActionBinding::LauncherQuery {
                query: query.into(),
                mode: crate::radial::model::QueryRunMode::OpenLauncher
            }
        );
        assert_eq!(label, query);
        assert_eq!(source_query, query);
        assert_eq!(
            trace_identity.source_query_digest,
            acceptance_trace::private_trace_text_digest(query)
        );
        let json = serde_json::to_string(binding).unwrap();
        assert!(!json.contains("hwnd") && !json.contains("window:switch") && !json.contains("91"));
        assert_eq!(app.usage, before_usage);
        assert_eq!(app.test_activation_trace, before_activation);
        assert!(app.test_recorded_history_queries.is_empty());
        assert_eq!(
            serde_json::to_vec(&crate::history::with_history(Clone::clone).unwrap()).unwrap(),
            before_history
        );
    }

    #[test]
    fn context_menu_resolves_semantic_actions_with_list_grid_parity() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let folder = Action {
            label: "Projects".into(),
            desc: "Folder".into(),
            action: "C:/work".into(),
            args: None,
        };
        let bookmark = Action {
            label: "Bookmark".into(),
            desc: "Web".into(),
            action: "https://example.com".into(),
            args: None,
        };
        app.folder_aliases
            .insert(folder.action.clone(), Some("work".into()));
        app.bookmark_aliases
            .insert(bookmark.action.clone(), Some("Docs".into()));

        let cases = vec![
            (folder, "folder.set_alias", true),
            (bookmark, "bookmark.set_alias", true),
            (
                Action {
                    label: "Timer".into(),
                    desc: "Timer".into(),
                    action: "timer:show:1".into(),
                    args: None,
                },
                "timer.pause",
                false,
            ),
            (
                Action {
                    label: "Stopwatch".into(),
                    desc: "Stopwatch".into(),
                    action: "stopwatch:show:2".into(),
                    args: None,
                },
                "stopwatch.copy_time",
                false,
            ),
            (
                Action {
                    label: "sig".into(),
                    desc: "Snippet".into(),
                    action: "clipboard:Regards".into(),
                    args: None,
                },
                "snippet.edit",
                true,
            ),
            (
                Action {
                    label: "scratch.txt".into(),
                    desc: "Tempfile".into(),
                    action: "C:/tmp/scratch.txt".into(),
                    args: None,
                },
                "tempfile.set_alias",
                true,
            ),
            (
                Action {
                    label: "Daily".into(),
                    desc: "Note".into(),
                    action: "note:open:daily".into(),
                    args: None,
                },
                "note.edit",
                true,
            ),
            (
                Action {
                    label: "Clipboard entry".into(),
                    desc: "Clipboard".into(),
                    action: "clipboard:copy:3".into(),
                    args: None,
                },
                "clipboard.edit",
                false,
            ),
            (
                Action {
                    label: "Todo".into(),
                    desc: "Todo".into(),
                    action: "todo:done:4".into(),
                    args: None,
                },
                "todo.edit",
                false,
            ),
            (
                Action {
                    label: "Dynamic".into(),
                    desc: "Plugin value".into(),
                    action: "plugin:future".into(),
                    args: None,
                },
                "result.favorite",
                true,
            ),
        ];

        for (action, expected_id, favorite_expected) in cases {
            app.resolved_grid_layout = false;
            let list = app.resolve_context_menu_actions(
                &action,
                crate::universal_actions::PinCapability::Writable { is_pinned: false },
            );
            app.resolved_grid_layout = true;
            let grid = app.resolve_context_menu_actions(
                &action,
                crate::universal_actions::PinCapability::Writable { is_pinned: false },
            );
            let ids = |actions: &[crate::universal_actions::UniversalAction]| {
                actions
                    .iter()
                    .map(|action| action.id.as_str().to_string())
                    .collect::<Vec<_>>()
            };
            assert!(
                ids(&list).iter().any(|id| id == expected_id),
                "missing {expected_id}"
            );
            assert!(ids(&list).iter().any(|id| id == "result.pin"));
            assert_eq!(
                ids(&list).iter().any(|id| id == "result.favorite"),
                favorite_expected,
                "favorite persistence policy mismatch for {}",
                action.action
            );
            assert_eq!(ids(&list), ids(&grid));
        }
    }

    #[test]
    fn context_action_is_deferred_until_all_result_rows_have_been_visited() {
        let _lock = MACRO_ACTIVATION_TEST_MUTEX.lock().unwrap();
        let executions = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&executions);
        set_execute_action_hook(Some(Box::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })));
        let selected = Action {
            label: "First folder".into(),
            desc: "Folder".into(),
            action: "C:/first".into(),
            args: None,
        };
        let resolved = crate::universal_actions::ResolvedActionTarget {
            target: crate::universal_actions::ActionTarget::Folder {
                path: selected.action.clone(),
            },
            selected_action: selected.clone(),
            custom_action_index: None,
        };
        let action = crate::universal_actions::UniversalActionRegistry
            .resolve(
                &resolved,
                &crate::universal_actions::ActionResolutionContext::new(
                    crate::universal_actions::ActionSurface::ContextMenu,
                    "query",
                ),
            )
            .into_iter()
            .find(|action| action.id == crate::universal_actions::action_ids::FOLDER_REMOVE)
            .unwrap();
        let mut rows = vec![0, 1, 2];
        let mut visited = Vec::new();
        let mut deferred = None;

        for row in rows.iter().copied() {
            visited.push(row);
            if row == 0 {
                defer_universal_context_action(&mut deferred, action.clone());
            }
            assert_eq!(executions.load(Ordering::SeqCst), 0);
        }
        assert_eq!(visited, [0, 1, 2]);
        assert_eq!(rows, [0, 1, 2]);

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.require_confirm_destructive = false;
        if deferred.is_some() {
            app.execute_deferred_result_action(deferred.take());
            rows.remove(0);
        }
        assert_eq!(rows, [1, 2]);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert!(app.pending_universal_confirm.is_none());
        set_execute_action_hook(None);
    }

    #[test]
    fn production_action_sheet_router_consumes_exact_ctrl_enter_and_preserves_tab() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.results = vec![Action {
            label: "Dynamic".into(),
            desc: "Plugin value".into(),
            action: "help:show".into(),
            args: None,
        }];
        focus_launcher_query(&ctx, &mut app);

        ctx.begin_frame(egui::RawInput {
            events: vec![
                key_press(egui::Key::Enter, egui::Modifiers::CTRL),
                key_press(egui::Key::Tab, egui::Modifiers::NONE),
            ],
            ..Default::default()
        });
        assert!(
            app.route_action_sheet_keyboard(&ctx, egui::Id::new("query_input"))
                .is_none()
        );
        assert!(app.action_sheet.is_open());
        assert!(app.test_activation_trace.is_empty());
        assert!(!ctx.input(|input| input.key_pressed(egui::Key::Enter)));
        assert!(ctx.input(|input| input.key_pressed(egui::Key::Tab)));
        let _ = ctx.end_frame();

        ctx.begin_frame(egui::RawInput {
            events: vec![key_press(egui::Key::Enter, egui::Modifiers::CTRL)],
            ..Default::default()
        });
        assert!(
            app.route_action_sheet_keyboard(&ctx, egui::Id::new("query_input"))
                .is_none()
        );
        assert!(app.action_sheet.is_open());
        assert!(app.test_activation_trace.is_empty());
        assert!(!ctx.input(|input| input.key_pressed(egui::Key::Enter)));
        let _ = ctx.end_frame();
    }

    #[test]
    fn production_action_sheet_router_leaves_shifted_and_plain_enter_for_launcher() {
        for modifiers in [
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Modifiers::NONE,
        ] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            app.results = vec![Action {
                label: "Dynamic".into(),
                desc: "Plugin value".into(),
                action: "help:show".into(),
                args: None,
            }];
            focus_launcher_query(&ctx, &mut app);
            ctx.begin_frame(egui::RawInput {
                events: vec![key_press(egui::Key::Enter, modifiers)],
                ..Default::default()
            });
            assert!(
                app.route_action_sheet_keyboard(&ctx, egui::Id::new("query_input"))
                    .is_none()
            );
            assert!(!app.action_sheet.is_open());
            assert!(ctx.input(|input| input.key_pressed(egui::Key::Enter)));
            let _ = ctx.end_frame();
        }
    }

    #[test]
    fn dynamic_fallback_executes_through_normal_enter_and_action_sheet_paths() {
        let _lock = MACRO_ACTIVATION_TEST_MUTEX.lock().unwrap();
        let executions = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&executions);
        set_execute_action_hook(Some(Box::new(move |action| {
            assert_eq!(action.action, "third_party:opaque:command");
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })));
        let ctx = egui::Context::default();
        let mut app = app_with_fake(&ctx);
        app.query = "dynamic universal".into();
        app.search();
        assert_eq!(app.results.len(), 1);
        let selected = app.results[0].clone();
        assert_eq!(selected.action, "third_party:opaque:command");

        let index = app.handle_key(egui::Key::Enter).unwrap();
        let deferred =
            deferred_activation_from_results(&app.results, index, ActivationSource::Enter).unwrap();
        app.activate_action(deferred.action, deferred.query_override, deferred.source);
        assert_eq!(
            app.test_activation_trace.last().unwrap().1,
            ActivationSource::Enter
        );
        assert_eq!(executions.load(Ordering::SeqCst), 1);

        app.test_activation_trace.clear();
        app.results = vec![selected];
        assert!(app.open_action_sheet_for_index(0));
        assert!(matches!(
            app.action_sheet
                .target
                .as_ref()
                .map(|target| &target.target),
            Some(crate::universal_actions::ActionTarget::Generic { .. })
        ));
        let (action, source) = app
            .handle_action_sheet_key(Some(action_sheet::ActionSheetKey::Enter))
            .unwrap();
        app.execute_universal_action(
            action,
            crate::universal_actions::ActionSurface::ActionSheet,
            source,
        );
        assert_eq!(
            app.test_activation_trace.last().unwrap().1,
            ActivationSource::Enter
        );
        assert_eq!(executions.load(Ordering::SeqCst), 2);
        set_execute_action_hook(None);
    }

    #[test]
    fn context_menu_includes_custom_pin_and_favorite_capabilities() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let selected = Action {
            label: "Custom Tool".into(),
            desc: "Custom".into(),
            action: "tool.exe".into(),
            args: Some("--run".into()),
        };
        app.actions = Arc::new(vec![selected.clone()]);
        app.custom_len = 1;

        let actions = app.resolve_context_menu_actions(
            &selected,
            crate::universal_actions::PinCapability::Writable { is_pinned: true },
        );
        let ids = actions
            .iter()
            .map(|action| action.id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"custom_action.edit"));
        assert!(ids.contains(&"result.unpin"));
        assert!(ids.contains(&"result.replace_pin"));
        assert!(ids.contains(&"result.recompute_pins"));
        assert!(ids.contains(&"result.favorite"));
        assert!(!ids.contains(&"result.pin"));
        assert!(
            actions
                .iter()
                .find(|action| action.id.as_str() == "result.execute")
                .is_some_and(|action| !action
                    .effective_presentation(crate::universal_actions::ActionSurface::ContextMenu)
                    .visible)
        );

        let read_only = app.resolve_context_menu_actions(
            &selected,
            crate::universal_actions::PinCapability::ReadOnly {
                is_pinned: false,
                reason: "pins unavailable".into(),
            },
        );
        assert_eq!(
            read_only
                .iter()
                .find(|action| action.id.as_str() == "result.pin")
                .and_then(|action| action.availability.disabled_reason()),
            Some("pins unavailable")
        );
        assert!(
            read_only
                .iter()
                .find(|action| action.id.as_str() == "result.favorite")
                .is_some_and(|action| action.is_available())
        );
    }

    #[test]
    fn keyboard_navigation_is_consistent_between_grid_and_list_modes() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.results = (0..8)
            .map(|i| Action {
                label: format!("A{i}"),
                desc: "d".into(),
                action: format!("act:{i}"),
                args: None,
            })
            .collect();

        app.resolved_grid_layout = false;
        app.selected = Some(1);
        app.handle_key(egui::Key::ArrowDown);
        app.handle_key(egui::Key::ArrowUp);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::ArrowRight);
        assert_eq!(app.selected, Some(1));

        app.resolved_grid_layout = true;
        app.query_results_layout.cols = 3;
        app.selected = Some(4);
        app.handle_key(egui::Key::ArrowLeft);
        app.handle_key(egui::Key::ArrowRight);
        assert_eq!(app.selected, Some(4));
        app.handle_key(egui::Key::ArrowUp);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::ArrowDown);
        assert_eq!(app.selected, Some(4));
    }

    #[test]
    fn deferred_activation_from_results_clones_action_without_retaining_index() {
        let original = vec![
            Action {
                label: "First".into(),
                desc: "Command".into(),
                action: "query:first".into(),
                args: None,
            },
            Action {
                label: "cm camel-case".into(),
                desc: "Completion".into(),
                action: "query:cm camel-case".into(),
                args: Some("{\"query\":\"camel-case\"}".into()),
            },
        ];

        let deferred = deferred_activation_from_results(&original, 1, ActivationSource::Enter)
            .expect("selected result should resolve to a deferred activation");
        let mut refreshed = original.clone();
        refreshed[1] = Action {
            label: "Refreshed".into(),
            desc: "New list".into(),
            action: "refreshed:action".into(),
            args: None,
        };

        assert_eq!(deferred.action.label, "cm camel-case");
        assert_eq!(deferred.action.action, "query:cm camel-case");
        assert_eq!(deferred.source, ActivationSource::Enter);
        assert_eq!(refreshed[1].action, "refreshed:action");
    }

    #[test]
    fn deferred_activation_from_results_ignores_out_of_bounds_index() {
        let results = vec![Action {
            label: "Only".into(),
            desc: "Command".into(),
            action: "only".into(),
            args: None,
        }];

        assert!(deferred_activation_from_results(&results, 3, ActivationSource::Click).is_none());
    }

    #[test]
    fn pinned_panels_prevent_close_until_unpinned() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.pinned_panels.push(Panel::ClipboardDialog);
        app.clipboard_dialog.open = true;
        app.update_panel_stack();

        assert!(!app.close_front_dialog());
        assert!(app.clipboard_dialog.open);

        app.toggle_pin(Panel::ClipboardDialog);
        app.clipboard_dialog.open = true;
        app.update_panel_stack();
        assert!(app.close_front_dialog());
        assert!(!app.clipboard_dialog.open);
    }
}
