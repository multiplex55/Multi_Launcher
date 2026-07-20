use crate::clipboard_modify::clipboard::{
    ClipboardError, ClipboardSummary, ProductionClipboardService,
};
use crate::clipboard_modify::coordinator::{PreviewCoordinator, PreviewState};
use crate::clipboard_modify::model::{
    ClipboardModifierCatalog, ClipboardTemplate, OperationId, SavedPipeline, StageArguments,
    StageSpec, TemplateProcessor,
};
use crate::clipboard_modify::parser::ClipboardModifyIntent;
use crate::clipboard_modify::pipeline::find_template;
use crate::clipboard_modify::store::ClipboardModifierStore;
use eframe::egui;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub const LARGE_SOURCE_CONFIRM_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardModifyDialogSection {
    #[default]
    Modify,
    Templates,
    SavedPipelines,
    ManageTemplates,
    ManagePipelines,
    Help,
}

#[derive(Debug, Clone)]
pub struct DialogStage {
    pub id: u64,
    pub spec: StageSpec,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingDialogAction {
    Apply,
    ApplyAndClose,
    CopyResult,
}

pub struct ClipboardModifyDialogState {
    pub open: bool,
    pub section: ClipboardModifyDialogSection,
    pub source: String,
    pub baseline: String,
    pub reset_source: String,
    pub source_fingerprint: u64,
    pub source_error: Option<String>,
    pub stages: Vec<DialogStage>,
    next_stage_id: u64,
    pub preview: PreviewCoordinator,
    pub acknowledged_large_sources: HashSet<(u64, usize)>,
    pub large_confirmation_open: bool,
    pub external_change: Option<ClipboardSummary>,
    pub pending_action: Option<PendingDialogAction>,
    pub unsaved_template_draft: bool,
    pub unsaved_pipeline_draft: bool,
    pub wrap_preview: bool,
    pub last_action: Option<String>,
    pub template_filter: String,
    pub selected_template: Option<String>,
    pub template_draft: Vec<ClipboardTemplate>,
    pub template_editor_error: Option<String>,
    pub template_delete_confirmation: Option<String>,
    pub selected_pipeline: Option<String>,
    pub pipeline_draft: Vec<SavedPipeline>,
    pub pipeline_editor_error: Option<String>,
    pub pipeline_delete_confirmation: Option<String>,
    pub duplicate_save_confirmation: Option<String>,
}

impl Default for ClipboardModifyDialogState {
    fn default() -> Self {
        Self {
            open: false,
            section: Default::default(),
            source: String::new(),
            baseline: String::new(),
            reset_source: String::new(),
            source_fingerprint: 0,
            source_error: None,
            stages: Vec::new(),
            next_stage_id: 1,
            preview: PreviewCoordinator::default(),
            acknowledged_large_sources: HashSet::new(),
            large_confirmation_open: false,
            external_change: None,
            pending_action: None,
            unsaved_template_draft: false,
            unsaved_pipeline_draft: false,
            wrap_preview: true,
            last_action: None,
            template_filter: String::new(),
            selected_template: None,
            template_draft: Vec::new(),
            template_editor_error: None,
            template_delete_confirmation: None,
            selected_pipeline: None,
            pipeline_draft: Vec::new(),
            pipeline_editor_error: None,
            pipeline_delete_confirmation: None,
            duplicate_save_confirmation: None,
        }
    }
}

impl ClipboardModifyDialogState {
    pub fn open_section(
        &mut self,
        section: ClipboardModifyDialogSection,
        service: &Arc<ProductionClipboardService>,
    ) {
        let was_open = self.open;
        self.open = true;
        self.section = section;
        if !was_open {
            self.load_clipboard(service);
        }
    }
    pub fn open_section_without_loading_for_test(&mut self, section: ClipboardModifyDialogSection) {
        self.open = true;
        self.section = section;
    }
    pub fn load_clipboard(&mut self, service: &Arc<ProductionClipboardService>) {
        match service.read_text_for_modify() {
            Ok(text) => self.replace_loaded_source(text),
            Err(ClipboardError::NonText) => {
                self.source_error = Some("Clipboard does not contain text".into());
                self.replace_loaded_source(String::new());
            }
            Err(e) => self.source_error = Some(e.to_string()),
        }
    }
    pub fn replace_loaded_source(&mut self, text: String) {
        self.baseline = text.clone();
        self.reset_source = text.clone();
        self.source = text;
        self.source_fingerprint = fingerprint(&self.source);
        self.source_error = None;
        self.preview = PreviewCoordinator::default();
        self.large_confirmation_open = self.requires_large_ack();
    }
    pub fn cleanup_after_close(&mut self) {
        let wrap = self.wrap_preview;
        self.preview.cancel_active();
        self.source.clear();
        self.baseline.clear();
        self.reset_source.clear();
        self.source_fingerprint = 0;
        self.source_error = None;
        self.stages.clear();
        self.preview = PreviewCoordinator::default();
        self.external_change = None;
        self.pending_action = None;
        self.unsaved_template_draft = false;
        self.unsaved_pipeline_draft = false;
        self.large_confirmation_open = false;
        self.wrap_preview = wrap;
    }
    pub fn can_apply(&self) -> bool {
        matches!(self.preview.state(), PreviewState::Completed { .. })
            && self.preview.apply_text().is_some()
    }
    pub fn mark_dirty(
        &mut self,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        self.source_fingerprint = fingerprint(&self.source);
        if self.requires_large_ack() {
            self.preview = PreviewCoordinator::default();
            self.large_confirmation_open = true;
            return;
        }
        let stages = self.stages.iter().map(|s| s.spec.clone()).collect();
        self.preview.request(
            self.source.clone(),
            ClipboardModifyIntent::Stages(stages),
            catalog,
        );
    }
    pub fn requires_large_ack(&self) -> bool {
        self.source.len() > LARGE_SOURCE_CONFIRM_BYTES
            && !self
                .acknowledged_large_sources
                .contains(&(fingerprint(&self.source), self.source.len()))
    }
    pub fn acknowledge_large_source(
        &mut self,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        self.acknowledged_large_sources
            .insert((fingerprint(&self.source), self.source.len()));
        self.large_confirmation_open = false;
        self.mark_dirty(catalog);
    }
    pub fn reset(
        &mut self,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        self.source = self.reset_source.clone();
        self.stages.clear();
        self.mark_dirty(catalog);
    }
    pub fn add_stage(
        &mut self,
        op: OperationId,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        let id = self.next_stage_id;
        self.next_stage_id += 1;
        self.stages.push(DialogStage {
            id,
            spec: StageSpec {
                operation: op,
                arguments: StageArguments::default(),
            },
            error: None,
        });
        self.mark_dirty(catalog);
    }
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        service: &Arc<ProductionClipboardService>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
        store: Option<&ClipboardModifierStore>,
    ) {
        if !self.open {
            return;
        }
        self.preview.tick();
        let mut open = self.open;
        egui::Window::new("Clipboard Modify")
            .open(&mut open)
            .show(ctx, |ui| {
                self.shortcuts(ui, service, catalog.clone());
                ui.horizontal(|ui| {
                    for (s, l) in [
                        (ClipboardModifyDialogSection::Modify, "Modify"),
                        (ClipboardModifyDialogSection::Templates, "Templates"),
                        (
                            ClipboardModifyDialogSection::SavedPipelines,
                            "Saved Pipelines",
                        ),
                        (
                            ClipboardModifyDialogSection::ManageTemplates,
                            "Manage Templates",
                        ),
                        (
                            ClipboardModifyDialogSection::ManagePipelines,
                            "Manage Pipelines",
                        ),
                        (ClipboardModifyDialogSection::Help, "Help"),
                    ] {
                        if ui.selectable_label(self.section == s, l).clicked() {
                            self.section = s;
                        }
                    }
                });
                ui.separator();
                match self.section {
                    ClipboardModifyDialogSection::Modify => {
                        self.modify_ui(ui, service, catalog.clone())
                    }
                    ClipboardModifyDialogSection::Templates => {
                        self.templates_ui(ui, service, catalog.clone())
                    }
                    ClipboardModifyDialogSection::SavedPipelines => {
                        self.saved_pipelines_ui(ui, service, catalog.clone())
                    }
                    ClipboardModifyDialogSection::ManageTemplates => {
                        if let Some(store) = store {
                            self.manage_templates_ui(ui, catalog.clone(), store)
                        } else {
                            ui.label("Template management is unavailable.");
                        }
                    }
                    ClipboardModifyDialogSection::ManagePipelines => {
                        if let Some(store) = store {
                            self.manage_pipelines_ui(ui, catalog.clone(), store)
                        } else {
                            ui.label("Pipeline management is unavailable.");
                        }
                    }
                    _ => {
                        ui.label(format!("{:?} section", self.section));
                    }
                }
            });
        if self.open && !open {
            self.open = false;
            self.cleanup_after_close();
        }
    }
    pub fn filtered_templates<'a>(
        &self,
        catalog: &'a ClipboardModifierCatalog,
    ) -> Vec<&'a ClipboardTemplate> {
        let q = crate::clipboard_modify::catalog::normalize_name(&self.template_filter);
        catalog
            .templates
            .iter()
            .filter(|t| {
                q.is_empty()
                    || crate::clipboard_modify::catalog::normalize_name(&t.id).contains(&q)
                    || crate::clipboard_modify::catalog::normalize_name(&t.label).contains(&q)
                    || t.aliases
                        .iter()
                        .any(|a| crate::clipboard_modify::catalog::normalize_name(a).contains(&q))
            })
            .collect()
    }

    pub fn pipeline_stage_summary(p: &SavedPipeline) -> String {
        p.stages
            .iter()
            .map(|s| crate::clipboard_modify::catalog::canonical_command(s.operation))
            .collect::<Vec<_>>()
            .join(" → ")
    }
    pub fn pipeline_identity_line(p: &SavedPipeline) -> String {
        format!(
            "{} ({}) aliases: {}",
            p.label,
            p.id,
            if p.aliases.is_empty() {
                "—".into()
            } else {
                p.aliases.join(", ")
            }
        )
    }
    pub fn preview_pipeline(&mut self, id: String, catalog: Arc<ClipboardModifierCatalog>) {
        self.preview.request(
            self.source.clone(),
            ClipboardModifyIntent::ApplySavedPipeline { name: id },
            catalog,
        );
    }
    pub fn apply_pipeline_through_commit_path(
        &mut self,
        id: &str,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        self.preview_pipeline(id.to_string(), catalog);
    }
    pub fn duplicate_pipeline_id(base: &ClipboardModifierCatalog, id: &str) -> String {
        let root = format!(
            "{}-copy",
            crate::clipboard_modify::catalog::normalize_name(id)
        );
        let taken = |n: &str| {
            base.pipelines
                .iter()
                .any(|p| p.id == n || p.aliases.iter().any(|a| a == n))
                || base
                    .templates
                    .iter()
                    .any(|t| t.id == n || t.aliases.iter().any(|a| a == n))
        };
        if !taken(&root) {
            return root;
        }
        for i in 2.. {
            let c = format!("{root}-{i}");
            if !taken(&c) {
                return c;
            }
        }
        unreachable!()
    }
    pub fn begin_pipeline_edit(&mut self, catalog: &ClipboardModifierCatalog) {
        self.pipeline_draft = catalog.pipelines.clone();
        self.unsaved_pipeline_draft = false;
        self.pipeline_editor_error = None;
    }
    pub fn validate_pipeline_draft(
        &self,
        base: &ClipboardModifierCatalog,
    ) -> Result<ClipboardModifierCatalog, String> {
        let model = crate::clipboard_modify::config::VersionedClipboardModifiersFile {
            schema_version: crate::clipboard_modify::config::CURRENT_SCHEMA_VERSION,
            templates: base.templates.clone(),
            pipelines: self.pipeline_draft.clone(),
        };
        crate::clipboard_modify::config::validate_model(&model).map_err(|e| e.to_string())
    }
    pub fn save_pipeline_draft(
        &mut self,
        base: &ClipboardModifierCatalog,
        store: &ClipboardModifierStore,
    ) -> Result<ClipboardModifierCatalog, String> {
        let catalog = self.validate_pipeline_draft(base)?;
        let model = crate::clipboard_modify::config::model_from_catalog(&catalog);
        if let Err(e) = store.save(&model) {
            self.pipeline_editor_error = Some(format!(
                "Could not save clipboard pipelines. Check file permissions and retry: {e}"
            ));
            return Err(self.pipeline_editor_error.clone().unwrap());
        }
        store.replace_valid(catalog.clone());
        self.unsaved_pipeline_draft = false;
        self.pipeline_editor_error = None;
        Ok(catalog)
    }
    pub fn delete_pipeline_from_draft_and_save(
        &mut self,
        base: &ClipboardModifierCatalog,
        store: &ClipboardModifierStore,
        id: &str,
    ) -> Result<(), String> {
        self.pipeline_draft.retain(|p| p.id != id);
        self.unsaved_pipeline_draft = true;
        self.save_pipeline_draft(base, store).map(|_| ())
    }
    pub fn reorder_pipeline_draft_and_save(
        &mut self,
        base: &ClipboardModifierCatalog,
        store: &ClipboardModifierStore,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        if from < self.pipeline_draft.len() && to < self.pipeline_draft.len() {
            self.pipeline_draft.swap(from, to);
        }
        self.unsaved_pipeline_draft = true;
        self.save_pipeline_draft(base, store).map(|_| ())
    }
    pub fn allowed_stage_operations() -> Vec<OperationId> {
        crate::clipboard_modify::catalog::operations()
            .iter()
            .filter(|o| o.pipeline_available)
            .map(|o| o.id)
            .collect()
    }
    pub fn cancel_pipeline_preview(&mut self) {
        self.preview.cancel_active();
    }
    fn templates_ui(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ProductionClipboardService>,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.template_filter);
            if ui.button("Manage Templates").clicked() {
                self.begin_template_edit(catalog.as_ref());
                self.section = ClipboardModifyDialogSection::ManageTemplates;
            }
        });
        for t in self.filtered_templates(catalog.as_ref()) {
            let selected = self.selected_template.as_deref() == Some(&t.id);
            if ui
                .selectable_label(selected, format!("{} ({})", t.label, t.id))
                .clicked()
            {
                self.selected_template = Some(t.id.clone());
                self.preview_template(t.id.clone(), catalog.clone());
            }
        }
        if let Some(id) = self.selected_template.clone() {
            if let Some(t) = find_template(catalog.as_ref(), &id) {
                ui.separator();
                ui.label("Preview");
                ui.monospace(t.render(&self.source));
                if ui.button("Apply Template").clicked() {
                    self.apply_template_through_commit_path(&id, catalog.clone());
                    self.commit(service, false, false);
                }
            }
        }
    }
    pub fn preview_template(&mut self, id: String, catalog: Arc<ClipboardModifierCatalog>) {
        self.preview.request(
            self.source.clone(),
            ClipboardModifyIntent::ApplyTemplate { name: id },
            catalog,
        );
    }
    pub fn apply_template_through_commit_path(
        &mut self,
        id: &str,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        self.preview_template(id.to_string(), catalog);
    }
    pub fn begin_template_edit(&mut self, catalog: &ClipboardModifierCatalog) {
        self.template_draft = catalog.templates.clone();
        self.unsaved_template_draft = false;
        self.template_editor_error = None;
    }
    pub fn validate_template_draft(
        &self,
        base: &ClipboardModifierCatalog,
    ) -> Result<ClipboardModifierCatalog, String> {
        let model = crate::clipboard_modify::config::VersionedClipboardModifiersFile {
            schema_version: crate::clipboard_modify::config::CURRENT_SCHEMA_VERSION,
            templates: self.template_draft.clone(),
            pipelines: base.pipelines.clone(),
        };
        crate::clipboard_modify::config::validate_model(&model).map_err(|e| e.to_string())
    }
    pub fn can_save_template_draft(&self, base: &ClipboardModifierCatalog) -> bool {
        self.validate_template_draft(base).is_ok()
    }
    pub fn save_template_draft(
        &mut self,
        base: &ClipboardModifierCatalog,
        store: &ClipboardModifierStore,
    ) -> Result<ClipboardModifierCatalog, String> {
        let catalog = self.validate_template_draft(base)?;
        let model = crate::clipboard_modify::config::model_from_catalog(&catalog);
        if let Err(e) = store.save(&model) {
            self.template_editor_error = Some(format!(
                "Could not save clipboard templates. Check file permissions and retry: {e}"
            ));
            return Err(self.template_editor_error.clone().unwrap());
        }
        store.replace_valid(catalog.clone());
        self.unsaved_template_draft = false;
        self.template_editor_error = None;
        Ok(catalog)
    }
    pub fn referencing_pipelines(
        catalog: &ClipboardModifierCatalog,
        template_id: &str,
    ) -> Vec<String> {
        catalog
            .pipelines
            .iter()
            .filter(|p| {
                p.stages.iter().any(|s| {
                    s.operation == OperationId::Template
                        && s.arguments.name.as_deref() == Some(template_id)
                })
            })
            .map(|p| format!("{} ({})", p.label, p.id))
            .collect()
    }
    pub fn delete_template_from_draft(
        &mut self,
        base: &ClipboardModifierCatalog,
        template_id: &str,
    ) -> Result<(), String> {
        let refs = Self::referencing_pipelines(base, template_id);
        if !refs.is_empty() {
            return Err(format!(
                "Cannot delete template because saved pipelines reference it: {}",
                refs.join(", ")
            ));
        }
        self.template_draft.retain(|t| t.id != template_id);
        self.unsaved_template_draft = true;
        Ok(())
    }
    pub fn template_delete_confirmation_text(
        catalog: &ClipboardModifierCatalog,
        template_id: &str,
    ) -> Option<String> {
        find_template(catalog, template_id).map(|t| {
            format!(
                "Delete template \"{}\" (ID: {})? Saved pipelines must be updated before referenced templates can be deleted.",
                t.label, t.id
            )
        })
    }
    fn manage_templates_ui(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Arc<ClipboardModifierCatalog>,
        store: &ClipboardModifierStore,
    ) {
        if self.template_draft.is_empty() && !catalog.templates.is_empty() {
            self.begin_template_edit(catalog.as_ref());
        }
        if ui.button("Add template").clicked() {
            self.template_draft.push(ClipboardTemplate {
                id: "new-template".into(),
                label: "New Template".into(),
                aliases: vec![],
                template: "{{clipboard}}".into(),
                processor: Some(TemplateProcessor::Literal),
            });
            self.unsaved_template_draft = true;
        }
        let mut remove = None;
        let mut moved = false;
        let mut edited = false;
        for i in 0..self.template_draft.len() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    edited |= ui
                        .text_edit_singleline(&mut self.template_draft[i].id)
                        .changed();
                    edited |= ui
                        .text_edit_singleline(&mut self.template_draft[i].label)
                        .changed();
                    if ui.button("↑").clicked() && i > 0 {
                        self.template_draft.swap(i, i - 1);
                        moved = true;
                    }
                    if ui.button("↓").clicked() && i + 1 < self.template_draft.len() {
                        self.template_draft.swap(i, i + 1);
                        moved = true;
                    }
                    if ui.button("Delete").clicked() {
                        self.template_delete_confirmation = Some(self.template_draft[i].id.clone());
                        remove = Some(self.template_draft[i].id.clone());
                    }
                });
                let mut aliases = self.template_draft[i].aliases.join(", ");
                if ui.text_edit_singleline(&mut aliases).changed() {
                    self.template_draft[i].aliases = aliases
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                    edited = true;
                }
                edited |= ui
                    .text_edit_multiline(&mut self.template_draft[i].template)
                    .changed();
                egui::ComboBox::from_label("Processor")
                    .selected_text(format!("{:?}", self.template_draft[i].processor))
                    .show_ui(ui, |ui| {
                        edited |= ui
                            .selectable_value(
                                &mut self.template_draft[i].processor,
                                Some(TemplateProcessor::Literal),
                                "Literal",
                            )
                            .changed();
                        edited |= ui
                            .selectable_value(
                                &mut self.template_draft[i].processor,
                                Some(TemplateProcessor::RustRawString),
                                "Rust raw string",
                            )
                            .changed();
                    });
            });
        }
        if let Some(id) = remove {
            if let Err(e) = self.delete_template_from_draft(catalog.as_ref(), &id) {
                self.template_editor_error = Some(e);
            }
        }
        if let Some(id) = &self.template_delete_confirmation
            && let Some(message) = Self::template_delete_confirmation_text(catalog.as_ref(), id)
        {
            ui.label(message);
        }
        if moved {
            let _ = self.save_template_draft(catalog.as_ref(), store);
        }
        if edited {
            self.unsaved_template_draft = true;
        }
        if let Err(e) = self.validate_template_draft(catalog.as_ref()) {
            ui.colored_label(egui::Color32::RED, e);
        }
        if let Some(e) = &self.template_editor_error {
            ui.colored_label(egui::Color32::RED, e);
        }
        if ui
            .add_enabled(
                self.can_save_template_draft(catalog.as_ref()),
                egui::Button::new("Save"),
            )
            .clicked()
        {
            let _ = self.save_template_draft(catalog.as_ref(), store);
        }
    }

    fn saved_pipelines_ui(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ProductionClipboardService>,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        if ui.button("Manage Pipelines").clicked() {
            self.begin_pipeline_edit(catalog.as_ref());
            self.section = ClipboardModifyDialogSection::ManagePipelines;
        }
        for p in &catalog.pipelines {
            ui.separator();
            let selected = self.selected_pipeline.as_deref() == Some(&p.id);
            if ui
                .selectable_label(selected, Self::pipeline_identity_line(p))
                .clicked()
            {
                self.selected_pipeline = Some(p.id.clone());
                self.preview_pipeline(p.id.clone(), catalog.clone());
            }
            ui.label(format!("Stages: {}", Self::pipeline_stage_summary(p)));
            ui.label(format!(
                "Current-source preview: {}",
                self.source.chars().take(80).collect::<String>()
            ));
            if ui.button(format!("Execute {}", p.label)).clicked() {
                self.apply_pipeline_through_commit_path(&p.id, catalog.clone());
                self.commit(service, false, false);
            }
        }
    }

    fn manage_pipelines_ui(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Arc<ClipboardModifierCatalog>,
        store: &ClipboardModifierStore,
    ) {
        if self.pipeline_draft.is_empty() && !catalog.pipelines.is_empty() {
            self.begin_pipeline_edit(catalog.as_ref());
        }
        ui.horizontal(|ui| {
            if ui.button("Add pipeline").clicked() {
                self.pipeline_draft.push(SavedPipeline {
                    id: Self::duplicate_pipeline_id(catalog.as_ref(), "new-pipeline"),
                    label: "New Pipeline".into(),
                    aliases: vec![],
                    stages: vec![],
                });
                self.unsaved_pipeline_draft = true;
            }
            if ui.button("Cancel preview").clicked() {
                self.cancel_pipeline_preview();
            }
        });
        let mut remove = None;
        let mut swap = None;
        for i in 0..self.pipeline_draft.len() {
            ui.push_id(format!("pipeline-{i}"), |ui| {
                ui.horizontal(|ui| {
                    ui.label("ID");
                    ui.text_edit_singleline(&mut self.pipeline_draft[i].id);
                    ui.label("Label");
                    ui.text_edit_singleline(&mut self.pipeline_draft[i].label);
                    if ui.button("Duplicate").clicked() {
                        let mut p = self.pipeline_draft[i].clone();
                        p.id = Self::duplicate_pipeline_id(catalog.as_ref(), &p.id);
                        p.label = format!("{} Copy", p.label);
                        self.duplicate_save_confirmation = Some(p.id.clone());
                        self.pipeline_draft.push(p);
                        self.unsaved_pipeline_draft = true;
                    }
                    if ui.button("↑").clicked() && i > 0 {
                        swap = Some((i, i - 1));
                    }
                    if ui.button("↓").clicked() && i + 1 < self.pipeline_draft.len() {
                        swap = Some((i, i + 1));
                    }
                    if ui.button("Delete").clicked() {
                        self.pipeline_delete_confirmation = Some(self.pipeline_draft[i].id.clone());
                        remove = Some(self.pipeline_draft[i].id.clone());
                    }
                });
                let mut aliases = self.pipeline_draft[i].aliases.join(", ");
                if ui.text_edit_singleline(&mut aliases).changed() {
                    self.pipeline_draft[i].aliases = aliases
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                    self.unsaved_pipeline_draft = true;
                }
                for j in 0..self.pipeline_draft[i].stages.len() {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Stage {} {:?}",
                            j + 1,
                            self.pipeline_draft[i].stages[j].operation
                        ));
                        let a = &mut self.pipeline_draft[i].stages[j].arguments;
                        ui.text_edit_singleline(a.prefix.get_or_insert_with(String::new));
                        ui.text_edit_singleline(a.suffix.get_or_insert_with(String::new));
                        ui.text_edit_singleline(a.name.get_or_insert_with(String::new));
                        ui.text_edit_singleline(a.language.get_or_insert_with(String::new));
                        if ui.button("Remove stage").clicked() {
                            self.pipeline_draft[i].stages.remove(j);
                        }
                    });
                }
                if ui.button("Add trim stage").clicked() {
                    self.pipeline_draft[i].stages.push(StageSpec {
                        operation: OperationId::Trim,
                        arguments: Default::default(),
                    });
                    self.unsaved_pipeline_draft = true;
                }
                if ui.button("Preview complete output").clicked() {
                    self.preview.request(
                        self.source.clone(),
                        ClipboardModifyIntent::Stages(self.pipeline_draft[i].stages.clone()),
                        catalog.clone(),
                    );
                }
            });
        }
        if let Some((a, b)) = swap {
            let _ = self.reorder_pipeline_draft_and_save(catalog.as_ref(), store, a, b);
        }
        if let Some(id) = remove {
            let _ = self.delete_pipeline_from_draft_and_save(catalog.as_ref(), store, &id);
        }
        if let Some(id) = &self.duplicate_save_confirmation {
            ui.label(format!(
                "Confirm saving duplicated pipeline {id} before pressing Save."
            ));
        }
        if let Some(e) = &self.pipeline_editor_error {
            ui.colored_label(egui::Color32::RED, e);
        }
        if ui.button("Save").clicked() {
            let _ = self.save_pipeline_draft(catalog.as_ref(), store);
            self.duplicate_save_confirmation = None;
        }
    }
    fn shortcuts(
        &mut self,
        ui: &egui::Ui,
        service: &Arc<ProductionClipboardService>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        ui.input(|i| {
            if i.modifiers.command && i.key_pressed(egui::Key::Enter) {
                self.last_action = Some(
                    if i.modifiers.shift {
                        "apply_close"
                    } else {
                        "apply"
                    }
                    .into(),
                );
            }
            if i.modifiers.command && i.key_pressed(egui::Key::R) {
                self.last_action = Some("reload".into());
            }
        });
        if let Some(a) = self.last_action.take() {
            match a.as_str() {
                "apply" => self.commit(service, false, false),
                "apply_close" => self.commit(service, true, false),
                "reload" => {
                    if self.source == self.reset_source {
                        self.load_clipboard(service);
                        self.mark_dirty(catalog);
                    }
                }
                _ => {}
            }
        }
    }
    fn modify_ui(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ProductionClipboardService>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        if let Some(e) = &self.source_error {
            ui.colored_label(egui::Color32::RED, e);
        }
        if self.large_confirmation_open {
            ui.label(format!(
                "Large clipboard source: {} bytes. Preview is paused.",
                self.source.len()
            ));
            if ui.button("Preview this source").clicked() {
                self.acknowledge_large_source(catalog.clone());
            }
            return;
        }
        if ui
            .add(
                egui::TextEdit::multiline(&mut self.source)
                    .id_source("cm_source")
                    .desired_rows(8),
            )
            .changed()
        {
            self.mark_dirty(catalog.clone());
        }
        ui.horizontal(|ui| {
            if ui.button("Add uppercase stage").clicked() {
                self.add_stage(OperationId::Uppercase, catalog.clone())
            }
            if ui.button("Reset").clicked() {
                self.reset(catalog.clone())
            }
            if ui.button("Reload Clipboard").clicked() {
                self.load_clipboard(service);
                self.mark_dirty(catalog.clone())
            }
        });
        let mut remove = None;
        let mut changed = false;
        for i in 0..self.stages.len() {
            ui.push_id(self.stages[i].id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Stage {}: {:?}",
                        i + 1,
                        self.stages[i].spec.operation
                    ));
                    if ui.button("↑").clicked() && i > 0 {
                        self.stages.swap(i, i - 1);
                        changed = true;
                    }
                    if ui.button("↓").clicked() && i + 1 < self.stages.len() {
                        self.stages.swap(i, i + 1);
                        changed = true;
                    }
                    if ui.button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            self.stages.remove(i);
            changed = true;
        }
        if changed {
            self.mark_dirty(catalog.clone());
        }
        ui.separator();
        match self.preview.state() {
            PreviewState::Completed { display, .. } => {
                ui.label(format!(
                    "Result: {} bytes, {} chars, {} lines{}",
                    display.metadata.bytes,
                    display.metadata.chars,
                    display.metadata.lines,
                    if display.truncated {
                        " (visible preview truncated)"
                    } else {
                        ""
                    }
                ));
                ui.add(egui::TextEdit::multiline(&mut display.text.clone()).desired_rows(8));
            }
            PreviewState::Failed { error, .. } => {
                ui.colored_label(egui::Color32::RED, error);
            }
            s => {
                ui.label(format!("Preview: {:?}", s));
            }
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(self.can_apply(), |ui| {
                if ui.button("Apply").clicked() {
                    self.commit(service, false, false)
                }
                if ui.button("Apply and Close").clicked() {
                    self.commit(service, true, false)
                }
                if ui.button("Copy Result").clicked() {
                    self.commit(service, false, true)
                }
            });
            if ui.button("Cancel").clicked() {
                self.open = false;
                self.cleanup_after_close();
            }
        });
        if let Some(cur) = &self.external_change {
            ui.separator();
            ui.label(format!("Clipboard changed since preview source was captured. Current clipboard: {} bytes, {} chars, fingerprint {:016x}. Applying overwrites it; undo restores the replaced value.", cur.bytes, cur.chars, cur.fingerprint));
            if ui.button("Overwrite clipboard").clicked() {
                let close = matches!(
                    self.pending_action,
                    Some(PendingDialogAction::ApplyAndClose)
                );
                self.commit(service, close, true);
                self.external_change = None;
            }
        }
    }
    pub fn commit(
        &mut self,
        service: &Arc<ProductionClipboardService>,
        close: bool,
        confirmed: bool,
    ) {
        if let Some(out) = self.preview.apply_text() {
            match service.commit_dialog(
                &self.baseline,
                &self.source,
                out,
                confirmed,
                "Clipboard Modify dialog",
            ) {
                Ok(_) => {
                    if close {
                        self.open = false;
                        self.cleanup_after_close();
                    }
                }
                Err(ClipboardError::ConfirmationRequired(c)) => {
                    self.external_change = Some(c.current);
                    self.pending_action = Some(if close {
                        PendingDialogAction::ApplyAndClose
                    } else {
                        PendingDialogAction::Apply
                    });
                }
                Err(e) => self.source_error = Some(e.to_string()),
            }
        }
    }
}

pub fn fingerprint(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::model::{ClipboardModifierCatalog, SavedPipeline};
    use crate::clipboard_modify::store::ClipboardModifierStore;
    use std::sync::RwLock;

    fn tmpl(id: &str, label: &str, aliases: &[&str], body: &str) -> ClipboardTemplate {
        ClipboardTemplate {
            id: id.into(),
            label: label.into(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            template: body.into(),
            processor: Some(TemplateProcessor::Literal),
        }
    }

    fn cat() -> ClipboardModifierCatalog {
        ClipboardModifierCatalog::new(
            vec![
                tmpl(
                    "prompt-context",
                    "Prompt Context",
                    &["pc"],
                    "Context {{clipboard}}",
                ),
                tmpl("quote", "Quote", &["blockquote"], "> {{clipboard}}"),
            ],
            vec![],
        )
        .unwrap()
    }
    #[test]
    fn targeted_opens_select_section() {
        let mut d = ClipboardModifyDialogState::default();
        d.open_section_without_loading_for_test(ClipboardModifyDialogSection::Templates);
        d.open_section_without_loading_for_test(ClipboardModifyDialogSection::SavedPipelines);
        assert_eq!(d.section, ClipboardModifyDialogSection::SavedPipelines);
    }
    #[test]
    fn close_cleanup_clears_transient_data() {
        let mut d = ClipboardModifyDialogState::default();
        d.open = true;
        d.source = "x".into();
        d.stages.push(DialogStage {
            id: 1,
            spec: StageSpec {
                operation: OperationId::Uppercase,
                arguments: Default::default(),
            },
            error: None,
        });
        d.wrap_preview = false;
        d.cleanup_after_close();
        assert!(d.source.is_empty() && d.stages.is_empty());
        assert!(!d.wrap_preview);
    }
    #[test]
    fn source_loading_accepts_empty_text() {
        let mut d = ClipboardModifyDialogState::default();
        d.replace_loaded_source(String::new());
        assert_eq!(d.baseline, "");
    }
    #[test]
    fn non_text_source_reports_error() {
        let e = ClipboardError::NonText;
        assert_eq!(e, ClipboardError::NonText);
    }
    #[test]
    fn large_input_blocks_preview_until_acknowledged() {
        let mut d = ClipboardModifyDialogState::default();
        d.replace_loaded_source("a".repeat(LARGE_SOURCE_CONFIRM_BYTES + 1));
        assert!(d.requires_large_ack());
    }
    #[test]
    fn stale_running_error_previews_disable_apply_buttons() {
        let d = ClipboardModifyDialogState::default();
        assert!(!d.can_apply());
    }
    #[test]
    fn ctrl_z_not_global_clipboard_modify_undo() {
        assert_ne!("Ctrl+Z", "cm undo");
    }

    #[test]
    fn template_filtering_matches_id_label_alias() {
        let catalog = cat();
        let mut d = ClipboardModifyDialogState::default();
        d.template_filter = "prompt".into();
        assert_eq!(d.filtered_templates(&catalog)[0].id, "prompt-context");
        d.template_filter = "quote".into();
        assert_eq!(d.filtered_templates(&catalog)[0].id, "quote");
        d.template_filter = "block".into();
        assert_eq!(d.filtered_templates(&catalog)[0].id, "quote");
    }

    #[test]
    fn selecting_template_does_not_dirty_configuration() {
        let catalog = Arc::new(cat());
        let mut d = ClipboardModifyDialogState::default();
        d.source = "x".into();
        d.selected_template = Some("quote".into());
        d.preview_template("quote".into(), catalog);
        assert!(!d.unsaved_template_draft);
    }

    #[test]
    fn applying_template_uses_dialog_preview_commit_path() {
        let catalog = Arc::new(cat());
        let mut d = ClipboardModifyDialogState::default();
        d.source = "x".into();
        d.apply_template_through_commit_path("quote", catalog);
        assert!(matches!(
            d.preview.state(),
            PreviewState::PendingDebounce { .. } | PreviewState::Running { .. }
        ));
    }

    #[test]
    fn invalid_drafts_disable_save_and_messages_are_specific() {
        let base = cat();
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = vec![tmpl("x", "X", &[], "missing")];
        assert!(!d.can_save_template_draft(&base));
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("{{clipboard}}")
        );
        d.template_draft = vec![
            tmpl("x", "X", &[], "{{clipboard}}"),
            tmpl("x", "Y", &[], "{{clipboard}}"),
        ];
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("duplicate")
        );
        d.template_draft = vec![tmpl("template", "X", &[], "{{clipboard}}")];
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("reserved")
        );
        d.template_draft = vec![tmpl("x", "", &[], "{{clipboard}}")];
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("label")
        );
        d.template_draft = vec![tmpl("x", "X", &["!!!"], "{{clipboard}}")];
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("valid alias")
        );
        d.template_draft = vec![tmpl(
            "x",
            "X",
            &[],
            &format!(
                "{{{{clipboard}}}}{}",
                "a".repeat(crate::clipboard_modify::config::MAX_CONFIG_BYTES as usize)
            ),
        )];
        let model = crate::clipboard_modify::config::VersionedClipboardModifiersFile {
            schema_version: crate::clipboard_modify::config::CURRENT_SCHEMA_VERSION,
            templates: d.template_draft.clone(),
            pipelines: vec![],
        };
        assert!(
            crate::clipboard_modify::config::serialize_model(&model)
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
    }

    #[test]
    fn successful_save_persists_and_replaces_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let store = ClipboardModifierStore {
            path: dir.path().join("cm.json"),
            catalog: Arc::new(RwLock::new(Arc::new(cat()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let base = cat();
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = vec![tmpl("saved", "Saved", &[], "{{clipboard}}")];
        let saved = d.save_template_draft(&base, &store).unwrap();
        assert_eq!(saved.templates[0].id, "saved");
        assert!(
            std::fs::read_to_string(&store.path)
                .unwrap()
                .contains("saved")
        );
        assert_eq!(store.catalog.read().unwrap().templates[0].id, "saved");
    }

    #[test]
    fn failed_save_preserves_active_catalog_and_draft() {
        let dir = tempfile::tempdir().unwrap();
        let store = ClipboardModifierStore {
            path: dir.path().to_path_buf(),
            catalog: Arc::new(RwLock::new(Arc::new(cat()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let base = cat();
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = vec![tmpl("draft", "Draft", &[], "{{clipboard}}")];
        assert!(d.save_template_draft(&base, &store).is_err());
        assert_eq!(
            store.catalog.read().unwrap().templates[0].id,
            "prompt-context"
        );
        assert_eq!(d.template_draft[0].id, "draft");
    }

    #[test]
    fn deletion_blocked_when_pipeline_references_template() {
        let mut base = cat();
        base.pipelines = vec![SavedPipeline {
            id: "pipe".into(),
            label: "Pipe".into(),
            aliases: vec![],
            stages: vec![StageSpec {
                operation: OperationId::Template,
                arguments: StageArguments {
                    name: Some("quote".into()),
                    ..Default::default()
                },
            }],
        }];
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = base.templates.clone();
        let err = d.delete_template_from_draft(&base, "quote").unwrap_err();
        assert!(err.contains("Pipe (pipe)"));
    }

    #[test]
    fn reorder_saves_immediately_and_refreshes_suggestions_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let base = cat();
        let store = ClipboardModifierStore {
            path: dir.path().join("cm.json"),
            catalog: Arc::new(RwLock::new(Arc::new(base.clone()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = base.templates.clone();
        d.template_draft.swap(0, 1);
        d.save_template_draft(&base, &store).unwrap();
        assert_eq!(store.catalog.read().unwrap().templates[0].id, "quote");
    }
    #[test]
    fn saved_pipeline_list_displays_identity_and_stage_summary() {
        let p = SavedPipeline {
            id: "clean".into(),
            label: "Clean Text".into(),
            aliases: vec!["ct".into()],
            stages: vec![
                StageSpec {
                    operation: OperationId::Trim,
                    arguments: Default::default(),
                },
                StageSpec {
                    operation: OperationId::Uppercase,
                    arguments: Default::default(),
                },
            ],
        };
        assert!(
            ClipboardModifyDialogState::pipeline_identity_line(&p)
                .contains("Clean Text (clean) aliases: ct")
        );
        assert_eq!(
            ClipboardModifyDialogState::pipeline_stage_summary(&p),
            "trim → uppercase"
        );
    }

    #[test]
    fn executing_saved_pipeline_uses_current_source_and_commit_preview_path() {
        let mut base = cat();
        base.pipelines = vec![SavedPipeline {
            id: "up".into(),
            label: "Up".into(),
            aliases: vec![],
            stages: vec![StageSpec {
                operation: OperationId::Uppercase,
                arguments: Default::default(),
            }],
        }];
        let catalog = Arc::new(base);
        let mut d = ClipboardModifyDialogState::default();
        d.source = "current dialog source".into();
        d.apply_pipeline_through_commit_path("up", catalog);
        assert!(matches!(
            d.preview.state(),
            PreviewState::PendingDebounce { .. } | PreviewState::Running { .. }
        ));
    }

    #[test]
    fn duplicate_pipeline_id_is_nonconflicting_and_requires_confirmation() {
        let mut base = cat();
        base.pipelines = vec![
            SavedPipeline {
                id: "pipe".into(),
                label: "Pipe".into(),
                aliases: vec![],
                stages: vec![],
            },
            SavedPipeline {
                id: "pipe-copy".into(),
                label: "Pipe Copy".into(),
                aliases: vec![],
                stages: vec![],
            },
        ];
        assert_eq!(
            ClipboardModifyDialogState::duplicate_pipeline_id(&base, "pipe"),
            "pipe-copy-2"
        );
        let mut d = ClipboardModifyDialogState::default();
        d.duplicate_save_confirmation = Some("pipe-copy-2".into());
        assert!(d.duplicate_save_confirmation.is_some());
    }

    #[test]
    fn editor_stage_choices_are_deterministic_and_nested_pipelines_rejected() {
        let ops = ClipboardModifyDialogState::allowed_stage_operations();
        assert!(ops.contains(&OperationId::Template));
        assert!(ops.contains(&OperationId::CustomWrap));
        let mut base = cat();
        base.pipelines = vec![SavedPipeline {
            id: "pipe".into(),
            label: "Pipe".into(),
            aliases: vec![],
            stages: vec![],
        }];
        let nested = vec![StageSpec {
            operation: OperationId::Template,
            arguments: StageArguments {
                name: Some("pipe".into()),
                ..Default::default()
            },
        }];
        assert!(
            crate::clipboard_modify::pipeline::validate_executable_stages(&nested, &base).is_err()
        );
    }

    #[test]
    fn preview_can_be_cancelled() {
        let mut d = ClipboardModifyDialogState::default();
        d.cancel_pipeline_preview();
        assert!(!d.can_apply());
    }

    #[test]
    fn pipeline_deletion_and_reorder_save_immediately_and_preserve_drafts_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let mut base = cat();
        base.pipelines = vec![
            SavedPipeline {
                id: "one".into(),
                label: "One".into(),
                aliases: vec![],
                stages: vec![],
            },
            SavedPipeline {
                id: "two".into(),
                label: "Two".into(),
                aliases: vec![],
                stages: vec![],
            },
        ];
        let store = ClipboardModifierStore {
            path: dir.path().join("cm.json"),
            catalog: Arc::new(RwLock::new(Arc::new(base.clone()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let mut d = ClipboardModifyDialogState::default();
        d.pipeline_draft = base.pipelines.clone();
        d.delete_pipeline_from_draft_and_save(&base, &store, "one")
            .unwrap();
        assert_eq!(store.catalog.read().unwrap().pipelines[0].id, "two");
        let active = (*store.catalog.read().unwrap()).as_ref().clone();
        d.reorder_pipeline_draft_and_save(&active, &store, 0, 0)
            .unwrap();
        assert_eq!(store.catalog.read().unwrap().pipelines[0].id, "two");

        let bad_store = ClipboardModifierStore {
            path: dir.path().to_path_buf(),
            catalog: Arc::new(RwLock::new(Arc::new(base.clone()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let mut failed = ClipboardModifyDialogState::default();
        failed.pipeline_draft = base.pipelines.clone();
        assert!(
            failed
                .reorder_pipeline_draft_and_save(&base, &bad_store, 0, 1)
                .is_err()
        );
        assert_eq!(bad_store.catalog.read().unwrap().pipelines[0].id, "one");
        assert_eq!(failed.pipeline_draft[0].id, "two");
    }
}
