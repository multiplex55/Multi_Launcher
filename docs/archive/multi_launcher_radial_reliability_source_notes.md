# Multi Launcher — inspected source notes for the approved reliability plan

**Reviewed archive:** `multi_launcher(20260924-210943).zip`  
**SHA-256:** `f21ca0ee92d1b7572f779b936bb3ce1de7e685314f685ae8711d1734952d2544`  
**Review date:** 24 September 2026.

This is a source-grounded planning review, not implementation or runtime diagnosis. The archive and relevant source were inspected in the container. Application code was not changed; no Rust build, Nextest run, or native Windows acceptance was executed here. The archive contains no Git history. Earlier ledger pass counts and historical commit strings are not verification of this candidate.

The approved requirements and implementation plan supersede earlier recommendations that short taps should preserve the runtime radial. The user explicitly chose **short tap = toggle main grid AND dismiss runtime radial**.

## Assessment

The code already has a substantial radial runtime, typed invocation state, ordered ROOT visibility, Universal Actions, shared command dispatch, versioned radial storage, Designer history, previews, skin inheritance, package/import support, and an opt-in native runner. The plan is a reliability/integration/usability project, not a framework rebuild.

The action-label ambiguity, separate Inspector filtering, missing single-cell saved-query representation, closed-Designer catalog construction, and F11-specific native coverage are directly supported by the source. The focused-window hotkey failure is user-reported and has several source-visible ordering boundaries to investigate; it was not reproduced in this review. In particular, asynchronous activation and restoration of captured ROOT visibility are **investigation targets, not proven root causes**.

## Source map

| Area | Current owners to inspect/change narrowly |
|---|---|
| Input admission and gesture semantics | `src/hotkey/launcher_invocation.rs`; `src/radial/invocation.rs`; native hotkey integration reached from `src/main.rs` |
| Grid/radial coordination | `src/main.rs`; `src/radial/controller.rs`; `src/visibility.rs` |
| ROOT native restore and GUI visibility | `src/gui/render.rs`; `src/window_manager.rs`; `src/window_activation.rs` |
| Query/search/ranking and command policies | `src/gui/search.rs`; `src/gui/command_host.rs`; `src/commands/parser.rs`; `src/commands/handlers/launcher_query.rs`; `src/commands/outcome.rs`; `src/dashboard/widgets/quick_tools.rs` |
| Action identity, catalog, execution, handoff | `src/gui/universal_action_catalog.rs`; `src/gui/universal_action_executor.rs`; `src/gui/radial_actions.rs`; `src/universal_actions/`; `src/radial/bindings.rs`; `src/radial/dynamic.rs`; `src/radial/handoff.rs` |
| Model and storage | `src/radial/model.rs`; `migration.rs`; `validation.rs`; `store.rs`; `package.rs`; `import.rs` under `src/radial/` |
| Authoring state and transactions | `src/radial/authoring.rs`; `src/radial/authoring/menu.rs`; `src/gui/radial_editor/mod.rs`; `canvas.rs`; `preview.rs`; `skin_editor.rs` under `src/gui/radial_editor/` |
| Appearance and geometry | `src/radial/skin.rs`; `geometry.rs`; `render.rs`; `preparation.rs`; `assets.rs` |
| Native acceptance/evidence | `src/bin/radial_acceptance.rs`; `src/bin/radial_acceptance/native.rs`; `src/bin/radial_acceptance/suite.rs`; `src/radial/acceptance_trace.rs` |

All line ranges below refer to the unmodified archive. Locate symbols again after edits; line numbers are navigation aids, not stable APIs. Excerpts are copied from the uploaded source, not fabricated pseudocode.

## SN01 — Short-tap classification already exists

**Source:** `src/radial/invocation.rs:701–737`.

The release reducer emits CancelDeadline and ToggleLegacyLauncher for a physical timestamp before the deadline. Reuse it; the focused-window symptom is not evidence that another tap detector is missing.

```text
  701:     fn release(&mut self, id: InvocationId, at: Timestamp) -> Vec<InvocationIntent> {
  702:         use InvocationIntent as I;
  703:         match self.state.clone() {
  704:             InvocationState::Pending {
  705:                 id: pending,
  706:                 start,
  707:                 deadline,
  708:                 held_action,
  709:                 ..
  710:             } if pending == id => {
  711:                 if at < start {
  712:                     self.state = InvocationState::Idle;
  713:                     vec![I::CancelDeadline { id }]
  714:                 } else if at < deadline {
  715:                     self.state = InvocationState::Idle;
  716:                     vec![I::CancelDeadline { id }, I::ToggleLegacyLauncher { id }]
  717:                 } else if matches!(
  718:                     held_action,
  719:                     HeldRadialAction::Open {
  720:                         interaction: InteractionMode::ReleaseToSelect,
  721:                         ..
  722:                     }
  723:                 ) {
  724:                     self.state = InvocationState::Idle;
  725:                     vec![
  726:                         I::CancelDeadline { id },
  727:                         I::HoldCancelledBeforePresentation { id },
  728:                     ]
  729:                 } else {
  730:                     // A delayed release carries the physical timestamp.  If
  731:                     // it crossed the deadline, admit the already-captured
  732:                     // hold action exactly once before draining the release.
  733:                     let mut intents = vec![I::CancelDeadline { id }];
  734:                     intents.extend(self.promote_pending(false));
  735:                     intents.extend(self.release_holding(id));
  736:                     intents
  737:                 }
```

## SN02 — Main already orders short-tap visibility changes

**Source:** `src/main.rs:2334–2356`.

Main handles notices, asks the controller for events, records ordered grid toggles, and transfers radial keyboard ownership. Preserve gesture correlation through this boundary and intentionally replace runtime preservation with the approved short-tap dismissal.

```text
 2334:         let mut grid_toggle_batch = VisibilityToggleBatch::default();
 2335:         let mut invocation_route_failed = false;
 2336:         for notice in radial_notices {
 2337:             if let Some(cancellation) = notice.cancellation {
 2338:                 tracing::debug!(?cancellation, "radial invocation lifecycle cancelled");
 2339:                 invocation_route_failed |= cancellation == LifecycleCancellation::HookFailure;
 2340:             }
 2341:             if let Some(error) = notice.error {
 2342:                 tracing::error!(%error, "launcher invocation service failed closed");
 2343:             }
 2344:             for intent in &notice.intents {
 2345:                 if let Some((id, menu_id, interaction, context_token)) =
 2346:                     external_radial_lifecycle_metadata(intent, &radial_document)
 2347:                 {
 2348:                     external_radial_invocations.insert(id, (menu_id, interaction, context_token));
 2349:                 }
 2350:             }
 2351:             for event in radial_controller.handle_intents(notice.intents, settings.always_on_top) {
 2352:                 match event {
 2353:                     ControllerEvent::ToggleLegacyLauncher => {
 2354:                         let was_visible = grid_toggle_batch.record_toggle(&visibility);
 2355:                         radial_controller.handle_legacy_grid_toggle(was_visible);
 2356:                     }
```

## SN03 — Current hotkey-to-radial behavior is intentionally superseded

**Source:** `src/radial/controller.rs:3298–3309`.

handle_legacy_grid_toggle currently transfers keyboard ownership. It does not dismiss the runtime radial. This is an approved behavior change, not a reason to remove all keyboard-ownership support. Migrate legacy_grid_toggle_round_trip_restores_radial_keyboard_without_mouse (around line 5998) and legacy_hotkey_trigger_transfers_keyboard_for_direct_radial_session (around 6042); preserve separate valid non-hotkey owner tests.

```text
 3298:     }
 3299:     /// Transfer keyboard ownership for one legacy-grid toggle. The caller
 3300:     /// supplies the grid state immediately before that edge so several queued
 3301:     /// toggles remain ordered without synthesizing pointer movement.
 3302:     pub fn handle_legacy_grid_toggle(&mut self, grid_was_visible: bool) {
 3303:         if grid_was_visible {
 3304:             self.set_grid_keyboard_owner(GridKeyboardOwner::RadialMenu);
 3305:         } else {
 3306:             self.set_grid_keyboard_owner(GridKeyboardOwner::LegacyLauncher);
 3307:         }
 3308:     }
 3309:     fn record(&mut self, session_id: Option<SessionId>, message: String) {
```

## SN04 — ROOT hide is offscreen parking

**Source:** `src/visibility.rs:568–592`.

Ordinary hide issues a parking position, not Visible(false). Native acceptance must inspect geometry, desired state, identity and transitions. A blanket change to native hiding would alter the existing parked-ROOT wake/preparation design.

```text
  568:                 let pos_y = y - window_size.1 / 2.0;
  569:                 ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
  570:                     pos_x, pos_y,
  571:                 )));
  572:             }
  573:         }
  574:         ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
  575:         ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
  576:         ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
  577:     } else {
  578:         acceptance_trace::emit(Event::RootCommand {
  579:             command: RootCommandKind::ParkingBoundary,
  580:             correlation: if acceptance_trace::enabled() {
  581:                 acceptance_trace::root_command_correlation()
  582:             } else {
  583:                 Correlation::default()
  584:             },
  585:         });
  586:         let parked = safe_parking_position(ctx, offscreen, window_size);
  587:         ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
  588:             parked.0, parked.1,
  589:         )));
  590:     }
  591:     ctx.request_repaint();
  592: }
```

## SN05 — ROOT restore starts asynchronous native activation

**Source:** `src/window_manager.rs:156–200`.

restore_launcher_to_current_desktop builds a desktop-activation request and spawns work. Its trace correlation is not itself proof of cancellation by a newer visibility request. Audit the entire activation side-effect sequence in window_activation.rs. This is a possible race boundary, not a confirmed cause of the reported symptom.

```text
  156: /// Restore the launcher while explicitly relocating it onto the current desktop.
  157: pub fn restore_launcher_to_current_desktop(hwnd: windows::Win32::Foundation::HWND) {
  158:     let request =
  159:         crate::window_activation::WindowActivationRequest::move_to_current_desktop(hwnd.0 as usize);
  160:     let trace_enabled = acceptance_trace::enabled();
  161:     let trace_hwnd_value = hwnd.0 as usize;
  162:     let trace_hwnd = trace_hwnd_value as u64;
  163:     let correlation = if trace_enabled {
  164:         let trace_id = NEXT_RESTORE_TRACE_ID.fetch_add(1, Ordering::Relaxed);
  165:         Correlation {
  166:             request_id: trace_id,
  167:             request_kind: Default::default(),
  168:             session_id: 0,
  169:             generation: trace_id,
  170:             terminal: false,
  171:         }
  172:     } else {
  173:         Correlation::default()
  174:     };
  175:     if trace_enabled {
  176:         acceptance_trace::emit(Event::NativeActivation {
  177:             edge: NativeActivationEdge::RestoreRequested,
  178:             hwnd: trace_hwnd,
  179:             correlation,
  180:         });
  181:     }
  182:     // Desktop transitions and foreground verification use bounded backoff. Keep that work off
  183:     // egui's render path so a slow or policy-blocked target cannot stall a frame.
  184:     std::thread::spawn(move || {
  185:         if let Err(error) = crate::window_activation::activate_window(request) {
  186:             if trace_enabled {
  187:                 let terminal_correlation = Correlation {
  188:                     terminal: true,
  189:                     ..correlation
  190:                 };
  191:                 acceptance_trace::emit(Event::NativeActivation {
  192:                     edge: NativeActivationEdge::RestoreFailed,
  193:                     hwnd: trace_hwnd,
  194:                     correlation: terminal_correlation,
  195:                 });
  196:                 emit_window_snapshot(
  197:                     windows::Win32::Foundation::HWND(trace_hwnd_value as *mut _),
  198:                     terminal_correlation,
  199:                 );
  200:             }
```

## SN06 — Radial ordinary-state restoration writes visibility flags

**Source:** `src/gui/universal_action_executor.rs:457–491`.

RadialRootState captures ordinary GUI state and later writes captured visible/restore flags. Protect this restoration against an intervening newer hotkey decision. Do not allow preservation to erase explicit launcher-visibility commands or required UI interactions.

```text
  457: impl RadialRootState {
  458:     pub(super) fn capture(app: &LauncherApp) -> Self {
  459:         Self {
  460:             query: app.query.clone(),
  461:             pending_query: app.pending_query.clone(),
  462:             results: app.results.clone(),
  463:             selected: app.selected,
  464:             resolved_grid_layout: app.resolved_grid_layout,
  465:             visible: app.visible_flag.load(Ordering::SeqCst),
  466:             restore: app.restore_flag.load(Ordering::SeqCst),
  467:             focus_query: app.focus_query,
  468:             move_cursor_end: app.move_cursor_end,
  469:             last_results_valid: app.last_results_valid,
  470:             last_search_query: app.last_search_query.clone(),
  471:             suggestions: app.suggestions.clone(),
  472:             autocomplete_index: app.autocomplete_index,
  473:             query_history: app.query_history.clone(),
  474:         }
  475:     }
  476:     pub(super) fn restore(self, app: &mut LauncherApp) {
  477:         app.query = self.query;
  478:         app.pending_query = self.pending_query;
  479:         app.results = self.results;
  480:         app.selected = self.selected;
  481:         app.resolved_grid_layout = self.resolved_grid_layout;
  482:         app.visible_flag.store(self.visible, Ordering::SeqCst);
  483:         app.restore_flag.store(self.restore, Ordering::SeqCst);
  484:         app.focus_query = self.focus_query;
  485:         app.move_cursor_end = self.move_cursor_end;
  486:         app.last_results_valid = self.last_results_valid;
  487:         app.last_search_query = self.last_search_query;
  488:         app.suggestions = self.suggestions;
  489:         app.autocomplete_index = self.autocomplete_index;
  490:         app.query_history = self.query_history;
  491:     }
```

## SN07 — Closed Designer still constructs a catalog before its early return

**Source:** `src/gui/radial_editor/mod.rs:1424–1454`.

show_deferred calls app.universal_action_catalog_snapshot while holding the editor mutex, before checking the closed state. This source-confirmed unnecessary work is a bounded M1/M4 optimization target; no cost or hotkey-causality claim was measured.

```text
 1424:     pub(crate) fn show_deferred(shared: &Arc<Mutex<Self>>, ctx: &egui::Context, app: &LauncherApp) {
 1425:         let (
 1426:             open,
 1427:             viewport_close_pending,
 1428:             viewport_restore_pending,
 1429:             viewport_focus_pending,
 1430:             preferences,
 1431:             action_catalog,
 1432:             feature_defaults,
 1433:             diagnostics,
 1434:             require_confirm,
 1435:             launcher_always_on_top,
 1436:         ) = match shared.lock() {
 1437:             Ok(editor) => (
 1438:                 editor.open,
 1439:                 editor.viewport_close_pending,
 1440:                 editor.viewport_restore_pending,
 1441:                 editor.viewport_focus_pending,
 1442:                 editor.preferences.clone().normalized(),
 1443:                 app.universal_action_catalog_snapshot(),
 1444:                 app.radial_feature_settings.clone(),
 1445:                 app.radial_expected_diagnostics.iter().cloned().collect(),
 1446:                 app.require_confirm_destructive,
 1447:                 app.always_on_top,
 1448:             ),
 1449:             Err(_) => return,
 1450:         };
 1451:         if !open && !viewport_close_pending {
 1452:             return;
 1453:         }
 1454:         let viewport_id = radial_designer_viewport_id();
```

## SN08 — Picker labels alone can hide note identity

**Source:** `src/gui/radial_editor/mod.rs:2748–2807`.

Cell Properties currently renders an action presentation label and uses hover detail for the command. The approved editor should expose target/title/type/identifier directly and use shared presentation.

```text
 2748:                 if draft.content_kind == 1 {
 2749:                     ui.label(if draft.action_binding.is_some() {
 2750:                         "Action assigned"
 2751:                     } else {
 2752:                         "Choose an action"
 2753:                     });
 2754:                     let search_response = ui.add(
 2755:                         egui::TextEdit::singleline(&mut self.popup_action_filter)
 2756:                             .hint_text("Search actions"),
 2757:                     );
 2758:                     trace_designer_authoring_control(
 2759:                         ui,
 2760:                         &search_response,
 2761:                         DesignerAuthoringTarget::ActionSearch,
 2762:                         DesignerAuthoringRole::TextEdit,
 2763:                         None,
 2764:                         true,
 2765:                         false,
 2766:                         ViewportClass::Deferred,
 2767:                         correlation,
 2768:                     );
 2769:                     ui.small(format!(
 2770:                         "{} matching action(s); showing up to 50",
 2771:                         action_match_count
 2772:                     ));
 2773:                     egui::ScrollArea::vertical()
 2774:                         .max_height(120.0)
 2775:                         .show(ui, |ui| {
 2776:                             for row in &action_rows {
 2777:                                 let label = row.presentation.label.clone();
 2778:                                 let response = ui
 2779:                                     .selectable_label(
 2780:                                         draft.action_binding.as_ref() == row.binding.as_ref(),
 2781:                                         label,
 2782:                                     )
 2783:                                     .on_hover_text(&row.target_command);
 2784:                                 if let Some(custom_action_index) = row.custom_action_index {
 2785:                                     trace_designer_authoring_control(
 2786:                                         ui,
 2787:                                         &response,
 2788:                                         DesignerAuthoringTarget::ActionRow,
 2789:                                         DesignerAuthoringRole::Selectable,
 2790:                                         Some(custom_action_index),
 2791:                                         row.binding.is_some(),
 2792:                                         draft.action_binding.as_ref() == row.binding.as_ref(),
 2793:                                         ViewportClass::Deferred,
 2794:                                         correlation,
 2795:                                     );
 2796:                                 }
 2797:                                 if response.clicked() {
 2798:                                     match row.assignment() {
 2799:                                         Ok(binding) => draft.action_binding = Some(binding),
 2800:                                         Err(error) => {
 2801:                                             self.resource_notice = Some(ResourceNotice::warning(
 2802:                                                 format!("Action cannot be assigned: {error:?}"),
 2803:                                             ));
 2804:                                         }
 2805:                                     }
 2806:                                 }
 2807:                             }
```

## SN09 — Inspector has a second filter and repeated label presentation

**Source:** `src/gui/radial_editor/mod.rs:5432–5477`.

The Inspector has its own matching/rendering path. Replacing only the popup would leave inconsistent search and target labels. Inspect/migrate both into one shared component.

```text
 5432:         ui.separator();
 5433:         ui.label("Universal Action");
 5434:         ui.text_edit_singleline(&mut self.action_filter);
 5435:         let catalog = crate::gui::universal_action_catalog::UniversalActionAuthoringCatalog::build(
 5436:             &frame.action_catalog,
 5437:             &invocation_context,
 5438:             &self.action_filter,
 5439:         );
 5440:         egui::ScrollArea::vertical()
 5441:             .max_height(260.0)
 5442:             .show(ui, |ui| {
 5443:                 for row in catalog
 5444:                     .rows()
 5445:                     .iter()
 5446:                     .filter(|row| {
 5447:                         self.action_filter.is_empty()
 5448:                             || row
 5449:                                 .presentation
 5450:                                 .label
 5451:                                 .to_lowercase()
 5452:                                 .contains(&self.action_filter.to_lowercase())
 5453:                             || row
 5454:                                 .target_command
 5455:                                 .to_lowercase()
 5456:                                 .contains(&self.action_filter.to_lowercase())
 5457:                     })
 5458:                     .take(100)
 5459:                 {
 5460:                     let label = row.presentation.label.clone();
 5461:                     let assignable = row.binding.is_some();
 5462:                     let testable = assignable && row.availability.is_available();
 5463:                     ui.push_id(
 5464:                         menu::widget_key(
 5465:                             "action",
 5466:                             &format!("{}:{}", row.target_command, row.action_id),
 5467:                             "picker-row",
 5468:                         ),
 5469:                         |ui| {
 5470:                             ui.horizontal(|ui| {
 5471:                                 if ui
 5472:                                     .add_enabled(assignable, egui::Button::new(label))
 5473:                                     .on_disabled_hover_text(
 5474:                                         row.unavailable_reason
 5475:                                             .as_deref()
 5476:                                             .unwrap_or("Not persistable"),
 5477:                                     )
```

## SN10 — Note title/slug are already available to the catalog

**Source:** `src/gui/universal_action_catalog.rs:418–429`.

The catalog can represent a note target by slug and a displayed title. Show existing metadata; do not introduce a global note-identity migration just to solve ambiguous Edit Note rows.

```text
  418:         entries.extend(dashboard.notes.iter().map(|note| ResolvedActionTarget {
  419:             target: ActionTarget::Note {
  420:                 slug: note.slug.clone(),
  421:             },
  422:             selected_action: Action {
  423:                 label: note.title.clone(),
  424:                 desc: "Note".into(),
  425:                 action: format!("note:open:{}", note.slug),
  426:                 args: None,
  427:             },
  428:             custom_action_index: None,
  429:         }));
```

## SN11 — Catalog population performs query-only provider searches

**Source:** `src/gui/universal_action_catalog.rs:373–383`.

Snapshot construction consults query-only providers. Avoid invoking it on every idle frame; build/invalidate on demand and do not hold the Designer lock during provider work.

```text
  373:         // Query-only providers expose their stable authoring and management
  374:         // actions through the same read-only search used by launcher results.
  375:         // Screen Draw and dashboard actions are present in `command_cache`;
  376:         // every route still resolves through Universal Actions.
  377:         for query in ["crop", "ss", "fav", "mkmacro", "note", "cs", "cb"] {
  378:             entries.extend(
  379:                 self.search_read_only(query)
  380:                     .iter()
  381:                     .map(|action| resolver.resolve(action, &resolver_context)),
  382:             );
  383:         }
```

## SN12 — Quick Tools already distinguishes query and Auto Submit

**Source:** `src/dashboard/widgets/quick_tools.rs:149–169`.

Reuse the established distinction: manual query versus queryexec. Preserve Quick Tools existing behavior while allowing resolved-action-aware radial UI policy.

```text
  149:     fn action_for(entry: &QuickToolEntry) -> Option<WidgetAction> {
  150:         let query = entry.query.trim();
  151:         if query.is_empty() {
  152:             return None;
  153:         }
  154:         Some(WidgetAction {
  155:             action: Action {
  156:                 label: query.to_string(),
  157:                 desc: "Tool".into(),
  158:                 action: format!(
  159:                     "{}:{query}",
  160:                     if entry.auto_submit {
  161:                         "queryexec"
  162:                     } else {
  163:                         "query"
  164:                     }
  165:                 ),
  166:                 args: None,
  167:             },
  168:             query_override: (!entry.auto_submit).then(|| query.to_string()),
  169:         })
```

## SN13 — Query outcomes request GUI search and show/focus

**Source:** `src/commands/outcome.rs:85–96`.

CommandOutcome::query deliberately requests search/show/restore/focus for ordinary query navigation. Do not remove that behavior globally to fix radial flash.

```text
   85:     pub fn query(query: String) -> Self {
   86:         Self {
   87:             query: QueryPolicy::Set(query),
   88:             search: true,
   89:             visibility: VisibilityPolicy::Show,
   90:             restore: true,
   91:             focus: true,
   92:             move_cursor_end: true,
   93:             ..Self::default()
   94:         }
   95:     }
   96: }
```

## SN14 — First-result activation is followed by outer visibility policy

**Source:** `src/gui/command_host.rs:838–887`.

The outcome handler updates/searches GUI results, activates the first action, then applies the outcome visibility policy. A naive query wrapper can therefore re-show ROOT after the inner action. The plan separates the radial policy without breaking manual queries.

```text
  838:         if let QueryPolicy::Set(query) = outcome.query {
  839:             self.last_timer_query =
  840:                 query.starts_with("timer list") || query.starts_with("alarm list");
  841:             self.query = query;
  842:         }
  843:         if let PendingQueryPolicy::Set(query) = outcome.pending_query {
  844:             self.pending_query = Some(query);
  845:         }
  846:         if let ResultsPolicy::Replace(results) = outcome.results {
  847:             self.results = results;
  848:             self.selected = None;
  849:             self.last_search_query = self.query.clone();
  850:             self.last_results_valid = true;
  851:             self.update_suggestions();
  852:         }
  853:         if outcome.invalidate_results {
  854:             self.last_results_valid = false;
  855:         }
  856:         if outcome.search {
  857:             self.search();
  858:         }
  859:         if let Some(source) = outcome.activate_first_result
  860:             && let Some(action) = self.results.first().cloned()
  861:         {
  862:             self.activate_action(action, None, source);
  863:         }
  864: 
  865:         match outcome.visibility {
  866:             VisibilityPolicy::Keep => {}
  867:             VisibilityPolicy::Show => self.visible_flag.store(true, Ordering::SeqCst),
  868:             VisibilityPolicy::Hide => self.visible_flag.store(false, Ordering::SeqCst),
  869:             VisibilityPolicy::Toggle => {
  870:                 let next = !self.visible_flag.load(Ordering::SeqCst);
  871:                 self.visible_flag.store(next, Ordering::SeqCst);
  872:             }
  873:         }
  874:         if outcome.restore {
  875:             self.restore_flag.store(true, Ordering::SeqCst);
  876:         }
  877:         if outcome.move_cursor_end {
  878:             self.move_cursor_end = true;
  879:         }
  880:         if outcome.focus {
  881:             self.focus_input();
  882:         }
  883:         if let FavoriteLogPolicy::Ran { label, command } = outcome.favorite_log {
  884:             tracing::info!(fav = %label, command = %command, "ran favorite");
  885:         }
  886:         for toast in outcome.toasts {
  887:             if let ToastPolicy::Error(message) = toast {
```

## SN15 — Read-only result production already exists

**Source:** `src/gui/search.rs:510–537`.

search_read_only returns established provider results with usage weighting/sort without assigning the main GUI query/results. search() at line 237 has overlapping result-production logic and GUI cache/layout effects. Consolidate result production and test parity rather than copying another matcher.

```text
  510:     /// Runs the established launcher/provider search boundary without mutating GUI state.
  511:     pub(super) fn search_read_only(&self, raw_query: &str) -> Vec<Action> {
  512:         let trimmed = raw_query.trim();
  513:         let trimmed_lc = trimmed.to_lowercase();
  514:         if trimmed.is_empty() {
  515:             let mut results = self.command_cache.clone();
  516:             results.extend(self.actions.iter().map(|action| Action {
  517:                 label: format!("app {}", action.label),
  518:                 desc: action.desc.clone(),
  519:                 action: action.action.clone(),
  520:                 args: action.args.clone(),
  521:             }));
  522:             return results;
  523:         }
  524:         let search_actions =
  525:             trimmed_lc == APP_PREFIX || trimmed_lc.starts_with(&format!("{APP_PREFIX} "));
  526:         let action_query = search_actions
  527:             .then(|| trimmed.split_once(' ').map(|value| value.1).unwrap_or(""))
  528:             .unwrap_or("");
  529:         let mut scored = Vec::new();
  530:         if !trimmed_lc.starts_with("g ") && search_actions {
  531:             scored.extend(self.search_actions(action_query, &action_query.to_lowercase()));
  532:         }
  533:         scored.extend(self.search_plugins_for(raw_query, trimmed, &trimmed_lc));
  534:         self.apply_usage_weight(&mut scored);
  535:         scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
  536:         scored.into_iter().map(|(action, _)| action).collect()
  537:     }
```

## SN16 — The current persisted binding model lacks single-cell query/command variants

**Source:** `src/radial/model.rs:507–560`.

ActionBinding has Persisted and Contextual. DynamicSource::LauncherQuery is a many-result menu source, not a single cell with optional Auto Submit. CURRENT_SCHEMA_VERSION is 2 near the top of model.rs; extend serialization and migration deliberately.

```text
  507: #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  508: #[serde(tag = "kind", rename_all = "snake_case")]
  509: pub enum ActionBinding {
  510:     Persisted {
  511:         action: PersistedUniversalActionRef,
  512:     },
  513:     Contextual {
  514:         selector: TargetSelector,
  515:         action_id: crate::universal_actions::ActionId,
  516:     },
  517: }
  518: 
  519: #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  520: #[serde(tag = "kind", rename_all = "snake_case")]
  521: pub enum DynamicSource {
  522:     /// Compatibility form from schema v1. It reads the invocation's explicitly
  523:     /// supplied query and never the mutable root launcher query field.
  524:     LauncherResults {
  525:         max_items: usize,
  526:     },
  527:     LauncherQuery {
  528:         query: String,
  529:         max_items: usize,
  530:     },
  531:     Favorites,
  532:     RecentItems,
  533:     Clipboard,
  534:     Snippets,
  535:     Notes,
  536:     Windows,
  537:     Macros,
  538:     Applications,
  539:     Dashboard,
  540: }
  541: 
  542: #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  543: #[serde(rename_all = "snake_case")]
  544: pub enum Control {
  545:     Back,
  546:     Close,
  547:     NextPage,
  548:     PreviousPage,
  549:     Drag,
  550: }
  551: 
  552: #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  553: #[serde(tag = "kind", rename_all = "snake_case")]
  554: pub enum CellContent {
  555:     Action { binding: ActionBinding },
  556:     Submenu { menu_id: MenuId },
  557:     Dynamic { source: DynamicSource },
  558:     Spacer,
  559:     Control { control: Control },
  560: }
```

## SN17 — Query commands currently classify as launcher UI

**Source:** `src/radial/handoff.rs:62–80`.

All Command::Query values currently require LauncherUi. Auto Submit needs a resolution phase that discovers the actual first result and its interaction needs, while maintaining release/close/safety handoff.

```text
   62: pub(crate) fn command_requirement(command: &Command) -> InteractionRequirement {
   63:     match command {
   64:         Command::Crop(_)
   65:         | Command::ScreenDraw(_)
   66:         | Command::Macro(_)
   67:         | Command::Screenshot(_)
   68:         | Command::MouseGesture(_) => InteractionRequirement::ExclusiveCapture,
   69:         Command::Launcher(_)
   70:         | Command::Radial(
   71:             crate::commands::RadialCommand::Edit | crate::commands::RadialCommand::Skins,
   72:         )
   73:         | Command::Query(_)
   74:         | Command::Dialog(_)
   75:         | Command::FileSearch(_)
   76:         | Command::Diff(_) => InteractionRequirement::LauncherUi,
   77:         Command::Clipboard(crate::commands::ClipboardCommand::SetText { .. })
   78:         | Command::External(_) => InteractionRequirement::ExternalInput,
   79:         _ => InteractionRequirement::None,
   80:     }
```

## SN18 — Runtime dispatch already rejects stale/duplicate identities

**Source:** `src/gui/radial_actions.rs:677–714`.

execute_radial_dispatch checks preparation/config identities and consumed dispatches before resolving an action. Preserve/extend this ownership across query resolution; do not bypass it with a direct UI callback.

```text
  677:     pub(super) fn execute_radial_dispatch(&mut self, request: RadialDispatchRequest) {
  678:         let lease = self
  679:             .radial_preparations
  680:             .get(&request.identity.invocation_id);
  681:         if lease
  682:             != Some(&(
  683:                 request.identity.preparation_generation,
  684:                 request.identity.config_revision,
  685:             ))
  686:             || self.radial_current_preparation
  687:                 != Some((
  688:                     request.identity.invocation_id,
  689:                     request.identity.preparation_generation,
  690:                     request.identity.config_revision,
  691:                 ))
  692:             || self.radial_consumed_dispatches.contains(&request.identity)
  693:         {
  694:             self.report_error_message(
  695:                 "radial_action",
  696:                 "Rejected stale or duplicate radial dispatch lease",
  697:             );
  698:             return;
  699:         }
  700:         self.radial_consumed_dispatches
  701:             .push_back(request.identity.clone());
  702:         while self.radial_consumed_dispatches.len() > RADIAL_DISPATCH_TOMBSTONE_LIMIT {
  703:             self.radial_consumed_dispatches.pop_front();
  704:         }
  705:         match self.resolve_radial_action(&request) {
  706:             Ok(prepared) => {
  707:                 if prepared.requirement != request.requirement
  708:                     || prepared.requirement != interaction_requirement(&prepared.action)
  709:                 {
  710:                     self.report_error_message(
  711:                         "radial_action",
  712:                         "Radial action interaction requirements changed before dispatch",
  713:                     );
  714:                     return;
```

## SN19 — The decoder is the existing migration boundary

**Source:** `src/radial/migration.rs:16–48`.

decode_document refuses a newer schema and calls a v1 migration that stamps CURRENT_SCHEMA_VERSION. Adding v3 needs explicit v2-to-v3 sequencing, read-only load, validation, and compatible store/package usage.

```text
   16: 
   17: /// Decode, migrate, and validate without touching the source bytes. Both the
   18: /// runtime store and persistence health probe use this exact compatibility boundary.
   19: pub fn decode_document(bytes: &[u8]) -> Result<DecodedDocument, DocumentDecodeError> {
   20:     let mut value: serde_json::Value =
   21:         serde_json::from_slice(bytes).map_err(DocumentDecodeError::Malformed)?;
   22:     let version: u64 = serde_json::from_value(
   23:         value
   24:             .get("schema_version")
   25:             .cloned()
   26:             .unwrap_or(serde_json::Value::Null),
   27:     )
   28:     .map_err(DocumentDecodeError::Malformed)?;
   29:     if version > CURRENT_SCHEMA_VERSION as u64 {
   30:         return Err(DocumentDecodeError::UnsupportedNewerVersion {
   31:             found: version,
   32:             supported: CURRENT_SCHEMA_VERSION,
   33:         });
   34:     }
   35:     let migrated_from = if version == 1 {
   36:         migrate_v1(&mut value);
   37:         Some(1)
   38:     } else {
   39:         None
   40:     };
   41:     let document = serde_json::from_value(value).map_err(DocumentDecodeError::Malformed)?;
   42:     validate(&document).map_err(DocumentDecodeError::Validation)?;
   43:     Ok(DecodedDocument {
   44:         document,
   45:         migrated_from,
   46:     })
   47: }
   48: 
```

## SN20 — Native runner is F11-specific and close to its case cap

**Source:** `src/bin/radial_acceptance.rs:18–35`.

There are 31 case IDs and a MAX_CASES value of 32; the configured acceptance hotkey is F11. Generalize fixture/preflight/injection/observer/report paths and increase bounded capacity deliberately; do not only add cases to the suite array.

```text
   18: mod native;
   19: 
   20: const MAX_CASES: usize = 32;
   21: const MAX_ARTIFACTS: usize = 48;
   22: const MAX_PATH_BYTES: usize = 2_048;
   23: const MAX_RESULT_BYTES: usize = 2_048;
   24: const MAX_JSON_REPORT_BYTES: usize = 512 * 1024;
   25: const MAX_TEXT_REPORT_BYTES: usize = 256 * 1024;
   26: const ACCEPTANCE_HOTKEY: &str = "F11";
   27: const ACCEPTANCE_ACTION_COUNT: usize = 64;
   28: const ACCEPTANCE_TARGET_ACTION_INDEX: usize = ACCEPTANCE_ACTION_COUNT - 1;
   29: pub(crate) const CASE_IDS: [&str; 31] = [
   30:     "H0", "H1", "H2", "H3", "H4", "H5", "H6", "D0", "D1", "D2", "D4", "D5", "A0", "A1", "G0", "A2",
   31:     "G1", "G2", "A3", "A4", "A5", "A6", "A7", "A8", "D3", "D6", "D7", "R0", "R1", "R2", "CLEANUP",
   32: ];
   33: 
   34: #[derive(Debug)]
   35: struct Arguments {
```

## SN21 — Native input helper explicitly injects F11 with a release guard

**Source:** `src/bin/radial_acceptance/native.rs:1133–1150`.

The driver validates runner/child ownership and sends F11 down/up with a guard. Preserve its ownership/cleanup properties while generalizing chord injection. The observer also filters F11/F24 near line 504; profile string changes alone are not exact-chord coverage.

```text
 1133:     pub fn send_f11(
 1134:         &self,
 1135:         target_hwnd: HWND,
 1136:         target_process_id: u32,
 1137:         down_time: Duration,
 1138:     ) -> Result<F11TapEvidence, String> {
 1139:         if target_process_id != self.process_id && target_process_id != std::process::id() {
 1140:             return Err("F11 target must be owned by the acceptance runner or child".into());
 1141:         }
 1142:         let down = [key_input(VK_F11, false)];
 1143:         let down = send_validated_input(target_hwnd, target_process_id, &down, "F11 down")?;
 1144:         let mut release_guard = F11ReleaseGuard::new(target_hwnd, target_process_id);
 1145:         release_guard.armed = true;
 1146:         std::thread::sleep(down_time);
 1147:         let up = release_guard.release()?;
 1148:         Ok(F11TapEvidence { down, up })
 1149:     }
 1150: 
```

## Additional implementation-specific cautions

`src/commands/parser.rs` deliberately falls back to an external command when a string is not recognized as an internal command. An Advanced command editor must distinguish parsing interpretation from proof that a command exists or is safe; retain arguments and existing policies. Query text must not be silently converted to a shell command.

`src/gui/render.rs` owns shared result context-menu construction (`resolve_context_menu_actions` and `attach_result_context_menu`), so Add to radial belongs there or its shared intent layer rather than separate grid/list implementations.

`src/radial/authoring.rs` already has `StableSelection`, `DocumentMutation`, `EditKey`/`EditPhase`, editor session/draft generations, close decisions, and history. `authoring/menu.rs` already handles copying, duplication, graph rules, capacity/resize, and stable IDs. Bulk operations should reuse that transaction boundary.

`src/gui/radial_editor/skin_editor.rs` already exposes scoped overrides with provenance and Inherit/Clear semantics. `src/radial/skin.rs` defines layered effective styling. Simple controls and presets should map into these, not replace their semantics. Current document limits include 128 cells per ring; a usability warning is not authorization to silently impose a much smaller hard limit.

The current acceptance CLI supports `--launcher`/`--candidate`, `--output` or `--report`, `--source-revision`, diagnostic H6/mouse-gesture switches, and `--keep-profile-on-failure`. `--output` allows a nonexistent or existing empty directory; nonempty output is refused. Proposed `--suite` and `--hotkey` flags in the plan are new work. Current copied-profile status is a typed `NotRun` value, not implemented proof of a real profile-copy run.

## Existing tests and verification organization

Most radial/GUI tests are inline module tests, not top-level `tests/radial_*.rs` binaries. Reuse them. Existing exact-chord fake-time coverage is in `src/hotkey/launcher_invocation.rs`, including `exact_shift_alt_win_end_chord_uses_fake_time_for_tap_and_hold`. The native suite is separate and has focused ROOT/Designer cases but is F11-specific; its waited sequences are not uninterrupted mapped-chord bursts.

Confirmed top-level integration targets relevant to this change include `focus_visibility`, `gui_visibility`, `hotkey_events`, `hide_after_run`, `follow_mouse`, `trigger_visibility`, `plugin_commands`, `plugin_routing`, `preserve_command`, `query_autocomplete`, `notes_plugin`, `history`, `window_manager`, `settings_plugin`, `mkmacro_launcher_integration`, and `mouse_gestures_service`. Cargo also explicitly declares `domain` and `plugin_queries` suites. There is no `.config/nextest.toml` in this archive. Avoid inventing a target such as `radial_hotkeys` without adding it intentionally and updating the manifest.

## Provenance and boundaries

The nested `docs/references/Radial menu v4.zip`, `docs/references/RadifyClass-RadifySkinEditor-main.zip`, and screenshot files are available as design references. The current plan does not assume that reference code/assets can be copied wholesale or that they replace the native Rust implementation. Reinspect a particular reference feature only when implementing that scoped behavior.

`AGENTS.md` already requires explicit milestones, a single implementation writer, appropriate tests, full Nextest verification, and independent review. The `.codex/agents` configuration is part of the supplied source; it is not changed by this packet. Preserve the configured roles rather than introducing new model names or changing settings as incidental planning work.

## Inspected-file fingerprints

| Archive-relative file | SHA-256 |
|---|---|
| `AGENTS.md` | `0df35e92dda2036f6e46100a683f21ad652cc381a02b9ceb04d70b959c0f8723` |
| `Cargo.lock` | `2f3128b4276d9d9033d2f491c2dea14c73a90022f26d3cca2086827b6c3032b0` |
| `Cargo.toml` | `90202cd690d28d5bd5c8c1961b0421d7e3e86fb0d84bc9ad97b174f24efd1234` |
| `src/bin/radial_acceptance.rs` | `dc037c20b2d05fdfde379937461e02c275650257b71cf004a280be058e0af65a` |
| `src/bin/radial_acceptance/native.rs` | `5ce95488e0e18ef6cac14a01759e671a4d48f72cf9f6733561efafc5768c3306` |
| `src/commands/outcome.rs` | `10a5e7c73df7890ce5f12b5c9d2f810bdb86eb9f8870305b4a22d6dfc9e4e24f` |
| `src/dashboard/widgets/quick_tools.rs` | `735cb2ca25f877684695ee7b354ecb75c77b3e294cddb2ddf616b2aa9d48416f` |
| `src/gui/command_host.rs` | `476672dea4adf93a36c66e3e4f515783dd9399f73c4d1f050a8d1be2f5e5368e` |
| `src/gui/radial_actions.rs` | `b6027ee184a531cccf0ef2e852f0b48b3d645cb2292a6f4ac3d0c193f7524d15` |
| `src/gui/radial_editor/mod.rs` | `6c014ac051b400345e1957d6da3323d3908496ab682e83fcc9490292bc48e924` |
| `src/gui/search.rs` | `7c438820dcdbb07d68593e9e225ce45d060ec12c45a9e65c3a3d213f34cb8f19` |
| `src/gui/universal_action_catalog.rs` | `1c70cc4115cc86ee08d4129a6d4079a4d346b5578bc2c3e2dd9b9f72cd00ea25` |
| `src/gui/universal_action_executor.rs` | `594898b4ba8a56553ee78d80aa8d1fcb6e5cc8c78d4f0fffd78e160c0cc6575e` |
| `src/main.rs` | `4ee9ddad71630741529ad25a9e8d9c48d13721aba7d840905fa53787079a5822` |
| `src/radial/controller.rs` | `953368867f4e20cc65913d0b7479dbe2447d851ec0523de19d9cb40347734379` |
| `src/radial/handoff.rs` | `5396d4b5cb8e1175fdcefd4259608cdd3c9a3f5eab21ca494968ac7b569e6988` |
| `src/radial/invocation.rs` | `cb94081f9ba844653e5c29e09cbc3bdf70f8da3d660f771c9064eea5c52a2d02` |
| `src/radial/migration.rs` | `1a4a5ce030b8d5d8319e1b16d1b586d2590861e0514d12b6bcc415c955906717` |
| `src/radial/model.rs` | `a5a7669e5e11f70c3dab10f86fcaad487e636f849e37d4084635223fa365254a` |
| `src/visibility.rs` | `5ebd4246f19cd0924f1c18bc56323e55bb234afd3c62d77843716fbb5cca1b8b` |
| `src/window_manager.rs` | `0cd5f40965ff37d3744d032edcf6baa27310498e002de4062524cf7523a4c5ab` |
