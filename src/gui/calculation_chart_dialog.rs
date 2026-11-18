//! Read-only chart companion; actions are committed by its retained utility host.
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::UtilityPresentation;
use crate::calculation_pad::{ChartDataset, ChartDefinition, ChartKind, ChartScene, ChartSource, ChartVectorReference, ChartView, ChartReferenceValue, ResolvedReferenceLine, DocumentId, DocumentIncarnation, EvaluationBasis, LineOutcome, ScenePrimitive, SensitivityBasis, SensitivityTable, SessionEvaluation};
use eframe::egui;
use std::sync::Arc;

fn chart_target(scope: Option<&super::jump_mode::ui::JumpModeScope>, ui: &egui::Ui, response: &egui::Response, id: &str, label: &str, kind: super::jump_mode::TargetKind, order: &mut u64) {
    super::jump_mode::ui::register_and_apply_target(scope, ui, response, id, label, kind, order);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChartStamp { Evaluation(EvaluationBasis), Sensitivity(SensitivityBasis), SensitivityWithEvaluation(SensitivityBasis, EvaluationBasis) }

#[derive(Clone)]
pub(super) struct ChartRuntime {
    pub identity: Option<(DocumentId, DocumentIncarnation)>,
    pub evaluation: Option<Arc<SessionEvaluation>>,
    pub table: Option<Arc<SensitivityTable>>,
    pub definition: Option<ChartDefinition>,
    pub basis: Option<EvaluationBasis>,
    pub pad_host: Option<super::utility_host::HostIdentity>,
    pub pinned: bool,
}

#[derive(Default)]
pub(crate) struct CalculationChartDialog {
    pub(super) open: bool,
    epoch: u64,
    close_requested: bool,
    view: ChartView,
    pub(super) feedback: Option<String>,
    cache: std::cell::RefCell<Option<ChartCache>>,
    bound: Option<(DocumentId, DocumentIncarnation)>,
    popup_anchors: Vec<(egui::Id, egui::LayerId)>,
    popup_viewport: Option<egui::ViewportId>,
}

struct ChartCache {
    stamp: ChartStamp,
    definition: ChartDefinition,
    dataset: Arc<ChartDataset>,
    references: Vec<ResolvedReferenceLine>,
    scene: Option<(ChartView, u32, u32, Arc<ChartScene>)>,
}

#[derive(Default)]
pub(super) struct ChartActions {
    pub identity: Option<(DocumentId, DocumentIncarnation)>,
    pub pad_host: Option<super::utility_host::HostIdentity>,
    pub host_request: Option<UtilityPresentation>,
    pub close: bool,
    pub definition: Option<ChartDefinition>,
    pub view: Option<ChartView>,
    pub copy: Option<(ChartStamp, ChartDefinition, ChartScene)>,
    pub base: Option<ChartDefinition>,
    pub evaluation: Option<EvaluationBasis>,
    pub table: Option<SensitivityBasis>,
    pub basis: Option<EvaluationBasis>,
    pub popup_anchors: Vec<(egui::Id, egui::LayerId)>,
    pub viewport: Option<egui::ViewportId>,
    #[cfg(test)]
    pub controls: Vec<(String, egui::Rect)>,
}

impl CalculationChartDialog {
    pub(super) fn open(&mut self) { if !self.open { self.epoch = self.epoch.wrapping_add(1); self.open = true; self.close_requested = false; self.view.reset(); } }
    pub(super) fn close(&mut self) { self.open = false; self.close_requested = false; self.view.reset(); self.bound = None; self.popup_anchors.clear(); self.popup_viewport = None; *self.cache.borrow_mut() = None; }
    pub(super) fn request_close(&mut self) { self.close_requested = true; }
    pub(super) fn epoch(&self) -> u64 { self.epoch }
    pub(super) fn commit(&mut self, actions: &ChartActions) {
        if self.bound != actions.identity { self.view.reset(); self.bound = actions.identity; *self.cache.borrow_mut() = None; }
        if let Some(view) = actions.view { self.view = view; }
        self.popup_anchors.clone_from(&actions.popup_anchors);
        self.popup_viewport = actions.viewport;
    }
    pub(super) fn owned_popup_layer(&self, ctx: &egui::Context, base: egui::LayerId) -> Option<egui::LayerId> {
        if self.popup_viewport != Some(ctx.viewport_id()) { return None; }
        self.popup_anchors.iter().find_map(|(id, layer)| {
            if *layer != base || !ctx.read_response(*id).is_some_and(|response| response.enabled() && response.layer_id == base) { return None; }
            let popup = id.with("popup");
            egui::Popup::is_id_open(ctx, popup).then_some(egui::LayerId::new(egui::Order::Foreground, popup))
        })
    }
    pub(super) fn copy_with(&mut self, scene: &ChartScene, write: impl FnOnce(arboard::ImageData<'_>) -> Result<(), ()>) {
        self.feedback = Some(match scene.rasterize() {
            Ok(image) => {
                let data = arboard::ImageData { width: image.width() as usize, height: image.height() as usize, bytes: std::borrow::Cow::Borrowed(image.as_raw()) };
                match write(data) { Ok(()) => "Copied chart image to clipboard.".into(), Err(()) => "Could not copy chart image. Try again.".into() }
            }
            Err(error) => error,
        });
    }

    pub(super) fn render_host(&mut self, ctx: &egui::Context, runtime: &ChartRuntime, owner: bool, first: bool, child: Option<&mut egui::Ui>, presentation: UtilityPresentation) -> ChartActions {
        self.render_host_with_jump(ctx, runtime, owner, first, child, presentation, None)
    }

    pub(super) fn render_host_with_jump(&mut self, ctx: &egui::Context, runtime: &ChartRuntime, owner: bool, first: bool, child: Option<&mut egui::Ui>, presentation: UtilityPresentation, scope: Option<&super::jump_mode::ui::JumpModeScope>) -> ChartActions {
        use super::jump_mode::TargetKind;
        let mut actions = ChartActions { identity: runtime.identity, pad_host: runtime.pad_host, base: runtime.definition.clone(), evaluation: runtime.evaluation.as_ref().map(|value| value.basis), table: runtime.table.as_ref().map(|value| value.basis.clone()), basis: runtime.basis, viewport: Some(ctx.viewport_id()), ..Default::default() };
        let mut window_open = self.open;
        let mut order = 0;
        let mut body = |ui: &mut egui::Ui| {
            let active = owner && first && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            ui.horizontal(|ui| {
                let (button, next) = super::utility_host::presentation_button(ui, presentation, owner);
                chart_target(scope, ui, &button, "presentation", if presentation == UtilityPresentation::Docked { "Pop Out" } else { "Dock" }, TargetKind::Button, &mut order);
                if active && button.clicked() { actions.host_request = Some(next); }
                let close = ui.add_enabled(owner && !runtime.pinned, egui::Button::new("Close"));
                chart_target(scope, ui, &close, "close", "Close", TargetKind::Button, &mut order);
                if close.clicked() { actions.close = true; }
                #[cfg(test)]
                { actions.controls.push(("presentation".into(), button.rect)); actions.controls.push(("close".into(), close.rect)); }
            });
            if let Some(feedback) = &self.feedback { ui.label(feedback); }
            let Some(identity) = runtime.identity else { ui.label("Open a Calculation Pad to view charts."); return; };
            let vectors = runtime.evaluation.as_ref().filter(|evaluation| Some(evaluation.basis) == runtime.basis)
                .map(|evaluation| vector_choices(evaluation)).unwrap_or_default();
            let mut definition = runtime.definition.clone().unwrap_or_else(|| ChartDefinition { source: ChartSource::Vectors { x: None, y: vectors.first().map(|row| vec![row.0.clone()]).unwrap_or_default() }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new(), distribution: None, options: Default::default() });
            let original = definition.clone();
            ui.add_enabled_ui(owner, |ui| {
                ui.horizontal(|ui| {
                    for (kind, name) in [(ChartKind::Line, "Line"), (ChartKind::Bar, "Bar"), (ChartKind::Scatter, "Scatter")] {
                        let response = ui.selectable_value(&mut definition.kind, kind, name);
                        chart_target(scope, ui, &response, &format!("kind/{name}"), name, TargetKind::Selectable, &mut order);
                    }
                });
                ui.horizontal(|ui| {
                    let response = ui.selectable_label(matches!(&definition.source, ChartSource::Vectors { .. }), "Vectors");
                    chart_target(scope, ui, &response, "source/vectors", "Vectors", TargetKind::Selectable, &mut order);
                    if response.clicked() { definition.source = ChartSource::Vectors { x: None, y: vectors.first().map(|row| vec![row.0.clone()]).unwrap_or_default() }; }
                    let response = ui.selectable_label(matches!(&definition.source, ChartSource::Sensitivity { .. }), "Sensitivity");
                    chart_target(scope, ui, &response, "source/sensitivity", "Sensitivity", TargetKind::Selectable, &mut order);
                    if response.clicked() { definition.source = ChartSource::Sensitivity { outputs: runtime.table.as_ref().map(|table| table.basis.definition.outputs.clone()).unwrap_or_default() }; }
                });
                match &mut definition.source {
                    ChartSource::Vectors { x, y } => {
                        let selected = x.as_ref().map(|reference| vectors.iter().find(|row| &row.0 == reference).map(|row| row.1.clone()).unwrap_or_else(|| format!("{} (unavailable)", reference_label(reference)))).unwrap_or_else(|| "Indexes 0…N−1".into());
                        let response = egui::ComboBox::from_id_salt("chart-x").selected_text(selected).show_ui(ui, |ui| {
                            ui.selectable_value(x, None, "Indexes 0…N−1");
                            for (reference, label) in &vectors { let _response = ui.selectable_value(x, Some(reference.clone()), label); #[cfg(test)] actions.controls.push((format!("x/{label}"), _response.rect)); }
                        });
                        chart_target(scope, ui, &response.response, "source/x", "X vector", TargetKind::Button, &mut order);
                        #[cfg(test)] actions.controls.push(("x-menu".into(), response.response.rect));
                        actions.popup_anchors.push((response.response.id, response.response.layer_id));
                        let response = ui.menu_button("Y vectors", |ui| {
                            let _scroll = egui::ScrollArea::vertical().id_salt("chart-y-sources").max_height(240.0).show(ui, |ui| {
                                for (reference, label) in &vectors {
                                    let mut selected = y.contains(reference);
                                    let response = ui.checkbox(&mut selected, label);
                                    #[cfg(test)] actions.controls.push((format!("y/{label}"), response.rect));
                                    if response.changed() { if selected && y.len() < 8 { y.push(reference.clone()); } else if !selected { y.retain(|value| value != reference); } }
                                }
                                for reference in y.clone().into_iter().filter(|reference| !vectors.iter().any(|row| &row.0 == reference)) {
                                    let mut selected = true;
                                    if ui.checkbox(&mut selected, format!("{} (unavailable)", reference_label(&reference))).changed() && !selected { y.retain(|value| value != &reference); }
                                }
                            });
                            #[cfg(test)] actions.controls.push(("y-scroll".into(), _scroll.inner_rect));
                        });
                        chart_target(scope, ui, &response.response, "source/y", "Y vectors", TargetKind::Button, &mut order);
                        #[cfg(test)] actions.controls.push(("y-menu".into(), response.response.rect));
                        actions.popup_anchors.push((response.response.id, response.response.layer_id));
                    }
                    ChartSource::Sensitivity { outputs } => if let Some(table) = &runtime.table { let response = ui.menu_button("Y outputs", |ui| for name in &table.basis.definition.outputs { let mut selected = outputs.contains(name); let response = ui.checkbox(&mut selected, name); #[cfg(test)] actions.controls.push((format!("output/{name}"), response.rect)); if response.changed() { if selected && outputs.len() < 8 { outputs.push(name.clone()); } else if !selected { outputs.retain(|value| value != name); } } }); chart_target(scope, ui, &response.response, "source/outputs", "Y outputs", TargetKind::Button, &mut order); #[cfg(test)] actions.controls.push(("output-menu".into(), response.response.rect)); actions.popup_anchors.push((response.response.id, response.response.layer_id)); },
                }
                ui.horizontal(|ui| { ui.label("X label"); let response = ui.add(egui::TextEdit::singleline(&mut definition.x_label).char_limit(129).desired_width(100.0)); chart_target(scope, ui, &response, "label/x", "X label", TargetKind::TextEntry, &mut order); ui.label("Y label"); let response = ui.add(egui::TextEdit::singleline(&mut definition.y_label).char_limit(129).desired_width(100.0)); chart_target(scope, ui, &response, "label/y", "Y label", TargetKind::TextEntry, &mut order); });
            });
            if definition.source != original.source {
                let old = series_keys(&original.source);
                let new = series_keys(&definition.source);
                definition.hidden = definition.hidden.iter().filter_map(|index| old.get(*index)).filter_map(|key| new.iter().position(|candidate| candidate == key)).collect();
                definition.options.retain_selected_styles(&definition.source);
            }
            if definition != original && active { actions.definition = Some(definition.clone()); }
            let Some(stamp) = chart_stamp(runtime, &definition) else { ui.colored_label(ui.visuals().warn_fg_color, "Chart data is unavailable or outdated. Calculate or generate the current source."); return; };
            let dataset = {
                let mut cache = self.cache.borrow_mut();
                if cache.as_ref().is_none_or(|cache| cache.stamp != stamp || cache.definition != definition) {
                    match chart_data(runtime, &definition) {
                        Ok((_, dataset)) => match resolve_references(runtime, &definition) {
                            Ok(references) => *cache = Some(ChartCache { stamp: stamp.clone(), definition: definition.clone(), dataset: Arc::new(dataset), references, scene: None }),
                            Err(error) => { ui.colored_label(ui.visuals().error_fg_color, error); return; }
                        },
                        Err(error) => { ui.colored_label(ui.visuals().error_fg_color, error); return; }
                    }
                }
                cache.as_ref().map(|cache| Arc::clone(&cache.dataset)).unwrap()
            };
            if !matches!(definition.kind, ChartKind::Histogram | ChartKind::Cumulative) {
                ui.horizontal_wrapped(|ui| for (index, series) in dataset.series.iter().enumerate() { let mut visible = series.visible; let response = ui.add_enabled(owner, egui::Checkbox::new(&mut visible, &series.label)); chart_target(scope, ui, &response, &series_target_id(&definition.source, index), &format!("Series: {}", series.label), TargetKind::Checkbox, &mut order); if response.changed() && active { if visible { definition.hidden.retain(|i| *i != index); } else if !definition.hidden.contains(&index) { definition.hidden.push(index); } actions.definition = Some(definition.clone()); } });
            }
            if definition.kind == ChartKind::Bar {
                egui::CollapsingHeader::new("Bar labels").show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(120.0).id_salt("chart-bar-labels").show_rows(ui, 24.0, dataset.bar_labels.len(), |ui, range| {
                        for index in range {
                            let mut label = definition.bar_labels.get(index).cloned().unwrap_or_else(|| dataset.bar_labels[index].clone());
                            ui.horizontal(|ui| { ui.label(format!("{}", index + usize::from(matches!(&definition.source, ChartSource::Sensitivity { .. }))));
                                if ui.add_enabled(active, egui::TextEdit::singleline(&mut label).char_limit(129)).changed() {
                                    if definition.bar_labels.len() < dataset.bar_labels.len() { definition.bar_labels = dataset.bar_labels.clone(); }
                                    definition.bar_labels[index] = label; actions.definition = Some(definition.clone());
                                }
                            });
                        }
                    });
                });
            }
            let mut view = if self.bound == runtime.identity { self.view } else { ChartView::default() };
            ui.horizontal(|ui| {
                let response = ui.add_enabled(owner, egui::Button::new("−")); chart_target(scope, ui, &response, "zoom/out", "Zoom out", TargetKind::Button, &mut order); if active && response.clicked() { let _ = view.set(view.zoom_x / 1.2, view.zoom_y / 1.2, view.pan_x, view.pan_y); }
                let response = ui.add_enabled(owner, egui::Button::new("+")); chart_target(scope, ui, &response, "zoom/in", "Zoom in", TargetKind::Button, &mut order); if active && response.clicked() { let _ = view.set(view.zoom_x * 1.2, view.zoom_y * 1.2, view.pan_x, view.pan_y); }
                let response = ui.add_enabled(owner, egui::Button::new("Reset View")); chart_target(scope, ui, &response, "zoom/reset", "Reset View", TargetKind::Button, &mut order); if active && response.clicked() { view.reset(); }
            });
            let width = ui.available_width().clamp(160.0, 1024.0) as u32;
            let height = 300;
            let scene = {
                let mut cache = self.cache.borrow_mut();
                let cached = cache.as_ref().and_then(|cache| cache.scene.as_ref()).filter(|(old, w, h, _)| *old == view && *w == width && *h == height).map(|(_, _, _, scene)| Arc::clone(scene));
                if let Some(scene) = cached { Ok(Some(scene)) } else {
                    let references = cache.as_ref().map(|cache| cache.references.as_slice()).unwrap_or_default();
                    dataset.scene_for_definition_with_references(&definition, references, view, width, height).map(|scene| scene.map(|scene| { let scene = Arc::new(scene); if let Some(cache) = cache.as_mut() { cache.scene = Some((view, width, height, Arc::clone(&scene))); } scene }))
                }
            };
            match scene {
                Ok(Some(scene)) => {
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(width as f32, height as f32), egui::Sense::click_and_drag());
                    ui.painter().rect_filled(rect, 0.0, egui::Color32::WHITE);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| { ui.set_clip_rect(rect.intersect(ui.clip_rect())); paint_scene(ui, rect.min, &scene); });
                    if response.dragged() && active { let delta = response.drag_delta(); let _ = view.set(view.zoom_x, view.zoom_y, view.pan_x - f64::from(delta.x) / f64::from(width), view.pan_y + f64::from(delta.y) / f64::from(height)); }
                    if let Some(pointer) = response.hover_pos() && let Some(hit) = scene.hit([(pointer.x - rect.min.x), (pointer.y - rect.min.y)], 8.0) { let x = hit.bin_bounds.as_ref().map_or_else(|| hit.raw_x.clone(), |bounds| bounds.membership_label()); response.on_hover_text(format!("{} · {}{}", x, hit.raw_y, if hit.approximate { " (approximate)" } else { "" })); }
                    if scene.approximate || scene.coordinate_collisions { ui.label("Plot coordinates are approximate; exact values remain in tooltips."); }
                    let response = ui.add_enabled(owner && actions.definition.is_none(), egui::Button::new("Copy Chart as Image")); chart_target(scope, ui, &response, "copy", "Copy Chart as Image", TargetKind::Button, &mut order); if active && response.clicked() { actions.copy = Some((stamp, definition.clone(), (*scene).clone())); }
                }
                Ok(None) => { ui.label("No data"); }
                Err(error) => { ui.colored_label(ui.visuals().error_fg_color, error); }
            }
            if view != self.view && active { actions.view = Some(view); }
            let _ = identity;
        };
        if let Some(ui) = child { egui::ScrollArea::vertical().show(ui, |ui| body(ui)); }
        else { egui::Window::new("Charts").id(egui::Id::new("calculation-chart-window")).show_utility(ctx, "calculation-pad-charts", egui::Id::new("calculation-chart-window"), Some(&mut window_open), super::utility_dialog_size::UtilityWindowSizeRecipe::new(super::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::vec2(700.0, 560.0)), egui::vec2(360.0, 280.0), [None, None]), |ui| { egui::ScrollArea::vertical().show(ui, |ui| body(ui)); }); }
        actions.close |= (!window_open || self.close_requested) && !runtime.pinned;
        actions
    }
}

pub(super) fn chart_stamp(runtime: &ChartRuntime, definition: &ChartDefinition) -> Option<ChartStamp> {
    let evaluation = || runtime.evaluation.as_ref().filter(|value| Some(value.basis) == runtime.basis);
    if matches!(definition.kind, ChartKind::Histogram | ChartKind::Cumulative) {
        return evaluation().map(|value| ChartStamp::Evaluation(value.basis));
    }
    match &definition.source {
        ChartSource::Vectors { .. } => evaluation().map(|value| ChartStamp::Evaluation(value.basis)),
        ChartSource::Sensitivity { .. } => {
            let table = runtime.table.as_ref()?;
            if definition.options.references.iter().any(|reference| matches!(reference.value, ChartReferenceValue::Named(_))) {
                let evaluation = evaluation()?;
                if table.basis.document != evaluation.basis.document || table.basis.incarnation != evaluation.basis.incarnation || table.basis.revision != evaluation.basis.revision { return None; }
                Some(ChartStamp::SensitivityWithEvaluation(table.basis.clone(), evaluation.basis))
            } else { Some(ChartStamp::Sensitivity(table.basis.clone())) }
        }
    }
}

fn resolve_references(runtime: &ChartRuntime, definition: &ChartDefinition) -> Result<Vec<ResolvedReferenceLine>, String> {
    let mut resolved = Vec::new();
    for reference in &definition.options.references {
        let ChartReferenceValue::Named(name) = &reference.value else { continue; };
        let evaluation = runtime.evaluation.as_ref().filter(|evaluation| Some(evaluation.basis) == runtime.basis).ok_or("Current chart reference results are unavailable or outdated.")?;
        if evaluation.diagnostic.is_some() { return Err("Current chart reference evaluation failed.".into()); }
        let binding = evaluation.final_bindings.get(name).ok_or_else(|| format!("Chart reference '{name}' is unavailable."))?;
        let LineOutcome::Value(value) = &binding.outcome else { return Err(format!("Chart reference '{name}' failed or has no value.")); };
        let value = crate::calculation_pad::scalar_reference_coordinate(value).map_err(|error| format!("Chart reference '{name}': {error}"))?;
        resolved.push(ResolvedReferenceLine { id: reference.id, value });
    }
    Ok(resolved)
}

pub(super) fn actions_current(actions: &ChartActions, runtime: &ChartRuntime) -> bool {
    actions.identity.is_some() && actions.identity == runtime.identity && actions.pad_host.is_some() && actions.pad_host == runtime.pad_host
        && actions.base == runtime.definition && actions.basis == runtime.basis
        && actions.evaluation == runtime.evaluation.as_ref().map(|value| value.basis)
        && actions.table == runtime.table.as_ref().map(|value| value.basis.clone())
}

pub(super) fn chart_data(runtime: &ChartRuntime, definition: &ChartDefinition) -> Result<(ChartStamp, ChartDataset), String> {
    let stamp = chart_stamp(runtime, definition).ok_or("Chart data is unavailable or outdated.")?;
    resolve_references(runtime, definition)?;
    let mut dataset = if matches!(definition.kind, ChartKind::Histogram | ChartKind::Cumulative) {
        definition.validate_distribution()?;
        let distribution = definition.distribution.as_ref().ok_or("Choose a distribution vector.")?;
        let evaluation = runtime.evaluation.as_ref().filter(|evaluation| Some(evaluation.basis) == runtime.basis).ok_or("Current distribution results are unavailable or outdated.")?;
        let (label, outcome) = resolve_source(evaluation, &distribution.vector)?;
        let LineOutcome::Value(crate::calculation_pad::CalculationValue::Vector(vector)) = outcome else {
            return Err(format!("{} is not a numeric vector.", reference_label(&distribution.vector)));
        };
        if definition.kind == ChartKind::Histogram {
            ChartDataset::from_histogram(label, vector.components(), distribution)?
        } else { ChartDataset::from_cumulative(label, vector.components())? }
    } else { match &definition.source {
        ChartSource::Vectors { x, y } => {
            let evaluation = runtime.evaluation.as_ref().filter(|evaluation| Some(evaluation.basis) == runtime.basis).ok_or("Current vector results are unavailable or outdated.")?;
            let x = x.as_ref().map(|reference| resolve_vector(evaluation, reference)).transpose().map_err(|error| format!("X vector: {error}"))?.map(|(_, values)| values);
            let vectors = y.iter().map(|reference| resolve_vector(evaluation, reference)).collect::<Result<Vec<_>, _>>().map_err(|error| format!("Y vector: {error}"))?;
            ChartDataset::from_vectors(x, &vectors)?
        }
        ChartSource::Sensitivity { outputs } => {
            let table = runtime.table.as_ref().ok_or("The current sensitivity table is unavailable.")?;
            let columns = outputs.iter().map(|name| table.basis.definition.outputs.iter().position(|output| output == name)).collect::<Option<Vec<_>>>().ok_or("A selected sensitivity output is unavailable.")?;
            ChartDataset::from_sensitivity(table, &columns)?
        }
    }};
    if !definition.x_label.is_empty() { dataset.x_label = definition.x_label.clone(); }
    if !definition.y_label.is_empty() { dataset.y_label = definition.y_label.clone(); }
    if !matches!(definition.kind, ChartKind::Histogram | ChartKind::Cumulative) {
        for (generated, saved) in dataset.bar_labels.iter_mut().zip(&definition.bar_labels) { generated.clone_from(saved); }
        for (index, series) in dataset.series.iter_mut().enumerate() { series.visible = !definition.hidden.contains(&index); }
    }
    Ok((stamp, dataset))
}

fn vector_values(outcome: &LineOutcome) -> Option<&[f64]> {
    match outcome {
        LineOutcome::Value(crate::calculation_pad::CalculationValue::Vector(vector)) => Some(vector.components()),
        LineOutcome::Value(crate::calculation_pad::CalculationValue::Sequence(sequence)) => Some(sequence.values()),
        _ => None,
    }
}

fn reference_label(reference: &ChartVectorReference) -> String {
    match reference {
        ChartVectorReference::Physical(id) => format!("row #{}", id.get()),
        ChartVectorReference::Named { name } => name.clone(),
    }
}

fn vector_choices(evaluation: &SessionEvaluation) -> Vec<(ChartVectorReference, String)> {
    let mut named = evaluation.final_bindings.iter().filter_map(|(name, binding)| {
        vector_values(&binding.outcome).map(|values| (ChartVectorReference::Named { name: name.clone() }, format!("{name} ({} values)", values.len())))
    }).collect::<Vec<_>>();
    named.sort_by(|a, b| a.1.cmp(&b.1));
    named.extend(evaluation.lines.iter().filter_map(|line| vector_values(&line.outcome).map(|values| {
        let name = line.assignment_name.as_ref().map_or(String::new(), |name| format!(" · {name}"));
        (ChartVectorReference::Physical(line.id), format!("r{}{name} ({} values)", line.line_index + 1, values.len()))
    })));
    named
}

fn resolve_vector<'a>(evaluation: &'a SessionEvaluation, reference: &'a ChartVectorReference) -> Result<(&'a str, &'a [f64]), String> {
    let (label, outcome) = resolve_source(evaluation, reference)?;
    let values = vector_values(outcome).ok_or_else(|| format!("{} is not a vector or sequence.", reference_label(reference)))?;
    Ok((label, values))
}

fn resolve_source<'a>(evaluation: &'a SessionEvaluation, reference: &'a ChartVectorReference) -> Result<(&'a str, &'a LineOutcome), String> {
    let (label, outcome) = match reference {
        ChartVectorReference::Named { name } => {
            let binding = evaluation.final_bindings.get(name).ok_or_else(|| format!("named source '{name}' is unavailable."))?;
            (name.as_str(), &binding.outcome)
        }
        ChartVectorReference::Physical(id) => {
            let line = evaluation.lines.iter().find(|line| line.id == *id).ok_or_else(|| format!("physical source #{} is unavailable.", id.get()))?;
            (line.assignment_name.as_deref().unwrap_or("vector"), &line.outcome)
        }
    };
    if matches!(outcome, LineOutcome::Error(_)) {
        return Err(format!("{} failed.", reference_label(reference)));
    }
    Ok((label, outcome))
}

fn series_keys(source: &ChartSource) -> Vec<String> {
    match source { ChartSource::Vectors { y, .. } => y.iter().map(|reference| match reference { ChartVectorReference::Physical(id) => format!("line:{}", id.get()), ChartVectorReference::Named { name } => format!("name:{name}") }).collect(), ChartSource::Sensitivity { outputs } => outputs.iter().map(|name| format!("output:{name}")).collect() }
}

fn series_target_id(source: &ChartSource, index: usize) -> String {
    let keys = series_keys(source);
    keys.get(index).map_or_else(|| format!("series/missing/{index}"), |key| format!("series/{:x}", egui::Id::new(key).value()))
}

fn paint_scene(ui: &egui::Ui, origin: egui::Pos2, scene: &ChartScene) {
    let painter = ui.painter();
    let color = |c: &[u8; 4]| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    let point = |p: &[f32; 2]| origin + egui::vec2(p[0], p[1]);
    for primitive in &scene.primitives { match primitive {
        ScenePrimitive::Line { start, end, color: c, width } => { painter.line_segment([point(start), point(end)], egui::Stroke::new(*width, color(c))); }
        ScenePrimitive::Rect { min, max, color: c } => { painter.rect_filled(egui::Rect::from_min_max(point(min), point(max)), 0.0, color(c)); }
        ScenePrimitive::Circle { center, radius, color: c } => { painter.circle_filled(point(center), *radius, color(c)); }
        ScenePrimitive::Text { position, text, color: c, size } => { painter.text(point(position), egui::Align2::LEFT_TOP, text, egui::FontId::proportional(*size), color(c)); }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_series_jump_identity_follows_source_not_position() {
        let reference = |name: &str| ChartVectorReference::Named { name: name.into() };
        let first = ChartSource::Vectors { x: None, y: vec![reference("alpha"), reference("beta")] };
        let reordered = ChartSource::Vectors { x: None, y: vec![reference("beta"), reference("alpha")] };
        assert_eq!(series_target_id(&first, 0), series_target_id(&reordered, 1));
        assert_ne!(series_target_id(&first, 0), series_target_id(&reordered, 0));
    }

    #[test]
    fn cumulative_adapter_uses_current_vector_even_with_dormant_sensitivity() {
        use crate::calculation_pad::{DistributionBins, DistributionDefinition, DistributionScale};
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("samples = [1, 2, 2]\nother = 3").unwrap(); session.evaluation();
        let mut definition = ChartDefinition { source: ChartSource::Sensitivity { outputs: vec!["other".into()] }, kind: ChartKind::Cumulative, x_label: String::new(), y_label: String::new(), hidden: vec![0], bar_labels: vec![], distribution: Some(DistributionDefinition { vector: ChartVectorReference::Named { name: "samples".into() }, bins: DistributionBins::Width { width: crate::calculation_pad::ChartFiniteValue::try_from(0.001).unwrap() }, scale: DistributionScale::Count }), options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(definition.clone()), basis: session.evaluation_basis(), pad_host: None, pinned: false };
        assert!(matches!(chart_stamp(&runtime, &definition), Some(ChartStamp::Evaluation(_))));
        let dataset = chart_data(&runtime, &definition).unwrap().1;
        assert_eq!(dataset.series[0].points.len(), 2);
        assert!(dataset.series[0].visible);
        assert!(dataset.scene_for_definition(&definition, ChartView::default(), 640, 400).unwrap().is_some());
        definition.distribution.as_mut().unwrap().vector = ChartVectorReference::Named { name: "other".into() };
        assert!(chart_data(&runtime, &definition).unwrap_err().contains("not a numeric vector"));
        definition.distribution.as_mut().unwrap().vector = ChartVectorReference::Named { name: "samples".into() };
        session.replace_source("samples = [2, 3]\nother = 3").unwrap(); runtime.basis = session.evaluation_basis();
        assert!(chart_stamp(&runtime, &definition).is_none());
        assert!(chart_data(&runtime, &definition).is_err());
    }

    #[test]
    fn histogram_adapter_uses_current_vector_and_configured_scale() {
        use crate::calculation_pad::{DistributionBins, DistributionDefinition, DistributionScale};
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("samples = [1, 2, 2, 4]").unwrap(); session.evaluation();
        let definition = ChartDefinition { source: ChartSource::Sensitivity { outputs: vec![] }, kind: ChartKind::Histogram, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: Some(DistributionDefinition { vector: ChartVectorReference::Named { name: "samples".into() }, bins: DistributionBins::Count { count: 2 }, scale: DistributionScale::Percentage }), options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(definition.clone()), basis: session.evaluation_basis(), pad_host: None, pinned: false };
        let (stamp, dataset) = chart_data(&runtime, &definition).unwrap();
        assert!(matches!(stamp, ChartStamp::Evaluation(_)));
        assert_eq!(dataset.distribution.as_ref().unwrap().bins.iter().map(|bin| bin.count).collect::<Vec<_>>(), [3, 1]);
        assert_eq!(dataset.y_label, "Percentage");
        assert!(dataset.scene_for_definition(&definition, ChartView::default(), 640, 400).unwrap().unwrap().rasterize().is_ok());
        session.replace_source("samples = [5]").unwrap(); runtime.basis = session.evaluation_basis();
        assert!(chart_stamp(&runtime, &definition).is_none());
        assert!(chart_data(&runtime, &definition).is_err());
    }

    #[test]
    fn chart_named_reference_uses_current_evaluation_and_sensitivity_stamp() {
        use crate::calculation_pad::{ChartReferenceAxis, ChartReferenceLine, DocumentOptions, Sampling, SensitivityDefinition};
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("v = [1, 2]\ntarget = 1\ntarget = 2").unwrap();
        session.evaluation();
        let basis = session.evaluation_basis().unwrap();
        let sensitivity = SensitivityBasis { document: basis.document, incarnation: basis.incarnation, revision: basis.revision, options: DocumentOptions::default(), definition: SensitivityDefinition { input: session.line_id(1).unwrap(), start: "1".into(), end: "2".into(), sampling: Sampling::Count(2), outputs: vec!["target".into()] }, input_name: "target".into(), generation: 1 };
        let table = Arc::new(SensitivityTable { basis: sensitivity, input_kind: crate::calculation_pad::InputKind::Scalar, rows: Vec::new(), work_units: 0 });
        let mut definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Named { name: "v".into() }] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new(), distribution: None, options: Default::default() };
        definition.options.references.push(ChartReferenceLine { id: 1, axis: ChartReferenceAxis::Y, value: ChartReferenceValue::Named("target".into()), label: "target".into() });
        definition.options.next_reference_id = 2;
        let mut runtime = ChartRuntime { identity: Some((basis.document, basis.incarnation)), evaluation: session.shared_evaluation(), table: Some(table), definition: Some(definition.clone()), basis: Some(basis), pad_host: None, pinned: false };
        assert_eq!(resolve_references(&runtime, &definition).unwrap()[0].value, 2.0);
        assert!(chart_data(&runtime, &definition).is_ok());
        let vector_stamp = chart_stamp(&runtime, &definition).unwrap();
        definition.source = ChartSource::Sensitivity { outputs: vec!["target".into()] };
        assert!(matches!(chart_stamp(&runtime, &definition), Some(ChartStamp::SensitivityWithEvaluation(_, _))));
        Arc::make_mut(runtime.table.as_mut().unwrap()).basis.revision += 1;
        assert!(chart_stamp(&runtime, &definition).is_none());
        Arc::make_mut(runtime.table.as_mut().unwrap()).basis.revision -= 1;
        let mut stale = basis; stale.revision += 1;
        runtime.basis = Some(stale);
        assert!(chart_stamp(&runtime, &definition).is_none());
        assert!(chart_data(&runtime, &definition).is_err());
        assert_ne!(chart_stamp(&runtime, &definition), Some(vector_stamp));
        definition.options.references.clear();
        assert!(matches!(chart_stamp(&runtime, &definition), Some(ChartStamp::Sensitivity(_))));
        runtime.basis = Some(basis);
        definition.options.references.push(ChartReferenceLine { id: 2, axis: ChartReferenceAxis::Y, value: ChartReferenceValue::Named("v".into()), label: String::new() });
        assert!(resolve_references(&runtime, &definition).is_err());
        definition.options.references[0].value = ChartReferenceValue::Named("missing".into());
        assert!(resolve_references(&runtime, &definition).unwrap_err().contains("unavailable"));
    }

    fn chart_frame(ctx: &egui::Context, dialog: &mut CalculationChartDialog, runtime: &mut ChartRuntime, events: Vec<egui::Event>) -> ChartActions {
        let base = dialog.popup_anchors.first().map(|(_, layer)| *layer);
        let popup = base.and_then(|layer| dialog.owned_popup_layer(ctx, layer));
        let owner = !egui::Popup::is_any_open(ctx) || popup.is_some();
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 900.0))), focused: true, events, ..Default::default() };
        let mut actions = None;
        let output = ctx.run_ui(input, |ui| { actions = Some(dialog.render_host(ctx, runtime, owner, true, Some(ui), UtilityPresentation::Docked)); });
        output.drop_without_applying_deltas();
        let actions = actions.unwrap();
        if owner {
            dialog.commit(&actions);
            if let Some(definition) = &actions.definition { runtime.definition = Some(definition.clone()); }
        }
        actions
    }

    fn chart_click(ctx: &egui::Context, dialog: &mut CalculationChartDialog, runtime: &mut ChartRuntime, point: egui::Pos2) -> ChartActions {
        let mut result = None;
        for pressed in [true, false] {
            result = Some(chart_frame(ctx, dialog, runtime, vec![egui::Event::PointerMoved(point), egui::Event::PointerButton { pos: point, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }]));
        }
        result.unwrap()
    }

    #[test]
    fn chart_owned_popup_menus_accept_actual_selections_across_frames() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("v = [1, 2]\nw = [3, 4]\nx = 1\ny = x\nz = x + 1").unwrap();
        session.evaluation();
        let evaluation = session.shared_evaluation().unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        session.enable_background_evaluation(move || { let _ = tx.send(()); });
        session.request_sensitivity(crate::calculation_pad::SensitivityDefinition { input: ids[2], start: "1".into(), end: "2".into(), sampling: crate::calculation_pad::Sampling::Count(2), outputs: vec!["y".into(), "z".into()] }).unwrap();
        for _ in 0..20 { let _ = rx.recv_timeout(std::time::Duration::from_secs(5)); session.poll_sensitivity(); if session.sensitivity_table().is_some() { break; } }
        let table = session.sensitivity_table().unwrap();
        let definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Physical(ids[0])] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new(), distribution: None, options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: Some(evaluation), table: Some(table), definition: Some(definition), basis: session.evaluation_basis(), pad_host: Some(super::super::utility_host::HostIdentity { host: UtilityPresentation::Docked, generation: 1 }), pinned: false };
        let ctx = egui::Context::default(); let mut dialog = CalculationChartDialog::default(); dialog.open();
        let mut actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        let control = |actions: &ChartActions, name: &str| actions.controls.iter().find(|(key, _)| key == name).unwrap_or_else(|| panic!("missing {name}: {:?}", actions.controls)).1.center();
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "x-menu"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        let base = dialog.popup_anchors[0].1;
        assert!(dialog.owned_popup_layer(&ctx, base).is_some());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "x/w (2 values)"));
        assert!(matches!(&runtime.definition.as_ref().unwrap().source, ChartSource::Vectors { x: Some(ChartVectorReference::Named { name }), .. } if name == "w"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "y-menu"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "y/w (2 values)"));
        assert!(matches!(&runtime.definition.as_ref().unwrap().source, ChartSource::Vectors { y, .. } if y.contains(&ChartVectorReference::Named { name: "w".into() })));
        egui::Popup::close_all(&ctx);
        runtime.definition.as_mut().unwrap().source = ChartSource::Sensitivity { outputs: vec!["y".into()] };
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "output-menu"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "output/z"));
        assert!(matches!(&runtime.definition.as_ref().unwrap().source, ChartSource::Sensitivity { outputs } if outputs == &["y", "z"]));
        egui::Popup::close_all(&ctx);
        egui::Popup::open_id(&ctx, egui::Id::new("unrelated-popup"));
        let base = dialog.popup_anchors[0].1;
        assert!(dialog.owned_popup_layer(&ctx, base).is_none());
    }

    #[test]
    fn chart_y_menu_scrolls_to_tail_physical_result() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        let source = (0..40).map(|index| format!("v{index} = [1]")).collect::<Vec<_>>().join("\n");
        session.replace_source(source).unwrap();
        session.evaluation();
        let ids = session.line_ids().unwrap().to_vec();
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Named { name: "v0".into() }] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new(), distribution: None, options: Default::default() }), basis: session.evaluation_basis(), pad_host: Some(super::super::utility_host::HostIdentity { host: UtilityPresentation::Docked, generation: 1 }), pinned: false };
        let ctx = egui::Context::default();
        let mut dialog = CalculationChartDialog::default();
        dialog.open();
        let mut actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        let menu = actions.controls.iter().find(|(name, _)| name == "y-menu").unwrap().1.center();
        chart_click(&ctx, &mut dialog, &mut runtime, menu);
        for _ in 0..12 {
            actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
            let scroll = actions.controls.iter().find(|(name, _)| name == "y-scroll").unwrap().1;
            let tail = actions.controls.iter().find(|(name, _)| name == "y/r40 · v39 (1 values)").unwrap().1;
            if scroll.contains_rect(tail) {
                chart_click(&ctx, &mut dialog, &mut runtime, tail.center());
                assert!(matches!(&runtime.definition.as_ref().unwrap().source, ChartSource::Vectors { y, .. } if y.contains(&ChartVectorReference::Physical(ids[39]))));
                return;
            }
            chart_frame(&ctx, &mut dialog, &mut runtime, vec![egui::Event::PointerMoved(scroll.center()), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -800.0), modifiers: egui::Modifiers::NONE, phase: egui::TouchPhase::Move }]);
        }
        panic!("last physical vector did not become visible in the Y menu");
    }

    #[test]
    fn chart_adapter_reports_length_errors_and_uses_only_current_snapshots() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("[1, 2]\n[3]").unwrap();
        session.evaluation();
        let evaluation = session.shared_evaluation().unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        let mut definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Physical(ids[0])] }, kind: ChartKind::Bar, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: vec!["custom".into()], distribution: None, options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: Some(evaluation), table: None, definition: Some(definition.clone()), basis: session.evaluation_basis(), pad_host: None, pinned: false };
        assert_eq!(chart_data(&runtime, &definition).unwrap().1.bar_labels, vec!["custom", "1"]);
        definition.source = ChartSource::Vectors { x: None, y: ids.into_iter().map(ChartVectorReference::Physical).collect() };
        assert!(chart_data(&runtime, &definition).unwrap_err().contains("matching lengths"));
        session.replace_source("[1, 2]").unwrap();
        runtime.evaluation = session.shared_evaluation();
        assert!(chart_stamp(&runtime, &definition).is_none());
    }

    #[test]
    fn chart_rolling_series_requires_explicit_aligned_x_values() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source(concat!(
            "x = [0, 1, 2, 3]\ny = [10, 20, 15, 30]\nwindow = 2\n",
            "aligned = slice(x, window - 1, len(x))\n",
            "average = rolling_average(y, window)"
        )).unwrap();
        session.evaluation();
        let runtime = ChartRuntime {
            identity: session.current_id().zip(session.document_incarnation()),
            evaluation: session.shared_evaluation(), table: None, definition: None,
            basis: session.evaluation_basis(), pad_host: None, pinned: false,
        };
        let mut definition = ChartDefinition {
            source: ChartSource::Vectors {
                x: Some(ChartVectorReference::Named { name: "aligned".into() }),
                y: vec![ChartVectorReference::Named { name: "average".into() }],
            },
            kind: ChartKind::Line, x_label: String::new(), y_label: String::new(),
            hidden: Vec::new(), bar_labels: Vec::new(), distribution: None, options: Default::default(),
        };
        let dataset = chart_data(&runtime, &definition).unwrap().1;
        let points = &dataset.series[0].points;
        assert_eq!(points.iter().map(|point| point.as_ref().map(|point| (point.x, point.y))).collect::<Vec<_>>(),
            [Some((1.0, 15.0)), Some((2.0, 17.5)), Some((3.0, 22.5))]);
        definition.source = ChartSource::Vectors {
            x: Some(ChartVectorReference::Named { name: "x".into() }),
            y: vec![ChartVectorReference::Named { name: "average".into() }],
        };
        assert!(chart_data(&runtime, &definition).unwrap_err().contains("matching lengths"));
    }

    #[test]
    fn chart_empty_adapter_keeps_missing_mismatch_and_hidden_states_distinct() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("[]\n[1]\n1 / 0").unwrap();
        session.evaluation();
        let ids = session.line_ids().unwrap().to_vec();
        let mut definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Physical(ids[0])] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new(), distribution: None, options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(definition.clone()), basis: session.evaluation_basis(), pad_host: Some(super::super::utility_host::HostIdentity { host: UtilityPresentation::Docked, generation: 1 }), pinned: false };
        let dataset = chart_data(&runtime, &definition).unwrap().1;
        assert!(dataset.scene(ChartKind::Line, ChartView::default(), 640, 400).unwrap().is_none());
        let ctx = egui::Context::default();
        let mut dialog = CalculationChartDialog::default();
        dialog.open();
        assert!(chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()).copy.is_none());
        definition.hidden.push(0);
        assert!(chart_data(&runtime, &definition).unwrap().1.scene(ChartKind::Line, ChartView::default(), 640, 400).unwrap_err().contains("at least one"));
        definition.hidden.clear();
        definition.source = ChartSource::Vectors { x: Some(ChartVectorReference::Physical(ids[1])), y: vec![ChartVectorReference::Physical(ids[0])] };
        assert!(chart_data(&runtime, &definition).unwrap_err().contains("matching lengths"));
        definition.source = ChartSource::Vectors { x: Some(ChartVectorReference::Physical(ids[2])), y: vec![ChartVectorReference::Physical(ids[0])] };
        assert!(chart_data(&runtime, &definition).unwrap_err().contains("X vector:") && chart_data(&runtime, &definition).unwrap_err().contains("failed"));
        definition.source = ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Physical(ids[2])] };
        assert!(chart_data(&runtime, &definition).unwrap_err().contains("Y vector:") && chart_data(&runtime, &definition).unwrap_err().contains("failed"));
    }

    #[test]
    fn chart_named_source_follows_final_binding_and_current_basis() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("values = []\nfor i in range(3):\n    values = append(values, i)\nvalues = append(values, get([], 0))").unwrap();
        session.evaluation();
        let initial = session.shared_evaluation().unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        let definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Named { name: "values".into() }] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new(), distribution: None, options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: Some(initial.clone()), table: None, definition: Some(definition.clone()), basis: session.evaluation_basis(), pad_host: None, pinned: false };
        let choices = vector_choices(&initial);
        assert_eq!(choices.iter().filter(|(reference, _)| matches!(reference, ChartVectorReference::Named { name } if name == "values")).count(), 1);
        assert_eq!(chart_data(&runtime, &definition).unwrap().1.series[0].points.len(), 3);
        let physical = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Physical(ids[0])] }, ..definition.clone() };
        assert_eq!(chart_data(&runtime, &physical).unwrap().1.series[0].points.len(), 0);
        session.replace_source("values = []\nfor i in range(0):\n    values = append(values, i)\nvalues = append(values, get([], 0))").unwrap();
        runtime.basis = session.evaluation_basis();
        assert!(chart_stamp(&runtime, &definition).is_none());
        assert!(chart_data(&runtime, &definition).is_err());
        session.evaluation();
        runtime.evaluation = session.shared_evaluation();
        assert_eq!(chart_data(&runtime, &definition).unwrap().1.series[0].points.len(), 0);
        assert_eq!(initial.final_bindings["values"].line_index, 2);
    }

    #[test]
    fn chart_copy_uses_scene_and_surfaces_clipboard_failure() {
        let dataset = ChartDataset::from_vectors(None, &[("series", &[1.0, 2.0])]).unwrap();
        let scene = dataset.scene(ChartKind::Line, ChartView::default(), 400, 300).unwrap().unwrap();
        let mut dialog = CalculationChartDialog::default();
        let mut called = false;
        dialog.copy_with(&scene, |image| { called = true; assert_eq!((image.width, image.height), (400, 300)); Err(()) });
        assert!(called && dialog.feedback.as_deref().unwrap().contains("Could not copy"));
    }

    #[test]
    fn chart_presentation_and_close_controls_share_a_row_and_respect_pin() {
        let ctx = egui::Context::default();
        let mut dialog = CalculationChartDialog::default();
        dialog.open();
        let mut runtime = ChartRuntime { identity: None, evaluation: None, table: None, definition: None, basis: None, pad_host: None, pinned: true };
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        let presentation = actions.controls.iter().find(|(name, _)| name == "presentation").unwrap().1;
        let close = actions.controls.iter().find(|(name, _)| name == "close").unwrap().1;
        assert!(presentation.center().y >= close.top() && presentation.center().y <= close.bottom());
        assert!(presentation.right() <= close.left());
        assert!(!chart_click(&ctx, &mut dialog, &mut runtime, close.center()).close);
    }
}
