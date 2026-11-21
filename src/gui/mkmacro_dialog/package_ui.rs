use super::MkMacroDialog;
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::{
    common::atomic_file::save_atomic,
    mkmacro::{
        MkMacroTemplateCatalog, PackageImportPlan, export_package, plan_package_import,
        plan_template_instantiation,
    },
};
use eframe::egui;
use std::{collections::BTreeSet, fs};

#[derive(Clone)]
pub(super) enum PackageOpen {
    ExportLibrary { selected: Option<u64> },
    SaveTemplate { macro_id: u64 },
    Templates,
    Help,
}

#[allow(
    clippy::large_enum_variant,
    reason = "Package actions keep request-specific data inline for dialog dispatch."
)]
#[derive(Clone)]
pub(super) enum PackageHostAction {
    Open(PackageOpen),
    ExportSelected {
        root: u64,
        default_name: String,
    },
    Import,
    ApplyPlan(PackageImportPlan),
}

#[derive(Clone, Default)]
pub(super) struct PackageUiState {
    modal: Option<PackageModal>,
    #[cfg(test)] test_reuse: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)] test_help: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)] test_items: Vec<(&'static str, egui::Rect, egui::Rect, bool)>,
}

#[derive(Clone)]
enum PackageModal {
    ImportPreview { source: String, plan: PackageImportPlan },
}

impl PackageUiState {
    #[cfg(test)] pub(super) fn test_reuse(&self) -> Option<(egui::Rect, egui::Rect, bool)> { self.test_reuse }
    #[cfg(test)] pub(super) fn test_help(&self) -> Option<(egui::Rect, egui::Rect, bool)> { self.test_help }
    #[cfg(test)] pub(super) fn test_item(&self, key: &str) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_items.iter().find(|item| item.0 == key).map(|item| (item.1, item.2, item.3))
    }
    pub(super) fn is_open(&self) -> bool {
        self.modal.is_some()
    }

    pub(super) fn close(&mut self) {
        self.modal = None;
    }
}

pub(super) fn show_toolbar_menu(ui: &mut egui::Ui, dialog: &mut MkMacroDialog) {
    let selected = dialog.selected_macro_id;
    #[cfg(test)] dialog.package_ui.test_items.clear();
    let menu = ui.menu_button("Reuse", |ui| {
        if !dialog.host_child_input_allowed() {
            ui.disable();
        }
        if ui
            .add_enabled(
                selected.is_some(),
                egui::Button::new("Export Selected Macro…"),
            )
            .clicked()
        {
            if let Some(root) = selected {
                let default_name = dialog
                    .selected_macro()
                    .map(|macro_| package_filename(&macro_.name))
                    .unwrap_or_else(|| "macro.mkmacro".into());
                dialog.stage_host_toolbar_action(super::MkMacroHostActionKind::Package(
                    PackageHostAction::ExportSelected { root, default_name },
                ));
            }
            ui.close();
        }
        let export = ui.add_enabled(!dialog.draft.macros.is_empty(), egui::Button::new("Export Library…"));
        #[cfg(test)] dialog.package_ui.test_items.push(("export", export.rect, ui.clip_rect(), export.enabled()));
        if export.clicked() {
            dialog.stage_host_toolbar_action(super::MkMacroHostActionKind::Package(
                PackageHostAction::Open(PackageOpen::ExportLibrary { selected }),
            ));
            ui.close();
        }
        if ui.button("Import Library…").clicked() {
            dialog.stage_host_toolbar_action(super::MkMacroHostActionKind::Package(
                PackageHostAction::Import,
            ));
            ui.close();
        }
        ui.separator();
        let save = ui.add_enabled(selected.is_some(), egui::Button::new("Save as Template…"));
        #[cfg(test)] dialog.package_ui.test_items.push(("save", save.rect, ui.clip_rect(), save.enabled()));
        if save.clicked() {
            if let Some(macro_id) = selected {
                dialog.stage_host_toolbar_action(super::MkMacroHostActionKind::Package(
                    PackageHostAction::Open(PackageOpen::SaveTemplate { macro_id }),
                ));
            }
            ui.close();
        }
        let templates = ui.button("New from Template…");
        #[cfg(test)] dialog.package_ui.test_items.push(("templates", templates.rect, ui.clip_rect(), templates.enabled()));
        if templates.clicked() {
            dialog.stage_host_toolbar_action(super::MkMacroHostActionKind::Package(
                PackageHostAction::Open(PackageOpen::Templates),
            ));
            ui.close();
        }
        ui.separator();
        let help = ui.button("MkMacro Help");
        #[cfg(test)]
        {
            dialog.package_ui.test_help = Some((help.rect, ui.clip_rect(), help.enabled()));
            dialog.package_ui.test_items.push(("help", help.rect, ui.clip_rect(), help.enabled()));
        }
        if help.clicked() {
            dialog.stage_host_toolbar_action(super::MkMacroHostActionKind::Package(
                PackageHostAction::Open(PackageOpen::Help),
            ));
            ui.close();
        }
    });
    #[cfg(test)]
    {
        dialog.package_ui.test_reuse =
            Some((menu.response.rect, ui.clip_rect(), menu.response.enabled()));
    }
    #[cfg(not(test))]
    let _ = menu;
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
    let Some(PackageModal::ImportPreview { source, plan }) = dialog.package_ui.modal.take() else {
        return;
    };
    let mut open = true;
    let mut keep = true;
    let id = egui::Id::new("mkmacro_package_modal");
    egui::Window::new("Import Macro Library")
        .id(id)
        .enabled(dialog.host_child_input_allowed())
        .collapsible(false)
        .resizable(true)
        .show_utility(
            ctx,
            "mkmacro-package-import",
            id,
            Some(&mut open),
            crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::width(520.0),
            |ui| {
                let footer = package_footer_reserved_height(ui, &["Apply Import", "Cancel"]);
                let available = ui
                    .available_rect_before_wrap()
                    .intersect(ui.clip_rect())
                    .height()
                    .max(0.0);
                egui::ScrollArea::vertical()
                    .min_scrolled_height(0.0)
                    .auto_shrink([false, false])
                    .max_height((available - footer).max(0.0))
                    .show(ui, |ui| {
                        ui.label(format!("Source: {source}"));
                        import_policy(ui, &plan, false);
                    });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Apply Import").clicked() {
                        if dialog.defer_host_effects {
                            dialog.stage_host_toolbar_action(
                                super::MkMacroHostActionKind::Package(
                                    PackageHostAction::ApplyPlan(plan.clone()),
                                ),
                            );
                        } else if apply_plan(dialog, &plan).is_ok() {
                            keep = false;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            },
        );
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        keep = false;
    }
    if open && keep {
        dialog.package_ui.modal = Some(PackageModal::ImportPreview { source, plan });
    }
}

pub(super) fn host_action_is_current(dialog: &MkMacroDialog, action: &PackageHostAction) -> bool {
    match action {
        PackageHostAction::Open(PackageOpen::ExportLibrary { selected }) => {
            *selected == dialog.selected_macro_id && !dialog.draft.macros.is_empty()
        }
        PackageHostAction::Open(PackageOpen::SaveTemplate { macro_id }) => {
            dialog.selected_macro_id == Some(*macro_id)
                && dialog.draft.macros.iter().any(|macro_| macro_.id == *macro_id)
        }
        PackageHostAction::Open(PackageOpen::Templates | PackageOpen::Help) => true,
        PackageHostAction::ExportSelected { root, .. } => {
            dialog.selected_macro_id == Some(*root)
                && dialog.draft.macros.iter().any(|macro_| macro_.id == *root)
        }
        PackageHostAction::Import => true,
        PackageHostAction::ApplyPlan(_) => matches!(
            &dialog.package_ui.modal,
            Some(PackageModal::ImportPreview { .. })
        ),

    }
}

pub(super) fn complete_host_action(
    dialog: &mut MkMacroDialog,
    action: PackageHostAction,
) -> anyhow::Result<()> {
    match action {
        PackageHostAction::Open(_) => anyhow::bail!("Package opens require an accepted host receipt"),
        PackageHostAction::ExportSelected { root, default_name } => {
            let Some(picker) = owned_file_dialog(dialog)? else {
                return Ok(());
            };
            let Some(path) = picker
                .add_filter("MkMacro package", &["mkmacro"])
                .set_file_name(&default_name)
                .save_file()
            else {
                return Ok(());
            };
            export_package(&dialog.store, &dialog.draft, &[root])
                .and_then(|bytes| save_atomic(&path, &bytes))?;
            Ok(())
        }
        PackageHostAction::Import => {
            let Some(picker) = owned_file_dialog(dialog)? else {
                return Ok(());
            };
            let Some(path) = picker
                .add_filter("MkMacro package", &["mkmacro", "json"])
                .pick_file()
            else {
                return Ok(());
            };
            let bytes = fs::read(&path)?;
            let plan = plan_package_import(
                &dialog.store,
                &bytes,
                &dialog.draft,
                dialog.draft_revision,
                &dialog.baseline,
            )?;
            dialog.package_ui.modal = Some(PackageModal::ImportPreview {
                source: path.display().to_string(),
                plan,
            });
            dialog.command_error = None;
            Ok(())
        }
        PackageHostAction::ApplyPlan(plan) => {
            apply_plan(dialog, &plan)?;
            dialog.package_ui.close();
            Ok(())
        }

    }
}

fn owned_file_dialog(dialog: &MkMacroDialog) -> anyhow::Result<Option<rfd::FileDialog>> {
    #[cfg(windows)]
    {
        let Some((ctx, identity)) = dialog.picker_owner.as_ref() else {
            anyhow::bail!(
                "Could not identify the MkMacro window for the file dialog. Focus the editor and try again."
            );
        };
        let hwnd = crate::gui::utility_dialog_size::utility_picker_hwnd(ctx, *identity)
            .and_then(|hwnd| std::num::NonZeroIsize::new(hwnd as isize));
        let Some(hwnd) = hwnd else {
            anyhow::bail!(
                "The MkMacro window is not ready to own the file dialog. Focus the editor and try again."
            );
        };
        Ok(Some(rfd::FileDialog::new().set_parent(
            &crate::gui::add_action_dialog::NativeDialogParent(hwnd),
        )))
    }
    #[cfg(not(windows))]
    {
        let _ = dialog;
        Ok(Some(rfd::FileDialog::new()))
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
    ui.heading("Direct opens");
    ui.label("Choose Add Action → Programs & Launcher → Open Folder or Open URL. Browse fills the folder path; Run opens it in Explorer. No folder or bookmark record is created. Folder example: C:\\Users\\You\\AppData\\Local\\Temp\\DirectOpenDemo. URL example: https://example.test/; only explicit HTTP(S) URLs are accepted.");
    ui.label("Templates use ${name} for a current runtime value and $${name} for the literal text ${name}. String, Number, Boolean, and Point values use shared formatting; Null is rejected. Folder existence and access are checked when the step runs.");
    ui.label("Direct opens hand off to Windows; a URL handoff does not confirm that remote content loaded. Canceling after handoff does not close the external window. These actions are unavailable in unattended Hot Folder runs.");
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

// Ordinary package children own private state; Main owns the macro document.
#[derive(Clone)]
enum ChildContent {
    ExportLibrary {
        roots: BTreeSet<u64>,
    },
    SaveTemplate {
        name: String,
        description: String,
    },
    Templates {
        catalog: std::sync::Arc<MkMacroTemplateCatalog>,
        selected: Option<u64>,
        preview: Option<std::sync::Arc<PackageImportPlan>>,
    },
    Help,
}
trait PackageKind: Clone {
    const KEY: &'static str;
    const TITLE: &'static str;
}
macro_rules! package_kind {
    ($name:ident, $key:literal, $title:literal) => {
        #[derive(Clone)]
        struct $name;
        impl PackageKind for $name {
            const KEY: &'static str = $key;
            const TITLE: &'static str = $title;
        }
    };
}
package_kind!(ExportKind, "mkmacro-package-export", "Export Macro Library");
package_kind!(SaveKind, "mkmacro-template-save", "Save Macro Template");
package_kind!(TemplatesKind, "mkmacro-templates", "Macro Templates");
package_kind!(HelpKind, "mkmacro-help", "MkMacro Help");
#[derive(Clone)]
struct PackageChild<K: PackageKind> {
    content: Option<ChildContent>,
    epoch: u64,
    owner: u64,
    revision: u64,
    macro_id: Option<u64>,
    error: Option<String>,
    #[cfg(test)]
    test_body_scroll: Option<(egui::Id, egui::Rect, egui::Vec2, egui::Vec2)>,
    #[cfg(test)]
    test_footer_clip: Option<egui::Rect>,
    #[cfg(test)]
    test_close_response: Option<(egui::Id, egui::Rect)>,
    #[cfg(test)]
    test_save_response: Option<(egui::Rect, egui::Rect, bool)>,
    _kind: std::marker::PhantomData<K>,
}
impl<K: PackageKind> Default for PackageChild<K> {
    fn default() -> Self {
        Self {
            content: None,
            epoch: 0,
            owner: 0,
            revision: 0,
            macro_id: None,
            error: None,
            #[cfg(test)]
            test_body_scroll: None,
            #[cfg(test)]
            test_footer_clip: None,
            #[cfg(test)]
            test_close_response: None,
            #[cfg(test)]
            test_save_response: None,
            _kind: std::marker::PhantomData,
        }
    }
}
#[derive(Clone)]
enum ChildIntent {
    Close,
    Retired,
    Export(Vec<u64>),
    Save {
        macro_id: u64,
        name: String,
        description: String,
    },
    Preview(u64),
    Apply(std::sync::Arc<PackageImportPlan>),
}
#[derive(Default)]
struct ChildActions {
    presentation: Option<super::super::utility_host::UtilityPresentation>,
    intent: Option<ChildIntent>,
}
struct ChildCommand {
    intent: ChildIntent,
    hwnd: Option<usize>,
}
impl<K: PackageKind> super::super::utility_window_host::HostedUtilityDialog for PackageChild<K> {
    const KEY: &'static str = K::KEY;
    const TITLE: &'static str = K::TITLE;
    const SIZE: egui::Vec2 = egui::vec2(620.0, 520.0);
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 220.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = MkMacroDialog;
    type Actions = ChildActions;
    type Checkpoint = Self;
    type Command = ChildCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new(K::KEY))
    }
    fn navigation_owner(_: u64) -> super::super::jump_mode::ui::NavigationOwner {
        super::super::jump_mode::ui::NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.content.is_some()
    }
    fn session(&self) -> u64 {
        self.epoch
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: super::super::jump_mode::ui::NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer
                    || Some(layer) == popup
                    || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<super::super::jump_mode::ui::NavigationViewportBinding> {
        None
    }
    fn capture(&self) -> Self {
        self.clone()
    }
    fn restore(&mut self, checkpoint: Self) {
        *self = checkpoint;
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (
            Option<&super::super::jump_mode::ui::JumpModeScope>,
            Option<&super::super::jump_mode::ui::JumpModeScope>,
        ),
        _: super::super::jump_mode::ui::NavigationOwner,
        allowed: bool,
        _: super::super::jump_mode::ui::NavigationOwner,
        first: bool,
        _: u64,
        parent: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: super::super::utility_host::UtilityPresentation,
    ) -> ChildActions {
        let mut result = ChildActions::default();
        let mut open = true;
        let mut body = |ui: &mut egui::Ui| {
            let (button, next) =
                super::super::utility_host::presentation_button(ui, presentation, allowed);
            if first && !ctx.will_discard() && button.clicked() {
                result.presentation = Some(next);
            }
            ui.add_enabled_ui(allowed, |ui| {
                render_child_contents(ui, self, parent, &mut result)
            });
        };
        if let Some(ui) = child {
            body(ui);
        } else {
            let id = egui::Id::new(K::KEY);
            egui::Window::new(K::TITLE)
                .id(id)
                .resizable(true)
                .show_utility(
                    ctx,
                    K::KEY,
                    id,
                    Some(&mut open),
                    super::super::utility_dialog_size::UtilityWindowSizeRecipe::outer(Self::SIZE),
                    body,
                );
        }
        if !open {
            result.intent = Some(ChildIntent::Close);
        }
        if !first || ctx.will_discard() {
            result.intent = None;
            result.presentation = None;
        }
        result
    }
    fn host_request(
        actions: &ChildActions,
    ) -> Option<super::super::utility_host::UtilityPresentation> {
        actions.presentation
    }
    fn closed_command() -> ChildCommand {
        ChildCommand {
            intent: ChildIntent::Retired,
            hwnd: None,
        }
    }
    fn command_closes(_: &ChildCommand) -> bool {
        false
    }
    fn complete_actions(
        &mut self,
        actions: ChildActions,
        hwnd: Option<usize>,
    ) -> Vec<ChildCommand> {
        actions
            .intent
            .map(|intent| ChildCommand { intent, hwnd })
            .into_iter()
            .collect()
    }
    #[cfg(test)]
    fn test_button(_: &ChildActions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

fn render_child_contents<K: PackageKind>(
    ui: &mut egui::Ui,
    child: &mut PackageChild<K>,
    parent: &MkMacroDialog,
    result: &mut ChildActions,
) {
    if let Some(error) = &child.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    let macro_id = child.macro_id;
    let Some(content) = child.content.as_mut() else {
        return;
    };
    let labels: &[&str] = match content {
        ChildContent::ExportLibrary { .. } => &["Export…", "Cancel"],
        ChildContent::SaveTemplate { .. } => &["Save Template", "Cancel"],
        ChildContent::Templates {
            preview: Some(_), ..
        } => &["Create Independent Copy", "Cancel"],
        ChildContent::Templates { .. } => &["Preview Copy", "Cancel"],
        ChildContent::Help => &["Close"],
    };
    let footer = package_footer_reserved_height(ui, labels);
    let available = ui
        .available_rect_before_wrap()
        .intersect(ui.clip_rect())
        .height()
        .max(0.0);
    let body_output = egui::ScrollArea::vertical().min_scrolled_height(0.0).auto_shrink([false, false])
        .max_height((available - footer).max(0.0)).show(ui, |ui| match content {
            ChildContent::ExportLibrary { roots } => {
                ui.label("Choose explicit library roots. Authored Call dependencies are included automatically.");
                for macro_ in &parent.draft.macros {
                    let mut included = roots.contains(&macro_.id);
                    if ui.checkbox(&mut included, &macro_.name).changed() {
                        if included { roots.insert(macro_.id); } else { roots.remove(&macro_.id); }
                    }
                }
            }
            ChildContent::SaveTemplate { name, description } => {
                ui.label("The selected macro and its authored dependencies are captured from the current draft.");
                ui.label("Template name"); ui.text_edit_singleline(name);
                ui.label("Description"); ui.text_edit_multiline(description);
            }
            ChildContent::Templates { catalog, selected, preview } => {
                if catalog.templates.is_empty() { ui.label("No saved macro templates."); }
                for template in &catalog.templates {
                    if ui.selectable_label(*selected == Some(template.id), &template.name).clicked() {
                        *selected = Some(template.id); *preview = None;
                    }
                    if *selected == Some(template.id) && !template.description.is_empty() { ui.small(&template.description); }
                }
                if let Some(plan) = preview { ui.separator(); import_policy(ui, plan, true); }
            }
            ChildContent::Help => help(ui),
        });
    #[cfg(test)]
    {
        child.test_body_scroll = Some((
            body_output.id,
            body_output.inner_rect,
            body_output.content_size,
            body_output.state.offset,
        ));
        child.test_footer_clip = Some(ui.clip_rect());
    }
    #[cfg(not(test))]
    let _ = body_output;
    ui.separator();
    ui.horizontal_wrapped(|ui| match content {
        ChildContent::ExportLibrary { roots } => {
            if ui
                .add_enabled(!roots.is_empty(), egui::Button::new("Export…"))
                .clicked()
            {
                result.intent = Some(ChildIntent::Export(roots.iter().copied().collect()));
            }
            if ui.button("Cancel").clicked() {
                result.intent = Some(ChildIntent::Close);
            }
        }
        ChildContent::SaveTemplate { name, description } => {
            let response = ui.add_enabled(
                !name.trim().is_empty() && macro_id.is_some(),
                egui::Button::new("Save Template"),
            );
            #[cfg(test)]
            {
                child.test_save_response =
                    Some((response.rect, ui.clip_rect(), response.enabled()));
            }
            if response.clicked()
                && let Some(macro_id) = macro_id
            {
                result.intent = Some(ChildIntent::Save {
                    macro_id,
                    name: name.clone(),
                    description: description.clone(),
                });
            }
            if ui.button("Cancel").clicked() {
                result.intent = Some(ChildIntent::Close);
            }
        }
        ChildContent::Templates {
            selected, preview, ..
        } => {
            if let Some(plan) = preview {
                if ui.button("Create Independent Copy").clicked() {
                    result.intent = Some(ChildIntent::Apply(plan.clone()));
                }
            } else if ui
                .add_enabled(selected.is_some(), egui::Button::new("Preview Copy"))
                .clicked()
                && let Some(id) = *selected
            {
                result.intent = Some(ChildIntent::Preview(id));
            }
            if ui.button("Cancel").clicked() {
                result.intent = Some(ChildIntent::Close);
            }
        }
        ChildContent::Help => {
            let response = ui.button("Close");
            #[cfg(test)]
            {
                child.test_close_response = Some((response.id, response.rect));
            }
            if response.clicked() {
                result.intent = Some(ChildIntent::Close);
            }
        }
    });
}
#[derive(Default)]
pub(super) struct PackageHosts {
    export: PackageChild<ExportKind>,
    export_host: super::super::utility_window_host::UtilityWindowHost<PackageChild<ExportKind>>,
    save: PackageChild<SaveKind>,
    save_host: super::super::utility_window_host::UtilityWindowHost<PackageChild<SaveKind>>,
    templates: PackageChild<TemplatesKind>,
    templates_host:
        super::super::utility_window_host::UtilityWindowHost<PackageChild<TemplatesKind>>,
    help: PackageChild<HelpKind>,
    help_host: super::super::utility_window_host::UtilityWindowHost<PackageChild<HelpKind>>,
}
impl PackageHosts {
    #[cfg(test)]
    pub(super) fn test_save_viewport(&self) -> egui::ViewportId {
        super::super::utility_window_host::UtilityWindowHost::<PackageChild<SaveKind>>::test_viewport(self.save.epoch)
    }
    #[cfg(test)]
    pub(super) fn test_save_response(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.save.test_save_response
    }
    #[cfg(test)]
    pub(super) fn test_help_popped(&self) -> bool {
        self.help.content.is_some() && self.help_host.is_popped_out()
    }
    #[cfg(test)]
    pub(super) fn test_help_epoch(&self) -> u64 {
        self.help.epoch
    }
    #[cfg(test)]
    pub(super) fn test_set_save_name(&mut self, name: &str) {
        if let Some(ChildContent::SaveTemplate { name: value, .. }) = self.save.content.as_mut() {
            *value = name.into();
        }
    }
    #[cfg(test)]
    pub(super) fn test_save_name(&self) -> Option<&str> {
        match self.save.content.as_ref() {
            Some(ChildContent::SaveTemplate { name, .. }) => Some(name),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn test_popped(&self, key: &str) -> bool {
        match key {
            "export" => self.export.content.is_some() && self.export_host.is_popped_out(),
            "save" => self.save.content.is_some() && self.save_host.is_popped_out(),
            "templates" => self.templates.content.is_some() && self.templates_host.is_popped_out(),
            "help" => self.help.content.is_some() && self.help_host.is_popped_out(),
            _ => false,
        }
    }
    fn any_open(&self) -> bool {
        self.export.content.is_some()
            || self.save.content.is_some()
            || self.templates.content.is_some()
            || self.help.content.is_some()
    }
    pub(super) fn root_layer(&self) -> Option<egui::LayerId> {
        if self.export.content.is_some() {
            return self.export_host.root_layer();
        }
        if self.save.content.is_some() {
            return self.save_host.root_layer();
        }
        if self.templates.content.is_some() {
            return self.templates_host.root_layer();
        }
        if self.help.content.is_some() {
            return self.help_host.root_layer();
        }
        None
    }
    pub(super) fn has_root_layer(&self) -> bool {
        self.root_layer().is_some()
    }
    pub(super) fn needs_root_ui_when_hidden(&self) -> bool {
        self.export_host
            .needs_root_ui_when_hidden(self.export.content.is_some())
            || self
                .save_host
                .needs_root_ui_when_hidden(self.save.content.is_some())
            || self
                .templates_host
                .needs_root_ui_when_hidden(self.templates.content.is_some())
            || self
                .help_host
                .needs_root_ui_when_hidden(self.help.content.is_some())
    }
    pub(super) fn close_all(&mut self, ctx: &egui::Context) {
        if self.export.content.take().is_some() {
            self.export_host.close_from_app(ctx);
        }
        if self.save.content.take().is_some() {
            self.save_host.close_from_app(ctx);
        }
        if self.templates.content.take().is_some() {
            self.templates_host.close_from_app(ctx);
        }
        if self.help.content.take().is_some() {
            self.help_host.close_from_app(ctx);
        }
    }
    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        parent: &MkMacroDialog,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        show_child(
            ctx,
            parent,
            &mut self.export_host,
            &mut self.export,
            first,
            root_visible,
            root_input,
            native_input,
            root_jump_active,
        );
        show_child(
            ctx,
            parent,
            &mut self.save_host,
            &mut self.save,
            first,
            root_visible,
            root_input,
            native_input,
            root_jump_active,
        );
        show_child(
            ctx,
            parent,
            &mut self.templates_host,
            &mut self.templates,
            first,
            root_visible,
            root_input,
            native_input,
            root_jump_active,
        );
        show_child(
            ctx,
            parent,
            &mut self.help_host,
            &mut self.help,
            first,
            root_visible,
            root_input,
            native_input,
            root_jump_active,
        );
    }
    pub(super) fn complete(&mut self, ctx: &egui::Context, parent: &mut MkMacroDialog) -> bool {
        complete_child(ctx, parent, &mut self.export_host, &mut self.export)
            | complete_child(ctx, parent, &mut self.save_host, &mut self.save)
            | complete_child(ctx, parent, &mut self.templates_host, &mut self.templates)
            | complete_child(ctx, parent, &mut self.help_host, &mut self.help)
    }
    pub(super) fn open(
        &mut self,
        ctx: &egui::Context,
        parent: &mut MkMacroDialog,
        request: PackageOpen,
        origin: super::super::utility_host::AcceptedParentOpen<u64>,
    ) -> bool {
        use super::super::utility_host::ChildOpenOrigin;
        if parent.package_ui.is_open() {
            return false;
        }
        let active = self.any_open();
        let origin = ChildOpenOrigin::Parent(origin);
        match request {
            PackageOpen::ExportLibrary { selected } => {
                if active && self.export.content.is_none() {
                    return false;
                }
                if self.export.content.is_none() {
                    self.export.begin(
                        parent,
                        ChildContent::ExportLibrary {
                            roots: selected.into_iter().collect(),
                        },
                    );
                }
                self.export_host
                    .open_from_origin(ctx, &self.export, active, &origin)
                    == Some(super::super::utility_host::UtilityPresentation::Docked)
            }
            PackageOpen::SaveTemplate { macro_id } => {
                if active && self.save.content.is_none() {
                    return false;
                }
                if self.save.content.is_none() {
                    let Some(macro_) = parent.draft.macros.iter().find(|item| item.id == macro_id)
                    else {
                        return false;
                    };
                    self.save.begin(
                        parent,
                        ChildContent::SaveTemplate {
                            name: macro_.name.clone(),
                            description: macro_.description.clone(),
                        },
                    );
                }
                self.save_host
                    .open_from_origin(ctx, &self.save, active, &origin)
                    == Some(super::super::utility_host::UtilityPresentation::Docked)
            }
            PackageOpen::Templates => {
                if active && self.templates.content.is_none() {
                    return false;
                }
                if self.templates.content.is_none() {
                    match parent.store.load_template_catalog() {
                        Ok(catalog) => self.templates.begin(
                            parent,
                            ChildContent::Templates {
                                catalog: std::sync::Arc::new(catalog),
                                selected: None,
                                preview: None,
                            },
                        ),
                        Err(error) => {
                            parent.command_error =
                                Some(format!("Could not load macro templates: {error}"));
                            return false;
                        }
                    }
                }
                self.templates_host
                    .open_from_origin(ctx, &self.templates, active, &origin)
                    == Some(super::super::utility_host::UtilityPresentation::Docked)
            }
            PackageOpen::Help => {
                if active && self.help.content.is_none() {
                    return false;
                }
                if self.help.content.is_none() {
                    self.help.begin(parent, ChildContent::Help);
                }
                self.help_host
                    .open_from_origin(ctx, &self.help, active, &origin)
                    == Some(super::super::utility_host::UtilityPresentation::Docked)
            }
        }
    }
}
impl<K: PackageKind> PackageChild<K> {
    fn begin(&mut self, parent: &MkMacroDialog, content: ChildContent) {
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.owner = parent.owner_instance;
        self.revision = parent.draft_revision();
        self.macro_id = parent.selected_macro_id;
        self.error = None;
        self.content = Some(content);
    }
    fn source_current(&self, parent: &MkMacroDialog) -> bool {
        parent.open
            && !parent.close_prompt_open
            && self.owner == parent.owner_instance
            && self.revision == parent.draft_revision()
            && self.macro_id == parent.selected_macro_id
    }
}
fn show_child<K: PackageKind>(
    ctx: &egui::Context,
    parent: &MkMacroDialog,
    host: &mut super::super::utility_window_host::UtilityWindowHost<PackageChild<K>>,
    child: &mut PackageChild<K>,
    first: bool,
    root_visible: bool,
    root_input: bool,
    native_input: bool,
    root_jump_active: bool,
) {
    use super::super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
    use super::super::utility_window_host::HostInput;
    let owner = root_input
        && host
            .root_layer()
            .is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
        && !egui::Popup::is_any_open(ctx);
    host.show(
        ctx,
        child,
        parent,
        HostInput {
            first,
            root_visible,
            root_input,
            native_input,
            root_owner: NavigationOwner::Blocked,
            parent_owner: owner,
            root_jump_active,
            close_shortcut: true,
            allow_os_close: true,
            confirmation_owner: NavigationOwner::Blocked,
            parent_instance: 0,
            root_scopes: (None, None),
        },
    );
}
fn child_file_dialog(hwnd: Option<usize>) -> anyhow::Result<rfd::FileDialog> {
    #[cfg(windows)]
    {
        let hwnd = hwnd
            .and_then(|value| std::num::NonZeroIsize::new(value as isize))
            .ok_or_else(|| {
                anyhow::anyhow!("The package window is not ready to own a file dialog.")
            })?;
        Ok(rfd::FileDialog::new()
            .set_parent(&crate::gui::add_action_dialog::NativeDialogParent(hwnd)))
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Ok(rfd::FileDialog::new())
    }
}
fn complete_child<K: PackageKind>(
    ctx: &egui::Context,
    parent: &mut MkMacroDialog,
    host: &mut super::super::utility_window_host::UtilityWindowHost<PackageChild<K>>,
    child: &mut PackageChild<K>,
) -> bool {
    let Some(completion) = host.complete(ctx, child) else {
        return false;
    };
    let mut show_root = completion.show_root;
    for command in completion.commands {
        let close = match command.intent {
            ChildIntent::Retired => {
                child.content = None;
                false
            }
            ChildIntent::Close => true,
            intent => {
                if !child.source_current(parent) {
                    child.error = Some(
                        "The macro or draft changed. Reopen this tool before applying.".into(),
                    );
                    continue;
                }
                let result: anyhow::Result<bool> = (|| match intent {
                    ChildIntent::Export(roots) => {
                        let picker = child_file_dialog(command.hwnd)?;
                        let Some(path) = picker
                            .add_filter("MkMacro package", &["mkmacro"])
                            .set_file_name("macro-library.mkmacro")
                            .save_file()
                        else {
                            return Ok(false);
                        };
                        if !child.source_current(parent) {
                            anyhow::bail!("The macro or draft changed while choosing a file.");
                        }
                        let bytes = export_package(&parent.store, &parent.draft, &roots)?;
                        save_atomic(&path, &bytes)?;
                        Ok(true)
                    }
                    ChildIntent::Save {
                        macro_id,
                        name,
                        description,
                    } => {
                        parent.store.save_macro_template(
                            &parent.draft,
                            macro_id,
                            &name,
                            &description,
                        )?;
                        Ok(true)
                    }
                    ChildIntent::Preview(id) => {
                        let Some(ChildContent::Templates { catalog, .. }) = child.content.as_ref()
                        else {
                            return Ok(false);
                        };
                        let Some(template) =
                            catalog.templates.iter().find(|template| template.id == id)
                        else {
                            anyhow::bail!("The selected template no longer exists.");
                        };
                        let plan = plan_template_instantiation(
                            &parent.store,
                            template,
                            &parent.draft,
                            parent.draft_revision,
                            &parent.baseline,
                        )?;
                        if let Some(ChildContent::Templates { preview, .. }) =
                            child.content.as_mut()
                        {
                            *preview = Some(std::sync::Arc::new(plan));
                        }
                        Ok(false)
                    }
                    ChildIntent::Apply(plan) => {
                        apply_plan(parent, &plan)?;
                        Ok(true)
                    }
                    ChildIntent::Close | ChildIntent::Retired => Ok(false),
                })();
                match result {
                    Ok(close) => {
                        child.error = None;
                        close
                    }
                    Err(error) => {
                        child.error = Some(error.to_string());
                        false
                    }
                }
            }
        };
        if close {
            host.close_from_app(ctx);
            child.content = None;
        }
    }
    show_root &= child.content.is_some() && !host.is_popped_out();
    show_root
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
    fn package_children_keep_distinct_presentation_and_live_drafts() {
        use crate::gui::utility_host::{AcceptedParentOpen, UtilityPresentation, ViewportPassProvenance};
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let mut dialog = MkMacroDialog::new(std::sync::Arc::new(store));
        dialog.open = true;
        dialog.create_macro();
        dialog.host_state.transition(UtilityPresentation::Popout);
        let ctx = egui::Context::default();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = crate::gui::utility_presentation::UtilityPresentationManager::new(
            directory.path().join("settings.json").to_string_lossy().into_owned(), workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        for key in ["mkmacro-help", "mkmacro-templates", "mkmacro-template-save", "mkmacro-package-export"] {
            manager.remember(&ctx, crate::settings::UtilityToolKey::parse(key).unwrap(), crate::settings::RememberedPresentation::Docked);
        }
        let pass = ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT);
        let output = ctx.run_ui(egui::RawInput::default(), |_| {});
        let origin = AcceptedParentOpen::from_retained(&ctx, dialog.host_state.identity(), dialog.owner_instance, pass, pass).unwrap();
        output.drop_without_applying_deltas();
        let mut hosts = PackageHosts::default();
        let selected = dialog.selected_macro_id.unwrap();
        assert!(!hosts.open(&ctx, &mut dialog, PackageOpen::SaveTemplate { macro_id: selected }, origin.clone()));
        assert!(hosts.save_host.is_popped_out());
        let epoch = hosts.save.epoch;
        if let Some(ChildContent::SaveTemplate { name, .. }) = hosts.save.content.as_mut() { *name = "unsaved draft".into(); } else { panic!("save draft"); }
        assert!(!hosts.open(&ctx, &mut dialog, PackageOpen::SaveTemplate { macro_id: selected }, origin.clone()));
        assert_eq!(hosts.save.epoch, epoch);
        assert!(matches!(hosts.save.content.as_ref(), Some(ChildContent::SaveTemplate { name, .. }) if name == "unsaved draft"));
        hosts.close_all(&ctx);
        for request in [PackageOpen::ExportLibrary { selected: Some(selected) }, PackageOpen::Templates, PackageOpen::Help] {
            assert!(!hosts.open(&ctx, &mut dialog, request, origin.clone()));
            assert!(!hosts.has_root_layer());
            assert!(hosts.any_open());
            hosts.close_all(&ctx);
        }
        dialog.pending_package_open = Some((PackageOpen::Help, dialog.draft_revision, dialog.selected_macro_id, origin));
        dialog.host_state.transition(UtilityPresentation::Docked);
        assert!(!dialog.complete_package_children(&ctx));
        assert!(!dialog.package_hosts.any_open(), "a retired Main host cannot authorize a delayed open");
        workers.join_all();
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
        let mut help = PackageChild::<HelpKind>::default();
        help.begin(&dialog, ChildContent::Help);
        let mut host = super::super::super::utility_window_host::UtilityWindowHost::default();
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
            let output = context.run_ui(input(), |ctx| show_child(ctx, &dialog, &mut host, &mut help, true, true, true, true, false));
            let _ = complete_child(&context, &mut dialog, &mut host, &mut help);
            let close_response = help.test_close_response;
            if let Some(response) = close_response
                && let Some(bounds) = response_node_bounds(&output, Some(response), "Close")
            {
                last_close_node = Some((response.0, bounds));
            }
            if let Some(bounds) = named_node_bounds(&output, "Recording") {
                last_heading_node = Some(bounds);
            }
            let layout = match (
                help.test_body_scroll,
                help.test_footer_clip,
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
                    let output = context.run_ui(input(), |ctx| show_child(ctx, &dialog, &mut host, &mut help, true, true, true, true, false));
            let _ = complete_child(&context, &mut dialog, &mut host, &mut help);
                    let close_response = help.test_close_response;
                    if let Some(response) = close_response
                        && let Some(bounds) = response_node_bounds(&output, Some(response), "Close")
                    {
                        last_close_node = Some((response.0, bounds));
                    }
                    if let Some(bounds) = named_node_bounds(&output, "Recording") {
                        last_heading_node = Some(bounds);
                    }
                    let layout = match (
                        help.test_body_scroll,
                        help.test_footer_clip,
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
            && matches!(help.content, Some(ChildContent::Help));
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
