use super::jump_mode::ui::{JumpModeScope, NavigationOwner, NavigationViewportBinding};
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::{HostIdentity, UtilityPresentation};
use super::utility_window_host::{HostedUtilityDialog, UtilityWindowHost};
use eframe::egui;
use std::{marker::PhantomData, ops::Range, path::PathBuf, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FormKind {
    Table,
    Link,
    NewNote,
}

#[derive(Clone)]
pub(super) struct FormOpenRequest {
    pub kind: FormKind,
    pub editor: egui::Id,
    pub revision: u64,
    pub range: Range<usize>,
}

#[derive(Clone)]
pub(super) struct FormBasis {
    pub panel: u64,
    pub host: HostIdentity,
    pub slug: String,
    pub path: PathBuf,
    pub revision: u64,
    pub editor: egui::Id,
    pub range: Range<usize>,
    pub source: Arc<str>,
}

#[derive(Clone)]
pub(super) struct FormDraft {
    pub columns: usize,
    pub rows: usize,
    pub headers: Vec<String>,
    pub text: String,
    pub url: String,
    pub error: Option<String>,
}

pub(super) trait FormSpec {
    const KIND: FormKind;
    const KEY: &'static str;
    const TITLE: &'static str;
}
pub(super) struct Table;
pub(super) struct Link;
pub(super) struct NewNote;
impl FormSpec for Table {
    const KIND: FormKind = FormKind::Table;
    const KEY: &'static str = "note-custom-table-builder";
    const TITLE: &'static str = "Insert custom Markdown table";
}
impl FormSpec for Link {
    const KIND: FormKind = FormKind::Link;
    const KEY: &'static str = "note-insert-link";
    const TITLE: &'static str = "Insert Link";
}
impl FormSpec for NewNote {
    const KIND: FormKind = FormKind::NewNote;
    const KEY: &'static str = "note-link-new";
    const TITLE: &'static str = "Link new note";
}

pub(super) struct NoteForm<K: FormSpec> {
    pub open: bool,
    pub session: u64,
    pub basis: FormBasis,
    pub draft: FormDraft,
    marker: PhantomData<K>,
    #[cfg(test)]
    controls: Vec<(String, egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    window_rect: Option<egui::Rect>,
}
impl<K: FormSpec> NoteForm<K> {
    pub fn new(session: u64, basis: FormBasis) -> Self {
        let text = if K::KIND == FormKind::Link {
            basis.source[basis.range.clone()].to_owned()
        } else {
            String::new()
        };
        Self {
            open: true,
            session,
            basis,
            draft: FormDraft {
                columns: 2,
                rows: 1,
                headers: vec!["Column 1".into(), "Column 2".into()],
                text,
                url: String::new(),
                error: None,
            },
            marker: PhantomData,
            #[cfg(test)]
            controls: Vec::new(),
            #[cfg(test)]
            window_rect: None,
        }
    }
    #[cfg(test)]
    pub(super) fn test_control(&self, name: &str) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.controls
            .iter()
            .find(|(key, ..)| key == name)
            .map(|(_, rect, clip, enabled)| (*rect, *clip, *enabled))
    }
    #[cfg(test)]
    pub(super) fn test_window_rect(&self) -> Option<egui::Rect> {
        self.window_rect
    }
    fn layer(&self) -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new((K::KEY, self.session)))
    }
    fn render(
        &mut self,
        ctx: &egui::Context,
        allowed: bool,
        first: bool,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> FormActions {
        let mut draft = self.draft.clone();
        let mut submit = false;
        let mut close = false;
        let mut open = self.open;
        let mut host_request = None;
        #[cfg(test)]
        let mut controls = Vec::new();
        let mut contents = |ui: &mut egui::Ui| {
            let interactive = allowed && ui.is_visible() && !ui.is_sizing_pass();
            ui.horizontal(|ui| {
                let (host, next) =
                    super::utility_host::presentation_button(ui, presentation, interactive);
                #[cfg(test)]
                controls.push(("host".into(), host.rect, ui.clip_rect(), host.enabled()));
                if first && !ctx.will_discard() && host.clicked() {
                    host_request = Some(next);
                }
                ui.add(egui::Label::new(format!("Note: {}", self.basis.slug)).truncate())
                    .on_hover_text(self.basis.path.display().to_string());
            });
            ui.add_enabled_ui(interactive, |ui| {
                let footer = ui.spacing().interact_size.y + ui.spacing().item_spacing.y * 2.0;
                let visible_height = (ui.clip_rect().bottom() - ui.cursor().top()).max(0.0);
                let height = (ui.available_height().min(visible_height) - footer).max(0.0);
                egui::ScrollArea::vertical().id_salt((K::KEY, self.session, "body"))
                    .auto_shrink([false, false]).max_height(height).show(ui, |ui| {
                        match K::KIND {
                            FormKind::Table => {
                                ui.horizontal(|ui| { ui.label("Columns"); ui.add(egui::DragValue::new(&mut draft.columns).range(1..=crate::notes_markdown::MAX_MARKDOWN_CUSTOM_TABLE_DIMENSION)); });
                                ui.horizontal(|ui| { ui.label("Data rows (excluding header)"); ui.add(egui::DragValue::new(&mut draft.rows).range(1..=crate::notes_markdown::MAX_MARKDOWN_CUSTOM_TABLE_DIMENSION)); });
                                draft.headers.resize_with(draft.columns, String::new);
                                for (index, header) in draft.headers.iter_mut().enumerate() {
                                    ui.horizontal(|ui| { ui.label(format!("Column {}", index + 1)); ui.text_edit_singleline(header); });
                                }
                            }
                            FormKind::Link => {
                                ui.label("Text:");
                                let text = ui.text_edit_singleline(&mut draft.text);
                                #[cfg(test)] controls.push(("text".into(), text.rect, ui.clip_rect(), text.enabled()));
                                ui.label("URL:");
                                let url = ui.text_edit_singleline(&mut draft.url);
                                #[cfg(test)] controls.push(("url".into(), url.rect, ui.clip_rect(), url.enabled()));
                                if text.changed() || url.changed() { draft.error = None; }
                            }
                            FormKind::NewNote => {
                                ui.label("New note name");
                                let text = ui.text_edit_singleline(&mut draft.text);
                                #[cfg(test)] controls.push(("text".into(), text.rect, ui.clip_rect(), text.enabled()));
                                if text.changed() { draft.error = None; }
                                if draft.text.trim().is_empty() { ui.weak("Name is required"); }
                            }
                        }
                        if let Some(error) = &draft.error { ui.colored_label(ui.visuals().error_fg_color, error); }
                    });
                ui.horizontal_wrapped(|ui| {
                    let enabled = K::KIND != FormKind::NewNote || !draft.text.trim().is_empty();
                    let insert = ui.add_enabled(enabled, egui::Button::new(if K::KIND == FormKind::NewNote { "Link" } else { "Insert" }));
                    let cancel = ui.button("Cancel");
                    #[cfg(test)] {
                        controls.push(("submit".into(), insert.rect, ui.clip_rect(), insert.enabled()));
                        controls.push(("cancel".into(), cancel.rect, ui.clip_rect(), cancel.enabled()));
                    }
                    if insert.clicked() { submit = true; }
                    if cancel.clicked() { close = true; }
                });
            });
        };
        #[cfg(test)]
        let mut window_rect = None;
        if let Some(ui) = child {
            contents(ui);
        } else {
            let output = egui::Window::new(K::TITLE)
                .id(self.layer().id)
                .resizable(true)
                .default_size([460.0, 460.0])
                .min_size([320.0, 220.0])
                .show_utility(
                    ctx,
                    K::KEY,
                    self.layer().id,
                    Some(&mut open),
                    super::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        super::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::vec2(
                            460.0, 460.0,
                        )),
                        egui::vec2(320.0, 220.0),
                        [None, None],
                    ),
                    |ui| contents(ui),
                );
            #[cfg(test)]
            {
                window_rect = output.map(|output| output.response.rect);
            }
            #[cfg(not(test))]
            let _ = output;
        }
        #[cfg(test)]
        {
            self.window_rect = window_rect;
            self.controls = controls;
        }
        FormActions {
            session: self.session,
            draft,
            submit: submit && first && allowed && !ctx.will_discard(),
            close: close || !open,
            host_request,
        }
    }
}
pub(super) struct FormActions {
    pub session: u64,
    pub draft: FormDraft,
    pub submit: bool,
    pub close: bool,
    pub host_request: Option<UtilityPresentation>,
}
pub(super) enum FormCommand {
    Close,
    Apply(FormActions),
}
pub(super) type FormHost<K> = UtilityWindowHost<NoteForm<K>>;
impl<K: FormSpec> HostedUtilityDialog for NoteForm<K> {
    const KEY: &'static str = K::KEY;
    const TITLE: &'static str = K::TITLE;
    const SIZE: egui::Vec2 = egui::vec2(460.0, 460.0);
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 220.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = ();
    type Actions = FormActions;
    type Checkpoint = ();
    type Command = FormCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new(K::KEY))
    }
    fn dialog_layer(&self) -> egui::LayerId {
        self.layer()
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.session
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
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|m| m.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
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
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
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
    ) -> Self::Actions {
        self.render(ctx, allowed, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        FormCommand::Close
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, FormCommand::Close)
    }
    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![FormCommand::Close]
        } else {
            vec![FormCommand::Apply(actions)]
        }
    }
    #[cfg(test)]
    fn test_button(_: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}
pub(super) fn show_form<K: FormSpec>(
    ctx: &egui::Context,
    host: &mut FormHost<K>,
    form: &mut Option<NoteForm<K>>,
    first: bool,
    root_visible: bool,
    root_input: bool,
    native_input: bool,
    root_jump_active: bool,
) {
    let Some(form) = form.as_mut() else {
        return;
    };
    let layer = host.root_layer();
    let parent_owner = root_input
        && layer.is_some()
        && super::jump_mode::ui::visible_top_middle_layer(ctx) == layer
        && !egui::Popup::is_any_open(ctx)
        && super::jump_mode::ui::focused_response_layer(ctx)
            .is_none_or(|focus| Some(focus) == layer || focus.order == egui::Order::Background);
    host.show(
        ctx,
        form,
        &(),
        super::utility_window_host::HostInput {
            first,
            root_visible,
            root_input,
            native_input,
            root_owner: NavigationOwner::Blocked,
            parent_owner,
            root_jump_active,
            close_shortcut: true,
            allow_os_close: true,
            confirmation_owner: NavigationOwner::Blocked,
            parent_instance: 0,
            root_scopes: (None, None),
        },
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_form_keeps_actions_reachable_with_large_text() {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| {
            for font in style.text_styles.values_mut() {
                font.size *= 1.3;
            }
        });
        let basis = FormBasis {
            panel: 1,
            host: HostIdentity {
                host: UtilityPresentation::Docked,
                generation: 0,
            },
            slug: "note".into(),
            path: PathBuf::from("note.md"),
            revision: 0,
            editor: egui::Id::new("editor"),
            range: 0..0,
            source: Arc::from(""),
        };
        let mut form = NoteForm::<Table>::new(17, basis);
        form.draft.columns = 24;
        form.draft.headers.resize(24, "Header".into());
        for _ in 0..4 {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420.0, 300.0),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    let _ = form.render(ctx, true, true, None, UtilityPresentation::Docked);
                },
            )
            .drop_without_applying_deltas();
        }
        for name in ["submit", "cancel"] {
            let (rect, clip, enabled) = form.test_control(name).expect("action rendered");
            assert!(
                enabled && clip.contains_rect(rect),
                "{name}: rect={rect:?} clip={clip:?}"
            );
        }
    }
}
