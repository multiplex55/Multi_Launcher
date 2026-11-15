//! Read-only chart companion; actions are committed by its retained utility host.
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::UtilityPresentation;
use crate::calculation_pad::{ChartDataset, ChartDefinition, ChartKind, ChartScene, ChartSource, ChartView, DocumentId, DocumentIncarnation, EvaluationBasis, LineId, LineOutcome, ScenePrimitive, SensitivityBasis, SensitivityTable, SessionEvaluation};
use eframe::egui;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ChartStamp { Evaluation(EvaluationBasis), Sensitivity(SensitivityBasis) }

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
}

struct ChartCache {
    stamp: ChartStamp,
    definition: ChartDefinition,
    dataset: Arc<ChartDataset>,
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
}

impl CalculationChartDialog {
    pub(super) fn open(&mut self) { if !self.open { self.epoch = self.epoch.wrapping_add(1); self.open = true; self.close_requested = false; self.view.reset(); } }
    pub(super) fn close(&mut self) { self.open = false; self.close_requested = false; self.view.reset(); self.bound = None; *self.cache.borrow_mut() = None; }
    pub(super) fn request_close(&mut self) { self.close_requested = true; }
    pub(super) fn epoch(&self) -> u64 { self.epoch }
    pub(super) fn commit(&mut self, actions: &ChartActions) {
        if self.bound != actions.identity { self.view.reset(); self.bound = actions.identity; *self.cache.borrow_mut() = None; }
        if let Some(view) = actions.view { self.view = view; }
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
        let mut actions = ChartActions { identity: runtime.identity, pad_host: runtime.pad_host, base: runtime.definition.clone(), evaluation: runtime.evaluation.as_ref().map(|value| value.basis), table: runtime.table.as_ref().map(|value| value.basis.clone()), basis: runtime.basis, ..Default::default() };
        let mut window_open = self.open;
        let mut body = |ui: &mut egui::Ui| {
            let active = owner && first && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            let (button, next) = super::utility_host::presentation_button(ui, presentation, active);
            if active && button.clicked() { actions.host_request = Some(next); }
            if ui.add_enabled(active && !runtime.pinned, egui::Button::new("Close")).clicked() { actions.close = true; }
            if let Some(feedback) = &self.feedback { ui.label(feedback); }
            let Some(identity) = runtime.identity else { ui.label("Open a Calculation Pad to view charts."); return; };
            let vectors = runtime.evaluation.as_ref().map(|evaluation| evaluation.lines.iter().filter_map(|line| {
                let values = match &line.outcome {
                    LineOutcome::Value(crate::calculation_pad::CalculationValue::Vector(vector)) => vector.components(),
                    LineOutcome::Value(crate::calculation_pad::CalculationValue::Sequence(sequence)) => sequence.values(),
                    _ => return None,
                };
                Some((line.id, line.assignment_name.clone().unwrap_or_else(|| format!("r{}", line.line_index + 1)), values.to_vec()))
            }).collect::<Vec<_>>()).unwrap_or_default();
            let mut definition = runtime.definition.clone().unwrap_or_else(|| ChartDefinition { source: ChartSource::Vectors { x: None, y: vectors.first().map(|row| vec![row.0]).unwrap_or_default() }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new() });
            let original = definition.clone();
            ui.add_enabled_ui(active, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut definition.kind, ChartKind::Line, "Line");
                    ui.selectable_value(&mut definition.kind, ChartKind::Bar, "Bar");
                    ui.selectable_value(&mut definition.kind, ChartKind::Scatter, "Scatter");
                });
                ui.horizontal(|ui| {
                    if ui.selectable_label(matches!(&definition.source, ChartSource::Vectors { .. }), "Vectors").clicked() { definition.source = ChartSource::Vectors { x: None, y: vectors.first().map(|row| vec![row.0]).unwrap_or_default() }; }
                    if ui.selectable_label(matches!(&definition.source, ChartSource::Sensitivity { .. }), "Sensitivity").clicked() { definition.source = ChartSource::Sensitivity { outputs: runtime.table.as_ref().map(|table| table.basis.definition.outputs.clone()).unwrap_or_default() }; }
                });
                match &mut definition.source {
                    ChartSource::Vectors { x, y } => {
                        egui::ComboBox::from_id_salt("chart-x").selected_text(x.and_then(|id| vectors.iter().find(|row| row.0 == id).map(|row| row.1.clone())).unwrap_or_else(|| "Positions 1…N".into())).show_ui(ui, |ui| {
                            ui.selectable_value(x, None, "Positions 1…N");
                            for (id, name, _) in &vectors { ui.selectable_value(x, Some(*id), name); }
                        });
                        ui.menu_button("Y vectors", |ui| for (id, name, _) in &vectors { let mut selected = y.contains(id); if ui.checkbox(&mut selected, name).changed() { if selected && y.len() < 8 { y.push(*id); } else if !selected { y.retain(|value| value != id); } } });
                    }
                    ChartSource::Sensitivity { outputs } => if let Some(table) = &runtime.table { ui.menu_button("Y outputs", |ui| for name in &table.basis.definition.outputs { let mut selected = outputs.contains(name); if ui.checkbox(&mut selected, name).changed() { if selected && outputs.len() < 8 { outputs.push(name.clone()); } else if !selected { outputs.retain(|value| value != name); } } }); },
                }
                ui.horizontal(|ui| { ui.label("X label"); ui.add(egui::TextEdit::singleline(&mut definition.x_label).char_limit(129).desired_width(100.0)); ui.label("Y label"); ui.add(egui::TextEdit::singleline(&mut definition.y_label).char_limit(129).desired_width(100.0)); });
            });
            if definition.source != original.source {
                let old = series_keys(&original.source);
                let new = series_keys(&definition.source);
                definition.hidden = definition.hidden.iter().filter_map(|index| old.get(*index)).filter_map(|key| new.iter().position(|candidate| candidate == key)).collect();
            }
            if definition != original && active { actions.definition = Some(definition.clone()); }
            let Some(stamp) = chart_stamp(runtime, &definition) else { ui.colored_label(ui.visuals().warn_fg_color, "Chart data is unavailable or outdated. Calculate or generate the current source."); return; };
            let dataset = {
                let mut cache = self.cache.borrow_mut();
                if cache.as_ref().is_none_or(|cache| cache.stamp != stamp || cache.definition != definition) {
                    match chart_data(runtime, &definition) {
                        Ok((_, dataset)) => *cache = Some(ChartCache { stamp: stamp.clone(), definition: definition.clone(), dataset: Arc::new(dataset), scene: None }),
                        Err(error) => { ui.colored_label(ui.visuals().error_fg_color, error); return; }
                    }
                }
                cache.as_ref().map(|cache| Arc::clone(&cache.dataset)).unwrap()
            };
            ui.horizontal_wrapped(|ui| for (index, series) in dataset.series.iter().enumerate() { let mut visible = series.visible; if ui.checkbox(&mut visible, &series.label).changed() && active { if visible { definition.hidden.retain(|i| *i != index); } else if !definition.hidden.contains(&index) { definition.hidden.push(index); } actions.definition = Some(definition.clone()); } });
            if definition.kind == ChartKind::Bar {
                egui::CollapsingHeader::new("Bar labels").show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(120.0).id_salt("chart-bar-labels").show_rows(ui, 24.0, dataset.bar_labels.len(), |ui, range| {
                        for index in range {
                            let mut label = definition.bar_labels.get(index).cloned().unwrap_or_else(|| dataset.bar_labels[index].clone());
                            ui.horizontal(|ui| { ui.label(format!("{}", index + 1));
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
                if ui.add_enabled(active, egui::Button::new("−")).clicked() { let _ = view.set(view.zoom_x / 1.2, view.zoom_y / 1.2, view.pan_x, view.pan_y); }
                if ui.add_enabled(active, egui::Button::new("+")).clicked() { let _ = view.set(view.zoom_x * 1.2, view.zoom_y * 1.2, view.pan_x, view.pan_y); }
                if ui.add_enabled(active, egui::Button::new("Reset View")).clicked() { view.reset(); }
            });
            let width = ui.available_width().clamp(160.0, 1024.0) as u32;
            let height = 300;
            let scene = {
                let mut cache = self.cache.borrow_mut();
                let cached = cache.as_ref().and_then(|cache| cache.scene.as_ref()).filter(|(old, w, h, _)| *old == view && *w == width && *h == height).map(|(_, _, _, scene)| Arc::clone(scene));
                if let Some(scene) = cached { Ok(scene) } else { dataset.scene(definition.kind, view, width, height).map(|scene| { let scene = Arc::new(scene); if let Some(cache) = cache.as_mut() { cache.scene = Some((view, width, height, Arc::clone(&scene))); } scene }) }
            };
            match scene {
                Ok(scene) => {
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(width as f32, height as f32), egui::Sense::click_and_drag());
                    ui.painter().rect_filled(rect, 0.0, egui::Color32::WHITE);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| { ui.set_clip_rect(rect.intersect(ui.clip_rect())); paint_scene(ui, rect.min, &scene); });
                    if response.dragged() && active { let delta = response.drag_delta(); let _ = view.set(view.zoom_x, view.zoom_y, view.pan_x - f64::from(delta.x) / f64::from(width), view.pan_y + f64::from(delta.y) / f64::from(height)); }
                    if let Some(pointer) = response.hover_pos() && let Some(hit) = scene.hit([(pointer.x - rect.min.x), (pointer.y - rect.min.y)], 8.0) { response.on_hover_text(format!("{} · {}{}", hit.raw_x, hit.raw_y, if hit.approximate { " (approximate)" } else { "" })); }
                    if scene.approximate || scene.coordinate_collisions { ui.label("Plot coordinates are approximate; exact values remain in tooltips."); }
                    if ui.add_enabled(active && actions.definition.is_none(), egui::Button::new("Copy Chart as Image")).clicked() { actions.copy = Some((stamp, definition.clone(), (*scene).clone())); }
                }
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
    match &definition.source { ChartSource::Vectors { .. } => runtime.evaluation.as_ref().map(|value| ChartStamp::Evaluation(value.basis)), ChartSource::Sensitivity { .. } => runtime.table.as_ref().map(|value| ChartStamp::Sensitivity(value.basis.clone())) }
}

pub(super) fn actions_current(actions: &ChartActions, runtime: &ChartRuntime) -> bool {
    actions.identity.is_some() && actions.identity == runtime.identity && actions.pad_host.is_some() && actions.pad_host == runtime.pad_host
        && actions.base == runtime.definition && actions.basis == runtime.basis
        && actions.evaluation == runtime.evaluation.as_ref().map(|value| value.basis)
        && actions.table == runtime.table.as_ref().map(|value| value.basis.clone())
}

pub(super) fn chart_data(runtime: &ChartRuntime, definition: &ChartDefinition) -> Result<(ChartStamp, ChartDataset), String> {
    let (stamp, mut dataset) = match &definition.source {
        ChartSource::Vectors { x, y } => {
            let evaluation = runtime.evaluation.as_ref().ok_or("Current vector results are unavailable.")?;
            let find = |id: &LineId| evaluation.lines.iter().find(|row| row.id == *id).and_then(|row| {
                let values = match &row.outcome {
                    LineOutcome::Value(crate::calculation_pad::CalculationValue::Vector(vector)) => vector.components(),
                    LineOutcome::Value(crate::calculation_pad::CalculationValue::Sequence(sequence)) => sequence.values(),
                    _ => return None,
                };
                Some((row.assignment_name.as_deref().unwrap_or("vector"), values))
            });
            let x = x.as_ref().map(find).transpose_option().ok_or("The X vector is unavailable.")?.map(|(_, values)| values);
            let vectors = y.iter().map(find).collect::<Option<Vec<_>>>().ok_or("A selected Y vector is unavailable.")?;
            (ChartStamp::Evaluation(evaluation.basis), ChartDataset::from_vectors(x, &vectors)?)
        }
        ChartSource::Sensitivity { outputs } => {
            let table = runtime.table.as_ref().ok_or("The current sensitivity table is unavailable.")?;
            let columns = outputs.iter().map(|name| table.basis.definition.outputs.iter().position(|output| output == name)).collect::<Option<Vec<_>>>().ok_or("A selected sensitivity output is unavailable.")?;
            (ChartStamp::Sensitivity(table.basis.clone()), ChartDataset::from_sensitivity(table, &columns)?)
        }
    };
    if !definition.x_label.is_empty() { dataset.x_label = definition.x_label.clone(); }
    if !definition.y_label.is_empty() { dataset.y_label = definition.y_label.clone(); }
    if !definition.bar_labels.is_empty() { dataset.bar_labels = definition.bar_labels.clone(); }
    for (index, series) in dataset.series.iter_mut().enumerate() { series.visible = !definition.hidden.contains(&index); }
    Ok((stamp, dataset))
}

trait TransposeOption<T> { fn transpose_option(self) -> Option<Option<T>>; }
impl<T> TransposeOption<T> for Option<Option<T>> { fn transpose_option(self) -> Option<Option<T>> { match self { Some(value) => Some(Some(value?)), None => Some(None) } } }

fn series_keys(source: &ChartSource) -> Vec<String> {
    match source { ChartSource::Vectors { y, .. } => y.iter().map(|id| format!("line:{}", id.get())).collect(), ChartSource::Sensitivity { outputs } => outputs.iter().map(|name| format!("output:{name}")).collect() }
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
    fn chart_adapter_reports_length_errors_and_uses_only_current_snapshots() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("[1, 2]\n[3]").unwrap();
        session.evaluation();
        let evaluation = session.shared_evaluation().unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        let definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: ids }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: Some(evaluation), table: None, definition: Some(definition.clone()), basis: session.evaluation_basis(), pad_host: None, pinned: false };
        assert!(chart_data(&runtime, &definition).unwrap_err().contains("matching lengths"));
        session.replace_source("[1, 2]").unwrap();
        runtime.evaluation = session.shared_evaluation();
        assert!(chart_stamp(&runtime, &definition).is_none());
    }

    #[test]
    fn chart_copy_uses_scene_and_surfaces_clipboard_failure() {
        let dataset = ChartDataset::from_vectors(None, &[("series", &[1.0, 2.0])]).unwrap();
        let scene = dataset.scene(ChartKind::Line, ChartView::default(), 400, 300).unwrap();
        let mut dialog = CalculationChartDialog::default();
        let mut called = false;
        dialog.copy_with(&scene, |image| { called = true; assert_eq!((image.width, image.height), (400, 300)); Err(()) });
        assert!(called && dialog.feedback.as_deref().unwrap().contains("Could not copy"));
    }
}
