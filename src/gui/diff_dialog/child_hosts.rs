//! Private hosted editors for one retained Diff workspace.
use super::{DiffUiAction, TextInputDraft, TextInputPurpose};
use crate::gui::jump_mode::ui::{NavigationOwner, NavigationViewportBinding};
use crate::gui::utility_host::UtilityPresentation;
use crate::gui::utility_window_host::HostedUtilityDialog;
use eframe::egui;
use std::marker::PhantomData;

pub(super) enum Command {
    Close,
    Rules(crate::diff::text_compare::TextComparisonRules),
    Folder(crate::diff::model::FolderRulesDraft),
    Input,
    Paste(crate::diff::model::DiffSide),
}

pub(super) struct Actions {
    request: Option<UtilityPresentation>,
    command: Option<Command>,
}

pub(super) trait Kind: Sized {
    type Draft: Clone;
    const KEY: &'static str;
    const TITLE: &'static str;
    const SIZE: egui::Vec2;
    fn body(
        ui: &mut egui::Ui,
        draft: &mut Self::Draft,
        workspace: u64,
        view: u64,
        session: u64,
    ) -> Option<Command>;
}

pub(super) struct Editor<K: Kind> {
    pub open: bool,
    pub session: u64,
    pub workspace: u64,
    pub view: u64,
    pub draft: K::Draft,
    pub error: Option<String>,
    marker: PhantomData<K>,
}
impl<K: Kind> Editor<K> {
    pub fn new(session: u64, workspace: u64, view: u64, draft: K::Draft) -> Self {
        Self {
            open: true,
            session,
            workspace,
            view,
            draft,
            error: None,
            marker: PhantomData,
        }
    }
}
impl<K: Kind> HostedUtilityDialog for Editor<K> {
    const KEY: &'static str = K::KEY;
    const TITLE: &'static str = K::TITLE;
    const SIZE: egui::Vec2 = K::SIZE;
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 240.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = ();
    type Actions = Actions;
    type Checkpoint = K::Draft;
    type Command = Command;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new(K::KEY))
    }
    fn dialog_layer(&self) -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new((K::KEY, self.session)))
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
            && ctx.input(|i| i.focused)
            && ctx.memory(|m| m.top_modal_layer().is_none())
            && crate::gui::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|l| l == binding.base_layer)
            && crate::gui::jump_mode::ui::focused_response_layer(ctx).is_none_or(|l| {
                l == binding.base_layer || Some(l) == popup || l == egui::LayerId::background()
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
    fn capture(&self) -> Self::Checkpoint {
        self.draft.clone()
    }
    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        self.draft = checkpoint;
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (
            Option<&crate::gui::jump_mode::ui::JumpModeScope>,
            Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        ),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        _: &(),
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Actions {
        use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
        let mut request = None;
        let mut command = None;
        let mut open = true;
        let mut body = |ui: &mut egui::Ui| {
            let interactive = allowed && ui.is_visible() && !ui.is_sizing_pass();
            let (host, next) =
                crate::gui::utility_host::presentation_button(ui, presentation, interactive);
            if first && !ctx.will_discard() && host.clicked() {
                request = Some(next);
            }
            ui.add_enabled_ui(interactive, |ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let next = ui
                    .push_id(self.session, |ui| {
                        K::body(ui, &mut self.draft, self.workspace, self.view, self.session)
                    })
                    .inner;
                if first && !ctx.will_discard() {
                    command = next;
                }
            });
        };
        if let Some(ui) = child {
            body(ui);
        } else {
            let id = egui::Id::new((K::KEY, self.session));
            egui::Window::new(K::TITLE)
                .id(id)
                .collapsible(false)
                .resizable(true)
                .default_size(K::SIZE)
                .show_utility(
                    ctx,
                    K::KEY,
                    id,
                    Some(&mut open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::outer(K::SIZE),
                    body,
                );
        }
        if first && allowed && !ctx.will_discard() && !open {
            command = Some(Command::Close);
        }
        Actions { request, command }
    }
    fn host_request(actions: &Actions) -> Option<UtilityPresentation> {
        actions.request
    }
    fn closed_command() -> Command {
        Command::Close
    }
    fn command_closes(command: &Command) -> bool {
        matches!(command, Command::Close)
    }
    fn complete_actions(&mut self, actions: Actions, _: Option<usize>) -> Vec<Command> {
        actions.command.into_iter().collect()
    }
    #[cfg(test)]
    fn test_button(_: &Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

pub(super) struct Rules;
impl Kind for Rules {
    type Draft = super::rules_dialog::RulesDialogState;
    const KEY: &'static str = "diff-comparison-rules";
    const TITLE: &'static str = "Comparison rules";
    const SIZE: egui::Vec2 = egui::vec2(620.0, 500.0);
    fn body(ui: &mut egui::Ui, draft: &mut Self::Draft, _: u64, _: u64, _: u64) -> Option<Command> {
        let mut applied = None;
        super::rules_dialog::render_contents(ui, draft, &mut applied);
        let closed = !draft.open;
        draft.open = true;
        applied
            .map(Command::Rules)
            .or_else(|| closed.then_some(Command::Close))
    }
}
pub(super) struct Folder;
impl Kind for Folder {
    type Draft = crate::diff::model::FolderRulesDraft;
    const KEY: &'static str = "diff-folder-rules";
    const TITLE: &'static str = "Folder Rules";
    const SIZE: egui::Vec2 = egui::vec2(700.0, 560.0);
    fn body(ui: &mut egui::Ui, draft: &mut Self::Draft, _: u64, _: u64, _: u64) -> Option<Command> {
        match super::folder_rules_dialog::render_contents(ui, draft) {
            Some(super::folder_rules_dialog::FormAction::Apply) => {
                Some(Command::Folder(draft.clone()))
            }
            Some(super::folder_rules_dialog::FormAction::Cancel) => Some(Command::Close),
            None => None,
        }
    }
}
pub(super) struct Input;
impl Kind for Input {
    type Draft = TextInputDraft;
    const KEY: &'static str = "diff-text-input";
    const TITLE: &'static str = "Text inputs";
    const SIZE: egui::Vec2 = egui::vec2(760.0, 540.0);
    fn body(
        ui: &mut egui::Ui,
        draft: &mut Self::Draft,
        workspace: u64,
        view: u64,
        session: u64,
    ) -> Option<Command> {
        let identity = match draft.purpose {
            TextInputPurpose::NewComparison => 0,
            TextInputPurpose::EditInputs { .. } => view,
        };
        let mut actions = Vec::new();
        let mut submit = false;
        let mut cancel = false;
        super::render_text_input_contents(
            ui,
            draft,
            workspace,
            identity,
            session,
            &mut actions,
            &mut submit,
            &mut cancel,
        );
        if cancel {
            Some(Command::Close)
        } else if submit {
            Some(Command::Input)
        } else {
            actions.into_iter().find_map(|action| {
                if let DiffUiAction::PasteTextInput(side) = action {
                    Some(Command::Paste(side))
                } else {
                    None
                }
            })
        }
    }
}

use crate::diff::folder_scan::ScanRules;
use crate::diff::model::{DiffView, FolderRulesDraft};
use crate::gui::utility_host::{AcceptedParentOpen, ChildOpenOrigin};
use crate::gui::utility_window_host::{HostInput, UtilityWindowHost};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Basis {
    Rules {
        left: super::TextInputSourceSnapshot,
        right: super::TextInputSourceSnapshot,
        rules: crate::diff::text_compare::TextComparisonRules,
    },
    Folder {
        left: PathBuf,
        right: PathBuf,
        draft: FolderRulesDraft,
        applied: ScanRules,
    },
    Input,
}
struct Slot<K: Kind> {
    host: UtilityWindowHost<Editor<K>>,
    editor: Editor<K>,
    basis: Basis,
}
#[derive(Default)]
pub(super) struct Children {
    rules: Vec<Slot<Rules>>,
    folders: Vec<Slot<Folder>>,
    input: Option<Slot<Input>>,
    next_session: u64,
}
#[derive(Clone)]
pub(super) enum Open {
    Rules(u64, Basis),
    Folder(u64, Basis),
    Input(
        TextInputPurpose,
        Option<(
            super::TextInputSourceSnapshot,
            super::TextInputSourceSnapshot,
        )>,
    ),
}
impl Open {
    pub fn rules(parent: &super::DiffDialogState, view: u64) -> Option<Self> {
        if parent.workspace.current_view.id != view
            || !matches!(parent.workspace.current_view.view, DiffView::TextCompare(_))
        {
            return None;
        }
        let model = parent.text_views.get(&view)?;
        Some(Self::Rules(
            view,
            Basis::Rules {
                left: super::TextInputSourceSnapshot::capture(&model.left),
                right: super::TextInputSourceSnapshot::capture(&model.right),
                rules: model.rules.clone(),
            },
        ))
    }
    pub fn folder(parent: &super::DiffDialogState, view: u64) -> Option<Self> {
        if parent.workspace.current_view.id != view {
            return None;
        }
        let DiffView::FolderCompare(state) = &parent.workspace.current_view.view else {
            return None;
        };
        Some(Self::Folder(
            view,
            Basis::Folder {
                left: state.left_root.clone(),
                right: state.right_root.clone(),
                draft: state.draft_rules.clone(),
                applied: state.applied_scan_rules.clone(),
            },
        ))
    }
    pub fn input(parent: &super::DiffDialogState, purpose: TextInputPurpose) -> Option<Self> {
        let expected = match purpose {
            TextInputPurpose::NewComparison => None,
            TextInputPurpose::EditInputs { view_id } => {
                if parent.workspace.current_view.id != view_id {
                    return None;
                }
                let model = parent.text_views.get(&view_id)?;
                if model.origin != crate::diff::model::TextCompareOrigin::Memory {
                    return None;
                }
                Some((
                    super::TextInputSourceSnapshot::capture(&model.left),
                    super::TextInputSourceSnapshot::capture(&model.right),
                ))
            }
        };
        Some(Self::Input(purpose, expected))
    }
    fn is_current(&self, parent: &super::DiffDialogState) -> bool {
        match self {
            Self::Rules(view, basis) => Self::rules(parent, *view).is_some_and(
                |request| matches!(request, Self::Rules(_, current) if current == *basis),
            ),
            Self::Folder(view, basis) => Self::folder(parent, *view).is_some_and(
                |request| matches!(request, Self::Folder(_, current) if current == *basis),
            ),
            Self::Input(purpose, expected) => Self::input(parent, *purpose).is_some_and(
                |request| matches!(request, Self::Input(_, current) if current == *expected),
            ),
        }
    }
}
impl Children {
    fn next(&mut self) -> u64 {
        self.next_session = self.next_session.wrapping_add(1).max(1);
        self.next_session
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        jump_active: bool,
    ) {
        fn show_one<K: Kind>(
            slot: &mut Slot<K>,
            ctx: &egui::Context,
            first: bool,
            root_visible: bool,
            root_input: bool,
            native_input: bool,
            jump_active: bool,
        ) {
            let layer = slot.host.root_layer();
            let owner = root_input
                && !jump_active
                && layer.is_some()
                && crate::gui::jump_mode::ui::visible_top_middle_layer(ctx) == layer
                && !egui::Popup::is_any_open(ctx)
                && crate::gui::jump_mode::ui::focused_response_layer(ctx)
                    .is_none_or(|l| Some(l) == layer || l.order == egui::Order::Background);
            slot.host.show(
                ctx,
                &mut slot.editor,
                &(),
                HostInput {
                    first,
                    root_visible,
                    root_input,
                    native_input,
                    root_owner: NavigationOwner::Blocked,
                    parent_owner: owner,
                    root_jump_active: jump_active,
                    close_shortcut: true,
                    allow_os_close: true,
                    confirmation_owner: NavigationOwner::Blocked,
                    parent_instance: 0,
                    root_scopes: (None, None),
                },
            );
        }
        for slot in &mut self.rules {
            show_one(
                slot,
                ctx,
                first,
                root_visible,
                root_input,
                native_input,
                jump_active,
            );
        }
        for slot in &mut self.folders {
            show_one(
                slot,
                ctx,
                first,
                root_visible,
                root_input,
                native_input,
                jump_active,
            );
        }
        if let Some(slot) = &mut self.input {
            show_one(
                slot,
                ctx,
                first,
                root_visible,
                root_input,
                native_input,
                jump_active,
            );
        }
    }
    #[cfg(test)]
    pub(super) fn test_input_viewport(&self) -> Option<egui::ViewportId> {
        self.input
            .as_ref()
            .map(|slot| UtilityWindowHost::<Editor<Input>>::test_viewport(slot.editor.session))
    }
    #[cfg(test)]
    pub(super) fn test_input(&self) -> Option<(u64, bool, &str)> {
        self.input.as_ref().map(|slot| {
            (
                slot.editor.session,
                slot.host.is_popped_out(),
                slot.editor.draft.left.as_str(),
            )
        })
    }
    pub fn needs_root_ui_when_hidden(&self) -> bool {
        self.rules
            .iter()
            .any(|s| s.host.needs_root_ui_when_hidden(s.editor.open))
            || self
                .folders
                .iter()
                .any(|s| s.host.needs_root_ui_when_hidden(s.editor.open))
            || self
                .input
                .as_ref()
                .is_some_and(|s| s.host.needs_root_ui_when_hidden(s.editor.open))
    }
    pub fn root_layers(&self) -> Vec<egui::LayerId> {
        self.rules
            .iter()
            .filter_map(|s| s.host.root_layer())
            .chain(self.folders.iter().filter_map(|s| s.host.root_layer()))
            .chain(self.input.iter().filter_map(|s| s.host.root_layer()))
            .collect()
    }
    pub fn retire(&mut self, ctx: &egui::Context) {
        for slot in &mut self.rules {
            slot.host.close_from_app(ctx);
        }
        for slot in &mut self.folders {
            slot.host.close_from_app(ctx);
        }
        if let Some(slot) = &mut self.input {
            slot.host.close_from_app(ctx);
        }
        self.rules.clear();
        self.folders.clear();
        self.input = None;
    }
    pub fn open(
        &mut self,
        ctx: &egui::Context,
        parent: &mut super::DiffDialogState,
        open: Open,
        origin: &AcceptedParentOpen<(u64, u64)>,
    ) -> Option<UtilityPresentation> {
        let workspace = parent.workspace.workspace_id;
        let view = parent.workspace.current_view.id;
        if !parent.open
            || parent.close_prompt
            || parent.operation_preview.is_some()
            || !parent.host.accepts_child_origin(origin, workspace, view)
            || !open.is_current(parent)
        {
            return None;
        }
        let origin = ChildOpenOrigin::Parent(origin.clone());
        match open {
            Open::Rules(requested, captured_basis) if requested == view => {
                if let Some(slot) = self.rules.iter_mut().find(|s| {
                    s.editor.open && s.editor.workspace == workspace && s.editor.view == view
                }) {
                    return slot.host.open_from_origin(ctx, &slot.editor, true, &origin);
                }
                if !matches!(parent.workspace.current_view.view, DiffView::TextCompare(_)) {
                    return None;
                }
                let model = parent.text_views.get(&view)?;
                let basis = captured_basis;
                let draft = super::rules_dialog::RulesDialogState::new(&model.rules);
                let session = self.next();
                let mut slot = Slot {
                    host: UtilityWindowHost::default(),
                    editor: Editor::new(session, workspace, view, draft),
                    basis,
                };
                let presentation = slot
                    .host
                    .open_from_origin(ctx, &slot.editor, false, &origin);
                self.rules.push(slot);
                presentation
            }
            Open::Folder(requested, captured_basis) if requested == view => {
                if let Some(slot) = self.folders.iter_mut().find(|s| {
                    s.editor.open && s.editor.workspace == workspace && s.editor.view == view
                }) {
                    return slot.host.open_from_origin(ctx, &slot.editor, true, &origin);
                }
                let DiffView::FolderCompare(state) = &parent.workspace.current_view.view else {
                    return None;
                };
                let basis = captured_basis;
                let draft = state.draft_rules.clone();
                let session = self.next();
                let mut slot = Slot {
                    host: UtilityWindowHost::default(),
                    editor: Editor::new(session, workspace, view, draft),
                    basis,
                };
                let presentation = slot
                    .host
                    .open_from_origin(ctx, &slot.editor, false, &origin);
                self.folders.push(slot);
                presentation
            }
            Open::Input(purpose, _) => {
                if let Some(slot) = &mut self.input {
                    if slot.editor.open && slot.editor.workspace == workspace {
                        if slot.editor.draft.purpose != purpose {
                            slot.editor.draft.error = Some(
                                "Finish or cancel the current text input before opening another."
                                    .into(),
                            );
                        }
                        return slot.host.open_from_origin(ctx, &slot.editor, true, &origin);
                    }
                }
                parent.begin_text_input(purpose);
                let draft = parent.text_input.take()?;
                let session = self.next();
                let mut slot = Slot {
                    host: UtilityWindowHost::default(),
                    editor: Editor::new(session, workspace, view, draft),
                    basis: Basis::Input,
                };
                let presentation = slot
                    .host
                    .open_from_origin(ctx, &slot.editor, false, &origin);
                self.input = Some(slot);
                presentation
            }
            _ => None,
        }
    }
}
fn input_source_current(
    parent: &super::DiffDialogState,
    workspace: u64,
    draft: &TextInputDraft,
) -> bool {
    parent.open
        && !parent.close_prompt
        && parent.operation_preview.is_none()
        && parent.workspace.workspace_id == workspace
        && match draft.purpose {
            TextInputPurpose::NewComparison => true,
            TextInputPurpose::EditInputs { view_id } => {
                parent.workspace.current_view.id == view_id
                    && parent.text_views.get(&view_id).is_some_and(|model| {
                        model.origin == crate::diff::model::TextCompareOrigin::Memory
                            && draft.expected_sources
                                == Some((
                                    super::TextInputSourceSnapshot::capture(&model.left),
                                    super::TextInputSourceSnapshot::capture(&model.right),
                                ))
                    })
            }
        }
}

fn apply_rules(
    parent: &mut super::DiffDialogState,
    workspace: u64,
    view: u64,
    basis: &Basis,
    rules: crate::diff::text_compare::TextComparisonRules,
) -> bool {
    let valid = parent.open
        && !parent.close_prompt
        && parent.operation_preview.is_none()
        && parent.workspace.workspace_id == workspace
        && parent.workspace.current_view.id == view
        && matches!(parent.workspace.current_view.view, DiffView::TextCompare(_))
        && match (basis, parent.text_views.get(&view)) {
            (Basis::Rules { left, right, rules }, Some(model)) => {
                *left == super::TextInputSourceSnapshot::capture(&model.left)
                    && *right == super::TextInputSourceSnapshot::capture(&model.right)
                    && *rules == model.rules
            }
            _ => false,
        };
    if !valid {
        return false;
    }
    parent.apply_view_effects(
        view,
        vec![super::effects::ViewEffect::SetTextRules(rules)],
        super::effects::PickerOwner::Legacy,
    );
    true
}

fn apply_folder(
    parent: &mut super::DiffDialogState,
    workspace: u64,
    view: u64,
    basis: &Basis,
    draft: FolderRulesDraft,
) -> bool {
    let valid = parent.open
        && !parent.close_prompt
        && parent.operation_preview.is_none()
        && parent.workspace.workspace_id == workspace
        && parent.workspace.current_view.id == view
        && match (basis, &parent.workspace.current_view.view) {
            (
                Basis::Folder {
                    left,
                    right,
                    draft,
                    applied,
                },
                DiffView::FolderCompare(state),
            ) => {
                *left == state.left_root
                    && *right == state.right_root
                    && *draft == state.draft_rules
                    && *applied == state.applied_scan_rules
            }
            _ => false,
        };
    if !valid {
        return false;
    }
    if let DiffView::FolderCompare(state) = &mut parent.workspace.current_view.view {
        if draft.validate().is_err() {
            return false;
        }
        state.draft_rules = draft;
    }
    parent.apply_folder_action(super::folder_view::FolderViewAction::RequestRescan);
    true
}

#[derive(Clone, Copy)]
pub(super) enum RootRequest {
    Rules(u64),
    Folder(u64),
    Input(u64),
}
impl Children {
    pub fn has_requested_root(&self, requests: &[RootRequest]) -> bool {
        requests.iter().any(|request| match request {
            RootRequest::Rules(session) => self.rules.iter().any(|slot| {
                slot.editor.session == *session
                    && slot.editor.open
                    && slot.host.root_layer().is_some()
            }),
            RootRequest::Folder(session) => self.folders.iter().any(|slot| {
                slot.editor.session == *session
                    && slot.editor.open
                    && slot.host.root_layer().is_some()
            }),
            RootRequest::Input(session) => self.input.as_ref().is_some_and(|slot| {
                slot.editor.session == *session
                    && slot.editor.open
                    && slot.host.root_layer().is_some()
            }),
        })
    }
    pub fn complete(
        &mut self,
        ctx: &egui::Context,
        parent: &mut super::DiffDialogState,
    ) -> Vec<RootRequest> {
        let mut show_root = Vec::new();
        for slot in &mut self.rules {
            if let Some(completion) = slot.host.complete(ctx, &mut slot.editor) {
                if completion.show_root {
                    show_root.push(RootRequest::Rules(slot.editor.session));
                }
                for command in completion.commands {
                    match command {
                        Command::Close => slot.editor.open = false,
                        Command::Rules(rules) => {
                            if apply_rules(
                                parent,
                                slot.editor.workspace,
                                slot.editor.view,
                                &slot.basis,
                                rules,
                            ) {
                                slot.editor.open = false;
                                slot.host.close_from_app(ctx);
                            } else {
                                slot.editor.draft.error = Some(
                                    "The comparison changed. Reopen Rules for the current sources."
                                        .into(),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        for slot in &mut self.folders {
            if let Some(completion) = slot.host.complete(ctx, &mut slot.editor) {
                if completion.show_root {
                    show_root.push(RootRequest::Folder(slot.editor.session));
                }
                for command in completion.commands {
                    match command {
                        Command::Close => slot.editor.open = false,
                        Command::Folder(draft) => {
                            if apply_folder(
                                parent,
                                slot.editor.workspace,
                                slot.editor.view,
                                &slot.basis,
                                draft,
                            ) {
                                slot.editor.open = false;
                                slot.host.close_from_app(ctx);
                            } else {
                                slot.editor.error = Some(
                                    "The folder comparison or rules changed. Reopen this editor."
                                        .into(),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(slot) = &mut self.input {
            let input_current =
                input_source_current(parent, slot.editor.workspace, &slot.editor.draft);
            if let Some(completion) = slot.host.complete(ctx, &mut slot.editor) {
                if completion.show_root {
                    show_root.push(RootRequest::Input(slot.editor.session));
                }
                for command in completion.commands {
                    match command {
                        Command::Close => slot.editor.open = false,
                        Command::Input if input_current => {
                            match parent.apply_text_input(slot.editor.draft.clone()) {
                                Ok(()) => {
                                    slot.editor.open = false;
                                    slot.host.close_from_app(ctx);
                                }
                                Err((draft, error)) => {
                                    slot.editor.draft = draft;
                                    slot.editor.draft.error = Some(error);
                                }
                            }
                        }
                        Command::Paste(side) if input_current => {
                            let input = match side {
                                crate::diff::model::DiffSide::Left => &mut slot.editor.draft.left,
                                crate::diff::model::DiffSide::Right => &mut slot.editor.draft.right,
                            };
                            slot.editor.draft.error = super::paste_memory_input(input, || {
                                crate::actions::clipboard::get_text().map_err(|_| {
                                    "Clipboard is unavailable or does not contain plain text"
                                        .to_owned()
                                })
                            })
                            .err();
                        }
                        Command::Input | Command::Paste(_) => {
                            slot.editor.draft.error =
                                Some("The comparison changed. Reopen this editor.".into())
                        }
                        _ => {}
                    }
                }
            }
        }
        self.rules.retain(|s| s.editor.open);
        self.folders.retain(|s| s.editor.open);
        if self.input.as_ref().is_some_and(|s| !s.editor.open) {
            self.input = None;
        }
        show_root
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn memory_parent() -> super::super::DiffDialogState {
        let mut parent = super::super::DiffDialogState::default();
        parent.open = true;
        parent.begin_text_input(TextInputPurpose::NewComparison);
        let mut draft = parent.text_input.take().unwrap();
        draft.left = "left".into();
        draft.right = "right".into();
        parent
            .apply_text_input(draft)
            .map_err(|(_, error)| error)
            .unwrap();
        parent
    }

    #[test]
    fn captured_open_rejects_changed_or_swapped_sources_and_rules() {
        let mut parent = memory_parent();
        let view = parent.workspace.current_view.id;
        let rules = Open::rules(&parent, view).unwrap();
        let input = Open::input(&parent, TextInputPurpose::EditInputs { view_id: view }).unwrap();
        assert!(rules.is_current(&parent));
        assert!(input.is_current(&parent));
        parent
            .text_views
            .get_mut(&view)
            .unwrap()
            .left
            .replace_source("changed".into())
            .unwrap();
        assert!(!rules.is_current(&parent));
        assert!(!input.is_current(&parent));
        let fresh = Open::rules(&parent, view).unwrap();
        parent.text_views.get_mut(&view).unwrap().swap_sides();
        assert!(!fresh.is_current(&parent));
    }

    #[test]
    fn folder_capture_ignores_progress_but_rejects_changed_rule_basis() {
        let mut parent = super::super::DiffDialogState::default();
        parent.open = true;
        parent.workspace.current_view.view = DiffView::FolderCompare(Default::default());
        let view = parent.workspace.current_view.id;
        let open = Open::folder(&parent, view).unwrap();
        if let DiffView::FolderCompare(state) = &mut parent.workspace.current_view.view {
            state.left_scan_complete = true;
        }
        assert!(open.is_current(&parent));
        if let DiffView::FolderCompare(state) = &mut parent.workspace.current_view.view {
            state.draft_rules.exclude_rules = "*.tmp".into();
        }
        assert!(!open.is_current(&parent));
    }
    #[test]
    fn live_rules_open_reuses_private_draft_and_session() {
        let mut parent = memory_parent();
        let ctx = egui::Context::default();
        let workspace = parent.workspace.workspace_id;
        let view = parent.workspace.current_view.id;
        let pass =
            crate::gui::utility_host::ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT);
        let output = ctx.run_ui(egui::RawInput::default(), |_| {});
        let origin = AcceptedParentOpen::from_retained(
            &ctx,
            parent.host.test_identity(),
            (workspace, view),
            pass,
            pass,
        )
        .unwrap();
        let mut children = Children::default();
        let request = Open::rules(&parent, view).unwrap();
        assert_eq!(
            children.open(&ctx, &mut parent, request, &origin),
            Some(UtilityPresentation::Docked)
        );
        let session = children.rules[0].editor.session;
        children.rules[0].editor.draft.replacements = "unsaved => draft".into();
        let request = Open::rules(&parent, view).unwrap();
        assert_eq!(
            children.open(&ctx, &mut parent, request, &origin),
            Some(UtilityPresentation::Docked)
        );
        assert_eq!(children.rules.len(), 1);
        assert_eq!(children.rules[0].editor.session, session);
        assert_eq!(
            children.rules[0].editor.draft.replacements,
            "unsaved => draft"
        );
        assert!(parent.text_views[&view].rules.replacements.is_empty());
        children.rules[0].host.close_from_app(&ctx);
        children.rules[0].editor.open = false;
        children.rules.clear();
        let request = Open::rules(&parent, view).unwrap();
        assert_eq!(
            children.open(&ctx, &mut parent, request, &origin),
            Some(UtilityPresentation::Docked)
        );
        assert_ne!(children.rules[0].editor.session, session);
        parent.host.test_transition(UtilityPresentation::Popout);
        let request = Open::rules(&parent, view).unwrap();
        assert_eq!(children.open(&ctx, &mut parent, request, &origin), None);
        output.drop_without_applying_deltas();
    }
    #[test]
    fn folder_candidate_applies_once_and_cancel_leaves_live_draft() {
        let mut parent = super::super::DiffDialogState::default();
        parent.open = true;
        parent.workspace.current_view.view = DiffView::FolderCompare(Default::default());
        let workspace = parent.workspace.workspace_id;
        let view = parent.workspace.current_view.id;
        let Open::Folder(_, basis) = Open::folder(&parent, view).unwrap() else {
            unreachable!()
        };
        let mut private = match &basis {
            Basis::Folder { draft, .. } => draft.clone(),
            _ => unreachable!(),
        };
        private.exclude_rules = "*.tmp".into();
        let DiffView::FolderCompare(state) = &parent.workspace.current_view.view else {
            unreachable!()
        };
        assert!(state.draft_rules.exclude_rules.is_empty());
        assert!(apply_folder(
            &mut parent,
            workspace,
            view,
            &basis,
            private.clone()
        ));
        let generation = parent.folder_runtimes[&view].generation;
        assert!(parent.folder_runtimes[&view].restart_prepared);
        assert!(!apply_folder(&mut parent, workspace, view, &basis, private));
        assert_eq!(parent.folder_runtimes[&view].generation, generation);
    }
    #[test]
    fn rules_candidate_requires_current_basis_and_discard_restores_draft() {
        let mut parent = memory_parent();
        let workspace = parent.workspace.workspace_id;
        let view = parent.workspace.current_view.id;
        let Open::Rules(_, basis) = Open::rules(&parent, view).unwrap() else {
            unreachable!()
        };
        let rules = parent.text_views[&view].rules.clone();
        let mut editor = Editor::<Rules>::new(
            1,
            workspace,
            view,
            super::super::rules_dialog::RulesDialogState::new(&rules),
        );
        let checkpoint = editor.capture();
        editor.draft.replacements = "private => edit".into();
        editor.restore(checkpoint);
        assert!(editor.draft.replacements.is_empty());
        assert_eq!(parent.text_views[&view].rules, rules);
        let mut candidate = rules.clone();
        candidate.case_sensitive = !candidate.case_sensitive;
        assert!(apply_rules(
            &mut parent,
            workspace,
            view,
            &basis,
            candidate.clone()
        ));
        let updated = parent.text_views[&view].rules.clone();
        assert_eq!(updated.case_sensitive, candidate.case_sensitive);
        assert!(!apply_rules(
            &mut parent,
            workspace,
            view,
            &basis,
            candidate
        ));
        assert_eq!(parent.text_views[&view].rules, updated);
    }
    #[test]
    fn input_source_guard_rejects_swapped_sources_before_clipboard() {
        let mut parent = memory_parent();
        let workspace = parent.workspace.workspace_id;
        let view = parent.workspace.current_view.id;
        parent.begin_text_input(TextInputPurpose::EditInputs { view_id: view });
        let draft = parent.text_input.take().unwrap();
        assert!(input_source_current(&parent, workspace, &draft));
        parent.text_views.get_mut(&view).unwrap().swap_sides();
        assert!(!input_source_current(&parent, workspace, &draft));
    }
    fn rendered_frame(
        ctx: &egui::Context,
        children: &mut Children,
        parent: &mut super::super::DiffDialogState,
        events: Vec<egui::Event>,
        discard: bool,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events,
            focused: true,
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            children.show(
                ui.ctx(),
                ui.ctx().current_pass_index() == 0,
                true,
                true,
                true,
                false,
            );
            if discard && ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("discard child test");
            }
        });
        children.complete(ctx, parent);
        output.drop_without_applying_deltas();
    }
    fn button(ctx: &egui::Context, name: &str) -> egui::Rect {
        let (rect, clip, enabled) = ctx
            .data(|data| data.get_temp::<(egui::Rect, egui::Rect, bool)>(egui::Id::new(name)))
            .expect("rendered child button");
        assert!(
            enabled && clip.contains_rect(rect),
            "button={rect:?}, clip={clip:?}, enabled={enabled}"
        );
        rect
    }
    fn pointer(rect: egui::Rect, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(rect.center()),
            egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }
    fn origin(
        ctx: &egui::Context,
        parent: &super::super::DiffDialogState,
    ) -> AcceptedParentOpen<(u64, u64)> {
        let pass =
            crate::gui::utility_host::ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let output = ctx.run_ui(egui::RawInput::default(), |_| {});
        let receipt = AcceptedParentOpen::from_retained(
            ctx,
            parent.host.test_identity(),
            (
                parent.workspace.workspace_id,
                parent.workspace.current_view.id,
            ),
            pass,
            pass,
        )
        .unwrap();
        output.drop_without_applying_deltas();
        receipt
    }
    #[test]
    fn rendered_rules_apply_requires_retained_child_pass() {
        let mut parent = memory_parent();
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let view = parent.workspace.current_view.id;
        let receipt = origin(&ctx, &parent);
        let mut children = Children::default();
        let request = Open::rules(&parent, view).unwrap();
        children.open(&ctx, &mut parent, request, &receipt);
        let previous = parent.text_views[&view].rules.case_sensitive;
        children.rules[0].editor.draft.draft.case_sensitive = !previous;
        for _ in 0..4 {
            rendered_frame(&ctx, &mut children, &mut parent, Vec::new(), false);
        }
        let apply = button(&ctx, "diff-rules-apply-test");
        rendered_frame(
            &ctx,
            &mut children,
            &mut parent,
            pointer(apply, true),
            false,
        );
        rendered_frame(
            &ctx,
            &mut children,
            &mut parent,
            pointer(apply, false),
            true,
        );
        assert_eq!(parent.text_views[&view].rules.case_sensitive, previous);
        assert_eq!(children.rules.len(), 1);
        for _ in 0..2 {
            rendered_frame(&ctx, &mut children, &mut parent, Vec::new(), false);
        }
        let apply = button(&ctx, "diff-rules-apply-test");
        rendered_frame(
            &ctx,
            &mut children,
            &mut parent,
            pointer(apply, true),
            false,
        );
        rendered_frame(
            &ctx,
            &mut children,
            &mut parent,
            pointer(apply, false),
            false,
        );
        assert_eq!(parent.text_views[&view].rules.case_sensitive, !previous);
        assert!(children.rules.is_empty());
    }
    #[test]
    fn rendered_folder_cancel_then_apply_changes_live_rules_once() {
        let mut parent = super::super::DiffDialogState::default();
        parent.open = true;
        parent.workspace.current_view.view = DiffView::FolderCompare(Default::default());
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let view = parent.workspace.current_view.id;
        let receipt = origin(&ctx, &parent);
        let mut children = Children::default();
        let request = Open::folder(&parent, view).unwrap();
        children.open(&ctx, &mut parent, request, &receipt);
        children.folders[0].editor.draft.exclude_rules = "*.tmp".into();
        for _ in 0..4 {
            rendered_frame(&ctx, &mut children, &mut parent, Vec::new(), false);
        }
        let cancel = button(&ctx, "diff-folder-cancel-test");
        rendered_frame(
            &ctx,
            &mut children,
            &mut parent,
            pointer(cancel, true),
            false,
        );
        rendered_frame(
            &ctx,
            &mut children,
            &mut parent,
            pointer(cancel, false),
            false,
        );
        assert!(children.folders.is_empty());
        let DiffView::FolderCompare(state) = &parent.workspace.current_view.view else {
            unreachable!()
        };
        assert!(state.draft_rules.exclude_rules.is_empty());
        assert!(!parent.folder_runtimes.contains_key(&view));
        let request = Open::folder(&parent, view).unwrap();
        children.open(&ctx, &mut parent, request, &receipt);
        children.folders[0].editor.draft.exclude_rules = "*.tmp".into();
        for _ in 0..4 {
            rendered_frame(&ctx, &mut children, &mut parent, Vec::new(), false);
        }
        let apply = button(&ctx, "diff-folder-apply-test");
        rendered_frame(
            &ctx,
            &mut children,
            &mut parent,
            pointer(apply, true),
            false,
        );
        rendered_frame(
            &ctx,
            &mut children,
            &mut parent,
            pointer(apply, false),
            false,
        );
        assert!(children.folders.is_empty());
        assert!(parent.folder_runtimes[&view].restart_prepared);
        let generation = parent.folder_runtimes[&view].generation;
        rendered_frame(&ctx, &mut children, &mut parent, Vec::new(), false);
        assert_eq!(parent.folder_runtimes[&view].generation, generation);
    }
}
