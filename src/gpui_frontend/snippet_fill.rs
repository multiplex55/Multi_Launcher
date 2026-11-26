//! Retained fields for one checked Snippet Run.
use super::*;
use multi_launcher::plugins::snippets::SnippetInputKind;
use multi_launcher::snippet_prompt::{PromptError, PromptSession};

enum FieldInput {
    Single(Entity<InputState>),
    Multi(Entity<TextareaState>),
}

struct Field {
    key: String,
    label: String,
    required: bool,
    input: FieldInput,
    initial_value: String,
    initial_display: String,
}

fn restored_value(original: &str, initial_display: &str, current_display: &str) -> String {
    if current_display == initial_display { original.to_owned() } else { current_display.to_owned() }
}

fn next_focus(current: Option<usize>, count: usize, backwards: bool) -> usize {
    match current {
        Some(index) if backwards => (index + count - 1) % count,
        Some(index) => (index + 1) % count,
        None if backwards => count - 1,
        None => 0,
    }
}

pub(super) fn issued_mount_is_current(
    issued: &utility_mount::MountReceipt,
    current: &utility_mount::MountReceipt,
    installed: bool,
    open: bool,
    pending: bool,
) -> bool {
    issued.is_current() && issued.matches(current) && installed && open && !pending
}

impl Field {
    fn value(&self, cx: &App) -> String {
        let displayed = match &self.input {
            FieldInput::Single(input) => input.read(cx).value().to_string(),
            FieldInput::Multi(input) => input.read(cx).value().to_string(),
        };
        restored_value(&self.initial_value, &self.initial_display, &displayed)
    }
}

pub(super) struct SnippetFill {
    pub open: bool,
    pub(super) pending: bool,
    pub(super) session: Option<PromptSession>,
    pub(super) request: Option<external_launch::Request>,
    preview_parent: Option<(WeakEntity<snippets::SnippetsView>, utility_mount::MountReceipt, u64, WindowId)>,
    pub(super) owner: utility_mount::MountOwner,
    pub(super) mount: utility_mount::MountReceipt,
    root: WeakEntity<RootWindow>,
    return_focus: Option<FocusHandle>,
    focus: FocusHandle,
    copy_focus: FocusHandle,
    cancel_focus: FocusHandle,
    scroll: ScrollHandle,
    fields: Vec<Field>,
    subscriptions: Vec<Subscription>,
    preview: String,
    preview_issues: Vec<String>,
    feedback: Option<String>,
}

impl SnippetFill {
    pub(super) fn new(session: PromptSession, request: external_launch::Request, root: WeakEntity<RootWindow>, return_focus: Option<FocusHandle>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let mut subscriptions = Vec::new();
        let fields = session.fields().iter().map(|definition| {
            let value = session.value(&definition.name).unwrap_or_default().to_owned();
            let initial_value = value.clone();
            let input = match definition.input_kind {
                SnippetInputKind::SingleLine => {
                    let input = cx.new(|cx| InputState::new(window, cx));
                    input.update(cx, |input, cx| input.set_value(value, window, cx));
                    subscriptions.push(cx.subscribe(&input, |view: &mut Self, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) { view.refresh_preview(cx); }
                    }));
                    FieldInput::Single(input)
                }
                SnippetInputKind::Multiline => {
                    let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 8));
                    input.update(cx, |input, cx| input.set_value(value, window, cx));
                    subscriptions.push(cx.subscribe(&input, |view: &mut Self, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) { view.refresh_preview(cx); }
                    }));
                    FieldInput::Multi(input)
                }
            };
            let initial_display = match &input {
                FieldInput::Single(input) => input.read(cx).value().to_string(),
                FieldInput::Multi(input) => input.read(cx).value().to_string(),
            };
            Field { key: definition.name.clone(), label: definition.display_label().into_owned(), required: definition.required,
                input, initial_value, initial_display }
        }).collect();
        let mut view = Self { open: true, pending: false, session: Some(session), request: Some(request), preview_parent: None, owner, mount, root, return_focus,
            focus: cx.focus_handle(), copy_focus: cx.focus_handle(), cancel_focus: cx.focus_handle(), scroll: ScrollHandle::new(), fields,
            subscriptions, preview: String::new(), preview_issues: Vec::new(), feedback: None };
        view.refresh_preview(cx);
        view
    }

    pub(super) fn preview(session: PromptSession, parent: WeakEntity<snippets::SnippetsView>, parent_mount: utility_mount::MountReceipt,
        instance: u64, host: WindowId, root: WeakEntity<RootWindow>, return_focus: Option<FocusHandle>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let mut subscriptions = Vec::new();
        let fields = session.fields().iter().map(|definition| {
            let value = session.value(&definition.name).unwrap_or_default().to_owned();
            let initial_value = value.clone();
            let input = match definition.input_kind {
                SnippetInputKind::SingleLine => {
                    let input = cx.new(|cx| InputState::new(window, cx));
                    input.update(cx, |input, cx| input.set_value(value, window, cx));
                    subscriptions.push(cx.subscribe(&input, |view: &mut Self, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) { view.refresh_preview(cx); }
                    }));
                    FieldInput::Single(input)
                }
                SnippetInputKind::Multiline => {
                    let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 8));
                    input.update(cx, |input, cx| input.set_value(value, window, cx));
                    subscriptions.push(cx.subscribe(&input, |view: &mut Self, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) { view.refresh_preview(cx); }
                    }));
                    FieldInput::Multi(input)
                }
            };
            let initial_display = match &input { FieldInput::Single(input) => input.read(cx).value().to_string(), FieldInput::Multi(input) => input.read(cx).value().to_string() };
            Field { key: definition.name.clone(), label: definition.display_label().into_owned(), required: definition.required, input, initial_value, initial_display }
        }).collect();
        let mut view = Self { open: true, pending: false, session: Some(session), request: None,
            preview_parent: Some((parent, parent_mount, instance, host)), owner, mount, root, return_focus,
            focus: cx.focus_handle(), copy_focus: cx.focus_handle(), cancel_focus: cx.focus_handle(), scroll: ScrollHandle::new(), fields,
            subscriptions, preview: String::new(), preview_issues: Vec::new(), feedback: None };
        view.refresh_preview(cx);
        view
    }

    pub(super) fn focus_initial(&self, window: &mut Window, cx: &mut App) {
        let initial = self.session.as_ref().and_then(|session| session.initial_focus());
        if let Some((index, field)) = self.fields.iter().enumerate().find(|(_, field)| Some(field.key.as_str()) == initial) {
            self.scroll.scroll_to_top_of_item(index);
            match &field.input {
                FieldInput::Single(input) => input.focus_handle(cx).focus(window, cx),
                FieldInput::Multi(input) => input.focus_handle(cx).focus(window, cx),
            }
        } else { self.focus.focus(window, cx); }
    }

    fn cycle_fields(&self, backwards: bool, window: &mut Window, cx: &mut App) {
        let current = self.fields.iter().position(|field| match &field.input {
            FieldInput::Single(input) => input.focus_handle(cx).is_focused(window),
            FieldInput::Multi(input) => input.focus_handle(cx).is_focused(window),
        }).or_else(|| self.copy_focus.is_focused(window).then_some(self.fields.len()))
          .or_else(|| self.cancel_focus.is_focused(window).then_some(self.fields.len() + usize::from(self.preview_parent.is_none())));
        let count = self.fields.len() + if self.preview_parent.is_some() { 1 } else { 2 };
        let index = next_focus(current, count, backwards);
        if index == self.fields.len() && self.preview_parent.is_some() { self.cancel_focus.focus(window, cx); return; }
        if index == self.fields.len() { self.copy_focus.focus(window, cx); return; }
        if index == self.fields.len() + 1 { self.cancel_focus.focus(window, cx); return; }
        self.scroll.scroll_to_top_of_item(index);
        match &self.fields[index].input {
            FieldInput::Single(input) => input.focus_handle(cx).focus(window, cx),
            FieldInput::Multi(input) => input.focus_handle(cx).focus(window, cx),
        }
    }

    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_mut() else { return; };
        for field in &self.fields {
            let value = field.value(cx);
            session.set_value(&field.key, value);
        }
        match session.preview() {
            Ok(preview) => {
                self.preview = preview.text;
                self.preview_issues = preview.issues.into_iter().map(|issue| match issue.kind {
                    multi_launcher::snippet_prompt::PromptIssueKind::RequiredValueEmpty => format!("{} is required.", issue.key),
                    multi_launcher::snippet_prompt::PromptIssueKind::MissingValue => format!("{} is missing.", issue.key),
                    _ => "The snippet cannot be rendered.".to_owned(),
                }).collect();
            }
            Err(_) => { self.preview.clear(); self.preview_issues = vec!["The snippet cannot be rendered.".into()]; }
        }
        cx.notify();
    }

    pub(super) fn restart_draft(&self, cx: &App) -> Option<Vec<String>> {
        self.open.then(|| self.fields.iter().map(|field| field.value(cx)).collect())
    }

    pub(super) fn take_session(&mut self, cx: &mut Context<Self>) -> Option<PromptSession> {
        let mut session = self.session.take()?;
        for field in &self.fields {
            let value = field.value(cx);
            session.set_value(&field.key, value);
        }
        self.pending = true;
        self.feedback = None;
        for field in &self.fields {
            match &field.input {
                FieldInput::Single(input) => input.update(cx, |input, cx| input.set_readonly(true, cx)),
                FieldInput::Multi(input) => input.update(cx, |input, cx| input.set_readonly(true, cx)),
            }
        }
        cx.notify();
        Some(session)
    }

    pub(super) fn restore(&mut self, session: PromptSession, error: PromptError, cx: &mut Context<Self>) {
        self.session = Some(session);
        self.pending = false;
        self.feedback = Some(match error {
            PromptError::RequiredValueEmpty | PromptError::MissingValue => "Complete the required fields.",
            PromptError::StaleSnippet => "The saved snippet changed. Close and reopen it.",
            PromptError::Superseded => "The copy request was superseded. Try again.",
            _ => "Could not copy the snippet. Try again.",
        }.into());
        for field in &self.fields {
            match &field.input {
                FieldInput::Single(input) => input.update(cx, |input, cx| input.set_readonly(false, cx)),
                FieldInput::Multi(input) => input.update(cx, |input, cx| input.set_readonly(false, cx)),
            }
        }
        self.refresh_preview(cx);
        cx.notify();
    }

    pub(super) fn retire(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.pending = false;
        self.session = None;
        self.fields.clear();
        self.subscriptions.clear();
        self.preview.clear();
        self.preview_issues.clear();
        self.owner.retire();
        cx.notify();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || !self.open { return; }
        let owns = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        let return_focus = self.return_focus.take();
        let preview_parent = self.preview_parent.take();
        self.retire(cx);
        if let Some((parent, receipt, instance, host)) = preview_parent {
            let _ = parent.update(cx, |parent, cx| parent.return_from_preview(&receipt, instance, host, return_focus, owns, window, cx));
            return;
        }
        let _ = self.root.update(cx, |root, cx| {
            if owns && window.is_window_active() && !window.has_active_dialog(cx)
                && root.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) {
                if let Some(focus) = return_focus { focus.focus(window, cx); }
            }
            cx.notify();
        });
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_parent.is_some() || self.pending || !self.open || !self.mount.is_current() || !window.is_window_active()
            || window.has_active_dialog(cx)
            || !(self.focus.is_focused(window) || self.focus.contains_focused(window, cx)) { return; }
        let Some(root) = self.root.upgrade() else { return; };
        let root_state = root.read(cx);
        let Some(native) = root_state.native_window else { return; };
        let host = window.window_handle();
        let revision = root_state.root_visibility.request_revision.load(Ordering::Acquire);
        let focus = window.focused(cx);
        if root_state.root_window.is_none_or(|current| current.window_id() != host.window_id())
            || !json_utility::native_host_current(&root_state.root_visibility, revision, Some(native), false)
            || current_foreground_identity() != Some(native.identity) { return; }
        let receipt = self.mount.clone();
        let weak = cx.entity().downgrade();
        let root = self.root.clone();
        window.defer(cx, move |window, app| {
            let _ = root.update(app, |root, cx| root.submit_snippet_fill(weak, receipt, host, native, revision, focus, window, cx));
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn single_line_projection_preserves_unchanged_default() {
        assert_eq!(super::restored_value("one\r\ntwo", "one two", "one two"), "one\r\ntwo");
        assert_eq!(super::restored_value("one\r\ntwo", "one two", "edited"), "edited");
    }

    #[test]
    fn focus_cycles_fields_and_both_actions() {
        assert_eq!(super::next_focus(Some(1), 4, false), 2);
        assert_eq!(super::next_focus(Some(3), 4, false), 0);
        assert_eq!(super::next_focus(Some(0), 4, true), 3);
        assert_eq!(super::next_focus(None, 2, true), 1);
    }

    #[test]
    fn deferred_copy_rejects_replaced_or_uninstalled_mount() {
        let owner = super::utility_mount::MountOwner::default();
        let issued = owner.attach();
        assert!(super::issued_mount_is_current(&issued, &issued, true, true, false));
        assert!(!super::issued_mount_is_current(&issued, &issued, false, true, false));
        assert!(!super::issued_mount_is_current(&issued, &issued, true, true, true));
        let replacement = owner.attach();
        assert!(!super::issued_mount_is_current(&issued, &replacement, true, true, false));
        owner.retire();
        assert!(!super::issued_mount_is_current(&replacement, &replacement, true, true, false));
    }
}

impl Render for SnippetFill {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mount = self.mount.clone();
        let key_mount = mount.clone();
        let mut panel = div().id("snippet-fill").track_focus(&self.focus).w_full().flex().flex_col().gap_2().p_3()
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if !key_mount.is_current() || !key_mount.matches(&view.mount) || !view.open
                    || !window.is_window_active() || window.has_active_dialog(cx)
                    || !(view.focus.is_focused(window) || view.focus.contains_focused(window, cx)) { return; }
                let key = event.keystroke.key.as_str();
                let modifiers = event.keystroke.modifiers;
                if key == "escape" && modifiers == Modifiers::default() {
                    window.prevent_default(); cx.stop_propagation();
                    if !event.is_held {
                        let _ = view.root.update(cx, |root, _| root.pane_escape_latched = true);
                        view.cancel(window, cx);
                    }
                } else if key == "tab" && !modifiers.control && !modifiers.alt && !modifiers.platform {
                    window.prevent_default(); cx.stop_propagation();
                    view.cycle_fields(modifiers.shift, window, cx);
                } else if key == "enter" && modifiers.control && !modifiers.alt && !modifiers.platform && !modifiers.shift {
                    window.prevent_default(); cx.stop_propagation();
                    if !event.is_held { view.submit(window, cx); }
                }
            }))
            .child(Label::new(if self.preview_parent.is_some() { "Preview Snippet" } else { "Fill Snippet" }));
        let mut fields_panel = div().id("snippet-fill-fields").flex().flex_col().gap_2().max_h(px(360.0))
            .overflow_y_scroll().track_scroll(&self.scroll);
        for field in &self.fields {
            let label = if field.required { format!("{} *", field.label) } else { field.label.clone() };
            let row = div().flex().flex_col().gap_1().child(Label::new(label.clone()));
            fields_panel = fields_panel.child(match &field.input {
                FieldInput::Single(input) => row.child(Input::new(input).aria_label(label).tab_index(0)),
                FieldInput::Multi(input) => row.child(Textarea::new(input).aria_label(label).tab_index(0)),
            });
        }
        panel = panel.child(fields_panel);
        panel = panel.child(Label::new("Result"))
            .child(div().id("snippet-fill-preview").max_h(px(180.0)).overflow_y_scroll().child(Label::new(self.preview.clone())));
        for issue in &self.preview_issues { panel = panel.child(Label::new(issue.clone())); }
        if let Some(feedback) = &self.feedback { panel = panel.child(Label::new(feedback.clone())); }
        let submit_mount = mount.clone();
        panel.child(div().flex().gap_2()
            .when(self.preview_parent.is_none(), |row| row.child(Button::new("snippet-fill-copy").label("Copy").track_focus(&self.copy_focus).disabled(self.pending)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if submit_mount.is_current() && submit_mount.matches(&view.mount) { view.submit(window, cx); }
                }))))
            .child(Button::new("snippet-fill-cancel").label(if self.preview_parent.is_some() { "Return to Editor" } else { "Cancel" }).track_focus(&self.cancel_focus).disabled(self.pending)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if mount.is_current() && mount.matches(&view.mount) { view.cancel(window, cx); }
                }))))
    }
}
