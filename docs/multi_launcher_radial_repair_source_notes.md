# Radial repair — source notes and evidence limits

**Inspected archive:** `multi_launcher(20260916-004931).zip`  
**SHA-256:** `d489a8d8d2248694062c88258421ea92aa5a60cbf083f1243c0a2afd0c310529`  
**Purpose:** Ground the approved runtime/editor repair brief in the latest supplied code. These notes do not replace the existing immutable Git baseline. The implementer must use the current checkout and preserve newer legitimate changes.

This review read the source and existing review notes. It did not run Cargo, Nextest, the application, Win32 interaction tests, or reference AHK scripts. Source observations below identify implementation paths and plausible causes; they are not claims that the reported symptoms have been reproduced or fixed on Windows.

The new **10/15/20-minute job observation schedule** is in the master repair brief and startup prompt. It supersedes the earlier source-review/questionnaire timing without changing any application timer.

## S1 — Queued GUI events and repaint delivery

Observed: this shared sender appends an event to registered channels but contains no repaint callback. The main controller sends RadialPrepare through this route; GUI update drains it, and the reply wakes main. This is consistent with the mouse-activity symptom. It does not prove all focused-hide failures have the same cause. Extend the existing delivery boundary rather than adding periodic UI polling.

### `src/gui/mod.rs:280–292`

```text
 280: pub fn register_event_sender(tx: Sender<WatchEvent>) {
 281:     if let Ok(mut guard) = APP_EVENT_TXS.lock() {
 282:         guard.push(tx);
 283:     }
 284: }
 285: 
 286: pub fn send_event(ev: WatchEvent) {
 287:     if let Ok(mut guard) = APP_EVENT_TXS.lock() {
 288:         guard.retain(|tx| tx.send(ev.clone()).is_ok());
 289:     }
 290: }
 291: 
 292: #[cfg(not(test))]
```

## S1b — Preparation is handled in the GUI drain

The receiver processes native preparation and dispatch requests here. An isolated direct call to prepare_radial in a unit test bypasses the queue/wakeup path being repaired.

### `src/gui/watch.rs:42–52`

```text
  42:     pub fn process_watch_events(&mut self) {
  43:         while let Ok(ev) = self.rx.try_recv() {
  44:             match ev {
  45:                 WatchEvent::RadialDispatch(request) => self.execute_radial_dispatch(request),
  46:                 WatchEvent::RadialPrepare(envelope) => self.prepare_radial(envelope),
  47:                 WatchEvent::RadialInvalidate => self.invalidate_radial_leases(),
  48:                 WatchEvent::RadialConfigDiagnostic(diagnostic) => {
  49:                     if let Some(diagnostic) = diagnostic {
  50:                         self.report_error_message(
  51:                             "radial.reload",
  52:                             format!("Radial menu configuration was not reloaded: {diagnostic}"),
```

## S1c — Replies explicitly wake the main consumer

The response path already carries a reply and a wake handle. The repair must account for both directions and for Designer-owned replies after the UI moves to another viewport.

### `src/gui/radial_actions.rs:660–673`

```text
 660:         let _ = envelope.reply.send(RadialPrepareReply {
 661:             generation: request.generation,
 662:             invocation_id: request.invocation_id,
 663:             menu_id,
 664:             unavailable,
 665:             dynamic,
 666:             frame,
 667:             static_cells,
 668:             frames,
 669:         });
 670:         let _ = envelope.wake.send(());
 671:     }
 672:     /// Resolve a stable radial binding against the catalogs that are current
 673:     /// at dispatch time. This intentionally rebuilds exact identities; stored
```

## S2 — Current active-radial press contract differs from the new requirement

Observed: a new ChordPressed while RadialActive immediately closes the radial. The user has now explicitly replaced that behavior with tap toggles grid / hold toggles radial. The active menu lifecycle must survive a new pending short-tap cycle; this requires updating the reducer and its tests, not only a label or delay.

### `src/radial/invocation.rs:296–307`

```text
 296:                 InvocationState::RadialActive { session_id, .. } => {
 297:                     let session_id = session_id.clone();
 298:                     self.state = InvocationState::AwaitingOwnedRelease {
 299:                         id,
 300:                         reason: DrainReason::Dismissed,
 301:                     };
 302:                     vec![I::CloseRadial { session_id }]
 303:                 }
 304:                 _ => vec![],
 305:             },
 306:             E::ModifierChanged { .. } => vec![],
 307:             E::Deadline { id, at, generation } => {
```

## S2b — The existing visibility toggle is not explicitly focus-gated

Observed: this path inverts the visible flag without checking whether the grid is focused. Focused-hide diagnosis must trace actual input delivery, event consumption, and later restore/show requests instead of adding an unfocus workaround.

### `src/visibility.rs:49–75`

```text
  49:     let mut changed = false;
  50:     if trigger.take() {
  51:         let old = visibility.load(Ordering::SeqCst);
  52:         let next = !old;
  53:         tracing::debug!(from=?old, to=?next, "visibility updated");
  54:         visibility.store(next, Ordering::SeqCst);
  55:         changed = old != next;
  56:         if let Ok(guard) = ctx_handle.lock() {
  57:             if let Some(c) = &*guard {
  58:                 apply_visibility(
  59:                     next,
  60:                     VisiblePlacementPolicy::ApplyConfiguredPlacement,
  61:                     c,
  62:                     offscreen,
  63:                     follow_mouse,
  64:                     static_enabled,
  65:                     static_pos,
  66:                     static_size,
  67:                     window_size,
  68:                 );
  69:                 if next {
  70:                     restore_flag.store(true, Ordering::SeqCst);
  71:                 }
  72:                 *queued_visibility = None;
  73:                 tracing::debug!("Applied queued visibility: {}", next);
  74:             } else {
  75:                 *queued_visibility = Some(next);
```

## S3 — Child navigation mixes presentation, requested anchor, and cursor geometry

Observed: the current path selects child.submenu_presentation, re-reads desktop geometry, and uses requested_anchor for SameCenter. The repair defines current-menu child-presentation scope and a fixed visible session center instead.

### `src/radial/controller.rs:1912–1938`

```text
1912:         let (desktop_anchor, work, scale) = desktop_geometry();
1913:         let anchor = match child.submenu_presentation {
1914:             SubmenuPresentation::SameCenter => active.layout.requested_anchor,
1915:             SubmenuPresentation::Cascade => active
1916:                 .layout
1917:                 .cells
1918:                 .iter()
1919:                 .find(|layout| &layout.cell_id == cell_id)
1920:                 .map(|layout| shape_center(&layout.shape, active.layout.origin, scale))
1921:                 .unwrap_or(desktop_anchor),
1922:         };
1923:         let Ok(mut layout) =
1924:             layout_document_menu(&self.document, &child, anchor, work, scale, 0.55)
1925:         else {
1926:             out.push(ControllerEvent::Error(
1927:                 "radial submenu layout failed".into(),
1928:             ));
1929:             return;
1930:         };
1931:         if let Some(frame) = prepared_child.as_ref() {
1932:             augment_special_cells(&mut layout, &frame.cells, &frame.menu);
1933:             apply_prepared_availability(&mut layout, frame);
1934:         }
1935:         if child.submenu_presentation == SubmenuPresentation::Cascade {
1936:             layout = cascade_layout(&active.layout, layout);
1937:         }
1938:         let pointer = active.pointer;
```

## S3b — Cascade coordinate conversion needs a unit audit

Observed: shape_center converts the shape center by scale and adds origin. The surrounding geometry constructs centers in desktop-logical space, so this is a strong lead for an extra-origin error. The implementation must confirm input/output units and test nonzero/negative origins rather than merely deleting an offset without tracing it.

### `src/radial/controller.rs:2429–2450`

```text
2429: fn shape_center(
2430:     shape: &super::geometry::HitShape,
2431:     origin: PhysicalPoint,
2432:     scale: ScaleFactor,
2433: ) -> PhysicalPoint {
2434:     let logical = match shape {
2435:         super::geometry::HitShape::Circle { center, .. }
2436:         | super::geometry::HitShape::Wedge { center, .. } => *center,
2437:     };
2438:     let offset = scale.logical_to_physical(logical);
2439:     PhysicalPoint {
2440:         x: origin.x + offset.x,
2441:         y: origin.y + offset.y,
2442:     }
2443: }
2444: 
2445: fn cascade_layout(parent: &LayoutSnapshot, mut child: LayoutSnapshot) -> LayoutSnapshot {
2446:     let mut ancestors = parent.cells.clone();
2447:     for cell in &mut ancestors {
2448:         cell.actionable = false;
2449:     }
2450:     ancestors.extend(child.cells);
```

## S4 — Tooltip preparation starts from label preparation

Observed: tooltip text is prepared in the cell-resource loop. In this snapshot the tooltip layout is derived from the existing narrow text request; the master brief requires a separate width/purpose and the full original label. Re-enabling a tooltip flag alone is not sufficient.

### `src/radial/preparation.rs:145–183`

```text
 145:                 family: (!cell.visual.font_family.is_empty())
 146:                     .then(|| cell.visual.font_family.clone()),
 147:                 size_milli: (cell.visual.font_size.max(1.0) * 1_000.0) as u32,
 148:                 bold: cell.visual.bold,
 149:                 italic: cell.visual.italic,
 150:                 dpi_milli: variant.dpi_milli,
 151:                 max_width_milli: (layout.style.item_size.max(1.0)
 152:                     * cell.visual.text_box_scale
 153:                     * 1_000.0) as u32,
 154:             };
 155:             let prepared_label = service.prepare(&cell.label, request.clone());
 156:             diagnostics.extend(
 157:                 prepared_label
 158:                     .diagnostics
 159:                     .iter()
 160:                     .map(|diagnostic| format!("radial font for {}: {diagnostic:?}", cell.cell_id)),
 161:             );
 162:             resources.text.insert(cell.cell_id.clone(), prepared_label);
 163:             if let Some(definition) = menu
 164:                 .rings
 165:                 .iter()
 166:                 .flat_map(|ring| &ring.cells)
 167:                 .find(|candidate| candidate.id == cell.cell_id)
 168:             {
 169:                 let explicit = match &definition.tooltip {
 170:                     Override::Value(value) if !value.is_empty() => Some(value.as_str()),
 171:                     _ => None,
 172:                 };
 173:                 let tooltip = match cell.visual.tooltip_mode {
 174:                     super::model::TooltipMode::Disabled => None,
 175:                     super::model::TooltipMode::Explicit => explicit,
 176:                     super::model::TooltipMode::Automatic => explicit.or(Some(cell.label.as_str())),
 177:                 };
 178:                 if let Some(tooltip) = tooltip {
 179:                     resources
 180:                         .tooltips
 181:                         .insert(cell.cell_id.clone(), service.prepare(tooltip, request));
 182:                 }
 183:             }
```

## S4b — Tooltip rendering bounds are small

Observed: tooltip rendering uses a compact rectangle near the active cell. The corrected rectangle must use prepared wrapped tooltip dimensions without changing the wheel center or interactive region.

### `src/radial/render.rs:349–367`

```text
 349:                     x: bounds.min.x,
 350:                     y: bounds.max.y + 4.0,
 351:                 },
 352:                 max: LogicalPoint {
 353:                     x: bounds.max.x.max(bounds.min.x + 80.0),
 354:                     y: bounds.max.y + 24.0,
 355:                 },
 356:             };
 357:             primitives.push(VectorPrimitive::Tooltip {
 358:                 bounds: tooltip_bounds,
 359:                 text: Arc::clone(tooltip),
 360:                 background: Rgba(10, 11, 14, 235),
 361:                 color: Rgba(248, 248, 248, 255),
 362:             });
 363:         }
 364:     }
 365:     push_image(
 366:         &mut primitives,
 367:         resources,
```

## S5 — Embedded editor and default window sizing

Observed: the current editor renders as an egui Window inside the existing context. This is not the independently resizable OS Designer window the user now approved.

### `src/gui/radial_editor/mod.rs:370–384`

```text
 370:             .as_ref()
 371:             .and_then(|session| session.selection.clone());
 372:         let mut window_open = true;
 373:         egui::Window::new("Radial Menu Editor")
 374:             .id(egui::Id::new("radial-menu-editor"))
 375:             .open(&mut window_open)
 376:             .default_size(egui::vec2(1100.0, 720.0))
 377:             .show(ctx, |ui| {
 378:                 self.toolbar(ui);
 379:                 ui.separator();
 380:                 let Some(session) = self.session.as_mut() else {
 381:                     ui.spinner();
 382:                     return;
 383:                 };
 384:                 if let Some(conflict_reason) = session
```

## S5b — Equal-width three-column layout

Observed: tree, preview, and inspector receive equal columns. The replacement uses manually sized optional side panes and a clipped canvas, retaining all current controls.

### `src/gui/radial_editor/mod.rs:424–448`

```text
 424:                 }
 425:                 ui.add_enabled_ui(!initial_snapshot_pending, |ui| {
 426:                     self.preview_controls(ui);
 427:                     ui.columns(3, |columns: &mut [egui::Ui]| {
 428:                         self.tree(&mut columns[0], &feature_defaults);
 429:                         self.preview.ui(
 430:                             &mut columns[1],
 431:                             &draft,
 432:                             generation,
 433:                             self.preview_zoom,
 434:                             self.preview_preset,
 435:                             preview_selection.as_ref(),
 436:                             prepared_preview.as_deref(),
 437:                         );
 438:                         self.inspector(&mut columns[2], app);
 439:                     });
 440:                     if self.show_resources {
 441:                         ui.separator();
 442:                         self.resources_ui(ui);
 443:                     }
 444:                 });
 445:             });
 446:         if !self
 447:             .session
 448:             .as_ref()
```

## S5c — Tree defaults open

Observed: menu headers default open. Explicit remembered collapse/expand state is required; selection and diagnostics must not force nodes open.

### `src/gui/radial_editor/mod.rs:1181–1189`

```text
1181:                 let header = egui::CollapsingHeader::new(&menu.name)
1182:                     .id_source(menu::widget_key("menu", menu.id.as_str(), "tree"))
1183:                     .default_open(true)
1184:                     .show(ui, |ui| {
1185:                         for ring in &menu.rings {
1186:                             let ring_selection = StableSelection::Ring {
1187:                                 menu_id: menu.id.clone(),
1188:                                 ring_id: ring.id.clone(),
1189:                             };
```

## S6 — Native class cursor setup

Observed: the inspected native class definition leaves unspecified fields at default. A static search of native.rs found no WM_SETCURSOR/LoadCursor/SetCursor handler. This is a plausible contributor to the inherited busy pointer, not proof that there is no blocking work.

### `src/radial/native.rs:1386–1406`

```text
1386:         let class: Vec<u16> = "MultiLauncherRadialHost\0".encode_utf16().collect();
1387:         let instance = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|e| e.to_string())?;
1388:         REGISTER
1389:             .get_or_init(|| {
1390:                 let wc = WNDCLASSW {
1391:                     hInstance: instance.into(),
1392:                     lpszClassName: PCWSTR(class.as_ptr()),
1393:                     lpfnWndProc: Some(wndproc),
1394:                     ..Default::default()
1395:                 };
1396:                 if unsafe { RegisterClassW(&wc) } == 0 {
1397:                     Err(format!(
1398:                         "failed to register radial host window class: {}",
1399:                         windows::core::Error::from_win32()
1400:                     ))
1401:                 } else {
1402:                     Ok(())
1403:                 }
1404:             })
1405:             .clone()?;
1406:         let (x, y, width, height) = physical_scene_bounds(scene.bounds, layout.scale_factor);
```

## S7 — Existing authoring Cancel semantics must not be replaced casually

The existing regression test names a checked Cancel-after-Apply revert. The requested change is layout/presentation/direct manipulation, not a redesign of the authoring transaction semantics. Preserve this contract and the actual implementation while moving the editor.

### `src/radial/authoring.rs:2686–2701`

```text
2686:     fn cancel_after_apply_requests_checked_revert_of_last_apply() {
2687:         let mut session = RadialAuthoringSession::new(snapshot("A", 1));
2688:         let id = session.draft.menus[0].id.clone();
2689:         session
2690:             .mutate(
2691:                 DocumentMutation::RenameMenu {
2692:                     id,
2693:                     name: "B".into(),
2694:                 },
2695:                 None,
2696:                 EditPhase::Atomic,
2697:             )
2698:             .unwrap();
2699:         let apply = session.request_commit(CommitDisposition::Apply).unwrap();
2700:         assert!(session.accept_reply(AuthoringReply::Published {
2701:             id: apply.id(),
```

## S8 — Local RM4 designer reference

The archive `docs/references/Radial menu v4.zip` contains `Radial menu v4/Internal/Codes/RMD classes.ahk` (82,364 bytes; SHA-256 `45c77824291e135955c336ecf9010bc023c87b5dec19c6f57df5655b4f4bec30`). It was read in place as text, not run or extracted into project source.

The longer earlier source review records center-to-slot and existing-item drag handlers in that file. Those are the inspected basis for using a slot-first/direct-manipulation workflow. The new center-click '+', compact properties, explicit safe swap, and separate Design/Runtime semantics are **approved requirements**, not claims that every one is already implemented in the current Rust editor or identical to RM4.

No third-party artwork/font/source is included in this handoff package. Reference material remains under `docs/references/` and must retain its applicable rights/attribution treatment.

## External API evidence, kept separate

The master brief references the pinned egui Context documentation for cross-thread repaint and deferred viewport ownership, Microsoft documentation for cursor handling, and Nextest documentation for captured reporting. Those sources explain API behavior, not test results in this repository.

## Evidence still required from implementation

Actual physical shared-chord handling with hidden/focused grid, no-mouse event progress, visible same-center transitions and Back, non-busy cursor, full nonintercepting tooltips, independent Designer operation, compact layout at modest dimensions, safe direct authoring, passing Cargo/Nextest, and performance/resource behavior all require their stated verification. Do not relabel these observations as passing live tests.
