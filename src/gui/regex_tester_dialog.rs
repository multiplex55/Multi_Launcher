use crate::clipboard_modify::clipboard::{ArboardClipboardBackend, ClipboardBackend};
use crate::gui::jump_mode::ui::{FocusDirective, register_and_apply_target};
use crate::gui::jump_mode::ui::{
    JumpModeScope, JumpModeUi, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
};
use crate::gui::jump_mode::{SurfaceId, TargetGeneration};
use crate::gui::jump_mode::{TargetId, TargetKind};
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, secondary_help, semantic_status,
    with_button_emphasis,
};
use crate::regex_tester::{EvaluationResult, MatchCompleteness, RegexSession};
use eframe::egui;
use std::sync::Arc;
use std::time::Instant;
mod clipboard;
mod editor_layout;
mod examples;
mod explanation;
mod highlighting;
mod history;
mod inspection;
mod keyboard;
mod presentation;
mod presets;
#[cfg(test)]
mod profiling;
mod reference;
mod substitution;
mod test_cases;

fn copy_text(
    clipboard: &dyn ClipboardBackend,
    text: &str,
    success: &'static str,
) -> Result<&'static str, String> {
    clipboard
        .write_text(text)
        .map(|()| success)
        .map_err(|error| format!("Could not copy: {}", clipboard::error_message(&error)))
}

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Regex Tester")
        .with_inner_size([960.0, 680.0])
        .with_min_inner_size([360.0, 240.0])
        .with_resizable(true)
}

fn utf8_prefix(text: &str, maximum_bytes: usize) -> &str {
    let mut end = text.len().min(maximum_bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn pattern_font(ui: &egui::Ui) -> egui::FontId {
    let mut font = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Monospace)
        .cloned()
        .unwrap_or_else(|| egui::FontId::monospace(14.0));
    // The regex pattern historically used 18 pt while the theme's installed
    // monospace role is 14 pt at 100%. Preserve that baseline and let the
    // installed theme role carry the overall typography scale exactly once.
    font.size *= 18.0 / 14.0;
    font
}

fn oversized_pattern_font(ui: &egui::Ui) -> egui::FontId {
    ui.style()
        .text_styles
        .get(&egui::TextStyle::Monospace)
        .cloned()
        .unwrap_or_else(|| egui::FontId::monospace(14.0))
}

fn pattern_editor_height(ui: &egui::Ui, font: &egui::FontId) -> f32 {
    pattern_editor_height_at_baseline(ui, font, 18.0)
}

fn pattern_editor_height_at_baseline(
    ui: &egui::Ui,
    font: &egui::FontId,
    baseline_size: f32,
) -> f32 {
    let typography_scale = font.size / baseline_size;
    let text_height =
        ui.ctx().fonts_mut(|fonts| fonts.row_height(font)) + ui.spacing().extra_text_line_spacing;
    (28.0 * typography_scale)
        .max(text_height)
        .max(ui.spacing().interact_size.y)
}

/// Fixed allocations stop editor scroll extents from growing the native window.
fn bounded_area(ui: &mut egui::Ui, rect: egui::Rect, id: &str, render: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_rect(rect, egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    render(&mut child);
}

#[derive(Clone, Copy, Debug)]
struct CoreLayout {
    header: egui::Rect,
    validation: Option<egui::Rect>,
    editor: egui::Rect,
    information: Option<egui::Rect>,
    status: egui::Rect,
}

impl CoreLayout {
    #[cfg(test)]
    fn new(bounds: egui::Rect, information_open: bool, invalid_pattern: bool) -> Self {
        Self::with_header_height(bounds, information_open, invalid_pattern, 100.0)
    }

    fn with_header_height(
        bounds: egui::Rect,
        information_open: bool,
        invalid_pattern: bool,
        desired_header_height: f32,
    ) -> Self {
        let gap = 8.0_f32.min(bounds.height() * 0.03);
        let status_height = 24.0_f32.min(bounds.height() * 0.15);
        let validation_height = invalid_pattern.then(|| 80.0_f32.min(bounds.height() * 0.2));
        let minimum_content_height = if information_open { 2.0 + gap } else { 1.0 };
        let reserved_height =
            status_height + validation_height.unwrap_or(0.0) + 2.0 * gap + minimum_content_height;
        let maximum_header_height = (bounds.height() - reserved_height).max(1.0);
        let header_height = desired_header_height
            .max(1.0)
            .min(bounds.height() * 0.85)
            .min(maximum_header_height);
        let header =
            egui::Rect::from_min_size(bounds.min, egui::vec2(bounds.width(), header_height));
        let status = egui::Rect::from_min_max(
            egui::pos2(bounds.left(), bounds.bottom() - status_height),
            bounds.max,
        );
        let validation = validation_height.map(|height| {
            egui::Rect::from_min_size(
                egui::pos2(bounds.left(), header.bottom()),
                egui::vec2(bounds.width(), height),
            )
        });
        let content_top = validation.map_or(header.bottom(), |area| area.bottom());
        let content = egui::Rect::from_min_max(
            egui::pos2(bounds.left(), content_top + gap),
            egui::pos2(bounds.right(), status.top() - gap),
        );
        let (editor, information) = if !information_open {
            (content, None)
        } else if bounds.width() >= 680.0 {
            let info_width = (bounds.width() * 0.3).clamp(220.0, 300.0);
            let split = content.right() - info_width;
            (
                egui::Rect::from_min_max(content.min, egui::pos2(split - gap, content.bottom())),
                Some(egui::Rect::from_min_max(
                    egui::pos2(split, content.top()),
                    content.max,
                )),
            )
        } else {
            // A stacked information area needs room below its section selector.
            // Validation already occupies extra space; retain editor room then.
            let info_fraction = if invalid_pattern { 0.3 } else { 0.5 };
            let max_info_height = (content.height() - gap - 28.0).max(1.0);
            let info_height = (content.height() * info_fraction)
                .min(160.0)
                .min(max_info_height);
            let split = content.bottom() - info_height;
            (
                egui::Rect::from_min_max(content.min, egui::pos2(content.right(), split - gap)),
                Some(egui::Rect::from_min_max(
                    egui::pos2(content.left(), split),
                    content.max,
                )),
            )
        };
        Self {
            header,
            validation,
            editor,
            information,
            status,
        }
    }
}

fn pattern_header_height(ui: &egui::Ui) -> f32 {
    let font = pattern_font(ui);
    let pattern_height = pattern_editor_height(ui, &font);
    (2.0 * ui.spacing().interact_size.y + pattern_height + 2.0 * ui.spacing().item_spacing.y)
        .max(100.0)
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum InformationSection {
    #[default]
    Matches,
    MatchDetails,
    Explanation,
    Reference,
    Examples,
    History,
    Presets,
    Substitution,
    Clipboard,
    TestCases,
}

/// Session-owned inputs survive closing the utility; persistence is explicit.
pub struct RegexTesterDialogState {
    pub open: bool,
    pub session: RegexSession,
    focus_pattern: bool,
    focus_viewport: bool,
    information_open: bool,
    scroll_to_selected: bool,
    information_section: InformationSection,
    test_cases_expanded: bool,
    reference: reference::ReferenceState,
    examples: examples::ExamplesState,
    history: Option<history::HistoryState>,
    presets: Option<presets::PresetState>,
    selected_capture: usize,
    clipboard: Arc<dyn ClipboardBackend>,
    copy_feedback: Option<Result<&'static str, String>>,
    native_jump_mode: JumpModeUi,
    native_jump_frame: Option<u64>,
    observed_viewport: Option<(u64, RegexViewportPresentation)>,
    #[cfg(test)]
    last_pattern_geometry: Option<(egui::Rect, egui::FontId)>,
    #[cfg(test)]
    last_oversized_preview_geometry: Option<(egui::Rect, egui::FontId)>,
    #[cfg(test)]
    last_header_viewport: Option<egui::Rect>,
    #[cfg(test)]
    last_metadata_geometry: Option<(egui::Rect, f32, f32)>,
    #[cfg(test)]
    last_pattern_controls: Option<(Option<egui::Rect>, [egui::Rect; 5])>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegexViewportPresentation {
    Immediate,
    Embedded,
}

pub(super) struct JumpTargets<'a> {
    scope: Option<&'a JumpModeScope>,
    visual_order: u64,
    suspended: bool,
}

impl<'a> JumpTargets<'a> {
    fn new(scope: Option<&'a JumpModeScope>) -> Self {
        Self {
            scope,
            visual_order: 0,
            suspended: false,
        }
    }

    fn suspend(&mut self) {
        self.suspended = true;
    }

    pub(super) fn register(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: impl Into<String>,
        kind: TargetKind,
    ) -> Option<(TargetId, Option<FocusDirective>)> {
        if self.suspended {
            return None;
        }
        let identity = identity.as_ref().to_owned();
        let id = self.scope.map(|scope| scope.target_id(&identity));
        let directive = register_and_apply_target(
            self.scope,
            ui,
            response,
            identity,
            label,
            kind,
            &mut self.visual_order,
        );
        id.map(|id| (id, directive))
    }

    pub(super) fn combo_options_scope(
        &self,
        anchor_identity: impl AsRef<str>,
    ) -> Option<JumpModeScope> {
        if self.suspended {
            return None;
        }
        self.scope
            .map(|scope| scope.combo_popup_scope(anchor_identity))
    }

    pub(super) fn child_popup_scope(&self, kind: impl Into<String>) -> Option<JumpModeScope> {
        self.scope.map(|scope| scope.child_popup_scope(kind))
    }

    pub(super) fn button(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
    ) {
        let _ = self.register(ui, response, identity, label, TargetKind::Button);
    }

    pub(super) fn text_entry(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
    ) {
        let _ = self.register(ui, response, identity, label, TargetKind::TextEntry);
    }

    pub(super) fn checkbox(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
    ) {
        let _ = self.register(ui, response, identity, label, TargetKind::Checkbox);
    }

    pub(super) fn combo_box(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
    ) {
        let _ = self.register(ui, response, identity, label, TargetKind::ComboBox);
    }

    pub(super) fn selectable(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
    ) {
        let _ = self.register(ui, response, identity, label, TargetKind::Selectable);
    }

    pub(super) fn action_selected(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
        committing: bool,
    ) -> bool {
        let Some((target_id, directive)) =
            self.register(ui, response, identity, label, TargetKind::Action)
        else {
            return false;
        };
        if committing && directive == Some(FocusDirective::Select) {
            response.request_focus();
            if let Some(scope) = self.scope {
                scope.record_select_focus_owner(&target_id, response.id);
            }
            true
        } else {
            false
        }
    }
}

impl Default for RegexTesterDialogState {
    fn default() -> Self {
        Self::with_clipboard(Arc::new(ArboardClipboardBackend))
    }
}

impl RegexTesterDialogState {
    pub fn with_clipboard(clipboard: Arc<dyn ClipboardBackend>) -> Self {
        Self {
            open: false,
            session: RegexSession::default(),
            focus_pattern: false,
            focus_viewport: false,
            information_open: true,
            scroll_to_selected: false,
            information_section: InformationSection::default(),
            test_cases_expanded: true,
            reference: reference::ReferenceState::default(),
            examples: examples::ExamplesState::default(),
            history: None,
            presets: None,
            selected_capture: 0,
            clipboard,
            copy_feedback: None,
            native_jump_mode: JumpModeUi::default(),
            native_jump_frame: None,
            observed_viewport: None,
            #[cfg(test)]
            last_pattern_geometry: None,
            #[cfg(test)]
            last_oversized_preview_geometry: None,
            #[cfg(test)]
            last_header_viewport: None,
            #[cfg(test)]
            last_metadata_geometry: None,
            #[cfg(test)]
            last_pattern_controls: None,
        }
    }

    pub fn open(&mut self) {
        self.session.ensure_initial_evaluation(Instant::now());
        self.focus_viewport = true;
        if !self.open {
            self.focus_pattern = true;
            self.open = true;
        }
    }

    /// Explicit production storage configuration; default construction does no IO.
    pub fn with_data_root(root: &crate::platform::app_data::AppDataRoot) -> Self {
        Self::with_storage_paths(
            root.path().join("regex_history.json"),
            root.path().join("regex_presets.json"),
        )
    }

    /// Explicit file paths also support isolated storage integration tests.
    pub fn with_storage_paths(
        history_path: impl Into<std::path::PathBuf>,
        preset_path: impl Into<std::path::PathBuf>,
    ) -> Self {
        let mut dialog = Self::default();
        dialog.history = Some(history::HistoryState::open(history_path));
        dialog.presets = Some(presets::PresetState::open(preset_path));
        dialog
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.show_with_jump_mode(ctx, None, None, true, true, false);
    }

    pub(crate) fn root_navigation_instance(&self, current: Option<u64>) -> Option<u64> {
        current.filter(|instance| {
            self.observed_viewport != Some((*instance, RegexViewportPresentation::Immediate))
        })
    }

    pub(crate) fn show_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        root_scope: Option<&JumpModeScope>,
        instance: Option<u64>,
        application_input_allowed: bool,
        root_native_owner_current: bool,
        jump_mode_enabled: bool,
    ) {
        if !self.open {
            if self.observed_viewport.take().is_some() {
                self.native_jump_mode.cancel_all();
            }
            self.native_jump_frame = None;
            return;
        }
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("regex_tester_viewport"),
            viewport_builder(),
            |child, class| {
                let process_case_actions = child.current_pass_index() == 0;
                let independent = class == egui::ViewportClass::Immediate;
                let presentation = if independent {
                    RegexViewportPresentation::Immediate
                } else {
                    RegexViewportPresentation::Embedded
                };
                if let Some(instance) = instance {
                    if self.observed_viewport != Some((instance, presentation)) {
                        self.observed_viewport = Some((instance, presentation));
                        self.native_jump_mode.cancel_all();
                        self.native_jump_frame = None;
                    }
                } else {
                    self.native_jump_mode.cancel_all();
                    self.native_jump_frame = None;
                }
                if independent && self.focus_viewport {
                    child.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                self.focus_viewport = false;
                if independent {
                    let close = self.show_native_viewport(
                        child,
                        instance,
                        application_input_allowed,
                        jump_mode_enabled,
                        process_case_actions,
                    );
                    if close {
                        self.open = false;
                        child.send_viewport_cmd(egui::ViewportCommand::Close);
                        self.native_jump_mode.cancel_all();
                        self.native_jump_frame = None;
                    }
                } else {
                    let close = process_case_actions
                        && root_native_owner_current
                        && child.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        });
                    if close {
                        self.open = false;
                        return;
                    }
                    // Standalone egui contexts embed viewports. Keep that fallback
                    // within its parent rather than changing root-window geometry.
                    let size = child.input(|input| input.content_rect().size());
                    let available = (size - egui::vec2(24.0, 48.0)).max(egui::vec2(1.0, 1.0));
                    let mut open = self.open;
                    egui::Window::new("Regex Tester")
                        .id(egui::Id::new("regex_tester_embedded_window"))
                        .open(&mut open)
                        .collapsible(false)
                        .resizable(true)
                        .default_size(available.min(egui::vec2(960.0, 680.0)))
                        .min_size(available.min(egui::vec2(360.0, 240.0)))
                        .max_size(available)
                        .show(child, |ui| {
                            ui.add_enabled_ui(root_native_owner_current, |ui| {
                                self.body_in_viewport_scoped(
                                    ui,
                                    false,
                                    process_case_actions
                                        && application_input_allowed
                                        && root_native_owner_current,
                                    root_scope,
                                    root_native_owner_current,
                                )
                            })
                        });
                    self.open = open;
                }
            },
        );
    }

    fn show_native_viewport(
        &mut self,
        child: &mut egui::Ui,
        instance: Option<u64>,
        application_input_allowed: bool,
        jump_mode_enabled: bool,
        process_actions: bool,
    ) -> bool {
        let ctx = child.ctx().clone();
        let viewport = egui::ViewportId::from_hash_of("regex_tester_viewport");
        let owner = instance.map(|instance| NavigationOwner::Dialog {
            kind: NavigationDialogKind::RegexTester,
            instance,
        });
        let binding = owner.map(|owner| NavigationViewportBinding {
            viewport,
            owner,
            base_layer: egui::LayerId::background(),
        });

        self.native_jump_mode.set_enabled(jump_mode_enabled);
        let frame_nr = ctx.cumulative_frame_nr();
        if self.native_jump_frame != Some(frame_nr) {
            self.native_jump_mode.begin_frame();
            self.native_jump_frame = Some(frame_nr);
        }

        let navigation_owner = binding.map_or(NavigationOwner::Blocked, |binding| {
            self.native_jump_mode.update_navigation_owner_for_viewport(
                &ctx,
                binding,
                application_input_allowed && jump_mode_enabled,
            )
        });
        let jump_owner_current = binding.is_some_and(|binding| {
            navigation_owner == binding.owner
                && self.native_jump_mode.frame_navigation_owner() == binding.owner
        });
        self.native_jump_mode.handle_viewport_input(
            &ctx,
            jump_owner_current,
            ctx.current_pass_index() == 0,
            viewport,
        );
        let native_owner_current = application_input_allowed
            && ctx.viewport_id() == viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(&ctx).is_none();
        let popup_open = egui::Popup::is_any_open(&ctx);
        let close = ctx.input_mut(|input| {
            (process_actions
                && native_owner_current
                && !popup_open
                && (input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)))
                || (process_actions && input.viewport().close_requested())
        });
        if close {
            self.native_jump_mode.cancel_all();
            self.native_jump_frame = None;
            return true;
        }
        self.native_jump_mode.begin_ui_pass();
        let scope = binding.filter(|_| jump_owner_current).and_then(|binding| {
            let NavigationOwner::Dialog { instance, .. } = binding.owner else {
                return None;
            };
            self.native_jump_mode.scope_for_viewport_binding(
                SurfaceId::dialog("regex-tester", instance),
                TargetGeneration(instance),
                "regex-tester/form",
                binding,
            )
        });

        egui::CentralPanel::default().show(child, |ui| {
            ui.add_enabled_ui(native_owner_current, |ui| {
                self.body_in_viewport_scoped(
                    ui,
                    true,
                    process_actions && ctx.current_pass_index() == 0 && native_owner_current,
                    scope.as_ref(),
                    native_owner_current,
                )
            });
        });

        self.native_jump_mode.finish_ui_pass();
        let after_owner = binding.map_or(NavigationOwner::Blocked, |binding| {
            self.native_jump_mode.update_navigation_owner_for_viewport(
                &ctx,
                binding,
                application_input_allowed && jump_mode_enabled,
            )
        });
        if let Some(binding) = binding {
            if self.native_jump_mode.prepare_pass_for_viewport(
                &ctx,
                binding,
                after_owner,
                application_input_allowed,
            ) {
                super::jump_mode::paint::paint(&ctx, &self.native_jump_mode);
            }
            if !ctx.will_discard() {
                self.native_jump_mode.complete_frame_for_viewport(
                    &ctx,
                    binding,
                    after_owner,
                    application_input_allowed,
                );
            }
        } else if !ctx.will_discard() {
            self.native_jump_mode.cancel_all();
        }
        false
    }

    fn body(&mut self, ui: &mut egui::Ui) -> CoreLayout {
        self.body_in_viewport(ui, false, true)
    }

    fn body_in_viewport(
        &mut self,
        ui: &mut egui::Ui,
        independent: bool,
        process_case_actions: bool,
    ) -> CoreLayout {
        self.body_in_viewport_scoped(ui, independent, process_case_actions, None, true)
    }

    fn body_in_viewport_scoped(
        &mut self,
        ui: &mut egui::Ui,
        independent: bool,
        process_case_actions: bool,
        scope: Option<&JumpModeScope>,
        native_owner_current: bool,
    ) -> CoreLayout {
        let preset_confirmation_open = self.information_section == InformationSection::Presets
            && self
                .presets
                .as_ref()
                .is_some_and(presets::PresetState::has_pending_delete_confirmation);
        let mut form_targets = JumpTargets::new(scope);
        let action_scope = scope.map(|scope| {
            scope.child_namespace(format!(
                "document/{}/cases/{}",
                self.session.revision(),
                self.session.test_case_revision()
            ))
        });
        let mut action_targets = JumpTargets::new(action_scope.as_ref());
        if preset_confirmation_open {
            form_targets.suspend();
            action_targets.suspend();
        }
        if process_case_actions
            && native_owner_current
            && let Some(forward) = keyboard::navigation(ui.ctx(), ui.layer_id(), independent)
        {
            let changed = if forward {
                self.session.next_match()
            } else {
                self.session.previous_match()
            };
            if changed {
                self.request_match_scroll(ui);
            }
        }
        let now = Instant::now();
        self.session.ensure_initial_evaluation(now);
        let bounds = ui.available_rect_before_wrap();
        let had_error = self.validation_error().is_some();
        let header_height = pattern_header_height(ui);
        // Recompute after the toggle so collapsing frees editor space this frame.
        let header =
            CoreLayout::with_header_height(bounds, self.information_open, had_error, header_height)
                .header;
        bounded_area(ui, header, "regex_tester_header", |ui| {
            let _output = egui::ScrollArea::vertical()
                .id_salt("regex_tester_header_scroll")
                .auto_shrink([true, false])
                .max_width(header.width())
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    self.pattern_area(
                        ui,
                        &mut form_targets,
                        &mut action_targets,
                        process_case_actions,
                    )
                });
            #[cfg(test)]
            {
                self.last_header_viewport = Some(_output.inner_rect);
            }
        });
        let layout =
            CoreLayout::with_header_height(bounds, self.information_open, had_error, header_height);
        bounded_area(ui, layout.editor, "regex_tester_editor_area", |ui| {
            self.text_area(
                ui,
                &mut form_targets,
                &mut action_targets,
                process_case_actions,
            )
        });
        if self.session.tick(now) {
            self.record_history();
            self.scroll_to_selected = false;
            ui.ctx().request_repaint();
        }
        if self.session.tick_test_cases(now) {
            ui.ctx().request_repaint();
        }
        if let Some(validation) = layout.validation {
            bounded_area(ui, validation, "regex_tester_validation", |ui| {
                self.validation_area(ui)
            });
        }
        if had_error != self.validation_error().is_some() {
            ui.ctx().request_repaint();
        }
        if let Some(information) = layout.information {
            bounded_area(ui, information, "regex_tester_information_area", |ui| {
                self.information_area(
                    ui,
                    process_case_actions,
                    &mut form_targets,
                    &mut action_targets,
                )
            });
        }
        bounded_area(ui, layout.status, "regex_tester_status", |ui| {
            self.status_area(ui, &mut action_targets, process_case_actions)
        });
        if let Some(delay) = self.session.pending_delay(Instant::now()) {
            ui.ctx().request_repaint_after(delay);
        }
        layout
    }

    fn pattern_area(
        &mut self,
        ui: &mut egui::Ui,
        form_targets: &mut JumpTargets<'_>,
        action_targets: &mut JumpTargets<'_>,
        committing: bool,
    ) {
        let font = pattern_font(ui);
        let editor_height = pattern_editor_height(ui, &font);
        #[cfg(test)]
        let metadata_available_width = ui.available_width();
        let _metadata_row = ui.horizontal_wrapped(|ui| {
            secondary_help(ui, "Engine: Rust regex · all matches");
            let label = if self.information_open {
                "Hide info"
            } else {
                "Show info"
            };
            let toggle = ui
                .button(label)
                .on_hover_text("Show or hide match information");
            form_targets.button(
                ui,
                &toggle,
                "information-toggle",
                "Toggle match information",
            );
            if with_button_emphasis(ui, toggle, ButtonEmphasis::Primary).clicked() && committing {
                self.information_open = !self.information_open;
            }
        });
        #[cfg(test)]
        let pattern_row_available_width = ui.available_width();
        #[cfg(test)]
        {
            self.last_metadata_geometry = Some((
                _metadata_row.response.rect,
                metadata_available_width,
                pattern_row_available_width,
            ));
        }
        #[cfg(test)]
        let mut suffix_rect = None;
        ui.horizontal(|ui| {
            compact_section_heading(ui, "Pattern", None);
            if self.session.draft.pattern.len() > self.session.policy().pattern_bytes {
                self.oversized_pattern_area(ui, action_targets, committing);
                return;
            }
            if self.session.draft.pattern.len() > crate::regex_tester::policy::NORMAL_PATTERN_BYTES {
                ui.colored_label(ui.visuals().warn_fg_color, "Large").on_hover_text("Pattern exceeds the normal 1 KiB range; interactive evaluation remains bounded at 4 KiB.");
            }
            ui.label(egui::RichText::new("/").font(font.clone()));
            let suffix = format!("/{}", self.session.draft.flags.suffix());
            let suffix_width = ui.ctx().fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(
                        suffix.clone(),
                        font.clone(),
                        ui.visuals().text_color(),
                    )
                    .size()
                    .x
            });
            let width =
                (ui.available_width() - suffix_width - ui.spacing().item_spacing.x).max(1.0);
            let maximum_bytes = self.session.policy().pattern_bytes;
            let layouter_font = font.clone();
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                editor_layout::plain(ui, text.as_str(), width, maximum_bytes, layouter_font.clone(), true)
            };
            let mut pattern_value = self.session.draft.pattern.clone();
            let pattern = ui.add_sized(
                [width, editor_height],
                egui::TextEdit::singleline(&mut pattern_value)
                    .id(egui::Id::new("regex_tester_pattern"))
                    .font(font.clone())
                    .layouter(&mut layouter)
                    .hint_text("Regular expression"),
            );
            form_targets.text_entry(ui, &pattern, "pattern", "Regular expression");
            if pattern.changed() && committing {
                self.session.draft.pattern = pattern_value;
                self.mark_changed();
            }
            if self.focus_pattern {
                pattern.request_focus();
                pattern.scroll_to_me(Some(egui::Align::Center));
                if committing {
                    self.focus_pattern = false;
                }
            }
            #[cfg(test)]
            {
                self.last_pattern_geometry = Some((pattern.rect, font.clone()));
            }
            let _suffix = ui.label(egui::RichText::new(suffix).font(font));
            #[cfg(test)]
            {
                suffix_rect = Some(_suffix.rect);
            }
        });
        let _flags = ui
            .horizontal_wrapped(|ui| self.flags_area(ui, action_targets, committing))
            .inner;
        #[cfg(test)]
        {
            self.last_pattern_controls = Some((suffix_rect, _flags.map(|response| response.rect)));
        }
    }

    fn flags_area(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut JumpTargets<'_>,
        committing: bool,
    ) -> [egui::Response; 5] {
        compact_section_heading(ui, "Flags", None);
        let mut flags = self.session.draft.flags;
        let responses = [
            ("i", &mut flags.case_insensitive, "Case insensitive"),
            (
                "m",
                &mut flags.multi_line,
                "Multiline: ^ and $ match line boundaries",
            ),
            ("s", &mut flags.dot_matches_new_line, "Dot matches newline"),
            (
                "u",
                &mut flags.unicode,
                "Unicode character classes and matching",
            ),
            (
                "x",
                &mut flags.ignore_whitespace,
                "Ignore pattern whitespace and allow comments",
            ),
        ]
        .map(|(letter, enabled, hint)| ui.checkbox(enabled, letter).on_hover_text(hint));
        for (index, response) in responses.iter().enumerate() {
            targets.checkbox(ui, response, format!("flag/{index}"), "Regex flag");
        }
        if committing && responses.iter().any(egui::Response::changed) {
            self.session.draft.flags = flags;
            self.mark_changed();
        }
        responses
    }

    fn oversized_pattern_area(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut JumpTargets<'_>,
        committing: bool,
    ) {
        let width = (ui.available_width() - 100.0).max(1.0);
        let font = oversized_pattern_font(ui);
        let editor_height = pattern_editor_height_at_baseline(ui, &font, 14.0);
        let mut preview = utf8_prefix(&self.session.draft.pattern, 128);
        let _preview_response = ui.add_sized(
            [width, editor_height],
            egui::TextEdit::singleline(&mut preview)
                .id(egui::Id::new("regex_tester_pattern_preview"))
                .font(font.clone())
                .interactive(false),
        ).on_hover_text(format!(
            "Read-only preview: first 128 bytes or fewer. Pattern is {} bytes; editing and evaluation pause above {} bytes. Original pattern is retained.",
            self.session.draft.pattern.len(), self.session.policy().pattern_bytes
        ));
        #[cfg(test)]
        {
            self.last_oversized_preview_geometry = Some((_preview_response.rect, font));
        }
        let clear = ui.button("Clear pattern");
        targets.button(ui, &clear, "clear-pattern", "Clear pattern");
        if clear.clicked() && committing {
            self.session.draft.pattern.clear();
            self.mark_changed();
            self.focus_pattern = true;
        }
    }

    fn text_area(
        &mut self,
        ui: &mut egui::Ui,
        form_targets: &mut JumpTargets<'_>,
        action_targets: &mut JumpTargets<'_>,
        committing: bool,
    ) {
        ui.label("Input / test text");
        if self.session.draft.test_text.len() > self.session.policy().text_bytes {
            self.oversized_text_area(ui, action_targets, committing);
            return;
        }
        if self.session.draft.test_text.len() > crate::regex_tester::policy::NORMAL_TEXT_BYTES {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Large input: above the normal 16 KiB range.",
            );
        }
        let maximum_bytes = self.session.policy().text_bytes;
        let editor_size = ui.available_size().max(egui::vec2(1.0, 1.0));
        egui::ScrollArea::both()
            .id_salt("regex_tester_text_scroll")
            .auto_shrink([false, false])
            .max_height(editor_size.y)
            .show(ui, |ui| {
                let changed = if committing {
                    let (text, view) = self.session.text_edit_parts();
                    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                        ui.ctx().fonts_mut(|fonts| {
                            fonts.layout_job(
                                editor_layout::oversized_job(text.as_str(), maximum_bytes)
                                    .unwrap_or_else(|| {
                                        highlighting::layout(text.as_str(), &view, ui, width)
                                    }),
                            )
                        })
                    };
                    let output = egui::TextEdit::multiline(text)
                        .id(egui::Id::new("regex_tester_test_text"))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(1)
                        .desired_width(editor_size.x)
                        .min_size(editor_size)
                        .layouter(&mut layouter)
                        .hint_text("Type or paste text to test")
                        .show(ui);
                    form_targets.text_entry(
                        ui,
                        &output.response,
                        "test-text",
                        "Input and test text",
                    );
                    highlighting::paint_markers(ui, &output, text, &view);
                    if std::mem::take(&mut self.scroll_to_selected)
                        && let Some(caret) = highlighting::active_caret(&output, text, &view)
                    {
                        ui.scroll_to_rect(caret.expand(4.0), Some(egui::Align::Center));
                    }
                    output.response.changed()
                } else {
                    let mut text = self.session.draft.test_text.clone();
                    let output = egui::TextEdit::multiline(&mut text)
                        .id(egui::Id::new("regex_tester_test_text"))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(1)
                        .desired_width(editor_size.x)
                        .min_size(editor_size)
                        .hint_text("Type or paste text to test")
                        .show(ui);
                    form_targets.text_entry(
                        ui,
                        &output.response,
                        "test-text",
                        "Input and test text",
                    );
                    false
                };
                if changed && committing {
                    self.mark_changed();
                }
            });
    }

    fn mark_changed(&mut self) {
        self.scroll_to_selected = false;
        self.copy_feedback = None;
        self.session.mark_changed(Instant::now());
    }

    fn request_match_scroll(&mut self, ui: &egui::Ui) {
        self.scroll_to_selected = true;
        ui.ctx().request_repaint();
    }

    fn information_area(
        &mut self,
        ui: &mut egui::Ui,
        process_case_actions: bool,
        form_targets: &mut JumpTargets<'_>,
        targets: &mut JumpTargets<'_>,
    ) {
        let popup_scope = form_targets.combo_options_scope("information-section");
        let mut popup_targets = JumpTargets::new(popup_scope.as_ref());
        let mut section = self.information_section;
        let section_response = egui::ComboBox::from_id_salt("regex_information_section")
            .width(ui.available_width().min(220.0))
            .selected_text(match section {
                InformationSection::Matches => "Matches",
                InformationSection::MatchDetails => "Selected match",
                InformationSection::Explanation => "Explanation",
                InformationSection::Reference => "Reference",
                InformationSection::Examples => "Examples",
                InformationSection::History => "History",
                InformationSection::Presets => "Presets",
                InformationSection::Substitution => "Substitution",
                InformationSection::Clipboard => "Clipboard",
                InformationSection::TestCases => "Test Cases",
            })
            .show_ui(ui, |ui| {
                let response =
                    ui.selectable_value(&mut section, InformationSection::Matches, "Matches");
                popup_targets.selectable(ui, &response, "section/matches", "Matches section");
                let response = ui.selectable_value(
                    &mut section,
                    InformationSection::MatchDetails,
                    "Selected match",
                );
                popup_targets.selectable(ui, &response, "section/details", "Match details section");
                let response = ui.selectable_value(
                    &mut section,
                    InformationSection::Explanation,
                    "Explanation",
                );
                popup_targets.selectable(
                    ui,
                    &response,
                    "section/explanation",
                    "Explanation section",
                );
                let response =
                    ui.selectable_value(&mut section, InformationSection::Reference, "Reference");
                popup_targets.selectable(ui, &response, "section/reference", "Reference section");
                let response =
                    ui.selectable_value(&mut section, InformationSection::Examples, "Examples");
                popup_targets.selectable(ui, &response, "section/examples", "Examples section");
                let response =
                    ui.selectable_value(&mut section, InformationSection::History, "History");
                popup_targets.selectable(ui, &response, "section/history", "History section");
                let response =
                    ui.selectable_value(&mut section, InformationSection::Presets, "Presets");
                popup_targets.selectable(ui, &response, "section/presets", "Presets section");
                let response = ui.selectable_value(
                    &mut section,
                    InformationSection::Substitution,
                    "Substitution",
                );
                popup_targets.selectable(
                    ui,
                    &response,
                    "section/substitution",
                    "Substitution section",
                );
                let response =
                    ui.selectable_value(&mut section, InformationSection::Clipboard, "Clipboard");
                popup_targets.selectable(ui, &response, "section/clipboard", "Clipboard section");
                let response =
                    ui.selectable_value(&mut section, InformationSection::TestCases, "Test Cases");
                popup_targets.selectable(ui, &response, "section/test-cases", "Test case section");
            });
        form_targets.combo_box(
            ui,
            &section_response
                .response
                .on_hover_text("Choose what to inspect or edit"),
            "information-section",
            "Information section",
        );
        if process_case_actions {
            self.information_section = section;
        }
        if section != InformationSection::TestCases
            && let Some(feedback) = &self.copy_feedback
        {
            match feedback {
                Ok(message) => {
                    semantic_status(ui, StatusTone::Success, message, None);
                }
                Err(message) => {
                    semantic_status(ui, StatusTone::Error, message, None);
                }
            }
        }
        if section == InformationSection::MatchDetails {
            self.match_details_area(ui, targets, process_case_actions);
            return;
        }
        if section == InformationSection::Explanation {
            if let Some(result) = self.session.explanation() {
                explanation::show(ui, result);
            } else {
                // Compiler validation is authoritative; pending revisions have
                // no accessible stale analysis.
                secondary_help(ui, &self.summary());
            }
            return;
        }
        if section == InformationSection::Reference {
            let action = self
                .reference
                .show_scoped(ui, targets, process_case_actions);
            if process_case_actions && let Some(action) = action {
                self.reference_action(action);
            }
            return;
        }
        if section == InformationSection::Examples {
            let example = self.examples.show_scoped(ui, targets, process_case_actions);
            if process_case_actions && let Some(example) = example {
                self.load_example(example);
            }
            return;
        }
        if section == InformationSection::History {
            if let Some(history) = &mut self.history {
                let retryable = matches!(
                    self.session.result(),
                    Some(EvaluationResult::Success { .. })
                ) && !self.session.draft.pattern.is_empty();
                let action = history.show_scoped(ui, retryable, targets);
                if process_case_actions && let Some(action) = action {
                    self.history_action(action);
                }
            } else {
                secondary_help(ui, "History storage is not configured.");
            }
            return;
        }
        if section == InformationSection::Presets {
            if let Some(presets) = &mut self.presets {
                let preset_scope = targets.scope.map(|scope| {
                    scope.child_namespace(format!("preset-store/{}", presets.target_generation()))
                });
                let mut preset_targets = JumpTargets::new(preset_scope.as_ref());
                if targets.suspended {
                    preset_targets.suspend();
                }
                let action = presets.show_scoped(ui, &mut preset_targets, process_case_actions);
                if process_case_actions && let Some(action) = action {
                    self.preset_action(action);
                }
            } else {
                secondary_help(ui, "Preset storage is not configured.");
            }
            return;
        }
        if section == InformationSection::Substitution {
            if substitution::show_scoped(ui, &mut self.session, targets, process_case_actions) {
                self.copy_feedback = None;
            }
            return;
        }
        if section == InformationSection::Clipboard {
            let action = clipboard::show_scoped(ui, &self.session, self.selected_capture, targets);
            if process_case_actions && let Some(action) = action {
                self.clipboard_action(action);
            }
            return;
        }
        if section == InformationSection::TestCases {
            test_cases::show_scoped(
                ui,
                &mut self.session,
                &mut self.test_cases_expanded,
                self.clipboard.as_ref(),
                &mut self.copy_feedback,
                process_case_actions,
                form_targets,
                targets,
            );
            return;
        }

        let count = self.session.matches().len();
        if count == 0 {
            secondary_help(ui, &self.summary());
            return;
        }
        let mut selected = None;
        let row_height = ui.spacing().interact_size.y;
        let revision = self.session.revision();
        egui::ScrollArea::vertical()
            .id_salt("regex_tester_information_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show_rows(ui, row_height, count, |ui, rows| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                for index in rows {
                    let matched = &self.session.matches()[index];
                    let matched_id = matched.id.index();
                    let preview = utf8_prefix(&matched.text, 80)
                        .replace('\r', "␍")
                        .replace('\n', "↵");
                    let label = format!(
                        "#{} · {}:{} · {}",
                        index + 1,
                        matched.location.line,
                        matched.location.column,
                        if preview.is_empty() {
                            "(zero width)"
                        } else {
                            &preview
                        }
                    );
                    let response = ui
                        .push_id(("regex_match_result_row", revision, matched_id), |ui| {
                            ui.selectable_label(self.session.selected_index() == Some(index), label)
                        })
                        .inner;
                    let identity = format!("match-result/{revision}/{matched_id}");
                    let selected_by_hint = targets.action_selected(
                        ui,
                        &response,
                        identity,
                        "Match result",
                        process_case_actions,
                    );
                    if (response.clicked() && process_case_actions) || selected_by_hint {
                        selected = Some(index);
                    }
                }
            });
        if let Some(index) = selected {
            self.session.select_match(index);
            self.request_match_scroll(ui);
        }
    }

    fn match_details_area(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut JumpTargets<'_>,
        committing: bool,
    ) {
        let Some(index) = self.session.selected_index() else {
            ui.weak(self.summary());
            return;
        };
        let mut copy = None;
        let matched = &self.session.matches()[index];
        let revision = self.session.revision();
        let matched_id = matched.id.index();
        egui::ScrollArea::vertical()
            .id_salt("regex_match_details_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.push_id(("regex_match_details", revision, matched_id), |ui| {
                    ui.set_max_width(ui.available_width());
                    copy = inspection::show_scoped(
                        ui,
                        matched,
                        &mut self.selected_capture,
                        targets,
                        revision,
                        committing,
                    );
                });
            });
        if let Some(target) = copy {
            self.copy_selected(target);
        }
    }

    fn copy_selected(&mut self, target: inspection::CopyTarget) {
        let text = self
            .session
            .selected_index()
            .and_then(|index| self.session.matches().get(index))
            .and_then(|matched| inspection::copy_text(matched, target));
        self.copy_feedback = Some(match text {
            Some(text) => copy_text(
                self.clipboard.as_ref(),
                text,
                match target {
                    inspection::CopyTarget::Match => "Copied match",
                    inspection::CopyTarget::Capture(_) => "Copied capture",
                },
            ),
            None => Err("The selected match or capture has no matched value".into()),
        });
    }

    fn clipboard_action(&mut self, action: clipboard::ClipboardAction) {
        use clipboard::ClipboardAction as Action;
        match action {
            Action::ImportText => match self.clipboard.read_text() {
                Ok(text) => {
                    self.session.draft.test_text = text;
                    self.mark_changed();
                    self.copy_feedback = Some(Ok("Imported clipboard text"));
                }
                Err(error) => {
                    self.copy_feedback = Some(Err(format!(
                        "Could not import: {}",
                        clipboard::error_message(&error)
                    )));
                }
            },
            Action::CopyMatch => self.copy_selected(inspection::CopyTarget::Match),
            Action::CopyCapture(index) => {
                self.copy_selected(inspection::CopyTarget::Capture(index))
            }
            Action::CopyPattern => {
                self.copy_feedback = Some(copy_text(
                    self.clipboard.as_ref(),
                    &self.session.draft.pattern,
                    "Copied pattern",
                ));
            }
            Action::CopyAllMatches => {
                let matches = self.session.matches();
                self.copy_feedback = Some(if matches.is_empty() {
                    Err("No current matches to copy.".into())
                } else {
                    let text = clipboard::all_matches(matches);
                    copy_text(self.clipboard.as_ref(), &text, "Copied displayed matches")
                });
            }
            Action::CopyReplacementResult => {
                self.copy_feedback =
                    Some(match self.session.substitution_result() {
                        Some(crate::regex_tester::SubstitutionEvaluationResult::Success(
                            result,
                        )) if self.session.substitution_enabled() => copy_text(
                            self.clipboard.as_ref(),
                            &result.output,
                            "Copied replacement result",
                        ),
                        _ => Err("No complete current replacement result to copy.".into()),
                    });
            }
            Action::CopyMatchInformation => {
                self.copy_feedback = Some(
                    match self
                        .session
                        .selected_index()
                        .and_then(|index| self.session.matches().get(index))
                    {
                        Some(matched) => {
                            let text = clipboard::match_information(matched);
                            copy_text(self.clipboard.as_ref(), &text, "Copied match information")
                        }
                        None => Err("No current selected match information to copy.".into()),
                    },
                );
            }
        }
    }

    fn reference_action(&mut self, action: reference::ReferenceAction) {
        match action {
            reference::ReferenceAction::Append(entry) => {
                self.session.draft.pattern.push_str(entry.syntax);
                self.mark_changed();
            }
            reference::ReferenceAction::Copy(entry) => {
                self.copy_feedback = Some(copy_text(
                    self.clipboard.as_ref(),
                    entry.syntax,
                    "Copied syntax",
                ));
            }
        }
    }

    fn load_example(&mut self, example: &crate::regex_tester::RegexExample) {
        let test_cases = self.session.draft.test_cases.clone();
        self.session.draft = example.to_draft();
        self.session.draft.test_cases = test_cases;
        self.mark_changed();
    }

    fn record_history(&mut self) {
        if let Some(history) = &mut self.history {
            history.record_success(&self.session);
        }
    }

    fn history_action(&mut self, action: history::HistoryAction) {
        let Some(history) = &mut self.history else {
            return;
        };
        match action {
            history::HistoryAction::Load(index) => {
                if let Some(entry) = history.store.entries().get(index) {
                    self.session.draft.pattern.clone_from(&entry.pattern);
                    self.session.draft.flags = entry.flags;
                    self.mark_changed();
                }
            }
            history::HistoryAction::Reload => {
                let _ = history.reload();
            }
            history::HistoryAction::Retry => {
                history.retry(&self.session);
            }
        }
    }

    fn preset_action(&mut self, action: presets::PresetAction) {
        if self
            .presets
            .as_mut()
            .is_some_and(|presets| presets.handle(action, &mut self.session.draft))
        {
            self.mark_changed();
            self.session.mark_test_cases_changed(Instant::now());
        }
    }

    fn validation_error(&self) -> Option<&str> {
        match self.session.result() {
            Some(EvaluationResult::InvalidPattern(error)) => Some(&error.message),
            _ => None,
        }
    }

    fn validation_area(&self, ui: &mut egui::Ui) {
        let Some(message) = self.validation_error() else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("regex_tester_validation_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.add(presentation::compiler_label(
                    message,
                    ui.visuals().error_fg_color,
                ));
            });
    }

    fn oversized_text_area(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut JumpTargets<'_>,
        committing: bool,
    ) {
        egui::ScrollArea::vertical()
            .id_salt("regex_tester_oversized_preview_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.colored_label(ui.visuals().warn_fg_color, format!(
                    "Text is {} bytes; live editing and evaluation pause above {} bytes. Original text is retained.",
                    self.session.draft.test_text.len(), self.session.policy().text_bytes
                ));
                let clear = ui.button("Clear text to resume editing");
                targets.button(ui, &clear, "clear-oversized-text", "Clear text");
                if clear.clicked() && committing {
                    self.session.draft.test_text.clear();
                    self.mark_changed();
                    return;
                }
                ui.weak("Read-only preview: first 4096 bytes or fewer");
                ui.add(egui::Label::new(egui::RichText::new(self.preview_text()).monospace()).wrap_mode(egui::TextWrapMode::Wrap));
            });
    }

    fn preview_text(&self) -> &str {
        utf8_prefix(&self.session.draft.test_text, 4096)
    }

    fn summary(&self) -> String {
        match self.session.result() {
            None => "Waiting for edits to settle".into(),
            Some(EvaluationResult::Success {
                matches,
                completeness: MatchCompleteness::Complete,
            }) => format!("{} matches", matches.len()),
            Some(EvaluationResult::Success {
                matches,
                completeness: MatchCompleteness::Truncated { at_least, .. },
            }) => format!("At least {at_least} matches; showing {}", matches.len()),
            Some(EvaluationResult::InvalidPattern(_)) => "Invalid pattern".into(),
            Some(EvaluationResult::Suspended(limit)) => {
                format!("Evaluation paused: {}", presentation::suspension(limit))
            }
        }
    }

    fn navigation_summary(&self) -> String {
        let Some(index) = self.session.selected_index() else {
            return self.summary();
        };
        let displayed = if matches!(
            self.session.result(),
            Some(EvaluationResult::Success {
                completeness: MatchCompleteness::Truncated { .. },
                ..
            })
        ) {
            " displayed"
        } else {
            ""
        };
        format!(
            "Match {} of {}{displayed}",
            index + 1,
            self.session.matches().len()
        )
    }

    fn status_area(&mut self, ui: &mut egui::Ui, targets: &mut JumpTargets<'_>, committing: bool) {
        ui.horizontal(|ui| {
            let enabled = !self.session.matches().is_empty();
            let previous = ui
                .add_enabled(enabled, egui::Button::new("Previous"))
                .on_hover_text("Previous match (Shift+F3)");
            targets.button(ui, &previous, "previous-match", "Previous match");
            if committing && previous.clicked() && self.session.previous_match() {
                self.request_match_scroll(ui);
            }
            let next = ui
                .add_enabled(enabled, egui::Button::new("Next"))
                .on_hover_text("Next match (F3)");
            targets.button(ui, &next, "next-match", "Next match");
            if committing && next.clicked() && self.session.next_match() {
                self.request_match_scroll(ui);
            }
            ui.add(
                egui::Label::new(egui::RichText::new(self.navigation_summary()).weak()).truncate(),
            )
            .on_hover_text(self.summary());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_tester_theme_scaled_layout_keeps_core_regions_and_source_bounded() {
        // Representative states, not a pixel matrix: each stresses a distinct
        // presentation path and retains the logical draft at different scales.
        for (size, scale, light, overall_scale, pattern, text, section, expanded) in [
            (
                egui::vec2(960.0, 680.0),
                1.0,
                false,
                1.0,
                "a{1,2}".repeat(400),
                "a".repeat(4000),
                InformationSection::MatchDetails,
                true,
            ),
            (
                egui::vec2(360.0, 240.0),
                1.5,
                true,
                0.75,
                "missing".into(),
                "é🦀".repeat(400),
                InformationSection::Substitution,
                true,
            ),
            (
                egui::vec2(1280.0, 800.0),
                2.0,
                false,
                2.0,
                ".".into(),
                "a".repeat(1001),
                InformationSection::Matches,
                true,
            ),
            (
                egui::vec2(360.0, 240.0),
                2.0,
                true,
                2.0,
                String::new(),
                String::new(),
                InformationSection::Matches,
                false,
            ),
            (
                egui::vec2(360.0, 240.0),
                1.0,
                false,
                2.0,
                "[".into(),
                "source".into(),
                InformationSection::Explanation,
                true,
            ),
            (
                egui::vec2(360.0, 240.0),
                1.5,
                false,
                0.75,
                "[".into(),
                "source".into(),
                InformationSection::Explanation,
                true,
            ),
        ] {
            let ctx = egui::Context::default();
            ctx.set_visuals(if light {
                egui::Visuals::light()
            } else {
                egui::Visuals::dark()
            });
            let mut theme = crate::settings::ThemeSettings::default();
            theme.preferences.typography_scale = overall_scale;
            crate::gui::theme::publish_committed(&ctx, &theme);
            if (overall_scale - 2.0).abs() < f32::EPSILON {
                let mut style = (*ctx.style_of(ctx.theme())).clone();
                style.override_font_id = Some(egui::FontId::proportional(33.0));
                ctx.set_style_of(ctx.theme(), style);
            }
            let mut dialog = RegexTesterDialogState::default();
            dialog.session.draft.pattern = pattern;
            dialog.session.draft.test_text = text;
            dialog.session.draft.replacement = "${name}\n$1".into();
            dialog.information_section = section;
            dialog.information_open = expanded;
            let now = Instant::now();
            dialog.session.mark_changed(now);
            dialog.session.set_substitution_enabled(true, now);
            dialog
                .session
                .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
            let draft = dialog.session.draft.clone();
            let revision = dialog.session.revision();
            let mut core_geometry = None;
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .expect("default raw input includes the root viewport")
                .native_pixels_per_point = Some(scale);
            ctx.run_ui(
                input,
                |ctx| {
                    assert_eq!(ctx.viewport_rect().size(), size);
                    assert!((ctx.pixels_per_point() - scale).abs() < 0.001);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        let layout = dialog.body(ui);
                        core_geometry = Some((bounds, layout.header));
                        if let Some((pattern_rect, ..)) = &dialog.last_pattern_geometry {
                            assert!(
                                pattern_rect.left() >= layout.header.left() - 0.1
                                    && pattern_rect.right() <= layout.header.right() + 0.1,
                                "the pattern editor remains within the horizontally bounded header at overall scale {overall_scale}: header={:?}, pattern={pattern_rect:?}, metadata={:?}",
                                layout.header,
                                dialog.last_metadata_geometry
                            );
                        }
                        if let Some((metadata_rect, before, after)) =
                            dialog.last_metadata_geometry
                        {
                            assert!(before <= layout.header.width() + 0.1);
                            assert!(after <= layout.header.width() + 0.1);
                            assert!(metadata_rect.left() >= layout.header.left() - 0.1);
                            assert!(metadata_rect.right() <= layout.header.right() + 0.1);
                        }
                        for area in [
                            Some(layout.header),
                            layout.validation,
                            Some(layout.editor),
                            layout.information,
                            Some(layout.status),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            assert!(bounds.contains_rect(area));
                            assert!(area.width() > 0.0 && area.height() > 0.0);
                        }
                        if let Some(info) = layout.information {
                            assert!(!layout.editor.intersects(info));
                            if size.x < 680.0 && layout.validation.is_none() {
                                assert!(
                                    info.height() >= 2.0 * ui.spacing().interact_size.y,
                                    "stacked information must fit its selector and content"
                                );
                            }
                        }
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            )
            .drop_without_applying_deltas();
            let (pattern_rect, pattern_font) = dialog
                .last_pattern_geometry
                .as_ref()
                .expect("pattern editor records its actual layout font and rectangle");
            let (core_bounds, header_rect) =
                core_geometry.expect("the central panel supplies bounded core geometry");
            assert!(
                egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(*pattern_rect),
                "pattern input must stay inside the viewport {size:?}; viewport={:?}, scale={scale}, core={core_bounds:?}, header={header_rect:?}, actual={pattern_rect:?}",
                ctx.viewport_rect()
            );
            assert!(
                (pattern_font.size - 18.0 * overall_scale).abs() < 0.01,
                "regex pattern retains its 18 pt baseline while applying overall typography once: actual={}, scale={overall_scale}",
                pattern_font.size
            );
            let style = ctx.style_of(ctx.theme());
            let text_height = ctx.fonts_mut(|fonts| fonts.row_height(pattern_font))
                + style.spacing.extra_text_line_spacing;
            let expected_height = (28.0 * overall_scale)
                .max(text_height)
                .max(style.spacing.interact_size.y);
            assert!(
                pattern_rect.height() >= expected_height - 0.1,
                "pattern input height must accommodate its effective font"
            );
            assert_eq!(dialog.session.draft, draft);
            assert_eq!(dialog.session.revision(), revision);
        }
    }

    fn render_scaled_regex_frame(
        ctx: &egui::Context,
        dialog: &mut RegexTesterDialogState,
        size: egui::Vec2,
        pixels_per_point: f32,
        events: Vec<egui::Event>,
    ) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("default raw input includes the root viewport")
            .native_pixels_per_point = Some(pixels_per_point);
        ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn regex_narrow_header_keeps_pattern_and_flags_reachable_at_200_percent() {
        let size = egui::vec2(360.0, 240.0);
        let pixels_per_point = 2.0;
        let ctx = egui::Context::default();
        let mut theme = crate::settings::ThemeSettings::default();
        theme.preferences.typography_scale = 2.0;
        crate::gui::theme::publish_committed(&ctx, &theme);
        let mut style = (*ctx.style_of(ctx.theme())).clone();
        style.animation_time = 0.0;
        style.scroll_animation = egui::style::ScrollAnimation::none();
        ctx.set_style_of(ctx.theme(), style);

        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "[".into();
        dialog.session.draft.test_text = "source".into();
        dialog.information_section = InformationSection::Explanation;
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.open();

        for _ in 0..3 {
            render_scaled_regex_frame(&ctx, &mut dialog, size, pixels_per_point, Vec::new());
        }
        let header_viewport = dialog
            .last_header_viewport
            .expect("the header scroll area records its actual viewport");
        let pattern_rect = dialog
            .last_pattern_geometry
            .as_ref()
            .expect("the focused pattern editor is laid out")
            .0;
        assert!(
            header_viewport.contains_rect(pattern_rect),
            "opening the dialog scrolls the focused pattern into view: viewport={header_viewport:?}, pattern={pattern_rect:?}"
        );
        let suffix_rect = dialog
            .last_pattern_controls
            .as_ref()
            .and_then(|(suffix, _)| *suffix)
            .expect("the editable pattern keeps its flag suffix visible in the pattern row");
        assert!(header_viewport.contains_rect(suffix_rect));
        assert!(ctx.memory(|memory| memory.has_focus(egui::Id::new("regex_tester_pattern"))));

        let wheel_position = egui::pos2(header_viewport.center().x, header_viewport.center().y);
        render_scaled_regex_frame(
            &ctx,
            &mut dialog,
            size,
            pixels_per_point,
            vec![
                egui::Event::PointerMoved(wheel_position),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -1200.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        render_scaled_regex_frame(&ctx, &mut dialog, size, pixels_per_point, Vec::new());
        let header_viewport = dialog.last_header_viewport.unwrap();
        let (_, flag_rects) = *dialog
            .last_pattern_controls
            .as_ref()
            .expect("flags and suffix retain response geometry");
        assert!(
            flag_rects
                .iter()
                .all(|rect| header_viewport.contains_rect(*rect)),
            "scrolling the bounded header reveals every flag control: viewport={header_viewport:?}, flags={flag_rects:?}"
        );
        assert!(flag_rects[0].left() >= header_viewport.left() - 0.1);
        assert!(flag_rects[4].right() <= header_viewport.right() + 0.1);

        let revision = dialog.session.revision();
        let flag_position = flag_rects[0].center();
        render_scaled_regex_frame(
            &ctx,
            &mut dialog,
            size,
            pixels_per_point,
            vec![
                egui::Event::PointerMoved(flag_position),
                egui::Event::PointerButton {
                    pos: flag_position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        render_scaled_regex_frame(
            &ctx,
            &mut dialog,
            size,
            pixels_per_point,
            vec![egui::Event::PointerButton {
                pos: flag_position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(dialog.session.draft.flags.case_insensitive);
        assert_eq!(dialog.session.revision(), revision + 1);
    }

    fn keyboard_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn keyboard_frame(
        dialog: &mut RegexTesterDialogState,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 680.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
            },
        )
        .drop_without_applying_deltas();
    }

    fn keyboard_dialog() -> RegexTesterDialogState {
        let mut dialog = RegexTesterDialogState::default();
        dialog.information_section = InformationSection::Substitution;
        dialog.session.draft.pattern = "one".into();
        dialog.session.draft.test_text = "one one one".into();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog.session.set_substitution_enabled(true, now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog
    }

    #[test]
    fn regex_tester_keyboard_f3_wraps_without_changing_source_or_cursor() {
        let ctx = egui::Context::default();
        let mut dialog = keyboard_dialog();
        keyboard_frame(&mut dialog, &ctx, Vec::new());
        let id = egui::Id::new("regex_tester_test_text");
        ctx.memory_mut(|memory| memory.request_focus(id));
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        let selection = egui::text::CCursorRange::one(egui::text::CCursor::new(2));
        state.cursor.set_char_range(Some(selection));
        state.store(&ctx, id);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        for (modifiers, index) in [
            (egui::Modifiers::SHIFT, 2),
            (egui::Modifiers::NONE, 0),
            (egui::Modifiers::NONE, 1),
        ] {
            keyboard_frame(
                &mut dialog,
                &ctx,
                vec![keyboard_event(egui::Key::F3, modifiers)],
            );
            assert_eq!(dialog.session.selected_index(), Some(index));
            assert!(!dialog.scroll_to_selected);
            assert!(!ctx.input(|input| input.events.iter().any(|event| matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::F3,
                    ..
                }
            ))));
            assert_eq!(
                egui::TextEdit::load_state(&ctx, id)
                    .unwrap()
                    .cursor
                    .char_range(),
                Some(selection)
            );
        }
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
    }

    #[test]
    fn regex_tester_keyboard_tab_traverses_source_and_replacement_without_edits() {
        let ctx = egui::Context::default();
        let mut dialog = keyboard_dialog();
        keyboard_frame(&mut dialog, &ctx, Vec::new());
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        for id in [
            "regex_tester_pattern",
            "regex_tester_test_text",
            "regex_substitution_replacement",
        ] {
            for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
                let id = egui::Id::new(id);
                ctx.memory_mut(|memory| memory.request_focus(id));
                keyboard_frame(
                    &mut dialog,
                    &ctx,
                    vec![keyboard_event(egui::Key::Tab, modifiers)],
                );
                // egui applies previous-widget focus on the following frame.
                keyboard_frame(&mut dialog, &ctx, Vec::new());
                assert_ne!(ctx.memory(|memory| memory.focused()), Some(id));
                assert!(ctx.memory(|memory| memory.focused()).is_some());
            }
        }
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
    }

    #[test]
    fn regex_tester_keyboard_modified_f3_and_popup_are_not_claimed() {
        let ctx = egui::Context::default();
        let mut dialog = keyboard_dialog();
        keyboard_frame(&mut dialog, &ctx, Vec::new());
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("regex_tester_test_text")));
        for modifiers in [
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT,
            egui::Modifiers::COMMAND,
        ] {
            keyboard_frame(
                &mut dialog,
                &ctx,
                vec![keyboard_event(egui::Key::F3, modifiers)],
            );
            assert_eq!(dialog.session.selected_index(), Some(0));
            assert!(ctx.input(|input| input.events.iter().any(|event| matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::F3,
                    ..
                }
            ))));
        }
        egui::Popup::open_id(&ctx, egui::Id::new("foreign_popup"));
        keyboard_frame(
            &mut dialog,
            &ctx,
            vec![keyboard_event(egui::Key::F3, egui::Modifiers::NONE)],
        );
        assert_eq!(dialog.session.selected_index(), Some(0));
        assert!(ctx.input(|input| input.events.iter().any(|event| matches!(
            event,
            egui::Event::Key {
                key: egui::Key::F3,
                ..
            }
        ))));
    }

    #[test]
    fn regex_tester_keyboard_foreign_layer_and_embedded_no_focus_leave_f3_available() {
        let ctx = egui::Context::default();
        let mut dialog = keyboard_dialog();
        let foreign_id = egui::Id::new("foreign_editor");
        let mut foreign = String::new();
        let mut run = |events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::Area::new(egui::Id::new("foreign_layer")).show(ctx, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut foreign).id(foreign_id));
                    });
                    egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
                },
            )
            .drop_without_applying_deltas();
        };
        run(Vec::new());
        ctx.memory_mut(|memory| memory.request_focus(foreign_id));
        run(vec![keyboard_event(egui::Key::F3, egui::Modifiers::NONE)]);
        assert!(ctx.input(|input| input.events.iter().any(|event| matches!(
            event,
            egui::Event::Key {
                key: egui::Key::F3,
                ..
            }
        ))));
        ctx.memory_mut(|memory| memory.surrender_focus(foreign_id));
        run(vec![keyboard_event(egui::Key::F3, egui::Modifiers::SHIFT)]);
        assert!(ctx.input(|input| input.events.iter().any(|event| matches!(
            event,
            egui::Event::Key {
                key: egui::Key::F3,
                ..
            }
        ))));
        assert_eq!(dialog.session.selected_index(), Some(0));
    }

    #[test]
    fn regex_tester_same_frame_paste_retains_full_unicode_buffers_in_all_three_editors() {
        let text = "🦀".repeat(256 * 1024);
        for (editor, id) in [
            (0, "regex_tester_pattern"),
            (1, "regex_tester_test_text"),
            (2, "regex_substitution_replacement"),
        ] {
            let mut dialog = RegexTesterDialogState::default();
            dialog.information_section = InformationSection::Substitution;
            let now = Instant::now();
            dialog.session.set_substitution_enabled(true, now);
            dialog
                .session
                .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
            let ctx = egui::Context::default();
            let run = |dialog: &mut RegexTesterDialogState, events: Vec<egui::Event>| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(960.0, 680.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            dialog.body(ui);
                        });
                    },
                )
                .drop_without_applying_deltas();
            };
            run(&mut dialog, Vec::new());
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
            run(&mut dialog, vec![egui::Event::Paste(text.clone())]);
            let buffer = |dialog: &RegexTesterDialogState| {
                match editor {
                    0 => dialog.session.draft.pattern.as_str(),
                    1 => dialog.session.draft.test_text.as_str(),
                    _ => dialog.session.draft.replacement.as_str(),
                }
                .to_owned()
            };
            assert_eq!(buffer(&dialog), text, "editor {editor} dropped pasted data");
            assert!(dialog.session.pending_delay(Instant::now()).is_none());
            if editor == 2 {
                assert!(matches!(
                    dialog.session.substitution_result(),
                    Some(crate::regex_tester::SubstitutionEvaluationResult::Suspended(_))
                ));
            } else {
                assert!(matches!(
                    dialog.session.result(),
                    Some(EvaluationResult::Suspended(_))
                ));
            }
            run(&mut dialog, Vec::new());
            assert_eq!(buffer(&dialog), text);
            assert!(
                !dialog
                    .session
                    .tick(Instant::now() + std::time::Duration::from_secs(1))
            );
            // Explicit recovery preserves normal editing through the same ID.
            match editor {
                0 => dialog.session.draft.pattern.clear(),
                1 => dialog.session.draft.test_text.clear(),
                _ => dialog.session.draft.replacement.clear(),
            }
            if editor == 2 {
                dialog.session.mark_replacement_changed(Instant::now());
            } else {
                dialog.session.mark_changed(Instant::now());
            }
            run(&mut dialog, Vec::new());
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
            run(&mut dialog, vec![egui::Event::Paste("é".into())]);
            assert_eq!(buffer(&dialog), "é");
        }
    }

    #[test]
    fn regex_tester_clipboard_full_replacement_copy_requires_current_success_and_render_has_no_io()
    {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.information_section = InformationSection::Clipboard;
        let now = Instant::now();
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(600);
        dialog.session.draft.replacement = "🦀".repeat(10);
        dialog.session.set_substitution_enabled(true, now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let Some(crate::regex_tester::SubstitutionEvaluationResult::Success(result)) =
            dialog.session.substitution_result()
        else {
            panic!("expected complete result");
        };
        let full_output = result.output.clone();
        assert!(full_output.len() > substitution::RESULT_PREVIEW_BYTES);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        assert_eq!(*backend.contents.lock().unwrap(), full_output);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
        dialog.session.draft.replacement = "Z".into();
        dialog.session.mark_replacement_changed(now);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        dialog.session.set_substitution_enabled(false, now);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        dialog.session.set_substitution_enabled(true, now);
        dialog.session.draft.pattern = "[".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.replacement = "X".repeat(4000);
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(matches!(
            dialog.session.substitution_result(),
            Some(crate::regex_tester::SubstitutionEvaluationResult::Suspended(_))
        ));
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        dialog.open();
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        dialog.body(ui);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_clipboard_import_reads_once_preserves_other_fields_and_handles_errors_and_limits()
     {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.session.draft.pattern = "/é/".into();
        dialog.session.draft.flags.case_insensitive = true;
        dialog.session.draft.test_text = "old private text".into();
        dialog.session.draft.replacement = "$1 replacement".into();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let old = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        *backend.contents.lock().unwrap() = "é🦀\ninput".into();
        dialog.clipboard_action(clipboard::ClipboardAction::ImportText);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
        assert_eq!(dialog.session.draft.test_text, "é🦀\ninput");
        assert_eq!(dialog.session.draft.pattern, old.pattern);
        assert_eq!(dialog.session.draft.flags, old.flags);
        assert_eq!(dialog.session.draft.replacement, old.replacement);
        assert_eq!(dialog.session.revision(), revision + 1);
        assert!(dialog.session.result().is_none());
        assert!(dialog.session.explanation().is_none());
        assert_eq!(*backend.contents.lock().unwrap(), "é🦀\ninput");
        let imported = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        for error in [
            ClipboardError::NonText,
            ClipboardError::Busy("held by another app".into()),
        ] {
            *backend.read_error.lock().unwrap() = Some(error);
            dialog.clipboard_action(clipboard::ClipboardAction::ImportText);
            assert_eq!(dialog.session.draft, imported);
            assert_eq!(dialog.session.revision(), revision);
            assert!(dialog.copy_feedback.as_ref().unwrap().is_err());
            assert_eq!(*backend.contents.lock().unwrap(), "é🦀\ninput");
        }
        assert_eq!(backend.reads.load(Ordering::SeqCst), 3);
        *backend.read_error.lock().unwrap() = None;
        let oversized = "é🦀".repeat(dialog.session.policy().text_bytes);
        *backend.contents.lock().unwrap() = oversized.clone();
        dialog.clipboard_action(clipboard::ClipboardAction::ImportText);
        assert_eq!(dialog.session.draft.test_text, oversized);
        assert!(matches!(
            dialog.session.result(),
            Some(EvaluationResult::Suspended(_))
        ));
        assert!(dialog.session.pending_delay(now).is_none());
        assert_eq!(backend.reads.load(Ordering::SeqCst), 4);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
        assert_eq!(
            clipboard::error_message(&ClipboardError::NonText),
            "Clipboard does not contain text."
        );
    }

    #[test]
    fn regex_tester_clipboard_copies_exact_values_information_and_rejects_stale_or_absent_values() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.draft.pattern = "/é/".into();
        dialog.clipboard_action(clipboard::ClipboardAction::CopyPattern);
        assert_eq!(*backend.contents.lock().unwrap(), "/é/");
        dialog.session.draft.pattern = "(?P<letter>é)(🦀)(z)?()".into();
        dialog.session.draft.test_text = "é🦀".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        for (action, expected) in [
            (clipboard::ClipboardAction::CopyMatch, "é🦀"),
            (clipboard::ClipboardAction::CopyCapture(0), "é"),
            (clipboard::ClipboardAction::CopyCapture(3), ""),
            (clipboard::ClipboardAction::CopyAllMatches, "é🦀"),
        ] {
            dialog.clipboard_action(action);
            assert_eq!(*backend.contents.lock().unwrap(), expected);
        }
        dialog.clipboard_action(clipboard::ClipboardAction::CopyMatchInformation);
        let information = backend.contents.lock().unwrap().clone();
        for label in [
            "Line 1, column 1 (Unicode scalars)",
            "UTF-8 bytes 0..6 (end exclusive)",
            "Text:\né🦀",
            "Capture #1 (letter)",
            "Capture #2",
            "Unmatched optional capture",
            "Matched empty capture",
        ] {
            assert!(information.contains(label), "missing {label}");
        }
        let writes = backend.attempts.load(Ordering::SeqCst);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyCapture(2));
        assert_eq!(backend.attempts.load(Ordering::SeqCst), writes);
        assert_eq!(dialog.session.draft, draft);
        dialog.session.draft.pattern = "é|🦀".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyAllMatches);
        assert_eq!(*backend.contents.lock().unwrap(), "é\n🦀");
        dialog.session.draft.pattern = "^|$".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyAllMatches);
        assert_eq!(*backend.contents.lock().unwrap(), "\n");
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(1001);
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert_eq!(
            clipboard::all_matches_label(&dialog.session),
            "Copy displayed matches"
        );
        dialog.clipboard_action(clipboard::ClipboardAction::CopyAllMatches);
        assert_eq!(
            *backend.contents.lock().unwrap(),
            vec!["a"; 1000].join("\n")
        );
        let writes = backend.attempts.load(Ordering::SeqCst);
        dialog.session.mark_changed(now);
        for action in [
            clipboard::ClipboardAction::CopyMatch,
            clipboard::ClipboardAction::CopyCapture(0),
            clipboard::ClipboardAction::CopyAllMatches,
            clipboard::ClipboardAction::CopyMatchInformation,
        ] {
            dialog.clipboard_action(action);
        }
        assert_eq!(backend.attempts.load(Ordering::SeqCst), writes);
        *backend.contents.lock().unwrap() = "keep existing clipboard".into();
        backend.fail.store(true, Ordering::SeqCst);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyPattern);
        assert_eq!(*backend.contents.lock().unwrap(), "keep existing clipboard");
        assert!(
            dialog
                .copy_feedback
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("Clipboard is busy")
        );
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_substitution_bounds_readonly_preview_and_retains_oversized_replacement() {
        let mut dialog = RegexTesterDialogState::default();
        dialog.information_section = InformationSection::Substitution;
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(600);
        dialog.session.draft.replacement = "🦀".repeat(10);
        let now = Instant::now();
        dialog.session.set_substitution_enabled(true, now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let Some(crate::regex_tester::SubstitutionEvaluationResult::Success(result)) =
            dialog.session.substitution_result()
        else {
            panic!("expected replacement result");
        };
        assert_eq!(result.output.len(), 24_000);
        assert_eq!(
            substitution::preview(&result.output).len(),
            substitution::RESULT_PREVIEW_BYTES
        );
        assert_eq!(substitution::preview("é🦀"), "é🦀");
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        for oversized in [false, true] {
            if oversized {
                dialog.session.draft.replacement =
                    "é🦀".repeat(dialog.session.policy().replacement_bytes);
                dialog.session.mark_replacement_changed(now);
            }
            let retained_replacement = dialog.session.draft.replacement.clone();
            for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let bounds = ui.available_rect_before_wrap();
                            dialog.body(ui);
                            assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                            assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                        });
                    },
                )
                .drop_without_applying_deltas();
            }
            assert_eq!(dialog.session.draft.pattern, draft.pattern);
            assert_eq!(dialog.session.draft.flags, draft.flags);
            assert_eq!(dialog.session.draft.test_text, draft.test_text);
            assert_eq!(dialog.session.draft.replacement, retained_replacement);
            assert_eq!(dialog.session.revision(), revision);
            assert_eq!(dialog.session.matches().len(), 600);
            assert!(dialog.session.pending_delay(now).is_none());
        }
    }

    #[test]
    fn regex_tester_presets_load_once_and_management_render_reopen_preserve_document() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("history.json");
        let preset_path = dir.path().join("presets.json");
        let mut store = crate::regex_tester::PresetStore::open(&preset_path);
        let id = store
            .create(crate::regex_tester::PresetInput {
                name: "Pattern only".into(),
                pattern: "é".into(),
                flags: crate::regex_tester::RegexFlags {
                    case_insensitive: true,
                    ..Default::default()
                },
                sample_text: None,
                replacement: None,
                test_cases: crate::regex_tester::RegexTestCases::default(),
            })
            .unwrap();
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_storage_paths(&history_path, &preset_path);
        dialog.clipboard = backend.clone();
        dialog.information_section = InformationSection::Presets;
        let now = Instant::now();
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "current private test".into();
        dialog.session.draft.replacement = "current replacement".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let revision = dialog.session.revision();
        dialog.preset_action(presets::PresetAction::CancelDelete);
        dialog.preset_action(presets::PresetAction::Reload);
        assert_eq!(dialog.session.revision(), revision);
        assert!(dialog.session.result().is_some());
        dialog.scroll_to_selected = true;
        dialog.copy_feedback = Some(Ok("Old feedback"));
        dialog.preset_action(presets::PresetAction::Load(id));
        assert_eq!(dialog.session.revision(), revision + 1);
        assert_eq!(dialog.session.draft.pattern, "é");
        assert!(dialog.session.draft.flags.case_insensitive);
        assert_eq!(dialog.session.draft.test_text, "current private test");
        assert_eq!(dialog.session.draft.replacement, "current replacement");
        assert!(dialog.session.result().is_none());
        assert!(dialog.session.explanation().is_none());
        assert!(!dialog.scroll_to_selected);
        assert!(dialog.copy_feedback.is_none());
        // Settle before checking rendering isolation, as body intentionally
        // performs the initial/pending evaluation once.
        dialog
            .session
            .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        dialog.body(ui);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            )
            .drop_without_applying_deltas();
        }
        dialog.open();
        dialog.open = false;
        dialog.open();
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert!(dialog.information_section == InformationSection::Presets);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_configured_history_records_restarts_and_loads_only_pattern_flags() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("regex_history.json");
        let preset_path = dir.path().join("regex_presets.json");
        let mut dialog = RegexTesterDialogState::with_storage_paths(&history_path, &preset_path);
        assert!(!history_path.exists());
        assert!(!preset_path.exists());
        assert!(dialog.presets.is_some());
        assert!(RegexTesterDialogState::default().history.is_none());
        let now = Instant::now();
        dialog.session.draft.pattern = "é".into();
        dialog.session.draft.flags.case_insensitive = true;
        dialog.session.draft.test_text = "secret É".into();
        dialog.session.draft.replacement = "secret replacement".into();
        dialog
            .session
            .mark_changed(now - std::time::Duration::from_secs(1));
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                dialog.body(ui);
            });
        })
        .drop_without_applying_deltas();
        assert!(history_path.exists());
        assert!(!preset_path.exists());
        let mut restarted = RegexTesterDialogState::with_storage_paths(&history_path, &preset_path);
        assert_eq!(
            restarted.history.as_ref().unwrap().store.entries()[0].pattern,
            "é"
        );
        restarted.session.draft.pattern = "a".into();
        restarted.session.draft.test_text = "retained test text".into();
        restarted.session.draft.replacement = "retained replacement".into();
        restarted
            .session
            .draft
            .test_cases
            .add(
                Some("retained case".into()),
                "authored input".into(),
                crate::regex_tester::RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let retained_cases = restarted.session.draft.test_cases.clone();
        restarted.session.mark_changed(now);
        restarted
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let revision = restarted.session.revision();
        restarted.scroll_to_selected = true;
        restarted.copy_feedback = Some(Ok("Old feedback"));
        restarted.history_action(history::HistoryAction::Load(0));
        assert_eq!(restarted.session.draft.pattern, "é");
        assert!(restarted.session.draft.flags.case_insensitive);
        assert_eq!(restarted.session.draft.test_text, "retained test text");
        assert_eq!(restarted.session.draft.replacement, "retained replacement");
        assert_eq!(restarted.session.draft.test_cases, retained_cases);
        assert_eq!(restarted.session.revision(), revision + 1);
        assert!(restarted.session.result().is_none());
        assert!(restarted.session.explanation().is_none());
        assert!(!restarted.scroll_to_selected);
        assert!(restarted.copy_feedback.is_none());
        restarted.information_section = InformationSection::History;
        restarted.open();
        restarted.open = false;
        restarted.open();
        assert_eq!(restarted.session.draft.pattern, "é");
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        restarted.body(ui);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            )
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn regex_tester_examples_load_editable_drafts_and_preserve_authored_cases() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.ensure_initial_evaluation(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog
            .session
            .draft
            .test_cases
            .add(
                Some("authored".into()),
                "keep me".into(),
                crate::regex_tester::RegexTestCaseExpectation::NoMatch,
            )
            .unwrap();
        let authored_cases = dialog.session.draft.test_cases.clone();
        let unicode = crate::regex_tester::RegexExample {
            id: "test-unicode-replacement",
            name: "Unicode capture",
            description: "Test complete draft assignment",
            pattern: "(?P<letter>é)(🦀)",
            flags: crate::regex_tester::RegexFlags::default(),
            sample_text: "é🦀",
            replacement: Some("$2:${letter}:$1"),
        };
        for example in [
            &unicode,
            crate::regex_tester::BUILT_IN_EXAMPLES
                .iter()
                .find(|entry| entry.id == "url")
                .unwrap(),
            crate::regex_tester::BUILT_IN_EXAMPLES
                .iter()
                .find(|entry| entry.id == "key-value")
                .unwrap(),
            crate::regex_tester::BUILT_IN_EXAMPLES
                .iter()
                .find(|entry| entry.id == "email-like")
                .unwrap(),
        ] {
            dialog.scroll_to_selected = true;
            dialog.copy_feedback = Some(Ok("Old feedback"));
            let revision = dialog.session.revision();
            dialog.load_example(example);
            let mut expected = example.to_draft();
            expected.test_cases = authored_cases.clone();
            assert_eq!(dialog.session.draft, expected);
            assert_eq!(dialog.session.revision(), revision + 1);
            assert!(dialog.session.result().is_none());
            assert!(dialog.session.explanation().is_none());
            assert!(!dialog.scroll_to_selected);
            assert!(dialog.copy_feedback.is_none());
            dialog
                .session
                .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE);
            assert!(matches!(
                dialog.session.result(),
                Some(EvaluationResult::Success { .. })
            ));
        }
        assert!(dialog.session.draft.replacement.is_empty());
        dialog.session.draft.pattern.push_str("/é");
        dialog.mark_changed();
        let edited = dialog.session.draft.clone();
        dialog.open();
        dialog.open = false;
        dialog.open();
        assert_eq!(dialog.session.draft, edited);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_examples_search_render_and_reopen_have_no_document_side_effects() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.ensure_initial_evaluation(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.information_section = InformationSection::Examples;
        for (query, id) in [
            ("HTTP(S)-LIKE URL", "url"),
            ("STANDARDS-COMPLETE", "email-like"),
            ("(?P<key>", "key-value"),
        ] {
            dialog.examples.query = query.into();
            assert!(dialog.examples.entries().any(|entry| entry.id == id));
        }
        dialog.examples.query = "not-a-catalog-entry".into();
        assert_eq!(dialog.examples.entries().count(), 0);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        for query in ["", "not-a-catalog-entry"] {
            dialog.examples.query = query.into();
            for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let bounds = ui.available_rect_before_wrap();
                            dialog.body(ui);
                            assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                            assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                        });
                    },
                )
                .drop_without_applying_deltas();
            }
        }
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
        dialog.examples.query = "key=value".into();
        dialog.open();
        dialog.open = false;
        dialog.open();
        assert!(dialog.information_section == InformationSection::Examples);
        assert_eq!(dialog.examples.query, "key=value");
    }

    #[test]
    fn regex_tester_reference_filters_render_and_reopen_preserve_draft_without_clipboard_io() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.ensure_initial_evaluation(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.information_section = InformationSection::Reference;
        dialog.reference.query = "DiGiT".into();
        dialog.reference.category = Some(crate::regex_tester::ReferenceCategory::CharacterClasses);
        assert!(
            dialog
                .reference
                .entries()
                .any(|entry| entry.id == "class-digit")
        );
        dialog.reference.category = Some(crate::regex_tester::ReferenceCategory::Anchors);
        assert!(
            dialog
                .reference
                .entries()
                .all(|entry| entry.category == crate::regex_tester::ReferenceCategory::Anchors)
        );
        dialog.reference.query = "not-a-catalog-entry".into();
        assert_eq!(dialog.reference.entries().count(), 0);
        dialog.reference.category = None;
        dialog.reference.query.clear();
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        dialog.body(ui);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
        dialog.reference.query = "DiGiT".into();
        dialog.reference.category = Some(crate::regex_tester::ReferenceCategory::CharacterClasses);
        dialog.open();
        dialog.open = false;
        dialog.open();
        assert!(dialog.information_section == InformationSection::Reference);
        assert_eq!(dialog.reference.query, "DiGiT");
        assert_eq!(
            dialog.reference.category,
            Some(crate::regex_tester::ReferenceCategory::CharacterClasses)
        );
    }

    #[test]
    fn regex_tester_reference_append_and_copy_are_exact_explicit_actions() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.draft.pattern = "é/".into();
        dialog.session.draft.test_text = "é/123".into();
        dialog.session.draft.replacement = "$1".into();
        dialog.session.draft.flags.case_insensitive = true;
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let digit = crate::regex_tester::QUICK_REFERENCE
            .iter()
            .find(|entry| entry.id == "class-digit")
            .unwrap();
        dialog.reference_action(reference::ReferenceAction::Copy(digit));
        assert_eq!(*backend.contents.lock().unwrap(), r"\d");
        assert_eq!(dialog.copy_feedback, Some(Ok("Copied syntax")));
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        dialog.reference_action(reference::ReferenceAction::Append(digit));
        assert_eq!(dialog.session.draft.pattern, "é/\\d");
        assert_eq!(dialog.session.draft.test_text, draft.test_text);
        assert_eq!(dialog.session.draft.flags, draft.flags);
        assert_eq!(dialog.session.draft.replacement, draft.replacement);
        assert_eq!(dialog.session.revision(), revision + 1);
        assert!(dialog.session.result().is_none());
        assert!(dialog.session.explanation().is_none());
        assert!(dialog.copy_feedback.is_none());
        let fragment = crate::regex_tester::QUICK_REFERENCE
            .iter()
            .find(|entry| entry.syntax == "*")
            .unwrap();
        dialog.session.draft.pattern.clear();
        dialog.reference_action(reference::ReferenceAction::Append(fragment));
        dialog
            .session
            .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(dialog.validation_error().is_some());
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_explanation_view_tracks_invalid_pending_and_valid_state() {
        let now = Instant::now();
        let mut dialog = RegexTesterDialogState::default();
        dialog.information_section = InformationSection::Explanation;
        dialog.session.draft.pattern = "[".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(dialog.validation_error().is_some());
        assert!(dialog.session.explanation().is_none());
        dialog.session.draft.pattern = "(?i:é)+".into();
        dialog.session.mark_changed(now);
        assert!(dialog.validation_error().is_none());
        assert!(dialog.session.explanation().is_none());
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(dialog.session.explanation().is_some());
        let draft = dialog.session.draft.clone();
        let ctx = egui::Context::default();
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let bounds = ui.available_rect_before_wrap();
                    dialog.body(ui);
                    assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                    assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                });
            })
            .drop_without_applying_deltas();
        }
        assert_eq!(dialog.session.draft, draft);
        assert!(dialog.session.pending_delay(now).is_none());
    }
    use crate::clipboard_modify::clipboard::ClipboardError;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct FakeClipboard {
        reads: AtomicUsize,
        read_error: Mutex<Option<ClipboardError>>,
        attempts: AtomicUsize,
        writes: Mutex<Vec<String>>,
        contents: Mutex<String>,
        fail: AtomicBool,
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            if let Some(error) = &*self.read_error.lock().unwrap() {
                return Err(error.clone());
            }
            Ok(self.contents.lock().unwrap().clone())
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            if self.fail.load(Ordering::Relaxed) {
                return Err(ClipboardError::Busy("blocked".into()));
            }
            self.writes.lock().unwrap().push(text.into());
            *self.contents.lock().unwrap() = text.into();
            Ok(())
        }
    }

    thread_local! {
        static REGEX_IMMEDIATE_RENDERER_INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static REGEX_VIEWPORT_EVENTS: std::cell::RefCell<Vec<egui::Event>> = const { std::cell::RefCell::new(Vec::new()) };
        static REGEX_VIEWPORT_FOCUSED_ID: std::cell::RefCell<Option<egui::Id>> = const { std::cell::RefCell::new(None) };
    }

    fn install_regex_immediate_viewport_renderer() {
        REGEX_IMMEDIATE_RENDERER_INSTALLED.with(|installed| {
            assert!(
                !installed.replace(true),
                "the test installs its immediate renderer once"
            );
        });
        egui::Context::set_immediate_viewport_renderer(|parent_ctx, mut viewport| {
            let viewport_id = viewport.ids.this;
            let parent_id = viewport.ids.parent;
            let size = viewport
                .builder
                .inner_size
                .unwrap_or_else(|| parent_ctx.content_rect().size());
            let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let mut viewports = parent_ctx.input(|input| input.raw.viewports.clone());
            viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(parent_id),
                    focused: Some(true),
                    inner_rect: Some(screen_rect),
                    title: viewport.builder.title.clone(),
                    ..Default::default()
                },
            );
            let events =
                REGEX_VIEWPORT_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
            let output = parent_ctx.run_ui(
                egui::RawInput {
                    viewport_id,
                    viewports,
                    screen_rect: Some(screen_rect),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(ui.ctx().viewport_id(), viewport_id);
                    assert_eq!(ui.ctx().parent_viewport_id(), parent_id);
                    (viewport.viewport_ui_cb)(ui);
                    REGEX_VIEWPORT_FOCUSED_ID.with(|focused| {
                        *focused.borrow_mut() = ui.ctx().memory(|memory| memory.focused());
                    });
                },
            );
            output.drop_without_applying_deltas();
            assert_eq!(parent_ctx.viewport_id(), parent_id);
        });
    }

    fn render_regex_immediate_frame(
        ctx: &egui::Context,
        dialog: &mut RegexTesterDialogState,
        events: Vec<egui::Event>,
    ) {
        REGEX_VIEWPORT_EVENTS.with(|pending| *pending.borrow_mut() = events);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 680.0),
                )),
                focused: true,
                ..Default::default()
            },
            |ctx| {
                dialog.show_with_jump_mode(ctx, None, Some(1), true, false, true);
            },
        );
        output.drop_without_applying_deltas();
    }

    fn regex_native_target(
        dialog: &RegexTesterDialogState,
        label: &str,
        local_identity: Option<&str>,
    ) -> crate::gui::jump_mode::ui::RegisteredTarget {
        dialog
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == label
                    && local_identity.is_none_or(|identity| {
                        matches!(
                            target.id().semantic(),
                            crate::gui::jump_mode::SemanticTargetId::Named(semantic)
                                if semantic.ends_with(&format!("{}:{identity}", identity.len()))
                        )
                    })
            })
            .cloned()
            .unwrap_or_else(|| panic!("visible Regex target is registered: {label}"))
    }

    fn complete_regex_native_hint(
        ctx: &egui::Context,
        dialog: &mut RegexTesterDialogState,
        target: &crate::gui::jump_mode::ui::RegisteredTarget,
    ) {
        // Release prior native activations and the preceding hint leader before
        // beginning another independent hint session.
        render_regex_immediate_frame(
            ctx,
            dialog,
            vec![
                keyboard_event_state(egui::Key::Space, false, egui::Modifiers::NONE),
                keyboard_event_state(egui::Key::Enter, false, egui::Modifiers::NONE),
            ],
        );
        render_regex_immediate_frame(
            ctx,
            dialog,
            vec![keyboard_event(egui::Key::Space, egui::Modifiers::SHIFT)],
        );
        let hint = dialog
            .native_jump_mode
            .active_session()
            .and_then(|session| session.hint_map().hint_for(target.id()))
            .expect("the current native Regex target receives a hint")
            .as_str()
            .to_owned();
        render_regex_immediate_frame(ctx, dialog, vec![egui::Event::Text(hint)]);
        assert!(dialog.native_jump_mode.active_session().is_none());
        assert_eq!(
            REGEX_VIEWPORT_FOCUSED_ID.with(|focused| *focused.borrow()),
            Some(target.response_id()),
            "hint completion focuses the exact current native response"
        );
    }

    fn keyboard_event_state(
        key: egui::Key,
        pressed: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn jump_mode_regex_native_viewport_copy_hint_focus_then_fresh_enter_copies_once() {
        install_regex_immediate_viewport_renderer();
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.open = true;
        dialog.information_section = InformationSection::Clipboard;
        dialog.session.draft.pattern = "safe".into();
        dialog.session.draft.test_text = "safe input".into();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);

        // The first callback observes the exact owner transition; the next
        // one collects the stable native viewport's actual responses.
        render_regex_immediate_frame(&ctx, &mut dialog, Vec::new());
        render_regex_immediate_frame(&ctx, &mut dialog, Vec::new());
        let copy_target = dialog
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| target.metadata().label() == "Copy pattern")
            .cloned()
            .expect("the visible clipboard action is registered in the native viewport");

        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![keyboard_event(egui::Key::Space, egui::Modifiers::SHIFT)],
        );
        let hint = dialog
            .native_jump_mode
            .active_session()
            .and_then(|session| session.hint_map().hint_for(copy_target.id()))
            .expect("the real Copy pattern target receives a hint")
            .as_str()
            .to_owned();
        render_regex_immediate_frame(&ctx, &mut dialog, vec![egui::Event::Text(hint)]);

        assert!(dialog.native_jump_mode.active_session().is_none());
        assert_eq!(backend.attempts.load(Ordering::Relaxed), 0);
        assert_eq!(
            REGEX_VIEWPORT_FOCUSED_ID.with(|focused| *focused.borrow()),
            Some(copy_target.response_id()),
            "the native Immediate viewport, not its parent context, owns the focus handoff"
        );

        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![keyboard_event(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(backend.attempts.load(Ordering::Relaxed), 1);
        assert_eq!(*backend.writes.lock().unwrap(), ["safe"]);
    }

    #[test]
    fn jump_mode_regex_native_viewport_edits_cases_navigates_and_cancels_preset_removal() {
        install_regex_immediate_viewport_renderer();
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let directory = tempfile::tempdir().unwrap();
        let history_path = directory.path().join("history.json");
        let preset_path = directory.path().join("presets.json");
        let mut dialog = RegexTesterDialogState::with_storage_paths(&history_path, &preset_path);
        dialog.open = true;
        dialog.information_section = InformationSection::TestCases;
        dialog.session.draft.pattern.clear();
        dialog.session.draft.test_text = "abcd".into();
        let first_case = dialog
            .session
            .draft
            .test_cases
            .add(
                Some("first row".into()),
                "a".into(),
                crate::regex_tester::RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let second_case = dialog
            .session
            .draft
            .test_cases
            .add(
                Some("second row".into()),
                "b".into(),
                crate::regex_tester::RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog.session.mark_test_cases_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);

        // Observe the exact owner transition, then collect its real visible
        // form, case, and navigation responses in the stable native viewport.
        render_regex_immediate_frame(&ctx, &mut dialog, Vec::new());
        render_regex_immediate_frame(&ctx, &mut dialog, Vec::new());

        let pattern = regex_native_target(&dialog, "Regular expression", None);
        complete_regex_native_hint(&ctx, &mut dialog, &pattern);
        assert!(dialog.session.draft.pattern.is_empty());
        render_regex_immediate_frame(&ctx, &mut dialog, vec![egui::Event::Text(".".into())]);
        assert_eq!(dialog.session.draft.pattern, ".");
        assert_eq!(
            REGEX_VIEWPORT_FOCUSED_ID.with(|focused| *focused.borrow()),
            Some(pattern.response_id()),
            "native pattern focus stays stable after a committed edit"
        );
        render_regex_immediate_frame(&ctx, &mut dialog, vec![egui::Event::Text("?".into())]);
        assert_eq!(dialog.session.draft.pattern, ".?");
        assert_eq!(
            REGEX_VIEWPORT_FOCUSED_ID.with(|focused| *focused.borrow()),
            Some(pattern.response_id()),
            "a second input after frame completion remains in the same pattern field"
        );
        let reevaluate = Instant::now();
        dialog
            .session
            .tick(reevaluate + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        render_regex_immediate_frame(&ctx, &mut dialog, Vec::new());
        assert!(dialog.session.matches().len() >= 2);
        assert_eq!(dialog.session.selected_index(), Some(0));

        let next = regex_native_target(&dialog, "Next match", None);
        complete_regex_native_hint(&ctx, &mut dialog, &next);
        assert_eq!(dialog.session.selected_index(), Some(0));
        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![keyboard_event(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(dialog.session.selected_index(), Some(1));

        let previous = regex_native_target(&dialog, "Previous match", None);
        complete_regex_native_hint(&ctx, &mut dialog, &previous);
        assert_eq!(dialog.session.selected_index(), Some(1));
        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![keyboard_event(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(dialog.session.selected_index(), Some(0));

        let second_move_up = format!("test-cases/{}/move-up", second_case.value());
        let move_up = regex_native_target(&dialog, "Move test case up", Some(&second_move_up));
        complete_regex_native_hint(&ctx, &mut dialog, &move_up);
        assert_eq!(
            dialog
                .session
                .draft
                .test_cases
                .as_slice()
                .iter()
                .map(|case| case.id)
                .collect::<Vec<_>>(),
            [first_case, second_case],
            "focusing the second row does not reorder it"
        );
        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![keyboard_event(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(
            dialog
                .session
                .draft
                .test_cases
                .as_slice()
                .iter()
                .map(|case| case.id)
                .collect::<Vec<_>>(),
            [second_case, first_case],
            "a distinct Enter reorders the exact typed case identity"
        );

        dialog.information_section = InformationSection::Presets;
        render_regex_immediate_frame(&ctx, &mut dialog, Vec::new());
        let preset_name = regex_native_target(&dialog, "Preset name", None);
        complete_regex_native_hint(&ctx, &mut dialog, &preset_name);
        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![egui::Event::Text("viewport preset".into())],
        );
        assert!(
            crate::regex_tester::PresetStore::open(&preset_path)
                .entries()
                .is_empty()
        );

        let save = regex_native_target(&dialog, "Save preset", None);
        complete_regex_native_hint(&ctx, &mut dialog, &save);
        assert!(
            crate::regex_tester::PresetStore::open(&preset_path)
                .entries()
                .is_empty()
        );
        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![keyboard_event(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        let saved = crate::regex_tester::PresetStore::open(&preset_path);
        assert_eq!(saved.entries().len(), 1);
        assert_eq!(saved.entries()[0].name, "viewport preset");
        assert_eq!(saved.entries()[0].pattern, ".?");
        assert_eq!(
            saved.entries()[0]
                .test_cases
                .as_slice()
                .iter()
                .map(|case| case.id)
                .collect::<Vec<_>>(),
            [second_case, first_case]
        );
        let saved_id = saved.entries()[0].id;
        render_regex_immediate_frame(&ctx, &mut dialog, Vec::new());

        let request_delete_identity = format!("presets/delete/{}", saved_id.value());
        let request_delete = regex_native_target(
            &dialog,
            "Request preset removal",
            Some(&request_delete_identity),
        );
        complete_regex_native_hint(&ctx, &mut dialog, &request_delete);
        assert!(
            !dialog
                .presets
                .as_ref()
                .unwrap()
                .has_pending_delete_confirmation()
        );
        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![keyboard_event(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert!(
            dialog
                .presets
                .as_ref()
                .unwrap()
                .has_pending_delete_confirmation()
        );
        assert_eq!(
            crate::regex_tester::PresetStore::open(&preset_path)
                .entries()
                .len(),
            1
        );
        render_regex_immediate_frame(&ctx, &mut dialog, Vec::new());

        let cancel = regex_native_target(&dialog, "Cancel preset removal", None);
        complete_regex_native_hint(&ctx, &mut dialog, &cancel);
        assert!(
            dialog
                .presets
                .as_ref()
                .unwrap()
                .has_pending_delete_confirmation()
        );
        render_regex_immediate_frame(
            &ctx,
            &mut dialog,
            vec![keyboard_event(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert!(
            !dialog
                .presets
                .as_ref()
                .unwrap()
                .has_pending_delete_confirmation()
        );
        assert_eq!(
            crate::regex_tester::PresetStore::open(&preset_path)
                .entries()
                .len(),
            1
        );
    }

    #[test]
    fn regex_tester_explicit_match_capture_copy_handles_unicode_empty_unmatched_and_failure() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.session.draft.pattern = r"(?P<word>é)(🦀)(z)?()".into();
        dialog.session.draft.test_text = "é🦀".into();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        dialog.copy_selected(inspection::CopyTarget::Match);
        dialog.copy_selected(inspection::CopyTarget::Capture(0));
        dialog.copy_selected(inspection::CopyTarget::Capture(3));
        assert_eq!(*backend.writes.lock().unwrap(), ["é🦀", "é", ""]);
        assert!(matches!(dialog.copy_feedback, Some(Ok("Copied capture"))));
        dialog.copy_selected(inspection::CopyTarget::Capture(2));
        assert_eq!(backend.attempts.load(Ordering::Relaxed), 3);
        assert!(matches!(dialog.copy_feedback, Some(Err(_))));
        *backend.contents.lock().unwrap() = "existing clipboard".into();
        backend.fail.store(true, Ordering::Relaxed);
        dialog.copy_selected(inspection::CopyTarget::Match);
        assert_eq!(backend.attempts.load(Ordering::Relaxed), 4);
        assert_eq!(*backend.contents.lock().unwrap(), "existing clipboard");
        assert!(
            dialog
                .copy_feedback
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("Could not copy")
        );
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(backend.reads.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn regex_tester_open_render_and_inspection_never_access_clipboard_implicitly() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.session.draft.pattern = "(a)".into();
        dialog.session.draft.test_text = "a".into();
        dialog.open();
        dialog
            .session
            .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.information_section = InformationSection::MatchDetails;
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ctx| dialog.show(ctx))
            .drop_without_applying_deltas();
        assert_eq!(backend.reads.load(Ordering::Relaxed), 0);
        assert_eq!(backend.attempts.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn regex_tester_native_geometry_is_large_and_resizable() {
        let builder = viewport_builder();
        assert_eq!(builder.inner_size, Some(egui::vec2(960.0, 680.0)));
        assert_eq!(builder.min_inner_size, Some(egui::vec2(360.0, 240.0)));
        assert_eq!(builder.resizable, Some(true));
    }

    #[test]
    fn regex_tester_pointer_row_activation_updates_selection_capture_and_scroll_without_edits() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "(?P<word>one|two|three)".into();
        dialog.session.draft.test_text =
            format!("one\n{}two\n{}three", "\n".repeat(200), "\n".repeat(200));
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let frame_time = std::cell::Cell::new(0.0);
        let scroll_id = std::cell::Cell::new(None);
        let render = |dialog: &mut RegexTesterDialogState, events| {
            let time = frame_time.get();
            frame_time.set(time + 1.0 / 60.0);
            ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960.0, 680.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        scroll_id.set(Some(
                            ui.id()
                                .with(egui::IdSalt::new("regex_tester_editor_area"))
                                .with(egui::IdSalt::new("regex_tester_text_scroll")),
                        ));
                        dialog.body(ui)
                    });
                },
            )
        };
        render(&mut dialog, Vec::new()).drop_without_applying_deltas();
        let editor_id = egui::Id::new("regex_tester_test_text");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        let mut state = egui::TextEdit::load_state(&ctx, editor_id).unwrap();
        let cursor = egui::text::CCursorRange::one(egui::text::CCursor::new(2));
        state.cursor.set_char_range(Some(cursor));
        state.store(&ctx, editor_id);
        let before = render(&mut dialog, Vec::new());
        // Locate actual rendered row geometry rather than duplicating layout or
        // adding production response collection solely for this test.
        let point = before
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text.starts_with("#3 ·") => {
                    Some(text.pos + egui::vec2(3.0, text.galley.size().y * 0.5))
                }
                _ => None,
            })
            .expect("third match row must be rendered");
        let input_y = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == draft.test_text => {
                        Some(text.pos.y)
                    }
                    _ => None,
                })
                .expect("input galley must be rendered")
        };
        let initial_y = input_y(&before);
        before.drop_without_applying_deltas();
        for pressed in [true, false] {
            render(
                &mut dialog,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(dialog.session.selected_index(), Some(2));
        assert_eq!(dialog.navigation_summary(), "Match 3 of 3");
        let selected = &dialog.session.matches()[dialog.session.selected_index().unwrap()];
        assert_eq!(selected.captures[0].group_index, 1);
        assert_eq!(selected.captures[0].name.as_deref(), Some("word"));
        assert_eq!(
            inspection::copy_text(selected, inspection::CopyTarget::Capture(0)),
            Some("three")
        );
        assert!(dialog.scroll_to_selected);
        render(&mut dialog, Vec::new()).drop_without_applying_deltas();
        assert!(!dialog.scroll_to_selected);
        // ScrollArea animates its target for up to 0.3s. Advance only egui's
        // deterministic input clock, with no sleep or production delay.
        frame_time.set(frame_time.get() + 0.5);
        render(&mut dialog, Vec::new()).drop_without_applying_deltas();
        let scrolled = egui::scroll_area::State::load(&ctx, scroll_id.get().unwrap()).unwrap();
        assert!(scrolled.offset.y > 0.0);
        // egui builds content geometry before advancing the animated offset;
        // the following frame paints at the offset that has now settled.
        let after = render(&mut dialog, Vec::new());
        assert!(
            input_y(&after) < initial_y,
            "editor must scroll towards the distant selected match: {initial_y} -> {}",
            input_y(&after)
        );
        after.drop_without_applying_deltas();
        assert!(!dialog.scroll_to_selected);
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert_eq!(
            egui::TextEdit::load_state(&ctx, editor_id)
                .unwrap()
                .cursor
                .char_range(),
            Some(cursor)
        );
    }

    #[test]
    fn regex_tester_navigation_scroll_is_one_shot_and_preserves_editor_cursor_and_source() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "one".into();
        dialog.session.draft.test_text = "one\n".repeat(200);
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        let render = |dialog: &mut RegexTesterDialogState| {
            ctx.run_ui(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
            })
            .drop_without_applying_deltas();
        };
        render(&mut dialog);
        let id = egui::Id::new("regex_tester_test_text");
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        let selection = egui::text::CCursorRange::one(egui::text::CCursor::new(2));
        state.cursor.set_char_range(Some(selection));
        state.store(&ctx, id);
        assert!(dialog.session.previous_match());
        dialog.scroll_to_selected = true;
        assert_eq!(dialog.navigation_summary(), "Match 200 of 200");
        render(&mut dialog);
        assert!(!dialog.scroll_to_selected);
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(
            egui::TextEdit::load_state(&ctx, id)
                .unwrap()
                .cursor
                .char_range(),
            Some(selection)
        );
        render(&mut dialog);
        assert!(!dialog.scroll_to_selected);
        dialog.scroll_to_selected = true;
        dialog.mark_changed();
        assert!(!dialog.scroll_to_selected);
    }

    #[test]
    fn regex_tester_navigation_summary_qualifies_truncated_rows_as_displayed() {
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(1001);
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert_eq!(dialog.navigation_summary(), "Match 1 of 1000 displayed");
        assert_eq!(dialog.summary(), "At least 1001 matches; showing 1000");
    }

    #[test]
    fn regex_tester_inline_validation_clears_while_pending_and_after_fixing() {
        let now = Instant::now();
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "[".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(
            dialog
                .validation_error()
                .unwrap()
                .contains("unclosed character class")
        );
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(360.0, 240.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let bounds = ui.available_rect_before_wrap();
                let layout = dialog.body(ui);
                assert!(bounds.contains_rect(layout.validation.unwrap()));
                assert!(bounds.contains_rect(layout.editor));
                assert!(layout.editor.height() > 25.0);
                assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
            });
        })
        .drop_without_applying_deltas();
        dialog.session.draft.pattern = "a".into();
        dialog.session.mark_changed(now);
        assert!(dialog.validation_error().is_none());
        assert!(dialog.session.result().is_none());
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(dialog.validation_error().is_none());
        assert!(matches!(
            dialog.session.result(),
            Some(EvaluationResult::Success { .. })
        ));
    }

    #[test]
    fn regex_tester_inline_validation_preserves_rust_unsupported_construct_messages() {
        let now = Instant::now();
        for (pattern, expected) in [("(?=a)", "look-around"), (r"(a)\1", "backreferences")] {
            let mut dialog = RegexTesterDialogState::default();
            dialog.session.draft.pattern = pattern.into();
            dialog.session.mark_changed(now);
            dialog
                .session
                .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
            let Some(EvaluationResult::InvalidPattern(error)) = dialog.session.result() else {
                panic!("invalid pattern required")
            };
            assert_eq!(dialog.validation_error(), Some(error.message.as_str()));
            assert!(dialog.validation_error().unwrap().contains(expected));
        }
    }

    #[test]
    fn regex_tester_idle_frames_do_not_reschedule_and_summary_reports_truncation() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(1001);
        ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
        })
        .drop_without_applying_deltas();
        assert!(
            dialog
                .session
                .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE)
        );
        assert_eq!(dialog.summary(), "At least 1001 matches; showing 1000");
        let revision = dialog.session.revision();
        ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
        })
        .drop_without_applying_deltas();
        assert_eq!(dialog.session.revision(), revision);
        assert!(dialog.session.pending_delay(Instant::now()).is_none());
    }

    #[test]
    fn regex_tester_oversized_text_is_retained_with_bounded_unicode_preview() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.test_text = "🦀".repeat(dialog.session.policy().text_bytes);
        let original = dialog.session.draft.test_text.clone();
        ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
        })
        .drop_without_applying_deltas();
        assert_eq!(dialog.session.draft.test_text, original);
        assert!(dialog.preview_text().len() <= 4096);
        assert!(original.starts_with(dialog.preview_text()));
        assert!(matches!(
            dialog.session.result(),
            Some(EvaluationResult::Suspended(_))
        ));
        assert!(dialog.session.pending_delay(Instant::now()).is_none());
    }

    #[test]
    fn regex_tester_oversized_unicode_pattern_is_retained_with_bounded_preview() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "文🦀".repeat(dialog.session.policy().pattern_bytes);
        dialog.session.draft.flags.case_insensitive = true;
        let original = dialog.session.draft.clone();
        let mut theme = crate::settings::ThemeSettings::default();
        theme.preferences.typography_scale = 1.0;
        crate::gui::theme::publish_committed(&ctx, &theme);
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
            })
            .drop_without_applying_deltas();
        }
        assert_eq!(dialog.session.draft, original);
        assert!(utf8_prefix(&original.pattern, 128).len() <= 128);
        assert!(
            original
                .pattern
                .starts_with(utf8_prefix(&original.pattern, 128))
        );
        assert!(matches!(
            dialog.session.result(),
            Some(EvaluationResult::Suspended(_))
        ));
        assert!(dialog.session.pending_delay(Instant::now()).is_none());
        let (_, font) = dialog
            .last_oversized_preview_geometry
            .as_ref()
            .expect("the oversized pattern preview records its installed font");
        assert!((font.size - 14.0).abs() < 0.01);

        theme.preferences.typography_scale = 2.0;
        crate::gui::theme::publish_committed(&ctx, &theme);
        ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
        })
        .drop_without_applying_deltas();
        let (preview_rect, font) = dialog
            .last_oversized_preview_geometry
            .as_ref()
            .expect("the scaled oversized preview records its actual geometry");
        assert!((font.size - 28.0).abs() < 0.01);
        let style = ctx.style_of(ctx.theme());
        let minimum_height = 56.0_f32
            .max(
                ctx.fonts_mut(|fonts| fonts.row_height(font))
                    + style.spacing.extra_text_line_spacing,
            )
            .max(style.spacing.interact_size.y);
        assert!(preview_rect.height() >= minimum_height - 0.1);
    }

    #[test]
    fn regex_tester_layout_keeps_long_inputs_within_normal_and_narrow_viewports() {
        for size in [egui::vec2(960.0, 680.0), egui::vec2(360.0, 240.0)] {
            for information_open in [true, false] {
                let ctx = egui::Context::default();
                let mut dialog = RegexTesterDialogState::default();
                dialog.information_open = information_open;
                dialog.session.draft.pattern = "path/to/file".repeat(100);
                dialog.session.draft.test_text = "a long editable line ".repeat(1000);
                let draft = dialog.session.draft.clone();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                ctx.run_ui(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        let layout = dialog.body(ui);
                        for area in [
                            Some(layout.header),
                            Some(layout.editor),
                            layout.information,
                            Some(layout.status),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            assert!(
                                bounds.contains_rect(area),
                                "{size:?}: {area:?} outside {bounds:?}"
                            );
                        }
                        assert!(layout.editor.width() > 100.0);
                        assert!(layout.editor.height() > 25.0);
                        if let Some(info) = layout.information {
                            assert!(!layout.editor.intersects(info));
                            if size.x > 680.0 {
                                assert!(info.left() > layout.editor.right());
                            } else {
                                assert!(info.top() > layout.editor.bottom());
                            }
                        }
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                })
                .drop_without_applying_deltas();
                assert_eq!(dialog.session.draft, draft);
            }
        }
    }

    #[test]
    fn regex_tester_information_collapse_restores_editor_space_and_survives_reopen() {
        let mut dialog = RegexTesterDialogState::default();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 680.0));
        let expanded = CoreLayout::new(bounds, dialog.information_open, false);
        dialog.information_open = false;
        dialog.open();
        dialog.open = false;
        dialog.open();
        let collapsed = CoreLayout::new(bounds, dialog.information_open, false);
        assert!(expanded.information.is_some());
        assert!(collapsed.information.is_none());
        assert!(collapsed.editor.width() > expanded.editor.width());
    }

    #[test]
    fn regex_tester_flag_controls_leave_slashes_in_raw_pattern() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "https?://example.com/a/b".into();
        let mut positions = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let mut targets = JumpTargets::new(None);
                    positions = dialog
                        .flags_area(ui, &mut targets, true)
                        .map(|response| response.rect.center())
                        .to_vec();
                });
            });
        })
        .drop_without_applying_deltas();
        for position in positions {
            let input = egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            };
            ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let mut targets = JumpTargets::new(None);
                        dialog.flags_area(ui, &mut targets, true)
                    });
                });
            })
            .drop_without_applying_deltas();
        }
        assert_eq!(dialog.session.draft.flags.suffix(), "imsx");
        assert_eq!(dialog.session.draft.pattern, "https?://example.com/a/b");
    }

    #[test]
    fn regex_tester_open_is_idempotent_and_preserves_session_draft() {
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "(?P<word>\\w+)".into();
        dialog.session.draft.test_text = "hello".into();
        dialog.session.draft.replacement = "$word".into();
        let draft = dialog.session.draft.clone();
        dialog.open();
        assert!(dialog.focus_pattern);
        dialog.focus_pattern = false;
        dialog.focus_viewport = false;
        dialog.open();
        assert!(!dialog.focus_pattern);
        assert!(dialog.focus_viewport);
        dialog.open = false;
        dialog.open();
        assert!(dialog.focus_pattern);
        assert_eq!(dialog.session.draft, draft);
    }

    #[test]
    fn regex_tester_embedded_shell_focuses_once_and_escape_closes() {
        let ctx = egui::Context::default();
        let mut dialog = keyboard_dialog();
        let draft = dialog.session.draft.clone();
        dialog.open();
        ctx.run_ui(egui::RawInput::default(), |ctx| dialog.show(ctx))
            .drop_without_applying_deltas();
        assert!(!dialog.focus_pattern);
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        ctx.run_ui(input, |ctx| dialog.show(ctx))
            .drop_without_applying_deltas();
        assert!(!dialog.open);
        assert_eq!(dialog.session.draft, draft);
        assert!(dialog.session.substitution_enabled());
        dialog.open();
        ctx.run_ui(egui::RawInput::default(), |ctx| dialog.show(ctx))
            .drop_without_applying_deltas();
        assert!(dialog.open);
        assert_eq!(dialog.session.draft, draft);
        assert!(dialog.session.substitution_enabled());
    }
}
