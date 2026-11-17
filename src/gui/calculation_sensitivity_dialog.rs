//! Hosted presentation for the Calculation Pad sensitivity worker.
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::{HostIdentity, UtilityPresentation};
use crate::calculation_pad::{DocumentId, DocumentIncarnation, EvaluationBasis, LineId, LiteralInput, Sampling, SensitivityBasis, SensitivityDefinition, SensitivitySettings, SensitivityStatus, SensitivityTable, TableExport};
use eframe::egui;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct SensitivityRuntime {
    pub identity: Option<(DocumentId, DocumentIncarnation)>,
    pub pad_host: Option<HostIdentity>,
    pub basis: Option<EvaluationBasis>,
    pub settings: Option<SensitivitySettings>,
    pub inputs: Vec<(LineId, LiteralInput)>,
    pub watches: Vec<String>,
    pub active: Option<SensitivityBasis>,
    pub status: SensitivityStatus,
    pub table: Option<Arc<SensitivityTable>>,
    pub pinned: bool,
}

pub(super) enum SensitivityCommand {
    Generate(SensitivitySettings), Cancel(SensitivityBasis), Copy(SensitivityBasis, TableExport),
}

#[derive(Default)]
pub(super) struct SensitivityActions {
    pub identity: Option<(DocumentId, DocumentIncarnation)>,
    pub pad_host: Option<HostIdentity>,
    pub basis: Option<EvaluationBasis>,
    pub base: Option<SensitivitySettings>,
    pub active: Option<SensitivityBasis>,
    pub settings: Option<Option<SensitivitySettings>>,
    pub command: Option<SensitivityCommand>,
    pub draft: Option<String>,
    pub host_request: Option<UtilityPresentation>,
    pub close: bool,
    pub viewport: Option<egui::ViewportId>,
    pub popup_anchors: Vec<(egui::Id, egui::LayerId)>,
    #[cfg(test)]
    pub generate_control: Option<(egui::Rect, egui::Rect, bool)>,
}

#[derive(Default)]
pub(crate) struct CalculationSensitivityDialog {
    pub(super) open: bool,
    epoch: u64,
    close_requested: bool,
    bound: Option<(DocumentId, DocumentIncarnation)>,
    output_draft: String,
    pub(super) feedback: Option<String>,
    popup_anchors: Vec<(egui::Id, egui::LayerId)>,
    popup_viewport: Option<egui::ViewportId>,
    #[cfg(test)]
    pub(super) generate_control: Option<(egui::Rect, egui::Rect, bool)>,
}

impl CalculationSensitivityDialog {
    pub(super) fn open(&mut self) { if !self.open { self.epoch = self.epoch.wrapping_add(1); self.open = true; self.close_requested = false; } }
    pub(super) fn close(&mut self) { self.open = false; self.close_requested = false; self.bound = None; self.output_draft.clear(); self.feedback = None; self.popup_anchors.clear(); self.popup_viewport = None; }
    pub(super) fn request_close(&mut self) { self.close_requested = true; }
    pub(super) fn epoch(&self) -> u64 { self.epoch }
    pub(super) fn commit(&mut self, actions: &SensitivityActions) {
        if self.bound != actions.identity { self.bound = actions.identity; self.output_draft.clear(); self.feedback = None; }
        if let Some(draft) = &actions.draft { self.output_draft.clone_from(draft); }
        self.popup_anchors.clone_from(&actions.popup_anchors);
        self.popup_viewport = actions.viewport;
        #[cfg(test)]
        { self.generate_control = actions.generate_control; }
    }
    pub(super) fn owned_popup_layer(&self, ctx: &egui::Context, base: egui::LayerId) -> Option<egui::LayerId> {
        if self.popup_viewport != Some(ctx.viewport_id()) { return None; }
        self.popup_anchors.iter().find_map(|(id, layer)| {
            if *layer != base || !ctx.read_response(*id).is_some_and(|response| response.enabled() && response.layer_id == base) { return None; }
            let popup = id.with("popup");
            egui::Popup::is_id_open(ctx, popup).then_some(egui::LayerId::new(egui::Order::Foreground, popup))
        })
    }
    pub(super) fn render_host(&mut self, ctx: &egui::Context, runtime: &SensitivityRuntime, owner: bool, first: bool, child: Option<&mut egui::Ui>, presentation: UtilityPresentation) -> SensitivityActions {
        let mut actions = SensitivityActions { identity: runtime.identity, pad_host: runtime.pad_host, basis: runtime.basis, base: runtime.settings.clone(), active: runtime.active.clone(), viewport: Some(ctx.viewport_id()), ..Default::default() };
        let mut window_open = self.open;
        let mut body = |ui: &mut egui::Ui| {
            let active = owner && first && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            let (button, next) = super::utility_host::presentation_button(ui, presentation, active);
            if active && button.clicked() { actions.host_request = Some(next); }
            if ui.add_enabled(active && !runtime.pinned, egui::Button::new("Close")).clicked() { actions.close = true; }
            if let Some(feedback) = &self.feedback { ui.colored_label(ui.visuals().error_fg_color, feedback); }
            if runtime.identity.is_none() { ui.label("Open a Calculation Pad to use sensitivity tables."); return; }
            let mut settings = runtime.settings.clone();
            let mut draft = if self.bound == runtime.identity { self.output_draft.clone() } else { String::new() };
            ui.add_enabled_ui(active, |ui| {
                if settings.is_some() && ui.button("Remove table definition").clicked() { settings = None; }
                if settings.is_none() && ui.button("Create table definition").clicked() && let Some((id, input)) = runtime.inputs.first() {
                    settings = Some(SensitivitySettings { input_name: input.name.clone(), definition: SensitivityDefinition { input: *id, start: input.value.clone(), end: input.value.clone(), sampling: Sampling::Count(5), outputs: runtime.watches.iter().take(8).cloned().collect() } });
                }
                let Some(current_settings) = settings.as_mut() else { ui.label("Choose an eligible literal input to begin."); return; };
                let selected = runtime.inputs.iter().find(|(id, input)| *id == current_settings.definition.input && input.name == current_settings.input_name);
                let response = egui::ComboBox::from_id_salt("sensitivity-input").selected_text(selected.map_or("Input unavailable".to_owned(), |(_, input)| format!("{} · line {}", input.name, input.line + 1))).show_ui(ui, |ui| {
                    for (id, input) in &runtime.inputs {
                        if ui.selectable_label(current_settings.definition.input == *id && current_settings.input_name == input.name, format!("{} · line {}", input.name, input.line + 1)).clicked() {
                            current_settings.definition.input = *id; current_settings.input_name = input.name.clone();
                        }
                    }
                });
                actions.popup_anchors.push((response.response.id, response.response.layer_id));
                if selected.is_none() { ui.colored_label(ui.visuals().warn_fg_color, "The selected input changed or is unavailable; choose it again."); }
                ui.horizontal(|ui| { ui.label("Start"); ui.add(egui::TextEdit::singleline(&mut current_settings.definition.start).desired_width(90.0).char_limit(257)); ui.label("End"); ui.add(egui::TextEdit::singleline(&mut current_settings.definition.end).desired_width(90.0).char_limit(257)); });
                ui.horizontal(|ui| {
                    let count_mode = matches!(current_settings.definition.sampling, Sampling::Count(_));
                    if ui.selectable_label(!count_mode, "Step").clicked() && count_mode { current_settings.definition.sampling = Sampling::Step("1".into()); }
                    if ui.selectable_label(count_mode, "Sample count").clicked() && !count_mode { current_settings.definition.sampling = Sampling::Count(5); }
                    match &mut current_settings.definition.sampling { Sampling::Step(step) => { ui.add(egui::TextEdit::singleline(step).desired_width(90.0).char_limit(257)); }, Sampling::Count(count) => { ui.add(egui::DragValue::new(count).range(1..=1000)); } }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Outputs:");
                    let mut remove = None;
                    for (index, name) in current_settings.definition.outputs.iter().enumerate() { if ui.small_button(format!("{name} ×")).clicked() { remove = Some(index); } }
                    if let Some(index) = remove { current_settings.definition.outputs.remove(index); }
                    ui.add(egui::TextEdit::singleline(&mut draft).desired_width(90.0).hint_text("name"));
                    if ui.button("Add").clicked() && current_settings.definition.outputs.len() < 8 && let Some(name) = crate::calculation_pad::watch_name(&draft) && !current_settings.definition.outputs.contains(&name) { current_settings.definition.outputs.push(name); draft.clear(); }
                });
                let response = ui.menu_button("Choose watched outputs", |ui| {
                    for name in &runtime.watches {
                        let mut checked = current_settings.definition.outputs.contains(name);
                        if ui.checkbox(&mut checked, name).changed() {
                            if checked && current_settings.definition.outputs.len() < 8 { current_settings.definition.outputs.push(name.clone()); }
                            else if !checked { current_settings.definition.outputs.retain(|output| output != name); }
                        }
                    }
                });
                actions.popup_anchors.push((response.response.id, response.response.layer_id));
                let current = runtime.table.as_ref().filter(|table| table.basis.definition == current_settings.definition && table.basis.input_name == current_settings.input_name);
                ui.horizontal(|ui| {
                    let generate = ui.add_enabled(selected.is_some(), egui::Button::new("Generate"));
                    #[cfg(test)]
                    { actions.generate_control = Some((generate.rect, ui.clip_rect(), generate.enabled())); }
                    if generate.clicked() { actions.command = Some(SensitivityCommand::Generate(current_settings.clone())); }
                    if let Some(basis) = &runtime.active && matches!(runtime.status, SensitivityStatus::Pending { .. }) && ui.button("Cancel").clicked() { actions.command = Some(SensitivityCommand::Cancel(basis.clone())); }
                    for (label, format) in [("Copy Table", TableExport::Table), ("Copy CSV", TableExport::Csv), ("Copy Markdown", TableExport::Markdown)] {
                        if ui.add_enabled(current.is_some(), egui::Button::new(label)).clicked() && let Some(table) = current { actions.command = Some(SensitivityCommand::Copy(table.basis.clone(), format)); }
                    }
                });
                match &runtime.status { SensitivityStatus::Pending { completed, total } => { ui.label(format!("Calculating {completed} / {total}")); }, SensitivityStatus::Failed(error) => { ui.colored_label(ui.visuals().error_fg_color, error); }, SensitivityStatus::Canceled => { ui.label("Canceled; no complete table was published."); }, SensitivityStatus::Outdated => { ui.label("Table is outdated; generate again to copy."); }, _ => {} }
                if let Some(table) = current {
                    ui.strong(crate::calculation_pad::table_header(table).join("  |  "));
                    egui::ScrollArea::vertical().max_height(240.0).id_salt("sensitivity-results").show_rows(ui, 22.0, table.rows.len(), |ui, rows| {
                        for row in &table.rows[rows] { let text = crate::calculation_pad::table_row(row).join("  |  "); ui.add(egui::Label::new(&text).truncate()).on_hover_text(text); }
                    });
                } else { ui.weak("Copy is available for a current completed table."); }
            });
            if active && settings != runtime.settings { actions.settings = Some(settings); }
            if active && (draft != self.output_draft || self.bound != runtime.identity) { actions.draft = Some(draft); }
        };
        if let Some(ui) = child { egui::ScrollArea::vertical().show(ui, |ui| body(ui)); }
        else { egui::Window::new("Sensitivity Tables").id(egui::Id::new("calculation-sensitivity-window")).show_utility(ctx, "calculation-pad-sensitivity", egui::Id::new("calculation-sensitivity-window"), Some(&mut window_open), super::utility_dialog_size::UtilityWindowSizeRecipe::new(super::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::vec2(700.0, 560.0)), egui::vec2(360.0, 280.0), [None, None]), |ui| { egui::ScrollArea::vertical().show(ui, |ui| body(ui)); }); }
        actions.close |= (!window_open || self.close_requested) && !runtime.pinned;
        actions
    }
}

pub(super) fn actions_current(actions: &SensitivityActions, runtime: &SensitivityRuntime) -> bool {
    actions.identity.is_some() && actions.identity == runtime.identity && actions.pad_host.is_some() && actions.pad_host == runtime.pad_host
        && actions.basis == runtime.basis && actions.base == runtime.settings && actions.active == runtime.active
}
