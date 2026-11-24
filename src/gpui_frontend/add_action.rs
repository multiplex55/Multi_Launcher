use super::*;

pub(super) struct AppEditorState {
    pub(super) open: bool,
    draft: Option<AddAppDraft>,
    status: Option<String>,
    owner_focus: FocusHandle,
    new_focus: FocusHandle,
    arguments_focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    return_scroll: Option<Point<Pixels>>,
}

struct AddAppDraft {
    label: Entity<InputState>,
    description: Entity<InputState>,
    path: Entity<InputState>,
    args: Entity<InputState>,
    show_args: bool,
}

pub(super) struct AddAppSnapshot {
    pub(super) label: String,
    pub(super) description: String,
    pub(super) path: String,
    pub(super) show_args: bool,
    pub(super) args: String,
}

impl AppEditorState {
    pub(super) fn new(cx: &mut Context<RootWindow>) -> Self {
        Self {
            open: false,
            draft: None,
            status: None,
            owner_focus: cx.focus_handle(),
            new_focus: cx.focus_handle(),
            arguments_focus: cx.focus_handle(),
            return_focus: None,
            return_scroll: None,
        }
    }

    pub(super) fn snapshot(&self, cx: &App) -> Option<AddAppSnapshot> {
        let draft = self.draft.as_ref()?;
        Some(AddAppSnapshot {
            label: draft.label.read(cx).value().to_string(),
            description: draft.description.read(cx).value().to_string(),
            path: draft.path.read(cx).value().to_string(),
            show_args: draft.show_args,
            args: draft.args.read(cx).value().to_string(),
        })
    }
}

impl RootWindow {
    /// The future menu route supplies its live origin before dismissing the menu.
    pub(super) fn open_app_editor(
        &mut self,
        origin: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_admission.is_fenced() {
            return;
        }
        if !self.app_editor.open {
            self.app_editor.return_focus =
                origin.filter(|focus| self.focus_handle.contains(focus, window));
            self.app_editor.return_scroll = Some(self.root_scroll.offset());
            self.app_editor.open = true;
        }
        let item = self.quick_help_scroll_item()
            + usize::from(self.quick_help.open)
            + usize::from(self.result_favorites.is_open());
        self.root_scroll.scroll_to_top_of_item(item);
        if let Some(draft) = &self.app_editor.draft {
            draft.label.focus_handle(cx).focus(window, cx);
        } else {
            self.app_editor.new_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn new_app_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() || self.app_editor.draft.is_some() {
            return;
        }
        let label = cx.new(|cx| InputState::new(window, cx));
        let description = cx.new(|cx| InputState::new(window, cx));
        let path = cx.new(|cx| InputState::new(window, cx));
        let args = cx.new(|cx| InputState::new(window, cx));
        label.focus_handle(cx).focus(window, cx);
        self.app_editor.draft = Some(AddAppDraft {
            label,
            description,
            path,
            args,
            show_args: false,
        });
        self.app_editor.status = None;
        cx.notify();
    }

    fn cancel_add_app(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.app_editor.draft.take().is_none() {
            return;
        }
        self.app_editor.status = None;
        if self.app_editor.owner_focus.contains_focused(window, cx)
            && !self.restart_admission.is_fenced()
        {
            self.app_editor.new_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn close_app_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.open {
            return;
        }
        let restore = self.app_editor.owner_focus.contains_focused(window, cx)
            && !self.restart_admission.is_fenced();
        self.app_editor.open = false;
        self.app_editor.draft = None;
        self.app_editor.status = None;
        let origin = self.app_editor.return_focus.take();
        let offset = self.app_editor.return_scroll.take();
        if restore {
            if let Some(origin) = origin.filter(|focus| self.focus_handle.contains(focus, window)) {
                if let Some(offset) = offset {
                    self.root_scroll.set_offset(offset);
                }
                origin.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn app_editor_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.app_editor.open
            || !quick_help_escape_capture_should_close(
                &window.context_stack(),
                self.app_editor.owner_focus.contains_focused(window, cx),
            )
        {
            return false;
        }
        if self.app_editor.draft.is_some() {
            self.cancel_add_app(window, cx);
        } else {
            self.close_app_editor(window, cx);
        }
        true
    }
}

#[derive(IntoElement)]
pub(super) struct AppEditorPanel {
    root: WeakEntity<RootWindow>,
    width: f32,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    palette: multi_launcher::gui::theme::SemanticPalette,
    owner_focus: FocusHandle,
    new_focus: FocusHandle,
    arguments_focus: FocusHandle,
    draft: Option<AddAppPanelDraft>,
    status: Option<String>,
}

struct AddAppPanelDraft {
    label: Entity<InputState>,
    description: Entity<InputState>,
    path: Entity<InputState>,
    args: Entity<InputState>,
    show_args: bool,
}

impl AppEditorPanel {
    pub(super) fn new(
        root: WeakEntity<RootWindow>,
        state: &AppEditorState,
        width: f32,
        metrics: multi_launcher::gui::theme::ThemeMetrics,
        palette: multi_launcher::gui::theme::SemanticPalette,
    ) -> Self {
        Self {
            root,
            width,
            metrics,
            palette,
            owner_focus: state.owner_focus.clone(),
            new_focus: state.new_focus.clone(),
            arguments_focus: state.arguments_focus.clone(),
            status: state.status.clone(),
            draft: state.draft.as_ref().map(|draft| AddAppPanelDraft {
                label: draft.label.clone(),
                description: draft.description.clone(),
                path: draft.path.clone(),
                args: draft.args.clone(),
                show_args: draft.show_args,
            }),
        }
    }
}

impl RenderOnce for AppEditorPanel {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let metrics = self.metrics;
        let mut panel = div()
            .id("app-editor")
            .track_focus(&self.owner_focus)
            .key_context("AppEditor")
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w(px(self.width))
            .max_w_full()
            .p(px(metrics.panel_padding))
            .bg(appearance::to_hsla(self.palette.panel_surface))
            .rounded(px(metrics.corner_radius))
            .text_color(appearance::to_hsla(self.palette.primary_text))
            .text_size(px(metrics.body_text_size))
            .line_height(px(metrics.line_height))
            .child(Label::new("App Editor").text_size(px(metrics.heading_text_size)));
        if let Some(draft) = self.draft {
            let checkbox_root = self.root.clone();
            let args_state = draft.args.clone();
            let arguments_focus = self.arguments_focus.clone();
            let checkbox = Checkbox::new("add-app-arguments")
                .checked(draft.show_args)
                .track_focus(&self.arguments_focus)
                .accessibility_label("Add arguments")
                .on_change(move |state, _, window, app| {
                    let _ = checkbox_root.update(app, |root, cx| {
                        if let Some(draft) = root.app_editor.draft.as_mut() {
                            let enabled = state == CheckboxState::Checked;
                            if !enabled && args_state.focus_handle(cx).is_focused(window) {
                                arguments_focus.focus(window, cx);
                            }
                            draft.show_args = enabled;
                            cx.notify();
                        }
                    });
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_x(px(metrics.item_spacing_x))
                        .child(
                            CheckboxIndicator::new()
                                .checked(draft.show_args)
                                .size(px(18.0))
                                .rounded(px(3.0))
                                .border_1()
                                .border_color(appearance::to_hsla(self.palette.control_border))
                                .when(draft.show_args, |indicator| {
                                    indicator.bg(appearance::to_hsla(self.palette.accent))
                                })
                                .items_center()
                                .justify_center()
                                .child(if draft.show_args { "✓" } else { "" }),
                        )
                        .child(Label::new("Add arguments")),
                );
            let cancel_root = self.root.clone();
            panel = panel
                .child(Label::new("Add App"))
                .child(Label::new("Label"))
                .child(Input::new(&draft.label).aria_label("Label"))
                .child(Label::new("Description"))
                .child(Input::new(&draft.description).aria_label("Description"))
                .child(Label::new("Path"))
                .child(Input::new(&draft.path).aria_label("Path"))
                .child(checkbox)
                .when(draft.show_args, |panel| {
                    panel
                        .child(Label::new("Args"))
                        .child(Input::new(&draft.args).aria_label("Args"))
                })
                .children(self.status.map(Label::new))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_x(px(metrics.item_spacing_x))
                        .child(Button::new("add-app-submit").label("Add").disabled(true))
                        .child(Button::new("add-app-cancel").label("Cancel").on_click(
                            move |_, window, app| {
                                let _ = cancel_root
                                    .update(app, |root, cx| root.cancel_add_app(window, cx));
                            },
                        )),
                );
        } else {
            let new_root = self.root.clone();
            let close_root = self.root;
            panel = panel
                .child(
                    Button::new("app-editor-new")
                        .label("New App")
                        .track_focus(&self.new_focus)
                        .on_click(move |_, window, app| {
                            let _ = new_root.update(app, |root, cx| root.new_app_draft(window, cx));
                        }),
                )
                .child(Button::new("app-editor-close").label("Close").on_click(
                    move |_, window, app| {
                        let _ =
                            close_root.update(app, |root, cx| root.close_app_editor(window, cx));
                    },
                ));
        }
        panel
    }
}
