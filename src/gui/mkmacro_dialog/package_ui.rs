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
    #[cfg(test)]
    test_body_scroll: Option<(egui::Id, egui::Rect, egui::Vec2, egui::Vec2)>,
    #[cfg(test)]
    test_footer_clip: Option<egui::Rect>,
    #[cfg(test)]
    test_close_response: Option<(egui::Id, egui::Rect)>,
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
            ui.close();
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
            ui.close();
        }
        if ui.button("Import Library…").clicked() {
            begin_import(dialog);
            ui.close();
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
            ui.close();
        }
        if ui.button("New from Template…").clicked() {
            open_templates(dialog);
            ui.close();
        }
        ui.separator();
        if ui.button("MkMacro Help").clicked() {
            dialog.package_ui.modal = Some(PackageModal::Help);
            ui.close();
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

fn footer_labels(modal: &PackageModal) -> &'static [&'static str] {
    match modal {
        PackageModal::ExportLibrary { .. } => &["Export…", "Cancel"],
        PackageModal::ImportPreview { .. } => &["Apply Import", "Cancel"],
        PackageModal::SaveTemplate { .. } => &["Save Template", "Cancel"],
        PackageModal::Templates { preview, .. } if preview.is_some() => {
            &["Create Independent Copy", "Cancel"]
        }
        PackageModal::Templates { .. } => &["Preview Copy", "Cancel"],
        PackageModal::Help => &["Close"],
    }
}

fn package_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let button_frame = native_button_style.frame;
    let frame_margin = if ui.visuals().button_frame {
        button_frame.total_margin().sum()
    } else {
        egui::Vec2::ZERO
    };
    let available_width = ui.available_width().max(1.0);
    let text_width = (available_width - frame_margin.x).max(1.0);
    let wrap_mode = ui.wrap_mode();
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    let mut row_width = 0.0;
    let mut row_height = 0.0_f32;
    let mut actions_height = 0.0;

    for label in labels {
        let text_size = egui::WidgetText::from(*label)
            .into_galley(ui, Some(wrap_mode), text_width, button_font.clone())
            .size();
        let button_width = (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width);
        let button_height = (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            actions_height += row_height + spacing.item_spacing.y;
            row_width = button_width;
            row_height = button_height;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
            row_height = row_height.max(button_height);
        }
    }
    actions_height += row_height.max(spacing.interact_size.y);
    let separator_spacing = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let footer_height = actions_height + 2.0 * spacing.item_spacing.y + separator_spacing;
    (footer_height * pixels_per_point).ceil() / pixels_per_point
}

pub(super) fn show(ctx: &egui::Context, dialog: &mut MkMacroDialog) {
    let Some(mut modal) = dialog.package_ui.modal.take() else {
        return;
    };
    let mut open = true;
    let mut keep = true;
    let labels = footer_labels(&modal);
    egui::Window::new(modal.title())
        .id(egui::Id::new("mkmacro_package_modal"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            let footer_height = package_footer_reserved_height(ui, labels);
            let body_max_height = (ui.available_height() - footer_height).max(0.0);
            let body_output = egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .min_scrolled_height(0.0)
                .max_height(body_max_height)
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
            #[cfg(test)]
            {
                dialog.package_ui.test_body_scroll = Some((
                    body_output.id,
                    body_output.inner_rect,
                    body_output.content_size,
                    body_output.state.offset,
                ));
            }
            #[cfg(not(test))]
            let _ = body_output;
            ui.separator();
            #[cfg(test)]
            {
                dialog.package_ui.test_footer_clip = Some(ui.clip_rect());
            }
            let mut close_response = None;
            ui.horizontal_wrapped(|ui| match &mut modal {
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
                    if ui.button("Apply Import").clicked() && apply_plan(dialog, plan).is_ok() {
                        keep = false;
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
                    } else if ui.button("Create Independent Copy").clicked()
                        && apply_plan(dialog, preview.as_ref().unwrap()).is_ok()
                    {
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() { keep = false; }
                }
                PackageModal::Help => {
                    let response = ui.button("Close");
                    close_response = Some((response.id, response.rect));
                    if response.clicked() { keep = false; }
                }
            });
            #[cfg(test)]
            {
                dialog.package_ui.test_close_response = close_response;
            }
            #[cfg(not(test))]
            let _ = close_response;
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
    ui.label("In the Signature Editor, String parameters can use Text, File picker, Folder picker, or an ordered Choice list with an optional default. Browse fills a file or folder path, and selecting a choice fills its value; neither runs the macro. Review or edit the invocation form, then choose Run separately. If a chooser is canceled or unavailable, its path remains editable.");
    ui.label("Calls use stable macro and signature IDs, so renaming or reordering does not break them. Recursive dependency cycles are prohibited. Breakpoints in called macros pause the same debug run.");
    ui.heading("Clipboard actions");
    ui.label("Read Clipboard Text reads the current text into a named String variable when the worker reaches that step. Write Clipboard Text publishes its exact literal or String variable at that step; neither action pastes text or restores earlier clipboard contents.");
    ui.label("Busy or non-text clipboard failures follow the step's Stop, Continue, or Retry policy. Clipboard actions are unsupported in unattended Hot Folder runs.");
    ui.label("Runs whose enabled reachable macro closure contains either action hide runtime variable values and sanitize dependent failure details. Ordinary macros keep their normal debug values and failure details.");
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
    fn mkmacro_package_chrome_keeps_long_help_body_scrolling_above_close_without_mutation() {
        use egui::accesskit::{NodeId, Role};

        fn response_node_bounds(
            output: &egui::FullOutput,
            response: Option<(egui::Id, egui::Rect)>,
            label: &str,
        ) -> Option<egui::Rect> {
            let (response_id, _) = response?;
            let node_id = NodeId(response_id.value());
            let (_, node) = output
                .platform_output
                .accesskit_update
                .as_ref()?
                .nodes
                .iter()
                .find(|(id, _)| *id == node_id)?;
            if node.role() != Role::Button || node.label() != Some(label) {
                return None;
            }
            let bounds = node.bounds()?;
            Some(egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            ))
        }

        fn named_node_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
            output
                .platform_output
                .accesskit_update
                .as_ref()?
                .nodes
                .iter()
                .find(|(_, node)| node.value() == Some(label))
                .and_then(|(_, node)| {
                    let bounds = node.bounds()?;
                    Some(egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ))
                })
        }

        fn same_rect(left: egui::Rect, right: egui::Rect) -> bool {
            (left.min.x - right.min.x).abs() < 0.5
                && (left.min.y - right.min.y).abs() < 0.5
                && (left.max.x - right.max.x).abs() < 0.5
                && (left.max.y - right.max.y).abs() < 0.5
        }

        fn collect_text_shapes(
            shape: &egui::epaint::Shape,
            clip_rect: egui::Rect,
            output: &mut Vec<(String, egui::Rect, egui::Rect, bool)>,
        ) {
            match shape {
                egui::epaint::Shape::Text(text) => output.push((
                    text.galley.job.text.clone(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                    clip_rect,
                    text.galley.elided,
                )),
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text_shapes(shape, clip_rect, output);
                    }
                }
                _ => {}
            }
        }

        fn trailing_help_text_shape(
            output: &egui::FullOutput,
            expected: &str,
        ) -> Option<(egui::Rect, egui::Rect, bool)> {
            let mut texts = Vec::new();
            for clipped_shape in &output.shapes {
                collect_text_shapes(&clipped_shape.shape, clipped_shape.clip_rect, &mut texts);
            }
            texts
                .into_iter()
                .find(|(text, _, _, _)| text == expected)
                .map(|(_, text_rect, clip_rect, elided)| (text_rect, clip_rect, elided))
        }

        fn same_text_shape(
            left: (egui::Rect, egui::Rect, bool),
            right: (egui::Rect, egui::Rect, bool),
        ) -> bool {
            same_rect(left.0, right.0) && same_rect(left.1, right.1) && left.2 == right.2
        }

        const TRAILING_HELP_TEXT: &str = "Imports are independent copies with fresh macro, folder, step, parameter, output, and asset identities. Imported authored hotkeys are retained; template instances clear every copied hotkey. Templates and all instances are snapshots—there are no live links.";

        #[derive(Clone, Copy)]
        struct Layout {
            body_scroll: (egui::Id, egui::Rect, egui::Vec2, egui::Vec2),
            footer_clip: egui::Rect,
            close_response: (egui::Id, egui::Rect),
            close_node: egui::Rect,
            heading_node: egui::Rect,
        }

        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let mut dialog = MkMacroDialog::new(std::sync::Arc::new(store));
        dialog.package_ui.modal = Some(PackageModal::Help);
        let draft_before = dialog.draft.clone();
        let baseline_before = dialog.baseline.clone();
        let persisted_before = dialog.store.snapshot();
        let revision_before = dialog.draft_revision;
        let dirty_before = dialog.dirty;
        let conflict_before = dialog.conflict;
        let selected_before = dialog.selected_macro_id;

        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut_of(context.theme(), |style| {
            style.override_font_id = Some(egui::FontId::new(22.0, egui::FontFamily::Proportional));
        });
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(520.0, 380.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };

        let mut previous_layout: Option<Layout> = None;
        let mut settled_layout: Option<Layout> = None;
        let mut last_close_node: Option<(egui::Id, egui::Rect)> = None;
        let mut last_heading_node: Option<egui::Rect> = None;
        for _ in 0..6 {
            let output = context.run_ui(input(), |ctx| show(ctx, &mut dialog));
            let close_response = dialog.package_ui.test_close_response;
            if let Some(response) = close_response
                && let Some(bounds) = response_node_bounds(&output, Some(response), "Close")
            {
                last_close_node = Some((response.0, bounds));
            }
            if let Some(bounds) = named_node_bounds(&output, "Recording") {
                last_heading_node = Some(bounds);
            }
            let layout = match (
                dialog.package_ui.test_body_scroll,
                dialog.package_ui.test_footer_clip,
                close_response,
                last_close_node,
                last_heading_node,
            ) {
                (
                    Some(body_scroll),
                    Some(footer_clip),
                    Some(close_response),
                    Some((close_id, close_node)),
                    Some(heading_node),
                ) if close_id == close_response.0 => Some(Layout {
                    body_scroll,
                    footer_clip,
                    close_response,
                    close_node,
                    heading_node,
                }),
                _ => None,
            };
            if let (Some(previous), Some(current)) = (previous_layout, layout)
                && previous.body_scroll.0 == current.body_scroll.0
                && same_rect(previous.body_scroll.1, current.body_scroll.1)
                && (previous.body_scroll.2 - current.body_scroll.2).length() < 0.5
                && (previous.body_scroll.3 - current.body_scroll.3).length() < 0.5
                && same_rect(previous.footer_clip, current.footer_clip)
                && same_rect(previous.close_response.1, current.close_response.1)
                && same_rect(previous.close_node, current.close_node)
                && same_rect(previous.heading_node, current.heading_node)
            {
                settled_layout = Some(current);
            }
            previous_layout = layout;
            output.drop_without_applying_deltas();
        }

        let initial_layout = settled_layout;
        let mut previous_scrolled_layout: Option<Layout> = None;
        let mut settled_scrolled_layout: Option<Layout> = None;
        let mut previous_trailing_text: Option<(egui::Rect, egui::Rect, bool)> = None;
        let mut settled_trailing_text: Option<(egui::Rect, egui::Rect, bool)> = None;
        let mut scroll_state_matches = false;
        if let Some(initial) = initial_layout {
            let (scroll_id, viewport, content_size, _) = initial.body_scroll;
            if let Some(mut scroll_state) = egui::scroll_area::State::load(&context, scroll_id) {
                let max_scroll = (content_size.y - viewport.height()).max(0.0);
                scroll_state.offset.y = max_scroll;
                scroll_state.store(&context, scroll_id);
                for _ in 0..5 {
                    let output = context.run_ui(input(), |ctx| show(ctx, &mut dialog));
                    let close_response = dialog.package_ui.test_close_response;
                    if let Some(response) = close_response
                        && let Some(bounds) = response_node_bounds(&output, Some(response), "Close")
                    {
                        last_close_node = Some((response.0, bounds));
                    }
                    if let Some(bounds) = named_node_bounds(&output, "Recording") {
                        last_heading_node = Some(bounds);
                    }
                    let layout = match (
                        dialog.package_ui.test_body_scroll,
                        dialog.package_ui.test_footer_clip,
                        close_response,
                        last_close_node,
                        last_heading_node,
                    ) {
                        (
                            Some(body_scroll),
                            Some(footer_clip),
                            Some(close_response),
                            Some((close_id, close_node)),
                            Some(heading_node),
                        ) if close_id == close_response.0 => Some(Layout {
                            body_scroll,
                            footer_clip,
                            close_response,
                            close_node,
                            heading_node,
                        }),
                        _ => None,
                    };
                    let trailing_text = trailing_help_text_shape(&output, TRAILING_HELP_TEXT);
                    if let (Some(previous), Some(current), Some(previous_text), Some(current_text)) = (
                        previous_scrolled_layout,
                        layout,
                        previous_trailing_text,
                        trailing_text,
                    ) && same_rect(previous.body_scroll.1, current.body_scroll.1)
                        && (previous.body_scroll.2 - current.body_scroll.2).length() < 0.5
                        && (previous.body_scroll.3 - current.body_scroll.3).length() < 0.5
                        && same_rect(previous.footer_clip, current.footer_clip)
                        && same_rect(previous.close_response.1, current.close_response.1)
                        && same_rect(previous.close_node, current.close_node)
                        && same_text_shape(previous_text, current_text)
                    {
                        settled_scrolled_layout = Some(current);
                        settled_trailing_text = Some(current_text);
                    }
                    previous_scrolled_layout = layout;
                    previous_trailing_text = trailing_text;
                    output.drop_without_applying_deltas();
                }
                scroll_state_matches = egui::scroll_area::State::load(&context, scroll_id)
                    .is_some_and(|state| {
                        settled_scrolled_layout.is_some_and(|layout| {
                            state.offset == layout.body_scroll.3
                                && state.offset.y
                                    <= (layout.body_scroll.2.y - layout.body_scroll.1.height())
                                        .max(0.0)
                                        + 0.5
                        })
                    });
            }
        }

        let unchanged = dialog.draft == draft_before
            && dialog.baseline == baseline_before
            && dialog.store.snapshot() == persisted_before
            && dialog.draft_revision == revision_before
            && dialog.dirty == dirty_before
            && dialog.conflict == conflict_before
            && dialog.selected_macro_id == selected_before
            && dialog
                .package_ui
                .modal
                .as_ref()
                .is_some_and(|modal| matches!(modal, PackageModal::Help));
        let layout = initial_layout;
        assert!(
            unchanged,
            "opening package help must not mutate the transaction"
        );
        let layout = layout.expect("the settled help modal exposes its native Close action");
        assert!(layout.body_scroll.1.is_positive());
        assert!(layout.body_scroll.2.y > layout.body_scroll.1.height());
        assert!(layout.body_scroll.3.y >= 0.0);
        assert!(
            layout.body_scroll.3.y
                <= (layout.body_scroll.2.y - layout.body_scroll.1.height()).max(0.0) + 0.5
        );
        assert!(layout.body_scroll.3.y.abs() <= 0.5);
        assert!(screen_rect.contains_rect(layout.body_scroll.1));
        assert!(screen_rect.contains_rect(layout.footer_clip));
        assert!(screen_rect.contains_rect(layout.close_response.1));
        assert!(screen_rect.contains_rect(layout.close_node));
        assert!(screen_rect.contains_rect(layout.heading_node));
        assert!(layout.body_scroll.1.contains_rect(layout.heading_node));
        assert!(layout.body_scroll.1.bottom() <= layout.close_response.1.top());
        assert!(layout.footer_clip.contains_rect(layout.close_response.1));
        assert!(same_rect(layout.close_response.1, layout.close_node));
        assert!(
            scroll_state_matches,
            "the settled native ScrollArea state uses the legal trailing offset"
        );
        let scrolled = settled_scrolled_layout
            .expect("the long Help body reaches a settled trailing scroll position");
        let trailing_text = settled_trailing_text
            .expect("the final Help label is emitted as a settled text galley");
        let max_scroll = (scrolled.body_scroll.2.y - scrolled.body_scroll.1.height()).max(0.0);
        assert!(scrolled.body_scroll.3.y > 0.0);
        assert!(scrolled.body_scroll.3.y <= max_scroll + 0.5);
        assert!(scrolled.body_scroll.1.contains_rect(trailing_text.0));
        assert!(scrolled.body_scroll.1.intersects(trailing_text.1));
        assert!(trailing_text.1.contains_rect(trailing_text.0));
        assert!(!trailing_text.2);
        assert!(scrolled.body_scroll.1.bottom() <= scrolled.close_response.1.top());
        assert!(same_rect(layout.body_scroll.1, scrolled.body_scroll.1));
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
