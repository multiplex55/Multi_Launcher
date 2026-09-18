# Multi Launcher — Radial Stabilization Source Notes

## Evidence scope

These notes support `multi_launcher_radial_stabilization_codex_plan.md`. The application ZIP and supplied RM4 scripts were inspected as source/data, and the broken Designer screenshot was examined visually. No Cargo build, Nextest suite, native Windows runtime, reference script, or benchmark was executed in preparing this handoff. No application source was changed.

The actual current checkout remains the implementation authority. Preserve the already-recorded immutable feature baseline and record a separate stabilization-start HEAD/diff before editing. This archive is not a live Git repository; no new Git SHA is fabricated here. Line ranges below identify the supplied snapshot, not future edits.

### File identities

- `multi_launcher(20260918-003954).zip` — 26,702,086 bytes; SHA-256 `b2b22e846b57c73ffe9a668a185fed5a09e623410fcc1402c25cf03c0523f070`.
- `Radial menu v4.zip` — 4,015,391 bytes; SHA-256 `efe57915bbc5f68fcd23d20eb66db4025d8c8fd7eaf26dd176acfb38a3fa1c19`.

### Confirmed user observations

The user reports that tap-to-hide **does not fail when Designer and native desktop preview are both closed**. The mapping is `Shift+Alt+Win+End` on a single key action, with every constituent key released between invocations. The mapping's actual firmware/software producer is unknown. Designer X and runtime radial hold-to-close misbehave; the main launcher remains responsive. Pending preview/save/import/unsaved state was uncertain. Do not transform these observations into a claim that every hotkey mode is broken or a confirmed application deadlock.

### How to interpret the findings

- **Observed code** is directly supported by the excerpts.
- **Hypothesis** requires a trace or live reproduction to establish causality.
- **Required behavior** comes from the approved answers, not an inference that the current code already does it.
- Historical ledger test results remain historical reports. They do not prove these native defects are fixed.

## S1. Designer panes inherit horizontal layout

**Observed:** Pane content is placed inside a horizontal parent using `allocate_ui`, without an explicit vertical child layout. The initial tree expansion fallback is true. This matches the screenshot's horizontal spread/character wrapping much more closely than a mere bad default window size.

**API interpretation:** Pinned egui's allocation/layout contract must be respected; a requested size is not a hard content/clip boundary. The repair must test actual Designer rectangles and rendered content, not only the presence of labels or a mathematical pane-width helper.

**Required change:** Visual board first, bounded vertical optional panes, readable labels, controlled expansion, and current backend reused. Do not enlarge/reset the main launcher to mask the defect.

### `src/gui/radial_editor/mod.rs:1497–1541`

```text
1497:                         let canvas_width =
1498:                             (available_width - tree_width - inspector_width - 12.0).max(180.0);
1499:                         ui.horizontal(|ui| {
1500:                             if self.tree_visible {
1501:                                 ui.allocate_ui(egui::vec2(tree_width, available_height), |ui| {
1502:                                     self.tree(ui, &frame.feature_defaults);
1503:                                 });
1504:                                 ui.separator();
1505:                             }
1506:                             ui.allocate_ui(egui::vec2(canvas_width, available_height), |ui| {
1507:                                 self.preview.ui(
1508:                                     ui,
1509:                                     &draft,
1510:                                     generation,
1511:                                     self.preview_zoom,
1512:                                     self.preview_preset,
1513:                                     preview_selection.as_ref(),
1514:                                     prepared_preview.as_deref(),
1515:                                     editor_session,
1516:                                     show_expected_layout_diagnostics,
1517:                                     self.designer_mode,
1518:                                     self.session.as_mut(),
1519:                                     &mut self.projected_selection,
1520:                                     &mut self.drag_payload,
1521:                                     &mut self.placement_draft,
1522:                                     &mut self.pending_drop,
1523:                                     &mut self.properties_popup,
1524:                                     &mut self.visited_path,
1525:                                     &mut self.canvas_pan,
1526:                                     &mut self.pan_drag_start,
1527:                                 );
1528:                             });
1529:                             if self.inspector_visible {
1530:                                 ui.separator();
1531:                                 ui.allocate_ui(
1532:                                     egui::vec2(inspector_width, available_height),
1533:                                     |ui| {
1534:                                         egui::ScrollArea::vertical()
1535:                                             .id_source("radial-designer-inspector")
1536:                                             .max_height(available_height)
1537:                                             .show(ui, |ui| self.inspector(ui, frame));
1538:                                     },
1539:                                 );
1540:                             }
1541:                         });
```

### `src/gui/radial_editor/mod.rs:2690–2718`

```text
2690:     fn tree(&mut self, ui: &mut egui::Ui, defaults: &crate::radial::model::RadialFeatureSettings) {
2691:         ui.heading("Menus and rings");
2692:         let drag_source = &mut self.drag_source;
2693:         let post_render = &mut self.post_render;
2694:         let focus_restore = &mut self.focus_restore;
2695:         let expanded_sections = &mut self.preferences.expanded_sections;
2696:         let preferences_dirty = &mut self.preferences_dirty;
2697:         let visited_path = &mut self.visited_path;
2698:         let Some(session) = self.session.as_mut() else {
2699:             return;
2700:         };
2701:         let menus = session.draft.menus.clone();
2702:         egui::ScrollArea::vertical().show(ui, |ui| {
2703:             for menu in &menus {
2704:                 let menu_selected =
2705:                     session.selection == Some(StableSelection::Menu(menu.id.clone()));
2706:                 let expansion_key = format!("menu:{}", menu.id);
2707:                 let default_open = expanded_sections
2708:                     .get(&expansion_key)
2709:                     .copied()
2710:                     // Keep the initial tree discoverable (and its stable
2711:                     // AccessKit names present) until the user explicitly
2712:                     // collapses this menu.  Once recorded, the preference is
2713:                     // authoritative and is never overridden by selection.
2714:                     .unwrap_or(true);
2715:                 let header = egui::CollapsingHeader::new(&menu.name)
2716:                     .id_source(menu::widget_key("menu", menu.id.as_str(), "tree"))
2717:                     .default_open(default_open)
2718:                     .open(Some(default_open))
```

## S2. Tooltip height units do not match the consumer

**Observed:** `measured_height_milli` is produced from an already-milli-unit font size by multiplying by 1,200 and the line count. The later logical-size conversion divides only by 1,000. At 13,000 milli-units, this yields 15,600 logical units for one line, instead of the existing 1.2-line-height convention's 15.6.

The placement clamp then fits that excessive box to work-area height, explaining the top-of-monitor/full-height shape. This is a source-supported dimensional defect; a native run is still needed to prove the complete user-visible repair.

**Required change:** Correct the producer/consumer unit contract, real displayed text extent, wrapping, padding, and both lower/upper numeric bounds. Do not merely clamp the wrong number.

### `src/radial/font_cache.rs:363–386`

```text
363:     let measured_width_milli = width_per_grapheme
364:         .saturating_mul(widest_graphemes as u64)
365:         .saturating_mul(1_000)
366:         .checked_div(dpi_milli)
367:         .unwrap_or(u64::MAX)
368:         .min(u32::MAX as u64) as u32;
369:     let measured_height_milli = (request.size_milli as u64)
370:         .saturating_mul(1_200)
371:         .saturating_mul(line_count.max(1) as u64)
372:         .min(u32::MAX as u64) as u32;
373:     PreparedTextLayout {
374:         source_text: text.into(),
375:         text: display.into(),
376:         selected_family: selected.into(),
377:         script,
378:         estimated_width_milli: width_per_grapheme
379:             .saturating_mul(widest_graphemes as u64)
380:             .min(u32::MAX as u64) as u32,
381:         measured_width_milli,
382:         measured_height_milli,
383:         line_count: line_count.min(u16::MAX as usize) as u16,
384:         glyphs,
385:         diagnostics,
386:     }
```

### `src/radial/tooltip.rs:65–80`

```text
65:     pub label_was_truncated: bool,
66: }
67: 
68: impl PreparedTooltip {
69:     pub fn combined_source(&self) -> String {
70:         let label = self.show_label.then_some(self.full_label.as_ref());
71:         match (label, self.description.as_deref()) {
72:             (Some(label), Some(description)) => format!("{label}\n{description}"),
73:             (Some(label), None) => label.to_owned(),
74:             (None, Some(description)) => description.to_owned(),
75:             (None, None) => String::new(),
76:         }
77:     }
78: 
79:     pub fn logical_size(&self) -> (f32, f32) {
80:         let width = self.layout.measured_width_milli as f32 / 1_000.0;
```

### `src/radial/tooltip.rs:272–308`

```text
272: /// Places a tooltip beside its hovered cell in the frozen monitor work area.
273: /// This returns visual geometry only; callers must not feed it into menu
274: /// fitting, pointer ownership, or the session's spatial anchor.
275: pub fn place_tooltip(
276:     anchor: LogicalRect,
277:     size: (f32, f32),
278:     work_area: PhysicalRect,
279:     scale: ScaleFactor,
280: ) -> LogicalRect {
281:     let min = scale.physical_to_logical(work_area.min);
282:     let max = scale.physical_to_logical(work_area.max);
283:     let area_width = (max.x - min.x).max(1.0);
284:     let area_height = (max.y - min.y).max(1.0);
285:     let width = size.0.max(1.0).min(area_width);
286:     let height = size.1.max(1.0).min(area_height);
287: 
288:     let mut x = anchor.max.x + TOOLTIP_GAP_LOGICAL;
289:     if x + width > max.x {
290:         x = anchor.min.x - TOOLTIP_GAP_LOGICAL - width;
291:     }
292:     let mut y = anchor.min.y + (anchor.max.y - anchor.min.y - height) * 0.5;
293:     if y + height > max.y {
294:         y = anchor.min.y - TOOLTIP_GAP_LOGICAL - height;
295:     }
296:     if y < min.y {
297:         y = anchor.max.y + TOOLTIP_GAP_LOGICAL;
298:     }
299: 
300:     x = x.clamp(min.x, (max.x - width).max(min.x));
301:     y = y.clamp(min.y, (max.y - height).max(min.y));
302:     LogicalRect {
303:         min: LogicalPoint { x, y },
304:         max: LogicalPoint {
305:             x: x + width,
306:             y: y + height,
307:         },
308:     }
```

### `src/radial/preparation.rs:263–289`

```text
263:                     let work_min = layout.scale_factor.physical_to_logical(work_area.min);
264:                     let work_max = layout.scale_factor.physical_to_logical(work_area.max);
265:                     let max_width = ((work_max.x - work_min.x - 24.0)
266:                         .max(1.0)
267:                         .min(MAX_TOOLTIP_WIDTH_LOGICAL)
268:                         * 1_000.0) as u32;
269:                     let max_height = ((work_max.y - work_min.y).max(1.0)
270:                         * MAX_TOOLTIP_HEIGHT_FRACTION
271:                         * 1_000.0) as u32;
272:                     let tooltip_request = FontRequest {
273:                         family: label_request.family.clone(),
274:                         size_milli: 13_000,
275:                         bold: false,
276:                         italic: false,
277:                         dpi_milli: variant.dpi_milli,
278:                         max_width_milli: max_width,
279:                         max_height_milli: max_height,
280:                         max_lines: MAX_TOOLTIP_LINES as u16,
281:                         purpose: FontLayoutPurpose::Tooltip,
282:                         wrap: FontWrapPolicy::Word,
283:                         alignment: FontAlignment::Left,
284:                     };
285:                     let layout = service.prepare(&source, tooltip_request.clone());
286:                     diagnostics.extend(layout.diagnostics.iter().map(|diagnostic| {
287:                         RadialDiagnostic::from_font(
288:                             &menu.id,
289:                             &cell.cell_id,
```

## S3. Hover uses a native presentation path with input hide/show

**Observed:** Tooltip bounds contribute to visual scene bounds. The native surface has separate input and visual HWNDs. `present` composes a new frame but then hides/repositions/reshapes the input host, moves/shows the visual host, publishes new pixels, updates state, and shows the input host again.

**Hypothesis:** The huge tooltip can move the visual backing extent to the monitor top. Moving/showing before bitmap publication can expose stale pixels; input hide/show can disturb hover/capture events. This is a causal lead, not a confirmed trace of all phantom drawing.

**Required change:** Distinguish pixels-only presentation from geometry/lifecycle changes. Keep the wheel's actual desktop position and input ownership stable when only hover/tooltip pixels change. A backing visual rectangle may change only with coherent new pixel publication.

### `src/radial/render.rs:374–408`

```text
374:     let mut visual_bounds = layout.visual_extent;
375:     push_image(
376:         &mut primitives,
377:         resources,
378:         &layout.style.menu_foreground,
379:         scaled_rect(layout.background_extent, layout.style.menu_foreground_scale),
380:         layout.style.image_quality,
381:         opacity(layout.style.menu_foreground_opacity),
382:     );
383:     if let (Some(cell_id), Some(work_area), Some(tooltip)) = (
384:         visible_tooltip,
385:         work_area,
386:         visible_tooltip.and_then(|cell_id| resources.tooltips.get(cell_id)),
387:     ) && let Some(cell) = layout.cells.iter().find(|cell| &cell.cell_id == cell_id)
388:     {
389:         let anchor = shape_bounds(&cell.shape);
390:         let tooltip_bounds = place_tooltip(
391:             anchor,
392:             tooltip.logical_size(),
393:             work_area,
394:             layout.scale_factor,
395:         );
396:         visual_bounds = union_rect(visual_bounds, tooltip_bounds);
397:         primitives.push(VectorPrimitive::Tooltip {
398:             bounds: tooltip_bounds,
399:             text: Arc::clone(tooltip),
400:             background: Rgba(10, 11, 14, 242),
401:             color: Rgba(248, 248, 248, 255),
402:         });
403:     }
404:     VectorScene {
405:         bounds: visual_bounds,
406:         generation,
407:         shape_quality: layout.style.shape_quality,
408:         primitives,
```

### `src/radial/native.rs:1794–1869`

```text
1794:         cancel_animation(self.visual_hwnd, unsafe { &mut *ptr });
1795:         unsafe { (*ptr).animation_epoch = std::time::Instant::now() };
1796:         let frame = unsafe { &mut *ptr }
1797:             .compositor
1798:             .compose(&scene, layout.scale_factor, 0)
1799:             .map_err(|error| format!("radial composition failed: {error:?}"))?;
1800:         let origin = bounds.input_origin;
1801:         let region = create_native_input_region(&native_input_region_plan(&layout, origin))?;
1802:         let mut style = unsafe { GetWindowLongPtrW(self.input_hwnd, GWL_EXSTYLE) };
1803:         if activate_on_show {
1804:             style &= !(WS_EX_NOACTIVATE.0 as isize);
1805:         } else {
1806:             style |= WS_EX_NOACTIVATE.0 as isize;
1807:         }
1808:         unsafe { SetWindowLongPtrW(self.input_hwnd, GWL_EXSTYLE, style) };
1809:         let _ = unsafe { ShowWindow(self.input_hwnd, SW_HIDE) };
1810:         let input_positioned = unsafe {
1811:             SetWindowPos(
1812:                 self.input_hwnd,
1813:                 if always_on_top {
1814:                     HWND_TOPMOST
1815:                 } else {
1816:                     HWND_NOTOPMOST
1817:                 },
1818:                 target_x,
1819:                 target_y,
1820:                 input_width,
1821:                 input_height,
1822:                 SWP_NOACTIVATE,
1823:             )
1824:         };
1825:         if let Err(error) = input_positioned {
1826:             return Err(format!("radial input proxy relayout failed: {error}"));
1827:         }
1828:         apply_native_input_region(self.input_hwnd, region)?;
1829:         unsafe {
1830:             SetWindowPos(
1831:                 self.visual_hwnd,
1832:                 if always_on_top {
1833:                     HWND_TOPMOST
1834:                 } else {
1835:                     HWND_NOTOPMOST
1836:                 },
1837:                 visual_x,
1838:                 visual_y,
1839:                 visual_width,
1840:                 visual_height,
1841:                 SWP_NOACTIVATE | SWP_SHOWWINDOW,
1842:             )
1843:         }
1844:         .map_err(|error| format!("radial relayout failed: {error}"))?;
1845:         present_layered(self.visual_hwnd, visual_x, visual_y, &frame.image)?;
1846:         unsafe {
1847:             (*ptr).layout = layout;
1848:             (*ptr).scene = scene;
1849:             (*ptr).origin = origin;
1850:             (*ptr).scale_factor = frame.scale_factor;
1851:             (*ptr).activate_on_show = activate_on_show;
1852:             (*ptr).input_x = target_x;
1853:             (*ptr).input_y = target_y;
1854:             (*ptr).visual_x = visual_x;
1855:             (*ptr).visual_y = visual_y;
1856:             (*ptr).visual_offset_x = visual_x.saturating_sub(target_x);
1857:             (*ptr).visual_offset_y = visual_y.saturating_sub(target_y);
1858:             (*ptr).presented.store(true, Ordering::Release);
1859:         }
1860:         let _ = unsafe {
1861:             ShowWindow(
1862:                 self.input_hwnd,
1863:                 if activate_on_show {
1864:                     SW_SHOW
1865:                 } else {
1866:                     SW_SHOWNOACTIVATE
1867:                 },
1868:             )
1869:         };
```

## S4. Wake delivery and independent chord lifecycle already exist

**Observed:** `send_event` queues messages, collects registered wake handles, releases the registry lock, and invokes wakes. The launcher key adapter already tracks an owned primary release and sends `PrimaryReleased` to its reducer.

**Implication:** The earlier “send_event never wakes the GUI” diagnosis is not current. Trace the actual user combination before changing ownership. Do not add a parallel listener or permanent repaint loop.

**Unknown:** Whether the Designer-only, preview-active, or another related state is sufficient to produce the failure, and whether the lost effect is input, dispatch, viewport targeting, restoration, or native delivery.

### `src/gui/mod.rs:390–431`

```text
390: /// Register a GUI event sink with the viewport that owns its work queue.
391: pub fn register_event_sender_with_wake(
392:     tx: Sender<WatchEvent>,
393:     wake: ViewportWake,
394: ) -> EventSinkRegistration {
395:     register_event_sink(tx, Some(wake))
396: }
397: 
398: pub fn register_event_sender(tx: Sender<WatchEvent>) -> EventSinkRegistration {
399:     register_event_sink(tx, None)
400: }
401: 
402: pub fn send_event(ev: WatchEvent) {
403:     let wakes = APP_EVENT_REGISTRY
404:         .lock()
405:         .map(|mut registry| {
406:             if registry.sinks.is_empty() {
407:                 if !registry.owner_registered {
408:                     if registry.pending_before_owner.len() == APP_EVENT_PENDING_CAPACITY {
409:                         registry.pending_before_owner.pop_front();
410:                     }
411:                     registry.pending_before_owner.push_back(ev);
412:                 }
413:                 return Vec::new();
414:             }
415:             let mut wakes = Vec::new();
416:             registry.sinks.retain_mut(|sink| {
417:                 if sink.sender.send(ev.clone()).is_err() {
418:                     return false;
419:                 }
420:                 sink.queued.fetch_add(1, Ordering::Release);
421:                 if let Some(wake) = &sink.wake {
422:                     wakes.push(wake.clone());
423:                 }
424:                 true
425:             });
426:             wakes
427:         })
428:         .unwrap_or_default();
429:     for wake in wakes {
430:         wake.wake();
431:     }
```

### `src/hotkey/launcher_invocation.rs:471–496`

```text
471:         }
472:     }
473: 
474:     fn process_owned_primary(&mut self, event: KeyEvent) -> AdapterOutcome {
475:         let id = self.owned.expect("owned primary has invocation");
476:         if self.owned_provenance != Some(event.provenance) {
477:             return AdapterOutcome::pass();
478:         }
479:         if event.transition == KeyTransition::Up {
480:             let consume = self.owned_primary_down_suppressed;
481:             self.owned = None;
482:             self.owned_primary = None;
483:             self.owned_provenance = None;
484:             self.owned_primary_down_suppressed = false;
485:             self.candidate_primary_down = None;
486:             return AdapterOutcome {
487:                 consume,
488:                 recovery: false,
489:                 intents: self
490:                     .reducer
491:                     .reduce(InvocationEvent::PrimaryReleased { id, at: event.at }),
492:             };
493:         }
494:         AdapterOutcome {
495:             consume: true,
496:             recovery: false,
```

## S5. Explicit wake identity is not automatically explicit command identity

**Observed:** The wake helper can target ROOT, while `ViewportCtx for egui::Context` forwards unqualified commands/repaints. Main's visibility owner and GUI update/restore paths both apply visibility.

**Hypothesis:** With a deferred child using the shared Context, an unqualified command may be routed in an unintended viewport scope, or a later legitimate/stale restore may undo a hide. The excerpts identify audit points, not proof of the exact user's failure.

**Required change:** Trace target viewport/native identity and all relevant desired/actual visibility edges. Bind cross-context root commands explicitly if necessary; do not globally reroute child-window commands to ROOT or disable every restore.

### `src/visibility.rs:18–61`

```text
18: 
19: impl ViewportWake {
20:     pub fn for_context(ctx: &egui::Context, viewport: egui::ViewportId) -> Self {
21:         let ctx = ctx.clone();
22:         Self {
23:             viewport,
24:             request: Arc::new(move |viewport| ctx.request_repaint_of(viewport)),
25:         }
26:     }
27: 
28:     pub fn root(ctx: &egui::Context) -> Self {
29:         Self::for_context(ctx, egui::ViewportId::ROOT)
30:     }
31: 
32:     #[cfg(test)]
33:     pub(crate) fn from_callback(
34:         viewport: egui::ViewportId,
35:         request: impl Fn(egui::ViewportId) + Send + Sync + 'static,
36:     ) -> Self {
37:         Self {
38:             viewport,
39:             request: Arc::new(request),
40:         }
41:     }
42: 
43:     pub(crate) fn wake(&self) {
44:         (self.request)(self.viewport);
45:     }
46: }
47: 
48: /// Trait abstracting over an `egui::Context` for viewport commands.
49: pub trait ViewportCtx {
50:     fn send_viewport_cmd(&self, cmd: egui::ViewportCommand);
51:     fn request_repaint(&self);
52: }
53: 
54: impl ViewportCtx for egui::Context {
55:     fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
56:         egui::Context::send_viewport_cmd(self, cmd);
57:     }
58: 
59:     fn request_repaint(&self) {
60:         egui::Context::request_repaint(self);
61:     }
```

### `src/visibility.rs:250–289`

```text
250: fn apply_visibility_owner<C: ViewportCtx>(
251:     next: bool,
252:     restore_flag: &Arc<AtomicBool>,
253:     ctx_handle: &Arc<Mutex<Option<C>>>,
254:     queued_visibility: &mut Option<bool>,
255:     offscreen: (f32, f32),
256:     follow_mouse: bool,
257:     static_enabled: bool,
258:     static_pos: Option<(f32, f32)>,
259:     static_size: Option<(f32, f32)>,
260:     window_size: (f32, f32),
261: ) {
262:     if let Ok(guard) = ctx_handle.lock() {
263:         if let Some(ctx) = &*guard {
264:             apply_visibility(
265:                 next,
266:                 VisiblePlacementPolicy::ApplyConfiguredPlacement,
267:                 ctx,
268:                 offscreen,
269:                 follow_mouse,
270:                 static_enabled,
271:                 static_pos,
272:                 static_size,
273:                 window_size,
274:             );
275:             restore_flag.store(next, Ordering::SeqCst);
276:             *queued_visibility = None;
277:             tracing::debug!("Applied queued visibility: {}", next);
278:         } else {
279:             *queued_visibility = Some(next);
280:             restore_flag.store(next, Ordering::SeqCst);
281:         }
282:     } else {
283:         *queued_visibility = Some(next);
284:         restore_flag.store(next, Ordering::SeqCst);
285:     }
286: }
287: 
288: /// Apply the current visibility state to the viewport.
289: pub fn apply_visibility<C: ViewportCtx>(
```

### `src/gui/render.rs:683–728`

```text
683:         if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
684:             self.window_pos = (rect.min.x as i32, rect.min.y as i32);
685:         }
686:         let do_restore = self.restore_flag.swap(false, Ordering::SeqCst);
687:         if self.visible_flag.load(Ordering::SeqCst) && self.help_flag.swap(false, Ordering::SeqCst)
688:         {
689:             self.help_window.overlay_open = !self.help_window.overlay_open;
690:         } else {
691:             // reset any queued toggle when window not visible
692:             self.help_flag.store(false, Ordering::SeqCst);
693:         }
694:         if do_restore && self.visible_flag.load(Ordering::SeqCst) {
695:             tracing::debug!("Restoring window on restore_flag");
696:             apply_visibility(
697:                 true,
698:                 VisiblePlacementPolicy::PreserveCurrentGeometry,
699:                 ctx,
700:                 self.offscreen_pos,
701:                 self.follow_mouse,
702:                 self.static_location_enabled,
703:                 self.static_pos.map(|(x, y)| (x as f32, y as f32)),
704:                 self.static_size.map(|(w, h)| (w as f32, h as f32)),
705:                 (self.window_size.0 as f32, self.window_size.1 as f32),
706:             );
707:             if let Some(hwnd) = crate::window_manager::get_hwnd(_frame) {
708:                 crate::window_manager::restore_launcher_to_current_desktop(hwnd);
709:             }
710:         }
711: 
712:         let should_be_visible = self.visible_flag.load(Ordering::SeqCst);
713:         let just_became_visible = !self.last_visible && should_be_visible;
714:         if self.last_visible != should_be_visible {
715:             tracing::debug!("gui thread -> visible: {}", should_be_visible);
716:             apply_visibility(
717:                 should_be_visible,
718:                 VisiblePlacementPolicy::ApplyConfiguredPlacement,
719:                 ctx,
720:                 self.offscreen_pos,
721:                 self.follow_mouse,
722:                 self.static_location_enabled,
723:                 self.static_pos.map(|(x, y)| (x as f32, y as f32)),
724:                 self.static_size.map(|(w, h)| (w as f32, h as f32)),
725:                 (self.window_size.0 as f32, self.window_size.1 as f32),
726:             );
727:             self.last_visible = should_be_visible;
728:         }
```

## S6. Pending requests can make Designer X a no-op

**Observed:** The Designer calls `request_close` on native close and sends CancelClose while it remains open. `close_decision` returns AwaitingRequest for any pending request; the GUI's corresponding branch is empty. A failed `send_commit` records an error without clearing/reconciling the pending request it just created. Reply handling may initiate a font-catalog request whenever it is unloaded and no request is pending.

**Required change:** Classify expendable versus durable requests, latch close intent, prevent new preparation while closing, reconcile exact send failures, preserve save/cancel transaction semantics, and keep late replies from resurrecting state. The user has not established that a durable operation was active, so idle close must be tested too.

### `src/gui/radial_editor/mod.rs:974–997`

```text
974:         ctx.show_viewport_deferred(viewport_id, builder, move |child, class| {
975:             let Ok(mut editor) = shared.lock() else {
976:                 return;
977:             };
978:             let close_requested = class != egui::ViewportClass::Embedded
979:                 && child.input(|input| input.viewport().close_requested());
980:             if close_requested {
981:                 editor.request_close();
982:                 // A dirty designer must keep its deferred viewport alive long
983:                 // enough to show Save/Discard/Keep editing.  Clean close is
984:                 // handled below by sending the actual Close command.
985:                 if editor.open {
986:                     child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
987:                 }
988:             }
989:             if !editor.open {
990:                 if class != egui::ViewportClass::Embedded {
991:                     child.send_viewport_cmd(egui::ViewportCommand::Close);
992:                 }
993:                 editor.viewport_close_pending = false;
994:                 return;
995:             }
996:             if class == egui::ViewportClass::Embedded {
997:                 // Embedded callbacks are not an authoring surface.  Keep the
```

### `src/gui/radial_editor/mod.rs:1132–1187`

```text
1132:     pub(crate) fn request_close(&mut self) {
1133:         self.preview.cancel_tooltip();
1134:         // Window move/resize state must be persisted when close interaction
1135:         // ends, even if the debounce interval has not elapsed.
1136:         self.preferences_flush_requested = true;
1137:         self.preference_debounce.flush();
1138:         self.intent_bridge.enqueue_preferences_ready();
1139:         self.intent_bridge.clear();
1140:         self.placement_draft = None;
1141:         self.pending_drop = None;
1142:         self.properties_popup = None;
1143:         self.properties_draft = None;
1144:         // A preview preparation has no durable side effect and can be
1145:         // cancelled locally.  This lets an OS close finish promptly without
1146:         // leaving a late preparation reply attached to a hidden viewport.
1147:         let pending_preview = self.session.as_ref().and_then(|session| {
1148:             session.pending_request.filter(|pending| {
1149:                 pending.kind == crate::radial::authoring::PendingRequestKind::PrepareEmbeddedPreview
1150:             })
1151:         });
1152:         if let Some(pending) = pending_preview {
1153:             // Preparation has no durable side effect, so it can be canceled
1154:             // locally.  Keep the draft alive long enough for the ordinary
1155:             // dirty-close decision below; a clean draft will close now while
1156:             // an edited draft still gets Save/Discard/Keep editing.
1157:             if let Some(session) = self.session.as_mut() {
1158:                 session.cancel_pending_request(
1159:                     pending.id,
1160:                     pending.generation,
1161:                     pending.editor_session,
1162:                 );
1163:             }
1164:         }
1165:         let Some(session) = self.session.as_ref() else {
1166:             self.open = false;
1167:             self.viewport_close_pending = true;
1168:             return;
1169:         };
1170:         let native_preview_active = session.native_preview_may_be_open
1171:             || session.native_preview_lease.is_some()
1172:             || session.pending_native_preview.is_some();
1173:         match session.close_decision() {
1174:             CloseDecision::CloseClean => {
1175:                 if native_preview_active {
1176:                     self.stop_native_preview();
1177:                 }
1178:                 self.preview.dispose();
1179:                 self.release_authoring_resources();
1180:                 self.open = false;
1181:                 self.viewport_close_pending = true;
1182:                 self.session = None;
1183:             }
1184:             CloseDecision::PromptDirty => self.close_prompt = true,
1185:             CloseDecision::AwaitingRequest => {}
1186:         }
1187:     }
```

### `src/radial/authoring.rs:1091–1099`

```text
1091:     pub fn close_decision(&self) -> CloseDecision {
1092:         if self.pending_request.is_some() {
1093:             CloseDecision::AwaitingRequest
1094:         } else if self.is_dirty() {
1095:             CloseDecision::PromptDirty
1096:         } else {
1097:             CloseDecision::CloseClean
1098:         }
1099:     }
```

### `src/gui/radial_editor/mod.rs:1210–1243`

```text
1210:     fn send_commit(&mut self, disposition: CommitDisposition) {
1211:         let Some(session) = self.session.as_mut() else {
1212:             return;
1213:         };
1214:         let request = session.request_commit(disposition);
1215:         match (request, &self.client) {
1216:             (Ok(request), Some(client)) => {
1217:                 if let Err(error) = client.send(request) {
1218:                     session.last_error = Some(format!("{error:?}"));
1219:                 }
1220:             }
1221:             (Err(error), _) => session.last_error = Some(format!("{error:?}")),
1222:             (_, None) => session.last_error = Some("Radial authoring service unavailable".into()),
1223:         }
1224:     }
1225: 
1226:     fn poll_replies(&mut self) {
1227:         let Some(client) = &self.client else { return };
1228:         let Some(session) = self.session.as_mut() else {
1229:             return;
1230:         };
1231:         while let Some(reply) = client.try_recv() {
1232:             session.accept_reply(reply);
1233:         }
1234:         if !session.font_catalog_loaded && session.pending_request.is_none() {
1235:             match session.request_font_catalog() {
1236:                 Ok(request) => {
1237:                     if let Err(error) = client.send(request) {
1238:                         session.last_error = Some(format!("{error:?}"));
1239:                     }
1240:                 }
1241:                 Err(error) => session.last_error = Some(format!("{error:?}")),
1242:             }
1243:         }
```

## S7. Designer viewport and request/preference bridges are already integrated

**Observed:** The editor already uses `show_viewport_deferred`, with a reply wake for the Designer and an intent wake for ROOT. ROOT consumes designer intents/preferences, and teardown releases bridge callbacks/resources.

**Implication:** This pass must repair that integration, not create another editor window/process. Audit lock scope, pending request cancellation, callback lifetimes, and preference-only settings reload effects. Repeated preference writes/restarts are an investigation question, not an established cause.

### `src/gui/radial_editor/mod.rs:925–947`

```text
925:         let viewport_id = radial_designer_viewport_id();
926:         let reply_ctx = ctx.clone();
927:         let reply_wake: Arc<dyn Fn() + Send + Sync> =
928:             Arc::new(move || reply_ctx.request_repaint_of(viewport_id));
929:         let intent_bridge = shared
930:             .lock()
931:             .map(|editor| Arc::clone(&editor.intent_bridge))
932:             .unwrap_or_else(|_| Arc::new(DesignerIntentBridge::default()));
933:         let root_ctx = ctx.clone();
934:         intent_bridge.set_wake(Some(Arc::new(move || {
935:             root_ctx.request_repaint_of(egui::ViewportId::ROOT);
936:         })));
937:         intent_bridge.set_viewport_wake(Some(Arc::clone(&reply_wake)));
938:         if let Ok(editor) = shared.lock()
939:             && let Some(client) = editor.client.as_ref()
940:         {
941:             client.set_reply_wake(Some(reply_wake));
942:         }
943:         let frame = DesignerFrameContext {
944:             feature_defaults,
945:             expected_diagnostics: diagnostics,
946:             action_catalog,
947:             require_confirm_destructive: require_confirm,
```

### `src/gui/render.rs:1211–1253`

```text
1211:         crate::gui::radial_editor::RadialEditorState::show_deferred(&self.radial_editor, ctx, self);
1212:         let file_dialog_pending = self
1213:             .radial_editor
1214:             .lock()
1215:             .map(|editor| editor.intent_bridge().has_pending_file_dialog())
1216:             .unwrap_or(false);
1217:         if file_dialog_pending {
1218:             crate::gui::radial_editor::RadialEditorState::process_pending_file_dialog(
1219:                 &self.radial_editor,
1220:             );
1221:         }
1222:         let designer_intents = self
1223:             .radial_editor
1224:             .lock()
1225:             .map(|editor| editor.intent_bridge())
1226:             .map(|bridge| bridge.drain())
1227:             .unwrap_or_default();
1228:         for intent in designer_intents {
1229:             match intent {
1230:                 crate::gui::radial_editor::DesignerUiIntent::TestAction {
1231:                     binding,
1232:                     invocation,
1233:                     history_query,
1234:                 } => {
1235:                     let _ =
1236:                         self.test_radial_authoring_action(&binding, &invocation, &history_query);
1237:                 }
1238:             }
1239:         }
1240:         let designer_preferences = self
1241:             .radial_editor
1242:             .lock()
1243:             .ok()
1244:             .and_then(|mut editor| editor.take_preferences_for_persist());
1245:         if let Some(preferences) = designer_preferences {
1246:             let settings_path = self.settings_path.clone();
1247:             if let Err(error) = crate::settings::Settings::update(&settings_path, |settings| {
1248:                 settings.radial_designer = preferences;
1249:                 Ok(())
1250:             }) {
1251:                 self.report_error_message("radial.designer.preferences", error.to_string());
1252:             }
1253:         }
```

### `src/gui/radial_editor/mod.rs:1268–1283`

```text
1268:     fn release_authoring_resources(&mut self) {
1269:         if self.preferences_dirty {
1270:             // Preserve the final close flush before unregistering the wake
1271:             // callbacks.  ROOT can consume the bit after this viewport has
1272:             // disposed its authoring resources.
1273:             self.intent_bridge.enqueue_preferences_ready();
1274:         }
1275:         self.intent_bridge.set_wake(None);
1276:         self.intent_bridge.set_viewport_wake(None);
1277:         self.intent_bridge.clear();
1278:         if let (Some(client), Some(session)) = (&self.client, &self.session) {
1279:             client.set_reply_wake(None);
1280:             client.release_resources(session.editor_session());
1281:         }
1282:         self.client = None;
1283:     }
```

## S8. Cascade flattens cells rather than complete per-frame scenes

**Observed:** Cascade child placement derives from the selected parent cell's shape center. `cascade_layout` appends ancestor cells/input regions into the child's layout, marking ancestor cells non-actionable. It retains the child's single menu-level presentation rather than a complete parent/child scene stack.

**Required behavior:** SameCenter remains the default with no repeat bulk migration. Explicit Cascade draws complete decorated frames back-to-front with close, consistent overlap. Exposed parent click navigates to that frame only; it does not execute the cell beneath the gesture. Reuse existing frame identity and renderer.

**Reference limitation:** The blue screenshot specifies desired overlap. The original RM4 source also uses parent-item offsets; do not claim the proposed stable small diagonal offset is an exact old algorithm.

### `src/radial/controller.rs:2539–2609`

```text
2539:         let (effective_presentation, mut layout) = match parent_presentation {
2540:             SubmenuPresentation::SameCenter => match layout_document_menu_fixed_center(
2541:                 &self.document,
2542:                 &child,
2543:                 parent_layout.origin,
2544:                 spatial.work_area,
2545:                 spatial.scale_factor,
2546:                 0.55,
2547:             ) {
2548:                 Ok(layout) => (SubmenuPresentation::SameCenter, layout),
2549:                 Err(error) => {
2550:                     out.push(ControllerEvent::SubmenuPlacementFailed {
2551:                         session_id,
2552:                         parent_frame_id,
2553:                         parent_menu_id,
2554:                         child_menu_id: menu_id,
2555:                         parent_presentation,
2556:                         message: format!("SameCenter placement failed: {error:?}"),
2557:                     });
2558:                     return;
2559:                 }
2560:             },
2561:             SubmenuPresentation::Cascade => {
2562:                 let Some(cell_layout) = parent_layout
2563:                     .cells
2564:                     .iter()
2565:                     .find(|layout| &layout.cell_id == cell_id)
2566:                 else {
2567:                     out.push(ControllerEvent::LayoutFailed {
2568:                         menu_id,
2569:                         error: super::geometry::LayoutError::InvalidStyle,
2570:                     });
2571:                     return;
2572:                 };
2573:                 let cascade_anchor = shape_center(&cell_layout.shape, spatial.scale_factor);
2574:                 match layout_document_menu_fixed_center(
2575:                     &self.document,
2576:                     &child,
2577:                     cascade_anchor,
2578:                     spatial.work_area,
2579:                     spatial.scale_factor,
2580:                     0.55,
2581:                 ) {
2582:                     Ok(layout) => (SubmenuPresentation::Cascade, layout),
2583:                     Err(cascade_error) => {
2584:                         match layout_document_menu_fixed_center(
2585:                             &self.document,
2586:                             &child,
2587:                             parent_layout.origin,
2588:                             spatial.work_area,
2589:                             spatial.scale_factor,
2590:                             0.55,
2591:                         ) {
2592:                             Ok(layout) => (SubmenuPresentation::SameCenter, layout),
2593:                             Err(fallback_error) => {
2594:                                 out.push(ControllerEvent::SubmenuPlacementFailed {
2595:                                     session_id,
2596:                                     parent_frame_id,
2597:                                     parent_menu_id,
2598:                                     child_menu_id: menu_id,
2599:                                     parent_presentation,
2600:                                     message: format!(
2601:                                         "Cascade placement failed: {cascade_error:?}; SameCenter fallback failed: {fallback_error:?}"
2602:                                     ),
2603:                                 });
2604:                                 return;
2605:                             }
2606:                         }
2607:                     }
2608:                 }
2609:             }
```

### `src/radial/geometry.rs:712–734`

```text
712: /// Compose the displayed ancestor regions with a Cascade child. Ancestor
713: /// cells remain visible but non-actionable; their owned hit regions remain
714: /// available for correct native input parity.
715: pub fn cascade_layout(parent: &LayoutSnapshot, mut child: LayoutSnapshot) -> LayoutSnapshot {
716:     let mut ancestors = parent.cells.clone();
717:     for cell in &mut ancestors {
718:         cell.actionable = false;
719:     }
720:     ancestors.extend(child.cells);
721:     child.cells = ancestors;
722:     let mut input_regions = parent.input_regions.clone();
723:     input_regions.extend(child.input_regions);
724:     child.input_regions = input_regions;
725:     child.input_extent.min.x = child.input_extent.min.x.min(parent.input_extent.min.x);
726:     child.input_extent.min.y = child.input_extent.min.y.min(parent.input_extent.min.y);
727:     child.input_extent.max.x = child.input_extent.max.x.max(parent.input_extent.max.x);
728:     child.input_extent.max.y = child.input_extent.max.y.max(parent.input_extent.max.y);
729:     child.visual_extent.min.x = child.visual_extent.min.x.min(parent.visual_extent.min.x);
730:     child.visual_extent.min.y = child.visual_extent.min.y.min(parent.visual_extent.min.y);
731:     child.visual_extent.max.x = child.visual_extent.max.x.max(parent.visual_extent.max.x);
732:     child.visual_extent.max.y = child.visual_extent.max.y.max(parent.visual_extent.max.y);
733:     child
734: }
```

## S9. Existing tooltip test does not bound the excessive height

**Observed:** The test verifies wrapping/source Unicode and `measured_height_milli > 12_000`, but does not assert an upper bound related to font size/line count. The thousandfold error can satisfy that assertion.

**Required change:** Add meaningful dimensional and production-render assertions, including DPI scaling once and glyph/padding containment. Do not replace the old test with a weaker snapshot that encodes the erroneous large box.

### `src/radial/font_cache.rs:712–741`

```text
712:     fn tooltip_layout_wraps_complete_unicode_and_dynamic_text_separately_from_labels() {
713:         let catalog = Catalog {
714:             families: [normalize_family("Segoe UI")].into_iter().collect(),
715:             lookups: AtomicUsize::new(0),
716:             loads: AtomicUsize::new(0),
717:         };
718:         let mut service = FontLayoutService::with_catalog(catalog, 8);
719:         let source = "日本語 dynamic result with several words and e\u{301} accents";
720:         let label_request = request(None, 60_000);
721:         let mut tooltip_request = label_request.clone();
722:         tooltip_request.max_width_milli = 120_000;
723:         tooltip_request.max_height_milli = 240_000;
724:         tooltip_request.max_lines = 12;
725:         tooltip_request.purpose = FontLayoutPurpose::Tooltip;
726:         tooltip_request.wrap = FontWrapPolicy::Word;
727:         tooltip_request.alignment = FontAlignment::Left;
728:         let label = service.prepare(source, label_request);
729:         let tooltip = service.prepare(source, tooltip_request);
730:         assert_ne!(label.text, tooltip.text);
731:         assert_eq!(&*tooltip.source_text, source);
732:         assert!(tooltip.text.contains("日本語"));
733:         assert!(tooltip.text.contains("e\u{301}"));
734:         assert!(tooltip.line_count > 1);
735:         assert!(tooltip.measured_height_milli > 12_000);
736:         assert!(
737:             !tooltip
738:                 .diagnostics
739:                 .contains(&FontDiagnostic::TooltipViewLimited)
740:         );
741:         assert!(!Arc::ptr_eq(&label, &tooltip));
```

## S10. Prior automated success is not native acceptance

**Observed ledger report:** The previous repair ledger lists its R1–R4 commits and R5 automated gate, while marking specific native cases unverified. These notes do not independently re-run or verify those historical counts/SHAs.

**Required change:** Keep historical evidence unchanged, but record a new stabilization-start candidate and actual reproductions/fixes. The current user's screenshot and observations are acceptance failures despite the reported earlier green suite.

### `docs/plans/radial-repair-and-designer.md:14–31`

```text
14: 
15: ## Live status
16: 
17: | Field | Value |
18: |---|---|
19: | repair start branch | `radial-menu-2` |
20: | repair start HEAD | `eda06f7667e481dd370da413fee9c5d92cfe0334` (`update`) |
21: | repair start worktree | clean: no staged, unstaged, or untracked changes |
22: | immutable feature baseline | `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` (unchanged) |
23: | branch point | `f7c5f61ed2faa2288f5c19ddeaea66de6f760a2b` (unchanged) |
24: | current milestone | R5 |
25: | implementation status | R1-R4 committed; R5 automated verification/review complete at `95c50c95`; native release acceptance remains unverified |
26: | last verified source | source commit `95c50c9564e56efb06ec87a6fa60e89538015958`; `cargo check` passed; focused Nextest `e6d52fdf-6abf-41fa-aac9-c1620e4cfa3c` passed 12/12; full Nextest `68fde588-c498-4dc5-8d1b-25eabfe25b76` passed 4,570/4,570 with 8 skipped |
27: | current native evidence gaps | H1-H5, N1-N4, V1-V3, D1-D5, and P1-P2 are unverified for this repair |
28: | active Cargo/build/Nextest job | none |
29: | next action | perform the H1-H5, N1-N4, V1-V3, D1-D5, and P1-P2 checklist on an interactive Windows acceptance host |
30: 
31: The repair-start Git diff was empty. Repository-local reference archives and images
```

## RM4 reference excerpts

Scripts were inspected inside the supplied archive as text only. Nothing was executed. Preserve the current project's licensing/attribution restrictions. The reference supports direct-manipulation editing but does not authorize importing arbitrary executable AHK behavior.

### `Radial menu v4/Internal/Codes/RMD classes.ahk:203–229`

Source member SHA-256: `45c77824291e135955c336ecf9010bc023c87b5dec19c6f57df5655b4f4bec30`.

```text
203: 	LButtonOnRBoardCenter(LName, x, y) {
204: 		pBitmapItem := this.t.Temp1.Item2BitmapForBoard(1, this.Bitmaps.FullNB, this.Bitmaps.EmptyNB, this.IB)
205: 			this.FakeItem.SetBitmap(pBitmapItem)
206: 		Gdip_DisposeImage(pBitmapItem)
207: 		xOffset := this.GC.RLStartX-this.l.ItemSize/2, yOffset := this.GC.RLStartY-this.l.ItemSize/2
208: 		ClientToScreen(this.hGui1, xOffset, yOffset)
209: 		this.FakeItem.Show(xOffset, yOffset)
210: 		this.FakeItem.DragNotActivate()
211: 		ReleasedAt := this.FakeItem.GetPos(1)	
212: 		this.FakeItem.Hide()	
213: 		StringSplit, r, ReleasedAt, :
214: 		x := r1, y := r2
215: 		ScreenToClient(this.hGui1, x, y)
216: 		ExportedItem := this.t.Temp1.ExportItem(1,0)	
217: 		if (this.GC.IsAreaAt("Radial layout", x,y) = 1)	
218: 		{
219: 			SelectedItemNum2 := this.L[LName].GetSelectedItem(this.GC.RLStartX, this.GC.RLStartY, x, y)
220: 			if (SelectedItemNum2 = "")	
221: 				return
222: 			WasSuccessful := this.t.Menu.ImportItem(ExportedItem,SelectedItemNum2)
223: 			if WasSuccessful
224: 			{
225: 				pBitmapItem := this.t.Menu.Item2BitmapForBoard(SelectedItemNum2, this.Bitmaps.Full, this.Bitmaps.Empty, this.IB)
226: 				this.RBoard.SetBitmap(pBitmapItem, SelectedItemNum2)
227: 				Gdip_DisposeImage(pBitmapItem)
228: 			}
229: 		}
```

### `Radial menu v4/Internal/Codes/RMD classes.ahk:524–541`

Source member SHA-256: `45c77824291e135955c336ecf9010bc023c87b5dec19c6f57df5655b4f4bec30`.

```text
524: 	GuiContextMenu() {
525: 		CoordMode, mouse, Screen
526: 		MouseGetPos, x, y, WinUMID
527: 		if (WinUMID != this.hGui1 or WinExist("A") != this.hGui1 or this.ViewType = "t" or this.D.Type = "")
528: 		return
529: 		ScreenToClient(this.hGui1, x, y)
530: 		if this.GC.IsAreaAt("Radial layout", x,y)	
531: 		{
532: 			LName := this.L.ItemSize "x" this.L.RadiusSizeFactor "x" this.L.ItemLayout
533: 			SelectedItemNum := this.L[LName].GetSelectedItem(this.GC.RLStartX, this.GC.RLStartY, x, y)
534: 			if (SelectedItemNum = 0 or SelectedItemNum = "" or this.t.Menu.DoesItemExist(SelectedItemNum) != 1)
535: 				return	
536: 			ItemString := this.t.Menu.Item2String(SelectedItemNum)
537: 			this.SEG.EditSetText(ItemString), this.SEG.EditReadOnly(0), this.SEG.OkFocus()
538: 			Gui 2:Show, , % "Item " SelectedItemNum " properties - " this.D.ShortName
539: 			this.D.SEG.1 := "Menu", this.D.SEG.2 := SelectedItemNum, this.D.SEG.3 := ItemString
540: 			Gui 1:+Disabled
541: 		}
```

### `Radial menu v4/Internal/Codes/RM2module.ahk:689–709`

Source member SHA-256: `63fe3530d0347e8efc9cc4094ea5d391435e976b9bda3ddbd1e10a885163f222`.

```text
689: RM2_ShowAsSubmenu(ChildGuiNum, ParentGuiNum, ParentItemNumber) {
690: if (RM2_IsMenu(ChildGuiNum) != 1)
691: return
692: if (RM2_IsMenu(ParentGuiNum) != 1)
693: return
694: ParentMenuHWND := RM2_Reg("M" ParentGuiNum "#" "HWND")
695: ParentMenuRadius := RM2_Reg("M" ParentGuiNum "#" "MenuRadius")
696: ItemSize := RM2_Reg("ItemSize")
697: oldDHW := A_DetectHiddenWindows 
698: DetectHiddenWindows, on
699: WinGetPos, ParentMenuX, ParentMenuY,,, ahk_id %ParentMenuHWND%
700: IsOneRinger := RM2_Reg("M" ParentGuiNum "#IsOneRinger")
701: if IsOneRinger
702: CurOffset := RM2_RegOR("M" ParentGuiNum "#I" ParentItemNumber "#Offset")
703: else
704: CurOffset := RM2_Reg("Offset" ParentItemNumber)
705: StringSplit, co, CurOffset, :
706: ChildMenuX := ParentMenuX+ParentMenuRadius+co1+ItemSize/2, ChildMenuY := ParentMenuY+ParentMenuRadius+co2+ItemSize/2
707: RM2_Show(ChildGuiNum, ChildMenuX, ChildMenuY)
708: DetectHiddenWindows, %oldDHW%
709: }
```

## Extracted application source identities

These hashes identify the exact bytes inspected here; use the actual current checkout during implementation.

| File | SHA-256 |
|---|---|
| `docs/plans/radial-repair-and-designer.md` | `84d0e542eedffc702b30bb559b7a55cbed54a42fb4ab6a551bfa579dc60b5ba9` |
| `src/gui/mod.rs` | `c453aa1889d7a5631c4f132ac38dd4b349d6b413af219a0d383e83be6b12e27d` |
| `src/gui/radial_editor/mod.rs` | `34b71c8ea81dde923ee00e735c84dbbf9a44f4d29fad007276a457d6f384af85` |
| `src/gui/render.rs` | `d00933fe3a80bb9037933c1ddd1c242fc71509314cee6d82897533662df11ba1` |
| `src/hotkey/launcher_invocation.rs` | `0ed09ba9c0dd13cd578073b2f8d443b436e267dc1b34f0d5dce9e26938d66f4d` |
| `src/radial/authoring.rs` | `8818d71c25cf0597538bb40e1dfe9c5b201710b29f0583a25fdee64d09484875` |
| `src/radial/controller.rs` | `3a7fbea4a194cb995f52c5eccb9258248b39ec944b889e9b764533aad7089e41` |
| `src/radial/font_cache.rs` | `9b5d9d8b9e04643074632785eae894606caec2b15d7d3904f9ee43c97d2e7302` |
| `src/radial/geometry.rs` | `e3b68dffdb020c00707b09d19198939d10fb0dc81144cd13ef45e51689b4d22a` |
| `src/radial/native.rs` | `dd96653e20c10fb51ba2834514b408b6754fffcf0f6da9954c720554acdd4087` |
| `src/radial/preparation.rs` | `9f6e73a6da39dd8e947d16bb9da5558545df9f208eda37ce8af92860be6636a3` |
| `src/radial/render.rs` | `d8ec1cd5b55e4c6747f7f6befbb67ebcdb654d267c4d433a06590407c0d89f06` |
| `src/radial/tooltip.rs` | `e966ef1f1183230af22900b7cb0c969fe7eab390ceef482d955496fb53b7e696` |
| `src/visibility.rs` | `c3a21e1feb1d25bff13c2290ce75d03d8f2eda91a37a47c668b70286b93ce8fa` |

## Remaining evidence limits

The exact live hotkey/close cause, stale-frame/native sequencing, compact-window usability, cross-process delivery, DPI behavior, and actual responsiveness require the implementation's tests and real Windows acceptance. No current-source test result or measured performance is claimed here. API references in the master brief supplement interpretation but do not replace source evidence.

The user's latest approved answers narrow the reproduction conditions and supersede old assumptions about universal tap failure or repeated submenu migration. Follow them even when an earlier ledger says a phase was complete.
