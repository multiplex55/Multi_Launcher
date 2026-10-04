# Radial Input Recovery — Current Source Notes

**Inspected archive:** `multi_launcher(20260922-180803).zip`  
**SHA-256:** `9827eb52e35ba59cd8123a240cc89b8c013de8c14591acde44198c9c4e238cb9`  
**Purpose:** Support the approved code-first input recovery/basic authoring goal. These are observations, not implemented fixes or a native diagnosis.

The archive was inspected read-only. No Cargo check/build/Nextest, Windows GUI execution, user-profile change, or reference script execution was performed while creating this handoff. The current checkout remains the implementation authority; preserve legitimate later work and the original immutable feature baseline. No live Git commit was resolved here.

## Latest user observations

The main Multi Lnchr fails its short-tap toggle specifically while active/focused. The requested behavior is grid-only toggling on every eligible short tap, including with main focus. Do not retain the older assumption that Designer must be open for failure: the new answer did not provide a controlled open/closed matrix result.

Both Designer modes display ordinary-looking controls but accept no client clicks; Tab does not visibly move focus. Their title bar/border can move/resize the window. This happens with the grid visible or hidden. Native preview state remains uncertain. Actual executable/data paths were not supplied beyond a test-directory context. The mapped chord remains Shift+Alt+Win+End with full release; producer and a captured trace are not established.

These facts do not prove a disabled HWND, absent input event, blocked GUI thread, or common cause with the root toggle. Follow the production input/readiness/mutation chain and the already-present bounded trace. Do not patch an assumed cause merely because an earlier version had that defect.

## 1. Explicit root targeting already exists

The root visibility wrapper explicitly targets ROOT for both commands and repaint. It also emits the existing root-command/sample trace. Reintroducing the same wrapper is not a new fix. Trace where the actual focused native outcome diverges.

### `src/visibility.rs:142–164`

```text
142: impl ViewportCtx for RootViewportCtx {
143:     fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
144:         let Some(command) = trace_root_command(&cmd) else {
145:             return self.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, cmd);
146:         };
147:         let correlation = if acceptance_trace::enabled() {
148:             acceptance_trace::root_command_correlation()
149:         } else {
150:             Correlation::default()
151:         };
152:         acceptance_trace::emit(Event::RootCommand {
153:             command,
154:             correlation,
155:         });
156:         acceptance_trace::request_window_sample(correlation);
157:         self.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, cmd);
158:     }
159: 
160:     fn request_repaint(&self) {
161:         self.ctx.request_repaint_of(egui::ViewportId::ROOT);
162:     }
163: }
164: 
```

## 2. Replayed focus and keep-open were already corrected

The deferred callback consumes the live pending focus flag. The editor also has an idempotent ensure_open() for already-open maintenance. Preserve these prior corrections.

### `src/gui/radial_editor/mod.rs:1224–1233`

```text
1224:             let focus_requested = std::mem::take(&mut editor.viewport_focus_pending);
1225:             if focus_requested {
1226:                 acceptance_trace::emit(Event::DesignerFocus {
1227:                     edge: FocusEdge::Requested,
1228:                     viewport: trace_viewport,
1229:                     correlation,
1230:                 });
1231:             }
1232:             if editor.viewport_restore_pending {
1233:                 let restored = work_area::restore_geometry(
```

## 3. Disposable authoring work already retires on draft changes

The current authoring session invalidates draft-bound disposable work and outdated native preview work. Durable/session-stable request distinctions must not be removed to force input or close behavior.

### `src/radial/authoring.rs:1289–1314`

```text
1289:     fn invalidate_draft_bound_pending_work(&mut self) {
1290:         if self.pending_request.is_some_and(|pending| {
1291:             pending.editor_session == self.editor_session
1292:                 && pending.kind.invalidated_by_draft_generation_change()
1293:         }) {
1294:             self.pending_request = None;
1295:         }
1296:         if self.pending_native_preview.is_some_and(|pending| {
1297:             pending.editor_session == self.editor_session
1298:                 && matches!(
1299:                     pending.kind,
1300:                     PendingRequestKind::StartNativePreview
1301:                         | PendingRequestKind::UpdateNativePreview
1302:                 )
1303:         }) {
1304:             self.pending_native_preview = None;
1305:             self.pending_native_context_sample = false;
1306:             self.native_preview_lease = None;
1307:             // A late Start reply may already have opened the native host even
1308:             // though its correlation and any local lease are now obsolete.
1309:             // Keep this flag set so the GUI can issue the terminal Stop
1310:             // request on its next sync.
1311:             self.native_preview_may_be_open = true;
1312:         }
1313:     }
1314: 
```

## 4. Readiness/conflict is explicitly classified

The existing trace can identify InitialSnapshot, Conflict, or Enabled for the body when pointer events reach the callback. The actual load/conflict state must be observed rather than inferred from ordinary-looking painted controls.

### `src/gui/radial_editor/mod.rs:1805–1818`

```text
1805:         let body_state = if initial_snapshot_pending {
1806:             BodyBlock::InitialSnapshot
1807:         } else if conflict_reason.is_some() {
1808:             BodyBlock::Conflict
1809:         } else {
1810:             BodyBlock::Enabled
1811:         };
1812:         if pointer_down || pointer_up {
1813:             acceptance_trace::emit(Event::DesignerBody {
1814:                 state: body_state,
1815:                 correlation: trace_correlation(self.session.as_ref()),
1816:             });
1817:         }
1818: 
```

## 4b. The Designer controls are gated

Both modes use the same readiness condition. Removing it would permit editing a placeholder/conflicting snapshot; fix the unresolved boundary rather than bypassing it.

### `src/gui/radial_editor/mod.rs:1841–1858`

```text
1841:             ui.add_enabled_ui(
1842:                 !initial_snapshot_pending && conflict_reason.is_none(),
1843:                 |ui| {
1844:                     self.designer_controls(ui);
1845:                     self.pending_drop_ui(ui);
1846:                     let pane = designer_pane_layout(
1847:                         ui.available_size(),
1848:                         self.tree_visible,
1849:                         self.inspector_visible,
1850:                         self.tree_width,
1851:                         self.inspector_width,
1852:                     );
1853:                     if self.show_resources {
1854:                         // Skins/assets are the main bounded content in Skins
1855:                         // mode.  They replace the full-height canvas rather
1856:                         // than being appended below it, so every control is
1857:                         // reachable through this internal scroll region at
1858:                         // compact viewport sizes.
```

## 5. Existing bounded trace is available

Use the existing environment variable and actual durable log path. Its 256-event budget means a missing event is meaningful only after confirming activation, capture, and remaining budget. Do not add another broad tracing subsystem or endless instrument-only loop.

### `src/radial/acceptance_trace.rs:13–17`

```text
13: pub(crate) const ENVIRONMENT_VARIABLE: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE";
14: pub(crate) const EVENT_BUDGET: usize = 256;
15: const TRACE_TARGET: &str = "multi_launcher.radial_acceptance";
16: 
17: #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
```

## 6. Basic menu/ring controls live in the optional tree

New Menu and Add Ring already route to the authoring backend but are only exposed here. Their Results are discarded at these callers. Move/reuse these operations in the ordinary toolbar and surface errors; do not create a second data model.

### `src/gui/radial_editor/mod.rs:3327–3345`

```text
3327:         ui.horizontal(|ui| {
3328:             if ui.button("New menu").clicked() {
3329:                 let _ = menu::create_menu_with_defaults(
3330:                     session,
3331:                     "menu",
3332:                     "New menu",
3333:                     defaults.default_interaction,
3334:                     defaults.default_submenu_presentation,
3335:                 );
3336:             }
3337:             if ui.button("Add ring").clicked() {
3338:                 if let Some(menu_id) = selected_menu_id(session) {
3339:                     let _ = menu::add_ring(session, &menu_id);
3340:                 }
3341:             }
3342:         });
3343:     }
3344: 
3345:     fn inspector(&mut self, ui: &mut egui::Ui, frame: &DesignerFrameContext) {
```

## 7. Slot edits use local drafts, but some failures are silent

The current Cells draft is applied when editing ends. A failed resize_plan is ignored; an unprompted apply_resize result is also discarded. Preserve safe shrink resolution while adding explicit proposal preview, error feedback, and ordinary-toolbar access.

### `src/gui/radial_editor/mod.rs:3648–3669`

```text
3648:         let resize_response = ui.add(egui::DragValue::new(requested).prefix("Cells "));
3649:         let resize_ended = resize_response.lost_focus()
3650:             || resize_response.drag_stopped()
3651:             || (resize_response.changed()
3652:                 && !resize_response.has_focus()
3653:                 && !resize_response.dragged());
3654:         if resize_ended {
3655:             let requested = self.ring_resize_drafts.remove(&resize_key).unwrap_or(count);
3656:             if let Ok(plan) = menu::resize_plan(&session.draft, &menu_id, &ring_id, requested) {
3657:                 if plan.requires_resolution() {
3658:                     self.resize_prompt = Some(plan);
3659:                 } else {
3660:                     let _ = menu::apply_resize(session, plan, None);
3661:                 }
3662:             }
3663:         } else if !resize_response.has_focus() && !resize_response.dragged() {
3664:             self.ring_resize_drafts.remove(&resize_key);
3665:         }
3666:         if ui.button("Delete ring").clicked() {
3667:             if count == 0 {
3668:                 let _ = menu::delete_ring(session, &menu_id, &ring_id);
3669:             } else {
```

## 8. New-ring geometry is ordinal based

The factory adds eight fresh spacer cells (earlier in the function), then computes radius from the number of existing rings. This does not account for every customized existing ring/style. Automatic authoring proposals must use actual current geometry and the shared validator.

### `src/radial/authoring/menu.rs:336–352`

```text
336:     let ordinal = menu.rings.len() as f32;
337:     menu.rings.push(RingDefinition {
338:         id: id.clone(),
339:         radius: 92.0 + ordinal * 64.0,
340:         cell_radius: 28.0,
341:         rotation_degrees: -90.0,
342:         gap: 4.0,
343:         cells,
344:         style: Default::default(),
345:     });
346:     session
347:         .replace_document_atomic(document)
348:         .map_err(|_| MenuEditError::MissingEntity)?;
349:     session.select(Some(StableSelection::Ring {
350:         menu_id: menu_id.clone(),
351:         ring_id: id.clone(),
352:     }));
```

## 9. Open in Inspector currently behaves like Cancel

The action sets cancel, and the subsequent cancellation branch clears the popup/draft. It neither reveals nor selects the Inspector. An explicit preserve/resolve/handoff is required; this is an actual wiring defect independent of the whole-client-input failure.

### `src/gui/radial_editor/mod.rs:2228–2243`

```text
2228:                     if ui.button("Open in Inspector").clicked() {
2229:                         cancel = true;
2230:                     }
2231:                 });
2232:             });
2233:         if !popup_open {
2234:             cancel = true;
2235:         }
2236:         if cancel {
2237:             self.properties_popup = None;
2238:             self.properties_draft = None;
2239:             return;
2240:         }
2241:         if apply {
2242:             let draft = self
2243:                 .properties_draft
```

## 10. Compact action catalog is truncated before user search

The compact popup requests an empty catalog filter and takes the first 50 rows. Search the current catalog before any rendering cap; preserve stable assignments and side-effect-free browsing. Do not repurpose the root launcher query to drive it.

### `src/gui/radial_editor/mod.rs:2099–2111`

```text
2099:         let action_rows =
2100:             crate::gui::universal_action_catalog::UniversalActionAuthoringCatalog::build(
2101:                 &frame.action_catalog,
2102:                 &invocation_context,
2103:                 "",
2104:             )
2105:             .rows()
2106:             .iter()
2107:             .take(50)
2108:             .cloned()
2109:             .collect::<Vec<_>>();
2110:         let submenu_choices = self
2111:             .session
```

## 11. Current model limits and file identities

These are authored-data limits, not a promise that 128 large circles fit on the current monitor. Dynamic source projection and runtime pagination remain separate. Geometry suggestions must respect these existing limits rather than increasing them silently.

### `src/radial/model.rs:7–24`

```text
7: pub const CURRENT_SCHEMA_VERSION: u32 = 2;
8: pub const RADIAL_FILE: &str = "radial.json";
9: pub const RADIAL_ASSETS_DIRECTORY: &str = "radial_assets";
10: 
11: pub mod limits {
12:     pub const MAX_MENUS: usize = 256;
13:     pub const MAX_RINGS_PER_MENU: usize = 16;
14:     pub const MAX_CELLS_PER_RING: usize = 128;
15:     pub const MAX_TOTAL_CELLS: usize = 8_192;
16:     pub const MAX_SUBMENU_DEPTH: usize = 16;
17:     pub const MAX_SKINS: usize = 128;
18:     pub const MAX_CONTEXT_RULES: usize = 512;
19:     pub const MAX_CUSTOM_TRIGGERS: usize = 128;
20:     pub const MAX_ASSETS: usize = 2_048;
21:     pub const MAX_MEDIA_SEARCH_ROOTS: usize = 32;
22:     pub const MAX_ITEM_SHORTCUTS_PER_CELL: usize = 16;
23:     pub const MAX_ITEM_HOTSTRINGS_PER_CELL: usize = 16;
24:     pub const MAX_TEXTURE_DIMENSION: u32 = 8_192;
```

## 12. Current circular validation already checks adjacent spacing

The proposal should reuse production effective-style/layout validation. The sine-spacing bound is useful when proposing a radius but is not a replacement for center/inter-ring/effective-style/work-area checks. Preserve wedge-specific semantics.

### `src/radial/validation.rs:458–475`

```text
458:                             "effective style causes adjacent circular cells to overlap",
459:                         ));
460:                     }
461:                 }
462:             }
463:             if menu.layout == LayoutKind::CircularCells && n >= 2 {
464:                 let spacing = 2.0 * ring.radius * (std::f32::consts::PI / n as f32).sin();
465:                 if spacing + 0.001 < 2.0 * ring.cell_radius + ring.gap {
466:                     errors.push(issue(
467:                         format!("{rp}.cells"),
468:                         "adjacent circular cells overlap or violate the requested gap",
469:                     ));
470:                 }
471:             }
472:             for (ci, cell) in ring.cells.iter().enumerate() {
473:                 let cp = format!("{rp}.cells[{ci}]");
474:                 validate_media_override(
475:                     &cell.icon,
```

## Other current integration points inspected

- `src/gui/radial_editor/mod.rs::show_deferred` builds the action catalog before checking whether the editor is open. This is avoidable preparation to address narrowly when appropriate, not evidence that it caused all input to fail.
- `src/radial/geometry.rs` uses effective item sizing, radius/menu scales, work-area fit, and fixed-center/root placement. Proposals must match these actual semantics, not assume every visual override affects geometry in the same way.
- `src/main.rs` resolves the startup settings/data root and enforces single-instance ownership. Do not compare a new on-disk executable to behavior from a different already-running instance.
- `docs/plans/radial-stabilization.md` records the September 22 corrective pass as diagnostic and user-profile native acceptance as NOT RUN. Its historical developer F2, source SHA, build hash, and focused counts are not newly verified results here.
- `AGENTS.md` calls for bounded milestones and validated changes. This handoff uses one code-first implementation milestone with work packages and an integrated final gate, not test-heavy per-widget milestones. Current user-approved light development testing and one final required full suite are both preserved.

## What remains unproven

No captured trace from the actual failing profile was supplied in the answers. The native first broken transition and its root cause remain to be identified. No new UI interaction, geometry proposal, performance, or test pass is claimed. Deliver real code changes and the requested usable workflow; use targeted evidence to choose the input repair, not another speculative patch or completion percentage.
