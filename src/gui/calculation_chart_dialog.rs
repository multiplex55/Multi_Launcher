//! Read-only chart companion; actions are committed by its retained utility host.
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::UtilityPresentation;
use crate::calculation_pad::{ChartDataset, ChartDefinition, ChartKind, ChartScene, ChartSource, ChartVectorReference, ChartView, ChartReferenceValue, ChartReferenceAxis, ChartReferenceLine, ChartSeriesKey, ChartSeriesStyle, ChartLineStyle, ChartMarkerStyle, ChartFiniteValue, ChartAxisBounds, ChartAxisOptions, ChartAxisScale, ChartAxisTicks, ChartOptions, ChartLegendPlacement, DistributionBins, DistributionData, DistributionDefinition, DistributionScale, ResolvedReferenceLine, DocumentId, DocumentIncarnation, EvaluationBasis, LineOutcome, ScenePrimitive, SensitivityBasis, SensitivityTable, SessionEvaluation};
use eframe::egui;
use std::sync::Arc;

fn chart_target(scope: Option<&super::jump_mode::ui::JumpModeScope>, ui: &egui::Ui, response: &egui::Response, id: &str, label: &str, kind: super::jump_mode::TargetKind, order: &mut u64) -> Option<super::jump_mode::ui::FocusDirective> {
    super::jump_mode::ui::register_and_apply_target(scope, ui, response, id, label, kind, order)
}

fn vector_key(reference: &ChartVectorReference) -> String {
    match reference {
        ChartVectorReference::Physical(id) => format!("physical/{}", id.get()),
        ChartVectorReference::Named { name } => format!("named/{:016x}", egui::Id::new(name).value()),
    }
}

fn popup_target(scope: Option<&super::jump_mode::ui::JumpModeScope>, ui: &egui::Ui, response: &egui::Response, id: &str, label: &str, kind: super::jump_mode::TargetKind, order: &mut u64) {
    chart_target(scope, ui, response, id, label, kind, order);
    if response.gained_focus() && response.enabled() && ui.is_visible() && !ui.is_sizing_pass()
        && !ui.ctx().will_discard() && !ui.clip_rect().contains_rect(response.rect) {
        response.scroll_to_me(None);
    }
}

fn distribution_summary(data: &DistributionData, kind: ChartKind, index: usize) -> Option<String> {
    if kind == ChartKind::Histogram {
        let bin = data.bins.get(index)?;
        Some(format!("{} · count {} · {:.4}% · cumulative {}", bin.bounds.membership_label(), bin.count, bin.percentage(data.total), bin.cumulative_count))
    } else {
        let step = data.ecdf.get(index)?;
        Some(format!("≤ {:.17e} · count {} · cumulative {} / {} ({:.4}%)", step.threshold, step.count, step.cumulative_count, data.total, step.cumulative_rate(data.total) * 100.0))
    }
}

fn axis_controls(ui: &mut egui::Ui, scope: Option<&super::jump_mode::ui::JumpModeScope>, axis: &mut AxisDraft, name: &str, order: &mut u64, actions: &mut ChartActions) {
    #[cfg(not(test))] let _ = &actions;
    use super::jump_mode::TargetKind;
    ui.group(|ui| {
        ui.label(format!("{name} axis"));
        ui.horizontal_wrapped(|ui| {
            for (scale, label) in [(ChartAxisScale::Linear, "Linear"), (ChartAxisScale::Log10, "Log10")] {
                let response = ui.selectable_value(&mut axis.scale, scale, label);
                if response.changed() { axis.ticks = TickMode::Auto; }
                chart_target(scope, ui, &response, &format!("options/{name}/scale/{label}"), &format!("{name} {label}"), TargetKind::Selectable, order);
                #[cfg(test)] actions.controls.push((format!("options/{name}/scale/{label}"), response.rect));
            }
            for (fixed, label) in [(false, "Auto"), (true, "Fixed")] {
                let response = ui.selectable_value(&mut axis.fixed, fixed, label);
                chart_target(scope, ui, &response, &format!("options/{name}/bounds/{label}"), &format!("{name} bounds {label}"), TargetKind::Selectable, order);
                #[cfg(test)] actions.controls.push((format!("options/{name}/bounds/{label}"), response.rect));
            }
        });
        if axis.fixed {
            ui.horizontal(|ui| for (field, label) in [(&mut axis.min, "Min"), (&mut axis.max, "Max")] {
                ui.label(label);
                let response = ui.add(egui::TextEdit::singleline(field).char_limit(40).desired_width(90.0));
                chart_target(scope, ui, &response, &format!("options/{name}/{label}"), &format!("{name} {label}"), TargetKind::TextEntry, order);
                #[cfg(test)] actions.controls.push((format!("options/{name}/{label}"), response.rect));
            });
        }
        ui.horizontal_wrapped(|ui| {
            let modes = if axis.scale == ChartAxisScale::Linear { &[(TickMode::Auto, "Auto ticks"), (TickMode::Step, "Step"), (TickMode::Density, "Density")][..] } else { &[(TickMode::Auto, "Auto ticks"), (TickMode::Density, "Density"), (TickMode::Decades, "Decades"), (TickMode::Subdivisions, "Subdivisions")][..] };
            for &(mode, label) in modes {
                let response = ui.selectable_value(&mut axis.ticks, mode, label);
                chart_target(scope, ui, &response, &format!("options/{name}/ticks/{label}"), &format!("{name} {label}"), TargetKind::Selectable, order);
                #[cfg(test)] actions.controls.push((format!("options/{name}/ticks/{label}"), response.rect));
            }
        });
        let field = match axis.ticks { TickMode::Step => Some((&mut axis.step, "Tick step")), TickMode::Density | TickMode::Subdivisions => Some((&mut axis.count, "Tick count")), _ => None };
        if let Some((text, label)) = field {
            let response = ui.add(egui::TextEdit::singleline(text).char_limit(40).desired_width(90.0));
            chart_target(scope, ui, &response, &format!("options/{name}/tick-value"), &format!("{name} {label}"), TargetKind::TextEntry, order);
            #[cfg(test)] actions.controls.push((format!("options/{name}/tick-value"), response.rect));
        }
    });
}

fn reference_controls(ui: &mut egui::Ui, scope: Option<&super::jump_mode::ui::JumpModeScope>, draft: &mut ChartOptionsDraft, runtime: &ChartRuntime, order: &mut u64, actions: &mut ChartActions, active: bool) {
    #[cfg(not(test))] let _ = &actions;
    use super::jump_mode::TargetKind;
    let choices = scalar_choices(runtime);
    ui.label("Reference lines");
    let mut remove = None;
    for reference in &mut draft.references {
        ui.push_id(("chart-reference", reference.id), |ui| ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                for (axis, label) in [(ChartReferenceAxis::X, "Vertical X"), (ChartReferenceAxis::Y, "Horizontal Y")] {
                    let response = ui.selectable_value(&mut reference.axis, axis, label);
                    chart_target(scope, ui, &response, &format!("options/reference/{}/axis/{label}", reference.id), label, TargetKind::Selectable, order);
                }
                for (named, label) in [(false, "Literal"), (true, "Named scalar")] {
                    let response = ui.selectable_value(&mut reference.named, named, label);
                    chart_target(scope, ui, &response, &format!("options/reference/{}/kind/{label}", reference.id), label, TargetKind::Selectable, order);
                    #[cfg(test)] actions.controls.push((format!("reference/{}/kind/{label}", reference.id), response.rect));
                    if response.changed() { reference.value = if named { choices.first().map_or_else(String::new, |choice| choice.0.clone()) } else { "0".into() }; }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Label");
                let response = ui.add(egui::TextEdit::singleline(&mut reference.label).char_limit(128).desired_width(120.0));
                chart_target(scope, ui, &response, &format!("options/reference/{}/label", reference.id), "Reference label", TargetKind::TextEntry, order);
                #[cfg(test)] actions.controls.push((format!("reference/{}/label", reference.id), response.rect));
                let response = ui.button("Remove"); chart_target(scope, ui, &response, &format!("options/reference/{}/remove", reference.id), "Remove reference", TargetKind::Button, order);
                #[cfg(test)] actions.controls.push((format!("reference/{}/remove", reference.id), response.rect));
                if active && response.clicked() { remove = Some(reference.id); }
            });
            if reference.named {
                ui.label("Choose a current scalar:");
                let search = ui.add(egui::TextEdit::singleline(&mut reference.search).hint_text("Search scalar names").char_limit(128));
                chart_target(scope, ui, &search, &format!("options/reference/{}/search", reference.id), "Search scalar names", TargetKind::TextEntry, order);
                #[cfg(test)] actions.controls.push((format!("reference/{}/search", reference.id), search.rect));
                let query = reference.search.to_lowercase();
                let filtered: Vec<_> = choices.iter().filter(|(name, _)| name.to_lowercase().contains(&query)).collect();
                egui::ScrollArea::vertical().id_salt(("reference-scalars", reference.id)).max_height(100.0).show_rows(ui, 20.0, filtered.len(), |ui, visible| {
                    for index in visible {
                        let (name, value) = filtered[index];
                        let token = egui::Id::new(name).value();
                        let response = ui.push_id(("scalar", token), |ui| ui.add_sized([ui.available_width(), 20.0], egui::Button::new(format!("{name} = {value}")).selected(reference.value == *name).truncate())).inner;
                        chart_target(scope, ui, &response, &format!("options/reference/{}/scalar/{token:x}", reference.id), &format!("Reference scalar {name}"), TargetKind::Selectable, order);
                        #[cfg(test)] actions.controls.push((format!("reference/{}/scalar/{name}", reference.id), response.rect));
                        if active && response.clicked() { reference.value.clone_from(name); }
                    }
                });
                if let Some((_, value)) = choices.iter().find(|(name, _)| name == &reference.value) { ui.label(format!("Current value: {value}")); }
                else { ui.colored_label(ui.visuals().warn_fg_color, "Named scalar is unavailable, invalid, or outdated."); }
            } else {
                let response = ui.add(egui::TextEdit::singleline(&mut reference.value).char_limit(40).desired_width(120.0));
                chart_target(scope, ui, &response, &format!("options/reference/{}/literal", reference.id), "Reference value", TargetKind::TextEntry, order);
                #[cfg(test)] actions.controls.push((format!("reference/{}/literal", reference.id), response.rect));
            }
        }));
    }
    if let Some(id) = remove { draft.references.retain(|reference| reference.id != id); }
    let response = ui.add_enabled(draft.references.len() < 16, egui::Button::new("Add reference"));
    chart_target(scope, ui, &response, "options/reference/add", "Add reference", TargetKind::Button, order);
    #[cfg(test)] actions.controls.push(("reference/add".into(), response.rect));
    if active && response.clicked() {
        match draft.options.allocate_reference_id() {
            Ok(id) => draft.references.push(ReferenceDraft { id, axis: ChartReferenceAxis::Y, named: false, value: "0".into(), label: String::new(), search: String::new() }),
            Err(error) => draft.error = Some(error.into()),
        }
    }
}

fn style_controls(ui: &mut egui::Ui, scope: Option<&super::jump_mode::ui::JumpModeScope>, draft: &mut ChartOptionsDraft, definition: &ChartDefinition, order: &mut u64, actions: &mut ChartActions, active: bool) {
    #[cfg(not(test))] let _ = &actions;
    use super::jump_mode::TargetKind;
    ui.label("Series appearance");
    for (index, (key, label)) in style_sources(definition).into_iter().enumerate() {
        let token = egui::Id::new(format!("{key:?}")).value();
        let distribution_style = matches!(key, ChartSeriesKey::Distribution(_));
        let distribution_active = matches!(definition.kind, ChartKind::Histogram | ChartKind::Cumulative);
        let active_role = distribution_style == distribution_active;
        let line_enabled = active_role && matches!(definition.kind, ChartKind::Line | ChartKind::Cumulative);
        let marker_enabled = active_role && matches!(definition.kind, ChartKind::Line | ChartKind::Scatter | ChartKind::Cumulative);
        let existing = draft.options.styles.iter().position(|style| style.key == key);
        let mut enabled = existing.is_some();
        let response = ui.checkbox(&mut enabled, format!("Customize {label}"));
        chart_target(scope, ui, &response, &format!("options/style/{token:x}/enabled"), &format!("Customize {label}"), TargetKind::Checkbox, order);
        #[cfg(test)] actions.controls.push((format!("style/{index}/enabled"), response.rect));
        if active && response.changed() {
            if let Some(index) = existing { draft.options.styles.remove(index); }
            else { draft.options.styles.push(ChartSeriesStyle { key: key.clone(), color: crate::calculation_pad::default_series_color(if distribution_style { 0 } else { index }), line: ChartLineStyle::Solid, marker: ChartMarkerStyle::Auto, thickness: ChartFiniteValue::try_from(2.0).unwrap() }); }
        }
        let Some(style) = draft.options.styles.iter_mut().find(|style| style.key == key) else { continue; };
        ui.indent(("chart-style", token), |ui| {
            if !active_role { ui.label("This series is dormant in the current chart mode."); }
            else if matches!(definition.kind, ChartKind::Bar | ChartKind::Histogram) { ui.label("Filled bars use the selected color."); }
            else if definition.kind == ChartKind::Scatter { ui.label("Scatter uses color and marker."); }
            ui.horizontal_wrapped(|ui| for (channel, name) in ["R", "G", "B", "Alpha"].into_iter().enumerate() {
                ui.label(name); let response = ui.add(egui::DragValue::new(&mut style.color[channel]).range(0..=255));
                chart_target(scope, ui, &response, &format!("options/style/{token:x}/color/{name}"), &format!("{label} {name}"), TargetKind::Slider, order);
                #[cfg(test)] actions.controls.push((format!("style/{index}/color/{name}"), response.rect));
            });
            ui.add_enabled_ui(line_enabled, |ui| ui.horizontal_wrapped(|ui| for (line, name) in [(ChartLineStyle::Solid, "Solid"), (ChartLineStyle::Dashed, "Dashed"), (ChartLineStyle::Dotted, "Dotted")] {
                let response = ui.selectable_value(&mut style.line, line, name); chart_target(scope, ui, &response, &format!("options/style/{token:x}/line/{name}"), &format!("{label} {name}"), TargetKind::Selectable, order);
                #[cfg(test)] actions.controls.push((format!("style/{index}/line/{name}"), response.rect));
            }));
            ui.add_enabled_ui(marker_enabled, |ui| ui.horizontal_wrapped(|ui| for (marker, name) in [(ChartMarkerStyle::Auto, "Auto"), (ChartMarkerStyle::None, "None"), (ChartMarkerStyle::Circle, "Circle"), (ChartMarkerStyle::Square, "Square"), (ChartMarkerStyle::Cross, "Cross")] {
                let response = ui.selectable_value(&mut style.marker, marker, name); chart_target(scope, ui, &response, &format!("options/style/{token:x}/marker/{name}"), &format!("{label} marker {name}"), TargetKind::Selectable, order);
                #[cfg(test)] actions.controls.push((format!("style/{index}/marker/{name}"), response.rect));
            }));
            let mut width = style.thickness.get();
            ui.add_enabled_ui(line_enabled || (marker_enabled && definition.kind == ChartKind::Scatter && style.marker == ChartMarkerStyle::Cross), |ui| ui.horizontal(|ui| { ui.label("Thickness"); let response = ui.add(egui::DragValue::new(&mut width).range(0.5..=8.0).speed(0.1)); chart_target(scope, ui, &response, &format!("options/style/{token:x}/width"), &format!("{label} thickness"), TargetKind::Slider, order); #[cfg(test)] actions.controls.push((format!("style/{index}/width"), response.rect)); }));
            if let Ok(value) = ChartFiniteValue::try_from(width) { style.thickness = value; }
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BinMode { Auto, Count, Width }

#[derive(Clone, PartialEq, Eq)]
struct DistributionDraft { basis: DistributionDefinition, mode: BinMode, count: String, width: String, error: Option<String> }
impl DistributionDraft {
    fn new(basis: &DistributionDefinition) -> Self {
        let (mode, count, width) = match basis.bins {
            DistributionBins::Auto => (BinMode::Auto, "10".into(), "1".into()),
            DistributionBins::Count { count } => (BinMode::Count, count.to_string(), "1".into()),
            DistributionBins::Width { width } => (BinMode::Width, "10".into(), width.get().to_string()),
        };
        Self { basis: basis.clone(), mode, count, width, error: None }
    }
    fn bins(&self) -> Result<DistributionBins, &'static str> {
        match self.mode {
            BinMode::Auto => Ok(DistributionBins::Auto),
            BinMode::Count => self.count.trim().parse::<u16>().ok().filter(|count| (1..=1024).contains(count)).map(|count| DistributionBins::Count { count }).ok_or("Bin count must be between 1 and 1024."),
            BinMode::Width => self.width.trim().parse::<f64>().ok().filter(|value| value.is_finite() && *value > 0.0).and_then(|value| ChartFiniteValue::try_from(value).ok()).map(|width| DistributionBins::Width { width }).ok_or("Bin width must be finite and positive."),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TickMode { Auto, Step, Density, Decades, Subdivisions }

#[derive(Clone, PartialEq, Eq)]
struct AxisDraft { scale: ChartAxisScale, fixed: bool, min: String, max: String, ticks: TickMode, step: String, count: String }
impl AxisDraft {
    fn new(axis: &ChartAxisOptions) -> Self {
        let (fixed, min, max) = match axis.bounds { ChartAxisBounds::Auto => (false, "0".into(), "1".into()), ChartAxisBounds::Fixed { min, max } => (true, min.get().to_string(), max.get().to_string()) };
        let (ticks, step, count) = match axis.ticks {
            ChartAxisTicks::Auto => (TickMode::Auto, "1".into(), "5".into()),
            ChartAxisTicks::LinearStep { step } => (TickMode::Step, step.get().to_string(), "5".into()),
            ChartAxisTicks::Density { count } => (TickMode::Density, "1".into(), count.to_string()),
            ChartAxisTicks::LogDecades => (TickMode::Decades, "1".into(), "5".into()),
            ChartAxisTicks::LogSubdivisions { count } => (TickMode::Subdivisions, "1".into(), count.to_string()),
        };
        Self { scale: axis.scale, fixed, min, max, ticks, step, count }
    }
    fn axis(&self) -> Result<ChartAxisOptions, String> {
        let number = |text: &str| text.trim().parse::<f64>().ok().and_then(|value| ChartFiniteValue::try_from(value).ok()).ok_or_else(|| "Enter a finite axis value.".to_owned());
        let bounds = if self.fixed { ChartAxisBounds::Fixed { min: number(&self.min)?, max: number(&self.max)? } } else { ChartAxisBounds::Auto };
        let ticks = match self.ticks {
            TickMode::Auto => ChartAxisTicks::Auto,
            TickMode::Step => ChartAxisTicks::LinearStep { step: number(&self.step)? },
            TickMode::Density => ChartAxisTicks::Density { count: self.count.trim().parse().map_err(|_| "Enter a valid tick density.")? },
            TickMode::Decades => ChartAxisTicks::LogDecades,
            TickMode::Subdivisions => ChartAxisTicks::LogSubdivisions { count: self.count.trim().parse().map_err(|_| "Enter valid log subdivisions.")? },
        };
        Ok(ChartAxisOptions { scale: self.scale, bounds, ticks })
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ReferenceDraft { id: u64, axis: ChartReferenceAxis, named: bool, value: String, label: String, search: String }

#[derive(Clone, PartialEq, Eq)]
struct ChartOptionsDraft { basis: ChartOptions, options: ChartOptions, x: AxisDraft, y: AxisDraft, references: Vec<ReferenceDraft>, error: Option<String> }
impl ChartOptionsDraft {
    fn new(definition: &ChartDefinition) -> Self {
        let references = definition.options.references.iter().map(|reference| {
            let (named, value) = match &reference.value { ChartReferenceValue::Literal(value) => (false, value.get().to_string()), ChartReferenceValue::Named(name) => (true, name.clone()) };
            ReferenceDraft { id: reference.id, axis: reference.axis, named, value, label: reference.label.clone(), search: String::new() }
        }).collect();
        Self { basis: definition.options.clone(), options: definition.options.clone(), x: AxisDraft::new(&definition.options.x), y: AxisDraft::new(&definition.options.y), references, error: None }
    }
    fn options(&self) -> Result<ChartOptions, String> {
        let mut options = self.options.clone(); options.x = self.x.axis()?; options.y = self.y.axis()?;
        options.x.validate().map_err(str::to_owned)?; options.y.validate().map_err(str::to_owned)?;
        options.references = self.references.iter().map(|reference| {
            let value = if reference.named { ChartReferenceValue::Named(reference.value.clone()) } else {
                let number = reference.value.trim().parse::<f64>().ok().and_then(|value| ChartFiniteValue::try_from(value).ok()).ok_or("Enter a finite reference value.")?;
                ChartReferenceValue::Literal(number)
            };
            Ok(ChartReferenceLine { id: reference.id, axis: reference.axis, value, label: reference.label.clone() })
        }).collect::<Result<Vec<_>, String>>()?;
        Ok(options)
    }
}

fn rebase_options_draft(draft: &ChartOptionsDraft, definition: &ChartDefinition) -> Option<ChartOptionsDraft> {
    let distribution = definition.distribution.as_ref().map(|distribution| &distribution.vector);
    let mut expected = draft.basis.clone();
    expected.retain_selected_styles(&definition.source, distribution);
    if expected != definition.options { return None; }
    let mut rebased = draft.clone();
    rebased.basis = definition.options.clone();
    rebased.options.retain_selected_styles(&definition.source, distribution);
    Some(rebased)
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
    distribution_draft: Option<DistributionDraft>,
    options_draft: Option<ChartOptionsDraft>,
    summary_row: Option<usize>,
    summary_focus: Option<usize>,
    summary_stamp: Option<(ChartStamp, ChartKind, DistributionDefinition)>,
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
    draft: Option<DistributionDraft>,
    options_draft: Option<Option<ChartOptionsDraft>>,
    summary_row: Option<usize>,
    summary_focus: Option<Option<usize>>,
    summary_stamp: Option<(ChartStamp, ChartKind, DistributionDefinition)>,
    #[cfg(test)]
    pub controls: Vec<(String, egui::Rect)>,
}

impl CalculationChartDialog {
    pub(super) fn jump_generation(&self, runtime: &ChartRuntime) -> super::jump_mode::TargetGeneration {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.epoch.hash(&mut hash);
        runtime.identity.hash(&mut hash);
        runtime.pad_host.map(|host| (std::mem::discriminant(&host.host), host.generation)).hash(&mut hash);
        for basis in [runtime.basis, runtime.evaluation.as_ref().map(|evaluation| evaluation.basis)] {
            basis.map(|basis| (basis.document, basis.incarnation, basis.revision, basis.generation)).hash(&mut hash);
        }
        if let Some(table) = &runtime.table {
            table.basis.document.hash(&mut hash);
            table.basis.incarnation.hash(&mut hash);
            table.basis.revision.hash(&mut hash);
            table.basis.generation.hash(&mut hash);
            table.basis.definition.outputs.hash(&mut hash);
        }
        if let Some(definition) = &runtime.definition {
            std::mem::discriminant(&definition.kind).hash(&mut hash);
            match &definition.source {
                ChartSource::Vectors { x, y } => {
                    0u8.hash(&mut hash);
                    x.as_ref().map(vector_key).hash(&mut hash);
                    for reference in y { vector_key(reference).hash(&mut hash); }
                }
                ChartSource::Sensitivity { outputs } => { 1u8.hash(&mut hash); outputs.hash(&mut hash); }
            }
            definition.distribution.as_ref().map(|distribution| vector_key(&distribution.vector)).hash(&mut hash);
        }
        super::jump_mode::TargetGeneration(hash.finish())
    }
    pub(super) fn open(&mut self) { if !self.open { self.epoch = self.epoch.wrapping_add(1); self.open = true; self.close_requested = false; self.view.reset(); } }
    pub(super) fn close(&mut self) { self.open = false; self.close_requested = false; self.view.reset(); self.bound = None; self.popup_anchors.clear(); self.popup_viewport = None; self.distribution_draft = None; self.options_draft = None; self.summary_row = None; self.summary_focus = None; self.summary_stamp = None; *self.cache.borrow_mut() = None; }
    pub(super) fn request_close(&mut self) { self.close_requested = true; }
    pub(super) fn epoch(&self) -> u64 { self.epoch }
    pub(super) fn commit(&mut self, actions: &ChartActions) {
        if self.bound != actions.identity { self.view.reset(); self.bound = actions.identity; self.distribution_draft = None; self.options_draft = None; self.summary_row = None; self.summary_focus = None; self.summary_stamp = None; *self.cache.borrow_mut() = None; }
        if let Some(stamp) = &actions.summary_stamp && self.summary_stamp.as_ref() != Some(stamp) { self.summary_row = None; self.summary_focus = None; self.summary_stamp = Some(stamp.clone()); }
        if let Some(view) = actions.view { self.view = view; }
        if let Some(draft) = &actions.draft { self.distribution_draft = Some(draft.clone()); }
        if let Some(draft) = &actions.options_draft { self.options_draft = draft.clone(); }
        if let Some(row) = actions.summary_row { self.summary_row = Some(row); }
        if let Some(focus) = actions.summary_focus { self.summary_focus = focus; }
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
        if !owner { self.summary_focus = None; }
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
            let distribution_vectors = runtime.evaluation.as_ref().filter(|evaluation| Some(evaluation.basis) == runtime.basis)
                .map(|evaluation| distribution_vector_choices(evaluation)).unwrap_or_default();
            let mut definition = runtime.definition.clone().unwrap_or_else(|| ChartDefinition { source: ChartSource::Vectors { x: None, y: vectors.first().map(|row| vec![row.0.clone()]).unwrap_or_default() }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: Vec::new(), bar_labels: Vec::new(), distribution: None, options: Default::default() });
            let original = definition.clone();
            let mut pending_bin_draft = false;
            let rebased_options = if self.bound == runtime.identity { self.options_draft.as_ref().and_then(|draft| rebase_options_draft(draft, &original)) } else { None };
            let mut pending_options = rebased_options.as_ref().is_some_and(|draft| draft.options().map_or(true, |options| options != original.options));
            ui.add_enabled_ui(owner, |ui| {
                ui.horizontal(|ui| {
                    for (kind, name) in [(ChartKind::Line, "Line"), (ChartKind::Bar, "Bar"), (ChartKind::Scatter, "Scatter"), (ChartKind::Histogram, "Histogram"), (ChartKind::Cumulative, "Cumulative")] {
                        let distribution_kind = matches!(kind, ChartKind::Histogram | ChartKind::Cumulative);
                        let available = !distribution_kind || definition.distribution.is_some() || !distribution_vectors.is_empty();
                        let response = ui.add_enabled(available, egui::Button::new(name).selected(definition.kind == kind));
                        chart_target(scope, ui, &response, &format!("kind/{name}"), name, TargetKind::Selectable, &mut order);
                        #[cfg(test)] actions.controls.push((format!("kind/{name}"), response.rect));
                        if response.clicked() && active {
                            if distribution_kind && definition.distribution.is_none()
                                && let Some((vector, _)) = distribution_vectors.first() {
                                definition.distribution = Some(DistributionDefinition { vector: vector.clone(), bins: DistributionBins::Auto, scale: DistributionScale::Count });
                            }
                            if !distribution_kind || definition.distribution.is_some() { definition.kind = kind; }
                        }
                    }
                });
                if matches!(definition.kind, ChartKind::Histogram | ChartKind::Cumulative) {
                    if let Some(distribution) = definition.distribution.as_mut() {
                        let selected = distribution_vectors.iter().find(|(vector, _)| vector == &distribution.vector).map(|(_, label)| label.clone()).unwrap_or_else(|| format!("{} (unavailable)", reference_label(&distribution.vector)));
                        let popup_scope = scope.map(|scope| scope.combo_popup_scope("distribution/vector"));
                        let response = egui::ComboBox::from_id_salt("chart-distribution-vector").selected_text(selected).show_ui(ui, |ui| {
                            let mut popup_order = 0;
                            for (reference, label) in &distribution_vectors {
                                let key = vector_key(reference);
                                let response = ui.push_id(&key, |ui| ui.selectable_value(&mut distribution.vector, reference.clone(), label)).inner;
                                popup_target(popup_scope.as_ref(), ui, &response, &key, label, TargetKind::Selectable, &mut popup_order);
                                #[cfg(test)] actions.controls.push((format!("distribution/{label}"), response.rect));
                            }
                        });
                        chart_target(scope, ui, &response.response, "distribution/vector", "Distribution vector", TargetKind::ComboBox, &mut order);
                        #[cfg(test)] actions.controls.push(("distribution-menu".into(), response.response.rect));
                        actions.popup_anchors.push((response.response.id, response.response.layer_id));
                        if definition.kind == ChartKind::Histogram {
                            let mut draft = self.distribution_draft.as_ref().filter(|draft| self.bound == runtime.identity && draft.basis.vector == distribution.vector && draft.basis.bins == distribution.bins).cloned().unwrap_or_else(|| DistributionDraft::new(distribution));
                            ui.horizontal(|ui| {
                                for (mode, name) in [(BinMode::Auto, "Auto bins"), (BinMode::Count, "Bin count"), (BinMode::Width, "Bin width")] {
                                    let response = ui.selectable_value(&mut draft.mode, mode, name);
                                    if response.changed() { draft.error = None; }
                                    chart_target(scope, ui, &response, &format!("distribution/bins/{name}"), name, TargetKind::Selectable, &mut order);
                                    #[cfg(test)] actions.controls.push((format!("bins/{name}"), response.rect));
                                }
                            });
                            ui.horizontal(|ui| {
                                let field = match draft.mode { BinMode::Count => Some((&mut draft.count, "Count value")), BinMode::Width => Some((&mut draft.width, "Width value")), BinMode::Auto => None };
                                if let Some((text, label)) = field {
                                    let response = ui.add(egui::TextEdit::singleline(text).char_limit(40).desired_width(100.0));
                                    if response.changed() { draft.error = None; }
                                    chart_target(scope, ui, &response, "distribution/bin-value", label, TargetKind::TextEntry, &mut order);
                                    #[cfg(test)] actions.controls.push(("bin-value".into(), response.rect));
                                }
                                let response = ui.button("Apply bins");
                                chart_target(scope, ui, &response, "distribution/apply-bins", "Apply bins", TargetKind::Button, &mut order);
                                #[cfg(test)] actions.controls.push(("apply-bins".into(), response.rect));
                                if active && response.clicked() {
                                    match draft.bins().map_err(str::to_owned).and_then(|bins| validate_histogram_bins(runtime, distribution, bins).map(|()| bins)) {
                                        Ok(bins) => { distribution.bins = bins; draft.error = None; }
                                        Err(error) => draft.error = Some(error),
                                    }
                                }
                            });
                            if let Err(error) = draft.bins() { ui.colored_label(ui.visuals().error_fg_color, error); }
                            else if let Some(error) = &draft.error { ui.colored_label(ui.visuals().error_fg_color, error); }
                            pending_bin_draft = draft.bins() != Ok(distribution.bins);
                            if active && self.distribution_draft.as_ref() != Some(&draft) { actions.draft = Some(draft); }
                            ui.horizontal(|ui| for (scale, label) in [(DistributionScale::Count, "Count"), (DistributionScale::Percentage, "Percentage")] {
                                let response = ui.selectable_value(&mut distribution.scale, scale, label);
                                chart_target(scope, ui, &response, &format!("distribution/scale/{label}"), label, TargetKind::Selectable, &mut order);
                                #[cfg(test)] actions.controls.push((format!("scale/{label}"), response.rect));
                            });
                        }
                    }
                } else {
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
                        let popup_scope = scope.map(|scope| scope.combo_popup_scope("source/x"));
                        let response = egui::ComboBox::from_id_salt("chart-x").selected_text(selected).show_ui(ui, |ui| {
                            let mut popup_order = 0;
                            let indexes = ui.selectable_value(x, None, "Indexes 0…N−1");
                            popup_target(popup_scope.as_ref(), ui, &indexes, "indexes", "Indexes 0…N−1", TargetKind::Selectable, &mut popup_order);
                            for (reference, label) in &vectors {
                                let key = vector_key(reference);
                                let response = ui.push_id(&key, |ui| ui.selectable_value(x, Some(reference.clone()), label)).inner;
                                popup_target(popup_scope.as_ref(), ui, &response, &key, label, TargetKind::Selectable, &mut popup_order);
                                #[cfg(test)] actions.controls.push((format!("x/{label}"), response.rect));
                            }
                        });
                        chart_target(scope, ui, &response.response, "source/x", "X vector", TargetKind::ComboBox, &mut order);
                        #[cfg(test)] actions.controls.push(("x-menu".into(), response.response.rect));
                        actions.popup_anchors.push((response.response.id, response.response.layer_id));
                        let popup_scope = scope.map(|scope| scope.combo_popup_scope("source/y"));
                        let response = ui.menu_button("Y vectors", |ui| {
                            let mut popup_order = 0;
                            let _scroll = egui::ScrollArea::vertical().id_salt("chart-y-sources").max_height(240.0).show(ui, |ui| {
                                for (reference, label) in &vectors {
                                    let mut selected = y.contains(reference);
                                    let key = vector_key(reference);
                                    let response = ui.push_id(&key, |ui| ui.checkbox(&mut selected, label)).inner;
                                    popup_target(popup_scope.as_ref(), ui, &response, &key, label, TargetKind::Checkbox, &mut popup_order);
                                    #[cfg(test)] actions.controls.push((format!("y/{label}"), response.rect));
                                    if response.changed() { if selected && y.len() < 8 { y.push(reference.clone()); } else if !selected { y.retain(|value| value != reference); } }
                                }
                                for reference in y.clone().into_iter().filter(|reference| !vectors.iter().any(|row| &row.0 == reference)) {
                                    let mut selected = true;
                                    let key = vector_key(&reference);
                                    let label = format!("{} (unavailable)", reference_label(&reference));
                                    let response = ui.push_id(&key, |ui| ui.checkbox(&mut selected, &label)).inner;
                                    popup_target(popup_scope.as_ref(), ui, &response, &key, &label, TargetKind::Checkbox, &mut popup_order);
                                    if response.changed() && !selected { y.retain(|value| value != &reference); }
                                }
                            });
                            #[cfg(test)] actions.controls.push(("y-scroll".into(), _scroll.inner_rect));
                        });
                        super::jump_mode::ui::register_and_apply_menu_anchor(scope, ui, &response.response, "source/y", "Y vectors", super::jump_mode::ui::MenuAnchorKind::Root, &mut order);
                        #[cfg(test)] actions.controls.push(("y-menu".into(), response.response.rect));
                        actions.popup_anchors.push((response.response.id, response.response.layer_id));
                    }
                    ChartSource::Sensitivity { outputs } => if let Some(table) = &runtime.table {
                        let popup_scope = scope.map(|scope| scope.combo_popup_scope("source/outputs"));
                        let response = ui.menu_button("Y outputs", |ui| {
                            let mut popup_order = 0;
                            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| for name in &table.basis.definition.outputs {
                                let mut selected = outputs.contains(name);
                                let key = format!("output/{:016x}", egui::Id::new(name).value());
                                let response = ui.push_id(&key, |ui| ui.checkbox(&mut selected, name)).inner;
                                popup_target(popup_scope.as_ref(), ui, &response, &key, name, TargetKind::Checkbox, &mut popup_order);
                                #[cfg(test)] actions.controls.push((format!("output/{name}"), response.rect));
                                if response.changed() { if selected && outputs.len() < 8 { outputs.push(name.clone()); } else if !selected { outputs.retain(|value| value != name); } }
                            });
                        });
                        super::jump_mode::ui::register_and_apply_menu_anchor(scope, ui, &response.response, "source/outputs", "Y outputs", super::jump_mode::ui::MenuAnchorKind::Root, &mut order);
                        #[cfg(test)] actions.controls.push(("output-menu".into(), response.response.rect));
                        actions.popup_anchors.push((response.response.id, response.response.layer_id));
                    },
                }
                }
                ui.horizontal(|ui| { ui.label("X label"); let response = ui.add(egui::TextEdit::singleline(&mut definition.x_label).char_limit(129).desired_width(100.0)); chart_target(scope, ui, &response, "label/x", "X label", TargetKind::TextEntry, &mut order); ui.label("Y label"); let response = ui.add(egui::TextEdit::singleline(&mut definition.y_label).char_limit(129).desired_width(100.0)); chart_target(scope, ui, &response, "label/y", "Y label", TargetKind::TextEntry, &mut order); });
            });
            ui.add_enabled_ui(owner, |ui| {
                let header = egui::CollapsingHeader::new("Chart Options").id_salt("chart-options").show(ui, |ui| {
                    let mut draft = rebased_options.clone().unwrap_or_else(|| ChartOptionsDraft::new(&original));
                    let previous = draft.clone();
                    axis_controls(ui, scope, &mut draft.x, "X", &mut order, &mut actions);
                    axis_controls(ui, scope, &mut draft.y, "Y", &mut order, &mut actions);
                    ui.horizontal(|ui| {
                        let response = ui.checkbox(&mut draft.options.grid, "Gridlines"); chart_target(scope, ui, &response, "options/grid", "Gridlines", TargetKind::Checkbox, &mut order);
                        #[cfg(test)] actions.controls.push(("options/grid".into(), response.rect));
                        ui.label("Title"); let response = ui.add(egui::TextEdit::singleline(&mut draft.options.title).char_limit(128)); chart_target(scope, ui, &response, "options/title", "Chart title", TargetKind::TextEntry, &mut order);
                        #[cfg(test)] actions.controls.push(("options/title".into(), response.rect));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Legend");
                        for (placement, label) in [(ChartLegendPlacement::Top, "Top"), (ChartLegendPlacement::Bottom, "Bottom"), (ChartLegendPlacement::Hidden, "Hidden")] {
                            let response = ui.selectable_value(&mut draft.options.legend, placement, label);
                            chart_target(scope, ui, &response, &format!("options/legend/{label}"), &format!("Legend {label}"), TargetKind::Selectable, &mut order);
                            #[cfg(test)] actions.controls.push((format!("options/legend/{label}"), response.rect));
                        }
                    });
                    let references = egui::CollapsingHeader::new("Reference lines").id_salt("chart-option-references").show(ui, |ui| reference_controls(ui, scope, &mut draft, runtime, &mut order, &mut actions, active));
                    chart_target(scope, ui, &references.header_response, "options/references", "Reference lines", TargetKind::Button, &mut order);
                    #[cfg(test)] actions.controls.push(("reference/header".into(), references.header_response.rect));
                    let styles = egui::CollapsingHeader::new("Series appearance").id_salt("chart-option-styles").show(ui, |ui| style_controls(ui, scope, &mut draft, &definition, &mut order, &mut actions, active));
                    chart_target(scope, ui, &styles.header_response, "options/styles", "Series appearance", TargetKind::Button, &mut order);
                    #[cfg(test)] actions.controls.push(("style/header".into(), styles.header_response.rect));
                    if draft.x != previous.x || draft.y != previous.y || draft.options != previous.options || draft.references != previous.references { draft.error = None; }
                    let mut finished = false;
                    ui.horizontal(|ui| {
                        let response = ui.button("Apply options"); chart_target(scope, ui, &response, "options/apply", "Apply chart options", TargetKind::Button, &mut order);
                        #[cfg(test)] actions.controls.push(("options/apply".into(), response.rect));
                        if active && response.clicked() {
                            let result = draft.options().and_then(|mut options| {
                                options.retain_selected_styles(&definition.source, definition.distribution.as_ref().map(|distribution| &distribution.vector));
                                let mut candidate = definition.clone(); candidate.options = options;
                                candidate.options.validate(&candidate.source, candidate.distribution.as_ref().map(|distribution| &distribution.vector)).map_err(str::to_owned)?;
                                let axes_changed = candidate.options.x != original.options.x || candidate.options.y != original.options.y;
                                if axes_changed && matches!(candidate.kind, ChartKind::Line | ChartKind::Scatter | ChartKind::Bar) && matches!(&candidate.source, ChartSource::Vectors { x: None, .. }) && candidate.options.x.scale == ChartAxisScale::Log10 { return Err("Logarithmic X requires an explicit positive X vector; index X includes zero.".into()); }
                                if axes_changed && matches!(candidate.kind, ChartKind::Bar | ChartKind::Histogram | ChartKind::Cumulative) && candidate.options.y.scale == ChartAxisScale::Log10 { return Err("This chart requires a linear Y axis for the zero baseline.".into()); }
                                let current_scalars = scalar_choices(runtime);
                                for reference in &candidate.options.references {
                                    let previous = original.options.references.iter().find(|old| old.id == reference.id);
                                    if let ChartReferenceValue::Named(name) = &reference.value && !previous.is_some_and(|old| old.value == reference.value) && !current_scalars.iter().any(|(current, _)| current == name) {
                                        return Err(format!("Reference '{name}' must resolve to a current scalar."));
                                    }
                                    let scale = if reference.axis == ChartReferenceAxis::X { candidate.options.x.scale } else { candidate.options.y.scale };
                                    let axis_changed = if reference.axis == ChartReferenceAxis::X { candidate.options.x.scale != original.options.x.scale } else { candidate.options.y.scale != original.options.y.scale };
                                    if (axis_changed || !previous.is_some_and(|old| old.axis == reference.axis && old.value == reference.value)) && scale == ChartAxisScale::Log10 {
                                        let value = match &reference.value { ChartReferenceValue::Literal(value) => Some(value.get()), ChartReferenceValue::Named(name) => current_scalars.iter().find(|(current, _)| current == name).map(|(_, value)| *value) };
                                        if value.is_some_and(|value| value <= 0.0) { return Err("Logarithmic chart reference values must be positive.".into()); }
                                    }
                                }
                                let references_changed = candidate.options.references != original.options.references;
                                if (axes_changed || references_changed) && let Ok((_, dataset)) = chart_data(runtime, &candidate) && let Ok(references) = resolve_references(runtime, &candidate) {
                                    dataset.scene_for_definition_with_references(&candidate, &references, ChartView::default(), 640, 400)?;
                                }
                                Ok(candidate.options)
                            });
                            match result { Ok(options) => { definition.options = options; actions.options_draft = Some(None); finished = true; }, Err(error) => draft.error = Some(error) }
                        }
                        let response = ui.button("Cancel options"); chart_target(scope, ui, &response, "options/cancel", "Cancel chart options", TargetKind::Button, &mut order);
                        #[cfg(test)] actions.controls.push(("options/cancel".into(), response.rect));
                        if active && response.clicked() { actions.options_draft = Some(None); finished = true; }
                        let response = ui.button("Reset axes to Auto"); chart_target(scope, ui, &response, "options/reset-axes", "Reset axes to Auto", TargetKind::Button, &mut order);
                        #[cfg(test)] actions.controls.push(("options/reset-axes".into(), response.rect));
                        if active && response.clicked() { draft.x = AxisDraft::new(&ChartAxisOptions::default()); draft.y = AxisDraft::new(&ChartAxisOptions::default()); draft.error = None; }
                    });
                    let validation_error = draft.options().err();
                    if let Some(error) = draft.error.as_ref().or(validation_error.as_ref()) { ui.colored_label(ui.visuals().error_fg_color, error); }
                    pending_options = !finished && draft.options().map_or(true, |options| options != original.options);
                    if active && !finished && self.options_draft.as_ref() != Some(&draft) { actions.options_draft = Some(Some(draft)); }
                });
                chart_target(scope, ui, &header.header_response, "options", "Chart Options", TargetKind::Button, &mut order);
                #[cfg(test)] actions.controls.push(("options/header".into(), header.header_response.rect));
            });
            if definition.source != original.source {
                let old = series_keys(&original.source);
                let new = series_keys(&definition.source);
                definition.hidden = definition.hidden.iter().filter_map(|index| old.get(*index)).filter_map(|key| new.iter().position(|candidate| candidate == key)).collect();
                definition.options.retain_selected_styles(&definition.source, definition.distribution.as_ref().map(|distribution| &distribution.vector));
            }
            if definition.distribution.as_ref().map(|distribution| &distribution.vector) != original.distribution.as_ref().map(|distribution| &distribution.vector) {
                definition.options.retain_selected_styles(&definition.source, definition.distribution.as_ref().map(|distribution| &distribution.vector));
            }
            if definition != original && active { actions.definition = Some(definition.clone()); }
            let Some(stamp) = chart_stamp(runtime, &definition) else { ui.colored_label(ui.visuals().warn_fg_color, "Chart data is unavailable or outdated. Calculate or generate the current source."); return; };
            if active && matches!(definition.kind, ChartKind::Histogram | ChartKind::Cumulative) && let Some(distribution) = &definition.distribution { actions.summary_stamp = Some((stamp.clone(), definition.kind, distribution.clone())); }
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
            if matches!(definition.kind, ChartKind::Histogram | ChartKind::Cumulative) && let Some(data) = &dataset.distribution {
                let rows = if definition.kind == ChartKind::Histogram { data.bins.len() } else { data.ecdf.len() };
                let snapshot = (stamp.clone(), definition.kind, definition.distribution.as_ref().unwrap().clone());
                let current_summary = self.bound == runtime.identity && self.summary_stamp.as_ref() == Some(&snapshot);
                let snapshot_id = egui::Id::new(format!("{snapshot:?}")).value();
                ui.label(format!("{} observations", data.total));
                ui.add_enabled_ui(owner, |ui| {
                let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id("distribution-summary-header"), true);
                let header = state.show_header(ui, |ui| { ui.label("Distribution summary"); });
                let summary_open = header.is_open();
                if active && (!summary_open || !current_summary) && self.summary_focus.is_some() { actions.summary_focus = Some(None); }
                let (toggle, _, _) = header.body_unindented(|ui| {
                    if !owner || !summary_open { ui.disable(); }
                    let mut scroll = egui::ScrollArea::vertical().id_salt(("distribution-summary", snapshot_id)).max_height(150.0);
                    if summary_open && current_summary && let Some(index) = self.summary_focus { scroll = scroll.vertical_scroll_offset((index as f32 * (20.0 + ui.spacing().item_spacing.y) - 60.0).max(0.0)); }
                    scroll.show_rows(ui, 20.0, rows, |ui, visible| {
                        for index in visible {
                            let text = distribution_summary(data, definition.kind, index).unwrap_or_default();
                            let response = ui.push_id(("distribution-row", snapshot_id, index), |ui| ui.add_sized([ui.available_width(), 20.0], egui::Button::new(&text).selected(current_summary && self.summary_row == Some(index)).truncate())).inner.on_hover_text(text);
                            let directive = chart_target(scope, ui, &response, &format!("distribution/summary/{snapshot_id:x}/{index}"), "Distribution summary row", TargetKind::Selectable, &mut order);
                            #[cfg(test)] actions.controls.push((format!("summary/{index}"), response.rect));
                            if active && summary_open && current_summary && self.summary_focus == Some(index) {
                                response.request_focus(); response.scroll_to_me(Some(egui::Align::Center)); actions.summary_focus = Some(None);
                            }
                            if active && summary_open && (response.clicked() || directive == Some(super::jump_mode::ui::FocusDirective::Select)) {
                                actions.summary_row = Some(index); response.request_focus(); response.scroll_to_me(Some(egui::Align::Center));
                            }
                            if active && summary_open && current_summary && response.has_focus() {
                                ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id, egui::EventFilter { vertical_arrows: true, ..Default::default() }));
                                let next = if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) { Some((index + 1).min(rows - 1)) }
                                    else if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) { Some(index.saturating_sub(1)) } else { None };
                                if let Some(next) = next { actions.summary_row = Some(next); actions.summary_focus = Some(Some(next)); }
                            }
                        }
                    });
                    if summary_open && current_summary && let Some(index) = self.summary_row && let Some(detail) = distribution_summary(data, definition.kind, index) {
                        let _response = ui.add(egui::Label::new(detail).wrap());
                        #[cfg(test)] actions.controls.push(("summary/detail".into(), _response.rect));
                    }
                });
                chart_target(scope, ui, &toggle, "distribution/summary", "Distribution summary", TargetKind::Button, &mut order);
                #[cfg(test)] actions.controls.push(("summary/header".into(), toggle.rect));
                });
            }
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
                    let response = ui.add_enabled(owner && actions.definition.is_none() && !pending_bin_draft && !pending_options, egui::Button::new("Copy Chart as Image")); chart_target(scope, ui, &response, "copy", "Copy Chart as Image", TargetKind::Button, &mut order); #[cfg(test)] actions.controls.push((if response.enabled() { "copy-enabled" } else { "copy-disabled" }.into(), response.rect)); if active && response.clicked() { actions.copy = Some((stamp, definition.clone(), (*scene).clone())); }
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

fn distribution_vector_choices(evaluation: &SessionEvaluation) -> Vec<(ChartVectorReference, String)> {
    let mut named = evaluation.final_bindings.iter().filter_map(|(name, binding)| match &binding.outcome {
        LineOutcome::Value(crate::calculation_pad::CalculationValue::Vector(vector)) => Some((ChartVectorReference::Named { name: name.clone() }, format!("{name} ({} values)", vector.components().len()))),
        _ => None,
    }).collect::<Vec<_>>();
    named.sort_by(|a, b| a.1.cmp(&b.1));
    named.extend(evaluation.lines.iter().filter_map(|line| match &line.outcome {
        LineOutcome::Value(crate::calculation_pad::CalculationValue::Vector(vector)) => {
            let name = line.assignment_name.as_ref().map_or(String::new(), |name| format!(" · {name}"));
            Some((ChartVectorReference::Physical(line.id), format!("r{}{name} ({} values)", line.line_index + 1, vector.components().len())))
        }
        _ => None,
    }));
    named
}

fn validate_histogram_bins(runtime: &ChartRuntime, definition: &DistributionDefinition, bins: DistributionBins) -> Result<(), String> {
    if !matches!(bins, DistributionBins::Width { .. }) { return Ok(()); }
    let evaluation = runtime.evaluation.as_ref().filter(|evaluation| Some(evaluation.basis) == runtime.basis).ok_or("Current distribution results are unavailable or outdated.")?;
    let (label, outcome) = resolve_source(evaluation, &definition.vector)?;
    let LineOutcome::Value(crate::calculation_pad::CalculationValue::Vector(vector)) = outcome else { return Err("The distribution source is not a numeric vector.".into()); };
    let mut candidate = definition.clone(); candidate.bins = bins;
    ChartDataset::from_histogram(label, vector.components(), &candidate).map(|_| ())
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

fn style_sources(definition: &ChartDefinition) -> Vec<(ChartSeriesKey, String)> {
    let mut sources = match &definition.source {
        ChartSource::Vectors { y, .. } => y.iter().map(|reference| (ChartSeriesKey::Vector(reference.clone()), reference_label(reference))).collect::<Vec<_>>(),
        ChartSource::Sensitivity { outputs } => outputs.iter().map(|name| (ChartSeriesKey::Sensitivity(name.clone()), name.clone())).collect::<Vec<_>>(),
    };
    if let Some(distribution) = &definition.distribution {
        let key = ChartSeriesKey::Distribution(distribution.vector.clone());
        sources.push((key, format!("Distribution: {}", reference_label(&distribution.vector))));
    }
    sources
}

fn scalar_choices(runtime: &ChartRuntime) -> Vec<(String, f64)> {
    let Some(evaluation) = runtime.evaluation.as_ref().filter(|evaluation| Some(evaluation.basis) == runtime.basis && evaluation.diagnostic.is_none()) else { return Vec::new(); };
    let mut choices = evaluation.final_bindings.iter().filter_map(|(name, binding)| {
        let LineOutcome::Value(value) = &binding.outcome else { return None; };
        crate::calculation_pad::scalar_reference_coordinate(value).ok().map(|value| (name.clone(), value))
    }).collect::<Vec<_>>();
    choices.sort_by(|a, b| a.0.cmp(&b.0)); choices
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
    fn chart_options_draft_rebases_removed_staged_style() {
        let named = |name: &str| ChartVectorReference::Named { name: name.into() };
        let mut definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![named("v"), named("w")] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: None, options: Default::default() };
        let mut draft = ChartOptionsDraft::new(&definition); draft.options.grid = true;
        draft.options.styles.push(ChartSeriesStyle { key: ChartSeriesKey::Vector(named("w")), color: [1, 2, 3, 255], line: ChartLineStyle::Solid, marker: ChartMarkerStyle::Auto, thickness: ChartFiniteValue::try_from(2.0).unwrap() });
        definition.source = ChartSource::Vectors { x: None, y: vec![named("v")] };
        let rebased = rebase_options_draft(&draft, &definition).unwrap();
        assert!(rebased.options.grid); assert!(rebased.options.styles.is_empty());
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
    fn histogram_width_change_in_data_reports_error_without_blocking_cdf() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("v = [1, 2]").unwrap(); session.evaluation();
        let spec = DistributionDefinition { vector: ChartVectorReference::Named { name: "v".into() }, bins: DistributionBins::Width { width: ChartFiniteValue::try_from(1.0).unwrap() }, scale: DistributionScale::Count };
        let mut definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![] }, kind: ChartKind::Histogram, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: Some(spec.clone()), options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(definition.clone()), basis: session.evaluation_basis(), pad_host: None, pinned: false };
        assert!(chart_data(&runtime, &definition).is_ok());
        session.replace_source("v = [1, 2000]").unwrap(); runtime.basis = session.evaluation_basis();
        assert!(chart_data(&runtime, &definition).is_err());
        session.evaluation(); runtime.evaluation = session.shared_evaluation();
        assert!(chart_data(&runtime, &definition).unwrap_err().contains("1024"));
        assert_eq!(definition.distribution.as_ref().unwrap().bins, spec.bins);
        definition.kind = ChartKind::Cumulative;
        assert!(chart_data(&runtime, &definition).is_ok());
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
    fn chart_distribution_controls_preserve_source_and_stage_bins() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("v = [1, 2, 2, 4]\nw = [3, 6]\ns = range(4)").unwrap(); session.evaluation();
        let source = ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Named { name: "v".into() }] };
        let definition = ChartDefinition { source: source.clone(), kind: ChartKind::Bar, x_label: "X".into(), y_label: "Y".into(), hidden: vec![0], bar_labels: vec!["saved".into()], distribution: None, options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(definition), basis: session.evaluation_basis(), pad_host: Some(super::super::utility_host::HostIdentity { host: UtilityPresentation::Docked, generation: 1 }), pinned: false };
        let ctx = egui::Context::default(); let mut dialog = CalculationChartDialog::default(); dialog.open();
        let control = |actions: &ChartActions, name: &str| actions.controls.iter().find(|(key, _)| key == name).unwrap_or_else(|| panic!("missing {name}: {:?}", actions.controls)).1.center();
        let mut actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "kind/Histogram"));
        assert_eq!(runtime.definition.as_ref().unwrap().kind, ChartKind::Histogram);
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "distribution-menu"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert!(!actions.controls.iter().any(|(key, _)| key.starts_with("distribution/s ")));
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "distribution/w (2 values)"));
        assert!(matches!(&runtime.definition.as_ref().unwrap().distribution.as_ref().unwrap().vector, ChartVectorReference::Named { name } if name == "w"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "bins/Bin count"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "bin-value"));
        let ctrl = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        let key = |key, modifiers| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
        chart_frame(&ctx, &mut dialog, &mut runtime, vec![key(egui::Key::A, ctrl), egui::Event::Text("0".into())]);
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert!(actions.controls.iter().any(|(name, _)| name == "copy-disabled"));
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "apply-bins"));
        assert_eq!(runtime.definition.as_ref().unwrap().distribution.as_ref().unwrap().bins, DistributionBins::Auto);
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "bin-value"));
        chart_frame(&ctx, &mut dialog, &mut runtime, vec![key(egui::Key::A, ctrl), egui::Event::Text("2".into())]);
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "apply-bins"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert_eq!(runtime.definition.as_ref().unwrap().distribution.as_ref().unwrap().bins, DistributionBins::Count { count: 2 });
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "bins/Bin width"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "bin-value"));
        chart_frame(&ctx, &mut dialog, &mut runtime, vec![key(egui::Key::A, ctrl), egui::Event::Text("0.001".into())]);
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "apply-bins"));
        assert_eq!(runtime.definition.as_ref().unwrap().distribution.as_ref().unwrap().bins, DistributionBins::Count { count: 2 });
        assert!(dialog.distribution_draft.as_ref().unwrap().error.as_deref().is_some_and(|error| error.contains("1024")));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "scale/Percentage"));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "summary/0"));
        chart_frame(&ctx, &mut dialog, &mut runtime, vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]);
        assert_eq!(dialog.summary_row, Some(1));
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "kind/Cumulative"));
        let chart = runtime.definition.as_ref().unwrap();
        assert_eq!(chart.kind, ChartKind::Cumulative);
        assert_eq!(chart.source, source); assert_eq!(chart.hidden, [0]); assert_eq!(chart.bar_labels, ["saved"]);
        assert_eq!(chart.distribution.as_ref().unwrap().bins, DistributionBins::Count { count: 2 });
        assert_eq!(chart.distribution.as_ref().unwrap().scale, DistributionScale::Percentage);
        session.replace_source("v = [9]").unwrap(); runtime.basis = session.evaluation_basis();
        actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert!(actions.copy.is_none());
        assert!(!actions.controls.iter().any(|(name, _)| name == "copy-enabled"));
    }

    #[test]
    fn chart_distribution_summary_keyboard_scrolls_virtual_rows() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("v = [0, 100]").unwrap(); session.evaluation();
        let definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![] }, kind: ChartKind::Histogram, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: Some(DistributionDefinition { vector: ChartVectorReference::Named { name: "v".into() }, bins: DistributionBins::Count { count: 32 }, scale: DistributionScale::Count }), options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(definition), basis: session.evaluation_basis(), pad_host: Some(super::super::utility_host::HostIdentity { host: UtilityPresentation::Docked, generation: 1 }), pinned: false };
        let ctx = egui::Context::default(); let mut dialog = CalculationChartDialog::default(); dialog.open();
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert!(actions.controls.iter().filter(|(name, _)| name.starts_with("summary/")).count() < 32);
        let first = actions.controls.iter().find(|(name, _)| name == "summary/0").unwrap().1.center();
        chart_click(&ctx, &mut dialog, &mut runtime, first);
        for _ in 0..10 {
            for pressed in [true, false] {
                chart_frame(&ctx, &mut dialog, &mut runtime, vec![egui::Event::Key { key: egui::Key::ArrowDown, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE }]);
            }
            chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        }
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert_eq!(dialog.summary_row, Some(10));
        assert!(actions.controls.iter().any(|(name, _)| name == "summary/10"));
        assert!(actions.controls.iter().any(|(name, _)| name == "summary/detail"));
        dialog.summary_focus = Some(11);
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 900.0))), focused: true, ..Default::default() };
        ctx.run_ui(input, |ui| { dialog.render_host(&ctx, &runtime, false, true, Some(ui), UtilityPresentation::Docked); }).drop_without_applying_deltas();
        assert!(dialog.summary_focus.is_none());
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        let header = actions.controls.iter().find(|(name, _)| name == "summary/header").unwrap().1.center();
        chart_frame(&ctx, &mut dialog, &mut runtime, vec![egui::Event::PointerMoved(header), egui::Event::PointerButton { pos: header, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }]);
        dialog.summary_focus = Some(11);
        chart_frame(&ctx, &mut dialog, &mut runtime, vec![egui::Event::PointerMoved(header), egui::Event::PointerButton { pos: header, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE }]);
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert!(dialog.summary_focus.is_none());
        assert!(!actions.controls.iter().any(|(name, _)| name == "summary/detail"));
    }

    #[test]
    fn chart_options_are_staged_and_reset_axes_without_resetting_view() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("v = [1, 2]").unwrap(); session.evaluation();
        let mut definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Named { name: "v".into() }] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: None, options: Default::default() };
        definition.options.x.bounds = ChartAxisBounds::Fixed { min: ChartFiniteValue::try_from(0.0).unwrap(), max: ChartFiniteValue::try_from(2.0).unwrap() };
        definition.options.references.push(ChartReferenceLine { id: 1, axis: ChartReferenceAxis::Y, value: ChartReferenceValue::Named("unavailable".into()), label: "Saved".into() });
        definition.options.next_reference_id = 2;
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: None, table: None, definition: Some(definition.clone()), basis: session.evaluation_basis(), pad_host: Some(super::super::utility_host::HostIdentity { host: UtilityPresentation::Docked, generation: 1 }), pinned: false };
        let ctx = egui::Context::default(); let mut dialog = CalculationChartDialog::default(); dialog.open();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        dialog.bound = runtime.identity; dialog.view.set(2.0, 2.0, 0.1, 0.0).unwrap();
        let control = |actions: &ChartActions, name: &str| actions.controls.iter().find(|(key, _)| key == name).unwrap_or_else(|| panic!("missing {name}: {:?}", actions.controls)).1.center();
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/header"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/grid"));
        assert!(!runtime.definition.as_ref().unwrap().options.grid);
        runtime.definition.as_mut().unwrap().x_label = "changed independently".into();
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert!(dialog.options_draft.as_ref().unwrap().options.grid);
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/X/bounds/Auto"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/X/scale/Log10"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/apply"));
        assert!(dialog.options_draft.as_ref().unwrap().error.as_deref().is_some_and(|error| error.contains("index X")));
        assert_eq!(runtime.definition.as_ref().unwrap().options.x.bounds, definition.options.x.bounds);
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/reset-axes"));
        assert_eq!(runtime.definition.as_ref().unwrap().options.x.bounds, definition.options.x.bounds);
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/apply"));
        let saved = &runtime.definition.as_ref().unwrap().options;
        assert!(saved.grid); assert_eq!(saved.x.bounds, ChartAxisBounds::Auto);
        assert_eq!(saved.references[0].value, ChartReferenceValue::Named("unavailable".into()));
        assert_eq!(dialog.view.zoom_x, 2.0); assert_eq!(dialog.view.pan_x, 0.1);
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        assert!(actions.copy.is_none());
    }

    #[test]
    fn chart_options_reference_and_style_widgets_apply_and_keep_ids() {
        let directory = tempfile::tempdir().unwrap(); let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("v = [1, 2]\nlimit = 2\nother = 3").unwrap(); session.evaluation();
        let definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Named { name: "v".into() }] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: None, options: Default::default() };
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(definition), basis: session.evaluation_basis(), pad_host: Some(super::super::utility_host::HostIdentity { host: UtilityPresentation::Docked, generation: 1 }), pinned: false };
        let ctx = egui::Context::default(); ctx.all_styles_mut(|style| style.animation_time = 0.0); let mut dialog = CalculationChartDialog::default(); dialog.open();
        let control = |actions: &ChartActions, name: &str| actions.controls.iter().find(|(key, _)| key == name).unwrap_or_else(|| panic!("missing {name}: {:?}", actions.controls)).1.center();
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/header"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/header"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/add"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/1/kind/Named scalar"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/1/search"));
        chart_frame(&ctx, &mut dialog, &mut runtime, vec![egui::Event::Text("other".into())]);
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/1/scalar/other"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/1/label"));
        chart_frame(&ctx, &mut dialog, &mut runtime, vec![egui::Event::Text("Limit".into())]);
        runtime.evaluation = None;
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/apply"));
        assert!(dialog.options_draft.as_ref().unwrap().error.as_deref().is_some_and(|error| error.contains("current scalar")));
        assert!(runtime.definition.as_ref().unwrap().options.references.is_empty());
        runtime.evaluation = session.shared_evaluation();
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/header"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "style/header"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "style/0/enabled"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "style/0/line/Dashed"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "style/0/marker/Cross"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "options/apply"));
        let options = &runtime.definition.as_ref().unwrap().options;
        assert_eq!(options.references.len(), 1); assert_eq!(options.references[0].id, 1); assert_eq!(options.references[0].label, "Limit");
        assert_eq!(options.references[0].value, ChartReferenceValue::Named("other".into()));
        assert_eq!(options.styles[0].line, ChartLineStyle::Dashed); assert_eq!(options.styles[0].marker, ChartMarkerStyle::Cross);
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "style/header"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/header"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/1/remove"));
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new()); chart_click(&ctx, &mut dialog, &mut runtime, control(&actions, "reference/add"));
        assert_eq!(dialog.options_draft.as_ref().unwrap().references[0].id, 2);
    }

    #[test]
    fn chart_options_reject_new_nonpositive_named_log_reference_with_unresolved_retained_reference() {
        let directory = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path().to_path_buf());
        let mut session = crate::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("v = [1, 2]\nnegative = -1").unwrap(); session.evaluation();
        let mut definition = ChartDefinition { source: ChartSource::Vectors { x: None, y: vec![ChartVectorReference::Named { name: "v".into() }] }, kind: ChartKind::Line, x_label: String::new(), y_label: String::new(), hidden: vec![], bar_labels: vec![], distribution: None, options: Default::default() };
        let first_id = definition.options.allocate_reference_id().unwrap();
        definition.options.references.push(ChartReferenceLine { id: first_id, axis: ChartReferenceAxis::Y, value: ChartReferenceValue::Named("missing".into()), label: String::new() });
        let mut runtime = ChartRuntime { identity: session.current_id().zip(session.document_incarnation()), evaluation: session.shared_evaluation(), table: None, definition: Some(definition), basis: session.evaluation_basis(), pad_host: Some(super::super::utility_host::HostIdentity { host: UtilityPresentation::Docked, generation: 1 }), pinned: false };
        let ctx = egui::Context::default(); ctx.all_styles_mut(|style| style.animation_time = 0.0); let mut dialog = CalculationChartDialog::default(); dialog.open();
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        let header = actions.controls.iter().find(|(name, _)| name == "options/header").unwrap().1.center(); chart_click(&ctx, &mut dialog, &mut runtime, header);
        let mut draft = ChartOptionsDraft::new(runtime.definition.as_ref().unwrap());
        let next_id = draft.options.allocate_reference_id().unwrap();
        draft.y.scale = ChartAxisScale::Log10;
        draft.references.push(ReferenceDraft { id: next_id, axis: ChartReferenceAxis::Y, named: true, value: "negative".into(), label: String::new(), search: String::new() });
        dialog.options_draft = Some(draft);
        let actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        let apply = actions.controls.iter().find(|(name, _)| name == "options/apply").unwrap().1.center(); chart_click(&ctx, &mut dialog, &mut runtime, apply);
        assert!(dialog.options_draft.as_ref().unwrap().error.as_deref().is_some_and(|error| error.contains("positive")));
        assert_eq!(runtime.definition.as_ref().unwrap().options.references.len(), 1);
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
        chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
        let mut actions = chart_frame(&ctx, &mut dialog, &mut runtime, Vec::new());
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
