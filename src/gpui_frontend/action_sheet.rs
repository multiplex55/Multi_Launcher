use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::gui::action_sheet::ActionSheetChoices;
use multi_launcher::universal_actions::UniversalAction;

pub(super) struct Sheet {
    serial: u64,
    generation: u64,
    row: usize,
    query: String,
    result: LauncherSearchResult,
    revision: u64,
    return_focus: FocusHandle,
    owner: FocusHandle,
    filter_focus: FocusHandle,
    shell: Option<FocusHandle>,
    choices: ActionSheetChoices,
    _subscription: Subscription,
    retirement_pending: bool,
}

impl Sheet {
    fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.owner.contains_focused(window, cx)
            || self
                .shell
                .as_ref()
                .is_some_and(|shell| shell.contains_focused(window, cx))
    }
}

fn snapshot_matches(
    results: &LauncherSearchResults,
    generation: u64,
    row: usize,
    query: &str,
    result: &LauncherSearchResult,
    revision: u64,
    current_revision: u64,
) -> bool {
    revision == current_revision
        && results.resolve_result(generation, row, query).as_ref() == Some(result)
}

fn primary_allowed(
    result: &LauncherSearchResult,
    candidate: &Action,
    indexed_current: bool,
    verified_folders: &[Action],
) -> bool {
    #[cfg(windows)]
    let saved_shortcut = result.saved_shortcut().is_some();
    #[cfg(not(windows))]
    let saved_shortcut = false;
    !matches!(parse_action(candidate), Ok(Command::Todo(multi_launcher::commands::TodoCommand::Done { .. }) | Command::Calendar(_)))
        && candidate == &result.action
        && (parse_installed_search_invocation(candidate.clone(), ActivationSource::Enter).is_ok()
            || matches!(&result.origin, LauncherResultOrigin::Omni(source) if super::omni_search::supported_child(source, candidate))
            || (matches!(&result.origin, LauncherResultOrigin::OmniNote { .. })
                && super::omni_search::supported_child(&multi_launcher::plugins::omni_search::OmniSource::Note, candidate))
            || result.saved_external().is_some()
            || saved_shortcut
            || result.saved_document().is_some()
            || result.saved_url().is_some()
            || (result.origin == LauncherResultOrigin::Saved
                && verified_folders.contains(candidate)
                && saved_folder::candidate(candidate).is_some())
            || indexed_current)
}

fn sheet_key_owned(key: &str, modified: bool, nested_popup: bool, navigation_focus: bool) -> bool {
    matches!(key, "escape" | "up" | "down" | "enter")
        && !(key == "escape" && nested_popup)
        && !(key == "enter" && !modified && !navigation_focus)
}

impl RootWindow {
    fn sheet_receipt_current(&self, sheet: &Sheet, window: &Window, cx: &Context<Self>) -> bool {
        #[cfg(windows)]
        if !self.native_window.is_some_and(|native| {
            self.root_visibility
                .is_current_visible_revision(sheet.revision, native)
        }) {
            return false;
        }
        self.action_sheet_serial == sheet.serial
            && self.root_visibility.visible.load(Ordering::Acquire)
            && self.root_visibility.active.load(Ordering::Acquire)
            && window.is_window_active()
            && self
                .root_window
                .is_some_and(|root| root.window_id() == window.window_handle().window_id())
            && !self.restart_admission.is_fenced()
            && !self.file_menu_open
            && !self.settings_menu_open
            && !self.help_menu_open
            && self.context_favorite_popup.is_none()
            && self.query.read(cx).value().as_str() == sheet.query
            && snapshot_matches(
                &self.launcher_results,
                sheet.generation,
                sheet.row,
                &sheet.query,
                &sheet.result,
                sheet.revision,
                self.root_visibility
                    .request_revision
                    .load(Ordering::Acquire),
            )
    }

    pub(super) fn retire_action_sheet_now(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        allow_unfocused: bool,
    ) {
        let Some(sheet) = self.action_sheet.as_ref() else {
            return;
        };
        if self.action_sheet_serial == sheet.serial {
            self.action_sheet_serial = self.action_sheet_serial.wrapping_add(1);
        }
        // Search admits this dialog only when no Kit dialog exists, and the
        // restart alert retires it before opening. Thus on hide it is the sole
        // active Kit dialog, even while its input popup owns focus.
        if (sheet.owns_focus(window, cx) || allow_unfocused)
            && !context_stack_has_primary(&window.context_stack(), "AlertDialog")
        {
            self.action_sheet = None;
            window.close_dialog(cx);
        }
        cx.notify();
    }

    pub(super) fn retire_stale_action_sheet(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sheet) = self.action_sheet.as_ref() else {
            return;
        };
        if self.sheet_receipt_current(sheet, window, cx) {
            return;
        }
        if self.action_sheet_serial == sheet.serial {
            self.action_sheet_serial = self.action_sheet_serial.wrapping_add(1);
        }
        if sheet.retirement_pending || !sheet.owns_focus(window, cx) {
            return;
        }
        let serial = sheet.serial;
        self.action_sheet.as_mut().unwrap().retirement_pending = true;
        let root = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let _ = root.update(cx, |root, cx| {
                let Some(sheet) = root
                    .action_sheet
                    .as_mut()
                    .filter(|sheet| sheet.serial == serial)
                else {
                    return;
                };
                sheet.retirement_pending = false;
                if !sheet.owns_focus(window, cx)
                    || context_stack_has_primary(&window.context_stack(), "PopupMenu")
                {
                    return;
                }
                root.action_sheet = None;
                window.close_dialog(cx);
                cx.notify();
            });
        });
    }

    pub(super) fn open_action_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.folder_authoring.revoke(); self.bookmark_authoring.revoke(); self.external_drop.revoke();
        if self.search_input_operation(window, cx).is_none()
            || self.action_sheet.is_some()
            || window.has_active_dialog(cx)
            || self.context_favorite_popup.is_some()
            || self.file_menu_open
            || self.settings_menu_open
            || self.help_menu_open
        {
            return;
        }
        let query = self.query.read(cx).value().to_string();
        let Some((generation, row)) = self.launcher_results.enter_target(&query) else {
            return;
        };
        let Some(action) = self
            .launcher_results
            .resolve_result(generation, row, &query)
        else {
            return;
        };
        if matches!(&action.origin, LauncherResultOrigin::Provider(name) if name != "folders" && name != "bookmarks" && super::external_launch::AssociationKind::from_provider_name(name).is_some())
        {
            return;
        }
        let indexed = matches!(&action.origin, LauncherResultOrigin::Indexed { .. });
        let indexed_current = indexed_applications::admitted_result(
            &action,
            self.indexed_applications.snapshot.as_ref(),
            &self.search_catalog.actions[self.search_catalog.saved_count..],
        )
        .is_some();
        let result = action;
        let action = &result.action;
        let folder_provider = matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "folders");
        let bookmark_provider = matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "bookmarks");
        if folder_provider && !self.search_catalog.providers.folder_row_is_current(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
            || bookmark_provider && !self.search_catalog.providers.bookmark_row_is_current(&query, action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref()) {
            return;
        }
        // Provider rows resolve against their live owner, scoped to the exact selected target.
        // A saved or foreign URL must not acquire an authoring target through this path.
        let targets = HashSet::from([action.action.as_str()]);
        let (live_folders, live_bookmarks) = self.search_catalog.providers.current_search_aliases(&targets);
        let empty = HashMap::new();
        let folder_aliases = if folder_provider { live_folders.as_ref().unwrap_or(&empty) }
            else if bookmark_provider { &empty } else { &self.search_catalog.folder_aliases_lc };
        let bookmark_aliases = if bookmark_provider { live_bookmarks.as_ref().unwrap_or(&empty) }
            else if folder_provider { &empty } else { &self.search_catalog.bookmark_aliases_lc };
        let resolver = ActionTargetResolverContext::new(
            folder_aliases,
            bookmark_aliases,
            if indexed {
                &[]
            } else {
                self.search_catalog.saved_actions()
            },
        );
        let target = ActionTargetResolver.resolve(action, &resolver);
        let mut context = ActionResolutionContext::new(ActionSurface::ActionSheet, &query);
        context.can_add_favorite = target.target.persistent_ref().is_some();
        let mut actions = UniversalActionRegistry.resolve(&target, &context);
        actions.retain(|choice| match &choice.operation {
            UniversalActionOperation::InvokePrimary(candidate) => {
                primary_allowed(&result, candidate, indexed_current, &self.search_catalog.verified_folders)
            }
            UniversalActionOperation::UiIntent(UniversalUiIntent::AddFavorite {
                action: candidate,
            }) => candidate == action && context.can_add_favorite,
            UniversalActionOperation::UiIntent(UniversalUiIntent::OpenBookmarkAlias { url }) =>
                matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "bookmarks") && *url == action.action,
            UniversalActionOperation::Command { command: Command::Storage(StorageCommand::BookmarkRemove(url)), .. } =>
                matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "bookmarks") && *url == action.action,
            UniversalActionOperation::UiIntent(UniversalUiIntent::OpenFolderAlias { path }) =>
                matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "folders") && *path == action.action,
            UniversalActionOperation::Command { command: Command::Storage(StorageCommand::FolderRemove(path)), .. } =>
                matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "folders") && *path == action.action,
            _ => false,
        });
        let choices = ActionSheetChoices::new(actions, &self.search_catalog.matcher);
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter actions"));
        let owner = cx.focus_handle();
        self.action_sheet_serial = self.action_sheet_serial.wrapping_add(1);
        let serial = self.action_sheet_serial;
        let subscription = cx.subscribe_in(
            &filter,
            window,
            move |root, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(sheet) = root
                        .action_sheet
                        .as_mut()
                        .filter(|sheet| sheet.serial == serial)
                    {
                        sheet.choices.set_filter(
                            input.read(cx).value().to_string(),
                            &root.search_catalog.matcher,
                        );
                        window.refresh();
                        cx.notify();
                    }
                }
            },
        );
        self.revoke_focus_bound_utility_operations(cx);
        self.action_sheet = Some(Sheet {
            serial,
            generation,
            row,
            query,
            result,
            revision: self
                .root_visibility
                .request_revision
                .load(Ordering::Acquire),
            return_focus: self.query.focus_handle(cx),
            owner: owner.clone(),
            filter_focus: filter.focus_handle(cx),
            shell: None,
            choices,
            _subscription: subscription,
            retirement_pending: false,
        });
        self.fresh_search_enter = None;
        self.retire_search_jump_mode();
        let root = cx.entity().downgrade();
        let first = Cell::new(true);
        self.external_launch.retire_focus_bound_effects();
        self.external_launch.retire_omni();
        window.open_dialog(cx, move |dialog, window, cx| {
            if first.replace(false) {
                filter.focus_handle(cx).focus(window, cx);
            }
            let mut rows = div().flex().flex_col();
            if let Some(root) = root.upgrade() {
                let root_state = root.read(cx);
                if let Some(sheet) = root_state
                    .action_sheet
                    .as_ref()
                    .filter(|sheet| sheet.serial == serial)
                {
                    if sheet.choices.filtered_actions().next().is_none() {
                        rows = rows.child(Label::new("No matching actions"));
                    }
                    for (position, choice) in sheet.choices.filtered_actions().cloned().enumerate()
                    {
                        let presentation =
                            choice.effective_presentation(ActionSurface::ActionSheet);
                        let selected = sheet.choices.selected_position() == Some(position);
                        let mut text = presentation.label;
                        if let Some(short) = presentation.short_label {
                            text.push_str(&format!(" ({short})"));
                        }
                        if let Some(description) = presentation.description {
                            text.push_str(&format!(" — {description}"));
                        }
                        if let Some(reason) = choice.availability.disabled_reason() {
                            text.push_str(&format!(" — {reason}"));
                        }
                        let weak = root.downgrade();
                        rows = rows.child(
                            Button::new(("sheet-choice", position))
                                .label(text)
                                .selected(selected)
                                .disabled(!choice.is_available())
                                .on_click(move |event, window, cx| {
                                    let choice = choice.clone();
                                    let source = result_click_source(event.is_keyboard());
                                    let _ = weak.update(cx, |root, cx| {
                                        root.finish_action_sheet(
                                            serial,
                                            Some((choice, source)),
                                            window,
                                            cx,
                                        )
                                    });
                                }),
                        );
                    }
                }
            }
            let close_root = root.clone();
            let close =
                Button::new("close-action-sheet")
                    .label("Close")
                    .on_click(move |_, window, cx| {
                        let _ = close_root.update(cx, |root, cx| {
                            root.finish_action_sheet(serial, None, window, cx)
                        });
                    });
            dialog
                .title("Actions")
                .keyboard(false)
                .close_button(false)
                .overlay_closable(false)
                .child(
                    div()
                        .track_focus(&owner)
                        .key_context("ActionSheet")
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(Input::new(&filter).aria_label("Filter actions"))
                        .child(
                            div()
                                .id("action-sheet-rows")
                                .max_h(px(320.))
                                .overflow_y_scroll()
                                .child(rows),
                        )
                        .child(close),
                )
        });
        // Kit focuses a new, separate dialog shell synchronously before its
        // first content render. Capture it so hide can retire even that interval.
        if let Some(sheet) = self
            .action_sheet
            .as_mut()
            .filter(|sheet| sheet.serial == serial)
        {
            sheet.shell = window.focused(cx);
        }
        cx.notify();
    }

    pub(super) fn action_sheet_key(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(sheet) = self.action_sheet.as_ref() else {
            return false;
        };
        if !sheet.owns_focus(window, cx) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        // A nested popup owns Escape and must get the first opportunity to dismiss.
        if !sheet_key_owned(
            key,
            event.keystroke.modifiers != Modifiers::default(),
            context_stack_has_primary(&event.context_stack, "PopupMenu"),
            sheet.filter_focus.is_focused(window)
                || sheet.owner.is_focused(window)
                || sheet
                    .shell
                    .as_ref()
                    .is_some_and(|shell| shell.is_focused(window)),
        ) {
            return false;
        }
        if event.keystroke.modifiers != Modifiers::default() {
            return true;
        }
        let serial = sheet.serial;
        match key {
            "escape" => self.finish_action_sheet(serial, None, window, cx),
            "enter" => {
                let selected = sheet.choices.selected_invocation(ActivationSource::Enter);
                if let Some(choice) = selected {
                    self.finish_action_sheet(serial, Some(choice), window, cx);
                }
            }
            "up" | "down" => {
                if let Some(sheet) = self.action_sheet.as_mut() {
                    sheet
                        .choices
                        .move_selection(if key == "up" { -1 } else { 1 });
                }
                window.refresh();
                cx.notify();
            }
            _ => {}
        }
        true
    }

    fn finish_action_sheet(
        &mut self,
        serial: u64,
        selected: Option<(UniversalAction, ActivationSource)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sheet) = self
            .action_sheet
            .as_ref()
            .filter(|sheet| sheet.serial == serial)
        else {
            return;
        };
        if !sheet.owns_focus(window, cx)
            || context_stack_has_primary(&window.context_stack(), "PopupMenu")
        {
            return;
        }
        let current = self.sheet_receipt_current(sheet, window, cx);
        if selected.is_some() && !current {
            return;
        }
        if selected.as_ref().is_some_and(|(choice, _)| {
            !sheet
                .choices
                .filtered_actions()
                .any(|current| current == choice && current.is_available())
        }) {
            return;
        }
        let sheet = self.action_sheet.take().unwrap();
        window.close_dialog(cx);
        if current {
            sheet.return_focus.focus(window, cx);
        }
        let Some((selected, source)) = selected else {
            cx.notify();
            return;
        };
        let root = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let _ = root.update(cx, |root, cx| {
                if root.action_sheet_serial != serial
                    || root.action_sheet.is_some()
                    || !root.sheet_receipt_current(&sheet, window, cx)
                    || !sheet.return_focus.is_focused(window)
                    || window.has_active_dialog(cx)
                {
                    return;
                }
                root.action_sheet_serial = root.action_sheet_serial.wrapping_add(1);
                match selected.operation {
                    UniversalActionOperation::InvokePrimary(action)
                        if action == sheet.result.action =>
                    {
                        root.activate_launcher_result(
                            sheet.generation,
                            sheet.row,
                            source,
                            LauncherResultActivationOrigin::DeferredSearchEnter,
                            window,
                            cx,
                        )
                    }
                    UniversalActionOperation::UiIntent(UniversalUiIntent::AddFavorite {
                        action,
                    }) if action == sheet.result.action => {
                        root.open_result_favorites(action, Some(sheet.return_focus), window, cx)
                    }
                    UniversalActionOperation::UiIntent(UniversalUiIntent::OpenBookmarkAlias { url })
                        if url == sheet.result.action.action => {
                        root.start_bookmark_sheet(sheet.generation, sheet.row, &sheet.query, sheet.result.action.clone(), false, source, window, cx);
                    }
                    UniversalActionOperation::Command { command: Command::Storage(StorageCommand::BookmarkRemove(url)), .. }
                        if url == sheet.result.action.action => {
                        root.start_bookmark_sheet(sheet.generation, sheet.row, &sheet.query, sheet.result.action.clone(), true, source, window, cx);
                    }
                    UniversalActionOperation::UiIntent(UniversalUiIntent::OpenFolderAlias { path })
                        if path == sheet.result.action.action => {
                        root.start_folder_sheet(sheet.generation, sheet.row, &sheet.query, sheet.result.action.clone(), false, source, window, cx);
                    }
                    UniversalActionOperation::Command { command: Command::Storage(StorageCommand::FolderRemove(path)), .. }
                        if path == sheet.result.action.action => {
                        root.start_folder_sheet(sheet.generation, sheet.row, &sheet.query, sheet.result.action.clone(), true, source, window, cx);
                    }
                    _ => {}
                }
            });
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        Action, LauncherResultOrigin, LauncherSearchResult, LauncherSearchResults,
        ResolvedQueryResultsLayout,
    };
    use super::{primary_allowed, sheet_key_owned, snapshot_matches};

    #[test]
    fn todo_toggle_is_not_an_action_sheet_primary() {
        let action = Action { label: "[ ] task".into(), desc: "Todo".into(), action: "todo:done:0".into(), args: None };
        let result = LauncherSearchResult { action: action.clone(), origin: LauncherResultOrigin::Omni(multi_launcher::plugins::omni_search::OmniSource::Todo), display_label: None, hide_description: false };
        assert!(!primary_allowed(&result, &action, false, &[]));
    }

    #[test]
    fn calendar_jump_is_not_an_action_sheet_primary() {
        let action = Action { label: "Meeting".into(), desc: "Calendar".into(), action: "calendar:jump:2026-09-06".into(), args: None };
        let result = LauncherSearchResult { action: action.clone(), origin: LauncherResultOrigin::CalendarJump { serial: 1, index: 0 }, display_label: None, hide_description: false };
        assert!(!primary_allowed(&result, &action, false, &[]));
    }

    #[test]
    fn receipt_rejects_changed_result_generation_payload_and_visibility() {
        let action = Action {
            label: "Hash".into(),
            desc: "fixture".into(),
            action: "hash".into(),
            args: None,
        };
        let mut results = LauncherSearchResults::default();
        let layout = ResolvedQueryResultsLayout::new(false, 1);
        results.replace_test_actions("hash".into(), vec![action.clone()], layout, 1);
        let generation = results.generation;
        let result = results.actions[0].clone();
        assert!(snapshot_matches(
            &results, generation, 0, "hash", &result, 4, 4
        ));
        assert!(!snapshot_matches(
            &results, generation, 0, "other", &result, 4, 4
        ));
        assert!(!snapshot_matches(
            &results, generation, 0, "hash", &result, 4, 5
        ));
        results.actions[0].origin = LauncherResultOrigin::Provider("forged".into());
        assert!(!snapshot_matches(
            &results, generation, 0, "hash", &result, 4, 4
        ));
        results.actions[0].origin = LauncherResultOrigin::Saved;
        results.actions[0].action.args = Some("changed".into());
        assert!(!snapshot_matches(
            &results, generation, 0, "hash", &result, 4, 4
        ));
        results.replace_test_actions("hash".into(), vec![action.clone()], layout, 1);
        assert!(!snapshot_matches(
            &results, generation, 0, "hash", &result, 4, 4
        ));
    }

    #[test]
    fn primary_execution_requires_saved_or_current_indexed_authority() {
        let action = Action {
            label: "App".into(),
            desc: "fixture".into(),
            action: r"C:\Apps\Tool.exe".into(),
            args: None,
        };
        let mut result = LauncherSearchResult {
            action: action.clone(),
            origin: LauncherResultOrigin::Saved,
            display_label: Some("app App".into()),
            hide_description: false,
        };
        assert!(primary_allowed(&result, &action, false, &[]));
        for target in [
            r"C:\Apps\Notes.txt",
            r"C:\Apps\Notes.rtf",
            r"C:\Apps\Notes.pdf",
            r"C:\Apps\Notes.docx",
            r"C:\Apps\Notes.xlsx",
            r"C:\Apps\Notes.pptx",
            "https://example.com",
            "http://example.com/path",
        ] {
            let admitted = Action {
                action: target.into(),
                ..action.clone()
            };
            result.action = admitted.clone();
            assert!(primary_allowed(&result, &admitted, false, &[]), "saved target: {target}");
            result.origin = LauncherResultOrigin::Provider("forged".into());
            assert!(!primary_allowed(&result, &admitted, false, &[]));
            result.origin = LauncherResultOrigin::Saved;
        }
        let shortcut = Action { action: r"C:\Apps\Link.lnk".into(), ..action.clone() };
        result.action = shortcut.clone();
        assert_eq!(primary_allowed(&result, &shortcut, false, &[]), cfg!(windows));
        let folder = Action { action: r"C:\Apps\Folder".into(), ..action.clone() };
        result.action = folder.clone();
        assert!(!primary_allowed(&result, &folder, false, &[]));
        assert_eq!(primary_allowed(&result, &folder, false, std::slice::from_ref(&folder)), cfg!(windows));
        result.origin = LauncherResultOrigin::Provider("forged".into());
        assert!(!primary_allowed(&result, &folder, false, std::slice::from_ref(&folder)));
        result.origin = LauncherResultOrigin::Saved;
        for target in [r"C:\Apps\Script.bat", "https://example.com/a b", "mailto:person@example.com"] {
            let unsupported = Action { action: target.into(), ..action.clone() };
            result.action = unsupported.clone();
            assert!(!primary_allowed(&result, &unsupported, false, &[]), "unsupported target: {target}");
        }
        let mut args = shortcut.clone();
        args.args = Some("--arg".into());
        result.action = args.clone();
        assert!(!primary_allowed(&result, &args, false, &[]));
        result.action = action.clone();
        result.origin = LauncherResultOrigin::Provider("forged".into());
        assert!(!primary_allowed(&result, &action, false, &[]));
        result.origin = LauncherResultOrigin::CachedCommand;
        assert!(!primary_allowed(&result, &action, false, &[]));
        let indexed_action = Action {
            label: "Tool.exe".into(),
            desc: r"\\?\C:\Apps\Tool.exe".into(),
            action: r"\\?\C:\Apps\Tool.exe".into(),
            args: None,
        };
        result.action = indexed_action.clone();
        result.origin = LauncherResultOrigin::Indexed {
            generation: 8,
            index: 0,
        };
        let identity = super::super::indexed_applications::Identity {
            generation: 8,
            roots: vec![std::path::PathBuf::from(r"C:\Apps")],
            raw_actions: std::sync::Arc::new(Vec::new()),
            authority: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        };
        #[cfg(windows)]
        assert!(super::super::indexed_applications::row_matches(&indexed_action, &identity.roots));
        let current = super::super::indexed_applications::admitted_result(
            &result,
            Some(&identity),
            std::slice::from_ref(&indexed_action),
        )
        .is_some();
        assert!(primary_allowed(&result, &indexed_action, current, &[]));
        assert!(!primary_allowed(&result, &indexed_action, false, &[]));
        result.origin = LauncherResultOrigin::Indexed {
            generation: 9,
            index: 0,
        };
        assert!(
            super::super::indexed_applications::admitted_result(
                &result,
                Some(&identity),
                std::slice::from_ref(&indexed_action),
            )
            .is_none()
        );
        let mut changed = indexed_action.clone();
        changed.args = Some("changed".into());
        assert!(!primary_allowed(&result, &changed, true, &[]));
        let installed = Action {
            label: "Hash".into(),
            desc: "".into(),
            action: "hash:open".into(),
            args: None,
        };
        result.action = installed.clone();
        result.origin = LauncherResultOrigin::CachedCommand;
        assert!(primary_allowed(&result, &installed, false, &[]));
    }

    #[test]
    fn saved_associations_resolve_stock_execute_in_action_sheet() {
        use multi_launcher::universal_actions::{ActionResolutionContext, ActionSurface, ActionTargetResolver, ActionTargetResolverContext, UniversalActionOperation, UniversalActionRegistry};
        use std::collections::HashMap;

        for target in [r"C:\Apps\Link.lnk", r"C:\Apps\Notes.txt", "https://example.com", r"C:\Apps\Folder"] {
            let action = Action { label: "Saved".into(), desc: "fixture".into(), action: target.into(), args: None };
            let saved = vec![action.clone()];
            let folders = HashMap::new();
            let bookmarks = HashMap::new();
            let resolver = ActionTargetResolverContext::new(&folders, &bookmarks, &saved);
            let resolved = ActionTargetResolver.resolve(&action, &resolver);
            let context = ActionResolutionContext::new(ActionSurface::ActionSheet, "Saved");
            let execute = UniversalActionRegistry.resolve(&resolved, &context).into_iter().find(|choice| {
                matches!(&choice.operation, UniversalActionOperation::InvokePrimary(candidate) if candidate == &action)
                    && choice.is_available()
                    && choice.effective_presentation(ActionSurface::ActionSheet).visible
            }).expect("stock registry must expose Execute");
            let result = LauncherSearchResult { action: action.clone(), origin: LauncherResultOrigin::Saved, display_label: None, hide_description: false };
            let verified = if target.ends_with("Folder") { saved.as_slice() } else { &[] };
            let admitted = match &execute.operation {
                UniversalActionOperation::InvokePrimary(candidate) => primary_allowed(&result, candidate, false, verified),
                _ => false,
            };
            let expected = !target.ends_with(".lnk") && !target.ends_with("Folder") || cfg!(windows);
            assert_eq!(admitted, expected, "{target}");
        }
    }

    #[test]
    fn action_sheet_keeps_navigation_and_yields_tab_and_nested_escape() {
        for key in ["escape", "up", "down", "enter"] {
            assert!(sheet_key_owned(key, false, false, true));
        }
        assert!(!sheet_key_owned("tab", false, false, true));
        assert!(!sheet_key_owned("escape", false, true, true));
        // After Tab reaches an action or Close, Enter belongs to that button.
        assert!(!sheet_key_owned("enter", false, false, false));
        assert!(sheet_key_owned("enter", true, false, false));
        assert!(sheet_key_owned("enter", false, false, true));
    }
}
