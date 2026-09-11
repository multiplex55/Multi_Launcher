use super::MkMacroDialog;
use crate::{
    common::atomic_file::save_atomic,
    mkmacro::{
        MkMacroTemplateCatalog, PackageImportPlan, export_package, plan_package_import,
        plan_template_instantiation,
    },
};
use eframe::egui;
use std::{collections::BTreeSet, fs};

#[derive(Default)]
pub(super) struct PackageUiState {
    modal: Option<PackageModal>,
}

enum PackageModal {
    ExportLibrary {
        roots: BTreeSet<u64>,
    },
    ImportPreview {
        source: String,
        plan: PackageImportPlan,
    },
    SaveTemplate {
        name: String,
        description: String,
    },
    Templates {
        catalog: MkMacroTemplateCatalog,
        selected: Option<u64>,
        preview: Option<PackageImportPlan>,
    },
    Help,
}

impl PackageUiState {
    pub(super) fn is_open(&self) -> bool {
        self.modal.is_some()
    }

    pub(super) fn close(&mut self) {
        self.modal = None;
    }
}

pub(super) fn show_toolbar_menu(ui: &mut egui::Ui, dialog: &mut MkMacroDialog) {
    let selected = dialog.selected_macro_id;
    ui.menu_button("Reuse", |ui| {
        if ui
            .add_enabled(
                selected.is_some(),
                egui::Button::new("Export Selected Macro…"),
            )
            .clicked()
        {
            export_selected(dialog);
            ui.close_menu();
        }
        if ui
            .add_enabled(
                !dialog.draft.macros.is_empty(),
                egui::Button::new("Export Library…"),
            )
            .clicked()
        {
            dialog.package_ui.modal = Some(PackageModal::ExportLibrary {
                roots: selected.into_iter().collect(),
            });
            ui.close_menu();
        }
        if ui.button("Import Library…").clicked() {
            begin_import(dialog);
            ui.close_menu();
        }
        ui.separator();
        if ui
            .add_enabled(selected.is_some(), egui::Button::new("Save as Template…"))
            .clicked()
        {
            if let Some(macro_) = dialog.selected_macro() {
                dialog.package_ui.modal = Some(PackageModal::SaveTemplate {
                    name: macro_.name.clone(),
                    description: macro_.description.clone(),
                });
            }
            ui.close_menu();
        }
        if ui.button("New from Template…").clicked() {
            open_templates(dialog);
            ui.close_menu();
        }
        ui.separator();
        if ui.button("MkMacro Help").clicked() {
            dialog.package_ui.modal = Some(PackageModal::Help);
            ui.close_menu();
        }
    });
}

fn export_selected(dialog: &mut MkMacroDialog) {
    let Some(root) = dialog.selected_macro_id else {
        return;
    };
    let default_name = dialog
        .selected_macro()
        .map(|macro_| package_filename(&macro_.name))
        .unwrap_or_else(|| "macro.mkmacro".into());
    let Some(path) = rfd::FileDialog::new()
        .add_filter("MkMacro package", &["mkmacro"])
        .set_file_name(&default_name)
        .save_file()
    else {
        return;
    };
    let result = export_package(&dialog.store, &dialog.draft, &[root])
        .and_then(|bytes| save_atomic(&path, &bytes));
    report(dialog, result.map(|_| ()), "export macro");
}

fn begin_import(dialog: &mut MkMacroDialog) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("MkMacro package", &["mkmacro", "json"])
        .pick_file()
    else {
        return;
    };
    let result = fs::read(&path).and_then(|bytes| {
        plan_package_import(
            &dialog.store,
            &bytes,
            &dialog.draft,
            dialog.draft_revision,
            &dialog.baseline,
        )
        .map_err(std::io::Error::other)
    });
    match result {
        Ok(plan) => {
            dialog.package_ui.modal = Some(PackageModal::ImportPreview {
                source: path.display().to_string(),
                plan,
            });
            dialog.command_error = None;
        }
        Err(error) => dialog.command_error = Some(format!("Could not preview import: {error}")),
    }
}

fn open_templates(dialog: &mut MkMacroDialog) {
    match dialog.store.load_template_catalog() {
        Ok(catalog) => {
            dialog.package_ui.modal = Some(PackageModal::Templates {
                catalog,
                selected: None,
                preview: None,
            });
            dialog.command_error = None;
        }
        Err(error) => {
            dialog.command_error = Some(format!("Could not load macro templates: {error}"))
        }
    }
}

pub(super) fn show(ctx: &egui::Context, dialog: &mut MkMacroDialog) {
    let Some(mut modal) = dialog.package_ui.modal.take() else {
        return;
    };
    let mut open = true;
    let mut keep = true;
    egui::Window::new(modal.title())
        .id(egui::Id::new("mkmacro_package_modal"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(430.0)
                .show(ui, |ui| match &mut modal {
                    PackageModal::ExportLibrary { roots } => {
                        ui.label("Choose explicit library roots. Every authored Call dependency, including disabled steps, is packaged automatically.");
                        ui.separator();
                        for macro_ in &dialog.draft.macros {
                            let mut included = roots.contains(&macro_.id);
                            if ui.checkbox(&mut included, &macro_.name).changed() {
                                if included {
                                    roots.insert(macro_.id);
                                } else {
                                    roots.remove(&macro_.id);
                                }
                            }
                        }
                    }
                    PackageModal::ImportPreview { source, plan } => {
                        ui.label(format!("Source: {source}"));
                        import_policy(ui, plan, false);
                    }
                    PackageModal::SaveTemplate { name, description } => {
                        ui.label("The selected macro and its complete authored dependency closure are captured from the current draft. Later edits do not change the template.");
                        ui.label("Template name");
                        ui.text_edit_singleline(name);
                        ui.label("Description");
                        ui.text_edit_multiline(description);
                    }
                    PackageModal::Templates { catalog, selected, preview } => {
                        if catalog.templates.is_empty() {
                            ui.label("No saved macro templates.");
                        }
                        for template in &catalog.templates {
                            let response = ui.selectable_label(
                                *selected == Some(template.id),
                                &template.name,
                            );
                            if response.clicked() {
                                *selected = Some(template.id);
                                *preview = None;
                            }
                            if *selected == Some(template.id) && !template.description.is_empty() {
                                ui.small(&template.description);
                            }
                        }
                        if let Some(plan) = preview {
                            ui.separator();
                            import_policy(ui, plan, true);
                        }
                    }
                    PackageModal::Help => help(ui),
                });
            ui.separator();
            ui.horizontal(|ui| match &mut modal {
                PackageModal::ExportLibrary { roots } => {
                    if ui.add_enabled(!roots.is_empty(), egui::Button::new("Export…")).clicked() {
                        let root_ids = roots.iter().copied().collect::<Vec<_>>();
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("MkMacro package", &["mkmacro"])
                            .set_file_name("macro-library.mkmacro")
                            .save_file()
                        {
                            let result = export_package(&dialog.store, &dialog.draft, &root_ids)
                                .and_then(|bytes| save_atomic(&path, &bytes));
                            if result.is_ok() {
                                keep = false;
                            }
                            report(dialog, result.map(|_| ()), "export library");
                        }
                    }
                    if ui.button("Cancel").clicked() { keep = false; }
                }
                PackageModal::ImportPreview { plan, .. } => {
                    if ui.button("Apply Import").clicked() {
                        if apply_plan(dialog, plan).is_ok() { keep = false; }
                    }
                    if ui.button("Cancel").clicked() { keep = false; }
                }
                PackageModal::SaveTemplate { name, description } => {
                    let enabled = !name.trim().is_empty() && dialog.selected_macro_id.is_some();
                    if ui.add_enabled(enabled, egui::Button::new("Save Template")).clicked() {
                        let result = dialog.store.save_macro_template(
                            &dialog.draft,
                            dialog.selected_macro_id.unwrap(),
                            name,
                            description,
                        );
                        if result.is_ok() { keep = false; }
                        report(dialog, result.map(|_| ()), "save template");
                    }
                    if ui.button("Cancel").clicked() { keep = false; }
                }
                PackageModal::Templates { catalog, selected, preview } => {
                    if preview.is_none() {
                        if ui.add_enabled(selected.is_some(), egui::Button::new("Preview Copy")).clicked()
                            && let Some(template) = selected.and_then(|id| catalog.templates.iter().find(|t| t.id == id))
                        {
                            match plan_template_instantiation(
                                &dialog.store,
                                template,
                                &dialog.draft,
                                dialog.draft_revision,
                                &dialog.baseline,
                            ) {
                                Ok(plan) => *preview = Some(plan),
                                Err(error) => dialog.command_error = Some(format!("Could not preview template: {error}")),
                            }
                        }
                    } else if ui.button("Create Independent Copy").clicked() {
                        if apply_plan(dialog, preview.as_ref().unwrap()).is_ok() { keep = false; }
                    }
                    if ui.button("Cancel").clicked() { keep = false; }
                }
                PackageModal::Help => {
                    if ui.button("Close").clicked() { keep = false; }
                }
            });
        });
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        keep = false;
    }
    if open && keep {
        dialog.package_ui.modal = Some(modal);
    }
}

impl PackageModal {
    fn title(&self) -> &'static str {
        match self {
            Self::ExportLibrary { .. } => "Export Macro Library",
            Self::ImportPreview { .. } => "Import Macro Library",
            Self::SaveTemplate { .. } => "Save Macro Template",
            Self::Templates { .. } => "New Macro from Template",
            Self::Help => "MkMacro Help",
        }
    }
}

fn apply_plan(dialog: &mut MkMacroDialog, plan: &PackageImportPlan) -> anyhow::Result<()> {
    match plan.apply(&dialog.store, &dialog.draft, dialog.draft_revision) {
        Ok(snapshot) => {
            let selected = plan.imported_root_ids.first().copied();
            dialog.draft = (*snapshot).clone();
            dialog.record_draft_revision();
            dialog.baseline = snapshot;
            dialog.dirty = false;
            dialog.conflict = false;
            dialog.set_selected_macro(selected);
            dialog.selection.clear();
            dialog.refresh_environment();
            dialog.command_error = None;
            Ok(())
        }
        Err(error) => {
            dialog.command_error = Some(format!("Could not apply package: {error}"));
            Err(error)
        }
    }
}

fn import_policy(ui: &mut egui::Ui, plan: &PackageImportPlan, template: bool) {
    let summary = &plan.summary;
    ui.heading("Preview");
    ui.label(format!(
        "Add {} macros ({} roots), {} dependency links, {} folders, and {} new images; reuse {} images.",
        summary.added_macros,
        plan.imported_root_ids.len(),
        summary.added_dependencies,
        summary.added_folders,
        summary.added_images,
        summary.reused_images,
    ));
    ui.label(format!(
        "Rename {} macros, {} folders, and {} images to resolve conflicts.",
        summary.renamed_macros, summary.renamed_folders, summary.renamed_images
    ));
    if template {
        ui.label("Policy: create fresh IDs and independent assets/dependencies; clear all copied hotkeys.");
    } else {
        ui.label("Policy: create fresh IDs and independent assets/dependencies; retain imported authored hotkeys.");
        let conflicts = crate::mkmacro::hotkeys::validate_hotkeys(plan.candidate(), &[]);
        if conflicts.is_empty() {
            ui.label("Hotkey conflicts after import: none detected.");
        } else {
            ui.colored_label(ui.visuals().warn_fg_color, "Hotkey conflicts after import:");
            for conflict in conflicts {
                ui.label(format!("• {}", conflict.message));
            }
        }
    }
    for warning in &summary.warnings {
        ui.colored_label(ui.visuals().warn_fg_color, warning);
    }
    ui.small("Apply saves the current draft and imported content together. Cancel leaves the draft and files unchanged. If the draft or persisted file changes, Apply is rejected and a new preview is required.");
}

fn help(ui: &mut egui::Ui) {
    ui.heading("Recording");
    ui.label("Record captures into a transient Review instead of changing or saving the macro. Pause/Resume, Marker, and Annotate preserve the capture timeline; Stop finalizes on a worker and opens Review. Play All and Play Selected Range run the proposal ephemerally through the normal runtime, and Apply is the only operation that inserts reviewed actions.");
    ui.label("Cleanup can fold physical key transitions into taps, holds, chords, and layout-aware text; simplify mouse movement/clicks; retain useful delays; and propose window, launch, repeat, clipboard-freeze, and clicked-control transformations. Suggestions stay reviewable and can be disabled.");
    ui.label("Use physical Key actions for shortcuts, navigation, sided modifiers, and scan-code-sensitive input. Use Unicode Text for characters/content. Clipboard and UI inspection observations are transient and clear when Review closes.");
    ui.label("Send Keys exposes Key Press, Key Down, Key Up, Hotkey, and Text. Use Hotkey for Ctrl+V, Ctrl+Shift+S, or Shift+F1; use Key Down Shift and a later Key Up Shift for an explicit hold; use Text for Unicode content.");
    ui.heading("Editing");
    ui.label("Ctrl+C / Ctrl+X / Ctrl+V copy, cut, and paste complete structured steps. Ctrl+D duplicates with fresh step IDs. Drag the primary selected row to reorder; all selected rows move together.");
    ui.label("Ctrl+F finds fields, Ctrl+H performs schema-aware replacement, and Ctrl+G jumps to a step. Fold block rows, add labels/comments/accent colors, bookmark steps, and use the Outline to navigate structure.");
    ui.heading("Reusable macros");
    ui.label("Parameters and named outputs are typed. Call Macro binds arguments explicitly and maps outputs explicitly; Return publishes declared outputs. Each call gets an isolated local-variable frame, while nested calls remain visible in the Runtime Inspector call stack.");
    ui.label("Calls use stable macro and signature IDs, so renaming or reordering does not break them. Recursive dependency cycles are prohibited. Breakpoints in called macros pause the same debug run.");
    ui.heading("Packages, libraries, and templates");
    ui.label("Export packages include every authored Call dependency and referenced image. A library has explicit root macros. Import always previews additions, renames, images, and hotkey conflicts before an explicit Apply.");
    ui.label("Imports are independent copies with fresh macro, folder, step, parameter, output, and asset identities. Imported authored hotkeys are retained; template instances clear every copied hotkey. Templates and all instances are snapshots—there are no live links.");
}

fn package_filename(name: &str) -> String {
    let stem = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    format!("{}.mkmacro", if stem.is_empty() { "macro" } else { &stem })
}

fn report(dialog: &mut MkMacroDialog, result: anyhow::Result<()>, operation: &str) {
    match result {
        Ok(()) => dialog.command_error = None,
        Err(error) => dialog.command_error = Some(format!("Could not {operation}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkMacro, MkMacroDocument, MkMacroStore};

    fn source_document() -> MkMacroDocument {
        MkMacroDocument {
            macros: vec![MkMacro {
                id: 9,
                name: "Imported".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                signature: Default::default(),
                steps: vec![],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn package_filename_is_safe_and_has_expected_extension() {
        assert_eq!(
            package_filename("My useful macro"),
            "My-useful-macro.mkmacro"
        );
        assert_eq!(package_filename("***"), "macro.mkmacro");
    }

    #[test]
    fn import_preview_cancel_is_lossless_and_apply_adopts_exact_store_snapshot() {
        let source_dir = tempfile::tempdir().unwrap();
        let (source_store, _) = MkMacroStore::open(source_dir.path()).unwrap();
        let bytes = export_package(&source_store, &source_document(), &[9]).unwrap();

        let destination_dir = tempfile::tempdir().unwrap();
        let (destination_store, _) = MkMacroStore::open(destination_dir.path()).unwrap();
        let mut dialog = MkMacroDialog::new(std::sync::Arc::new(destination_store));
        dialog.create_macro();
        let draft_before = dialog.draft.clone();
        let revision = dialog.draft_revision;
        let persisted = dialog.baseline.clone();
        let plan = plan_package_import(&dialog.store, &bytes, &dialog.draft, revision, &persisted)
            .unwrap();
        dialog.package_ui.modal = Some(PackageModal::ImportPreview {
            source: "test.mkmacro".into(),
            plan,
        });
        dialog.package_ui.close();
        assert_eq!(dialog.draft, draft_before);
        assert!(dialog.dirty);
        assert!(dialog.store.snapshot().macros.is_empty());

        let plan = plan_package_import(&dialog.store, &bytes, &dialog.draft, revision, &persisted)
            .unwrap();
        apply_plan(&mut dialog, &plan).unwrap();
        assert_eq!(dialog.draft.macros.len(), 2);
        assert_eq!(dialog.draft, *dialog.store.snapshot());
        assert_eq!(dialog.draft, *dialog.baseline);
        assert!(!dialog.dirty);
        assert!(!dialog.conflict);
        assert_eq!(
            dialog.selected_macro_id,
            plan.imported_root_ids.first().copied()
        );
    }
}
