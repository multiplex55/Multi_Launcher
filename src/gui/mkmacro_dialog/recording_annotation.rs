use super::super::jump_mode::ui::{JumpModeScope, NavigationOwner, NavigationViewportBinding};
use super::super::utility_host::{self, UtilityPresentation};
use super::super::utility_window_host::HostedUtilityDialog;
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use eframe::egui::{self, Id, LayerId, Order};

#[derive(Default)]
pub(super) struct AnnotationState {
    pub open: bool,
    pub text: String,
    pub target: Option<u64>,
    pub recording_id: Option<u64>,
    pub pause_owned: bool,
    pub pause_id: Option<u64>,
    pub epoch: u64,
    pub focus_requested: bool,
    #[cfg(test)]
    pub test_text: Option<(egui::Rect, egui::Rect, bool, bool)>,
    #[cfg(test)]
    pub test_add: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    pub test_cancel: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    pub test_host_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    pub test_owner: Option<(
        egui::ViewportId,
        bool,
        bool,
        Option<egui::LayerId>,
        Option<egui::LayerId>,
    )>,
}
#[derive(Default)]
pub(super) struct AnnotationActions {
    presentation: Option<UtilityPresentation>,
    command: Option<AnnotationCommand>,
    focus_delivered: bool,
}
pub(super) enum AnnotationCommand {
    Add,
    Cancel,
    Retired,
}
impl HostedUtilityDialog for AnnotationState {
    const KEY: &'static str = "mkmacro-recording-annotation";
    const TITLE: &'static str = "Recording annotation";
    const SIZE: egui::Vec2 = egui::vec2(420.0, 180.0);
    const MINIMUM: egui::Vec2 = egui::vec2(280.0, 140.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = ();
    type Actions = AnnotationActions;
    type Checkpoint = String;
    type Command = AnnotationCommand;
    fn layer() -> LayerId {
        LayerId::new(Order::Middle, Id::new(Self::KEY))
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch
    }
    fn parent_targets_allowed(&self, _: &()) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|i| i.focused)
            && ctx.memory(|m| m.top_modal_layer().is_none())
            && super::super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer
                    || Some(layer) == popup
                    || layer == LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: LayerId) -> Option<LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) -> String {
        self.text.clone()
    }
    fn restore(&mut self, text: String) {
        self.text = text;
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        _: &(),
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> AnnotationActions {
        let mut result = AnnotationActions::default();
        #[cfg(test)]
        {
            self.test_owner = Some((
                ctx.viewport_id(),
                ctx.input(|i| i.focused),
                allowed,
                super::super::jump_mode::ui::focused_response_layer(ctx),
                super::super::jump_mode::ui::visible_top_middle_layer(ctx),
            ));
        }
        let mut open = true;
        let mut body = |ui: &mut egui::Ui| {
            let (button, next) = utility_host::presentation_button(ui, presentation, allowed);
            #[cfg(test)]
            {
                self.test_host_button = Some((button.rect, ui.clip_rect(), button.enabled()));
            }
            if first && !ctx.will_discard() && button.clicked() {
                result.presentation = Some(next);
            }
            ui.add_enabled_ui(allowed, |ui| {
                let footer_height = ui.spacing().interact_size.y.max(
                    ui.text_style_height(&egui::TextStyle::Button)
                        + 2.0 * ui.spacing().button_padding.y,
                ) + ui.spacing().item_spacing.y;
                let body_height = (ui
                    .available_rect_before_wrap()
                    .intersect(ui.clip_rect())
                    .height()
                    - footer_height
                    - ui.spacing().item_spacing.y)
                    .max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("recording_annotation_body")
                    .max_height(body_height)
                    .min_scrolled_height(0.0)
                    .show(ui, |ui| {
                        ui.label("Add a note at the current recording position.");
                        let text = ui.text_edit_singleline(&mut self.text);
                        if self.focus_requested
                            && first
                            && !ctx.will_discard()
                            && text.enabled()
                            && ui.is_visible()
                            && !ui.is_sizing_pass()
                            && ui.clip_rect().intersects(text.rect)
                        {
                            text.request_focus();
                            result.focus_delivered = true;
                        }
                        #[cfg(test)]
                        {
                            self.test_text =
                                Some((text.rect, ui.clip_rect(), text.enabled(), text.has_focus()));
                        }
                    });
                ui.horizontal_wrapped(|ui| {
                    let add =
                        ui.add_enabled(!self.text.trim().is_empty(), egui::Button::new("Add note"));
                    #[cfg(test)]
                    {
                        self.test_add = Some((add.rect, ui.clip_rect(), add.enabled()));
                    }
                    if add.clicked() {
                        result.command = Some(AnnotationCommand::Add);
                    }
                    let cancel = ui.button("Cancel");
                    #[cfg(test)]
                    {
                        self.test_cancel = Some((cancel.rect, ui.clip_rect(), cancel.enabled()));
                    }
                    if cancel.clicked() {
                        result.command = Some(AnnotationCommand::Cancel);
                    }
                });
            });
        };
        if let Some(ui) = child {
            body(ui);
        } else {
            let id = Id::new(Self::KEY);
            egui::Window::new(Self::TITLE).id(id).show_utility(
                ctx,
                Self::KEY,
                id,
                Some(&mut open),
                crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::outer(Self::SIZE)
                    .with_minimum(Self::MINIMUM),
                body,
            );
        }
        if !open {
            result.command = Some(AnnotationCommand::Cancel);
        }
        if !first || ctx.will_discard() {
            result.command = None;
            result.presentation = None;
            result.focus_delivered = false;
        }
        result
    }
    fn host_request(actions: &AnnotationActions) -> Option<UtilityPresentation> {
        actions.presentation
    }
    fn closed_command() -> AnnotationCommand {
        AnnotationCommand::Retired
    }
    fn command_closes(_: &AnnotationCommand) -> bool {
        false
    }
    fn complete_actions(
        &mut self,
        actions: AnnotationActions,
        _: Option<usize>,
    ) -> Vec<AnnotationCommand> {
        if actions.focus_delivered {
            self.focus_requested = false;
        }
        actions.command.into_iter().collect()
    }
    #[cfg(test)]
    fn test_button(_: &AnnotationActions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}
