//! Retained Find controls backed by the shared Notes literal model.
use super::*;
use multi_launcher::plugins::note::find::{NoteFindState, NoteReplacementOutcome};

#[derive(Clone, Copy, PartialEq, Eq)]
enum FindAction { Previous, Next, ReplaceCurrent, ReplaceAll, Close }

#[derive(Clone, PartialEq, Eq)]
struct Receipt {
    epoch: u64,
    presentation: u64,
    content: u64,
    query: u64,
    navigation: u64,
    replacement: String,
}
fn receipt_current(open: bool, issued: &Receipt, current: &Receipt) -> bool { open && issued == current }
fn byte_offset_for_char(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map_or(text.len(), |(offset, _)| offset)
}

pub(super) struct FindOwner {
    pub(super) model: NoteFindState,
    query: Entity<InputState>,
    replacement: Entity<InputState>,
    _query_sub: Subscription,
    _replacement_sub: Subscription,
    controls: [FocusHandle; 5],
    epoch: u64,
}
impl FindOwner {
    pub(super) fn new(window: &mut Window, cx: &mut Context<NoteEditor>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));
        let query_sub = cx.subscribe_in(&query, window, |view: &mut NoteEditor, input: &Entity<InputState>, event: &InputEvent, window: &mut Window, cx: &mut Context<NoteEditor>| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                let nonempty = !query.is_empty();
                if view.find.model.set_query(query) && nonempty && view.find.model.is_open()
                    && input.focus_handle(cx).is_focused(window)
                    && view.find_available(window, cx) && !view.find_composing(window, cx)
                {
                    view.select_current_find(window, cx);
                }
                cx.notify();
            }
        });
        let replacement_sub = cx.subscribe(&replacement, |_: &mut NoteEditor, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) { cx.notify(); }
        });
        Self { model: NoteFindState::default(), query, replacement, _query_sub: query_sub, _replacement_sub: replacement_sub, controls: std::array::from_fn(|_| cx.focus_handle()), epoch: 0 }
    }
}
impl NoteEditor {
    pub(super) fn reset_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find.epoch = self.find.epoch.wrapping_add(1);
        self.find.model.reset_for_new_note();
        self.find.query.update(cx, |input, cx| input.set_value("", window, cx));
        self.find.replacement.update(cx, |input, cx| input.set_value("", window, cx));
        self.body.update(cx, |body, cx| body.set_selection_focus(None, cx));
    }
    pub(super) fn suspend_find(&mut self, cx: &mut Context<Self>) {
        self.find.epoch = self.find.epoch.wrapping_add(1);
        self.find.model.close();
        self.body.update(cx, |body, cx| body.set_selection_focus(None, cx));
    }
    fn find_available(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        matches!(self.mode, NoteViewMode::Edit | NoteViewMode::Split) && !self.pending && !self.switch_pending && self.decision.is_none()
            && self.focus.contains_focused(window, cx) && self.current(window, cx)
    }
    pub(super) fn find_composing(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.body.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || self.find.query.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
            || self.find.replacement.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
    }
    fn select_current_find(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let content = self.body.read(cx).value();
        if let Some(found) = self.find.model.current_match(content.as_str()).cloned() {
            self.body.update(cx, |body, cx| body.set_selected_range(found.byte_range, cx));
        }
    }
    pub(super) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.find_available(window, cx) || self.find_composing(window, cx) { return false; }
        let was_open = self.find.model.is_open();
        self.find.epoch = self.find.epoch.wrapping_add(1);
        self.find.model.open();
        let focus = self.focus.clone();
        self.body.update(cx, |body, cx| body.set_selection_focus(Some(focus), cx));
        if !was_open && !self.find.model.query().is_empty() { self.select_current_find(window, cx); }
        self.find.query.focus_handle(cx).focus(window, cx);
        cx.notify(); true
    }
    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.suspend_find(cx);
        self.body.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    pub(super) fn find_enter(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if event.is_held || event.keystroke.key != "enter" || !self.find.query.focus_handle(cx).is_focused(window)
            || event.keystroke.modifiers.control || event.keystroke.modifiers.alt || event.keystroke.modifiers.platform
            || !self.find.model.is_open() || !self.find_available(window, cx) || self.find_composing(window, cx)
            || self.find.query.read(cx).value().as_str() != self.find.model.query() { return false; }
        self.navigate_find(!event.keystroke.modifiers.shift, cx); true
    }
    fn navigate_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        let content = self.body.read(cx).value();
        let matched = if forward { self.find.model.next(content.as_str()) } else { self.find.model.previous(content.as_str()) }.cloned();
        if let Some(matched) = matched { self.body.update(cx, |body, cx| body.set_selected_range(matched.byte_range, cx)); }
        cx.notify();
    }
    fn find_receipt(&self, cx: &App) -> Receipt {
        Receipt { epoch: self.find.epoch, presentation: self.presentation_epoch, content: self.find.model.content_revision(), query: self.find.model.query_revision(), navigation: self.find.model.navigation_revision(), replacement: self.find.replacement.read(cx).value().to_string() }
    }
    fn settle_find_replacement(&mut self, outcome: &mut NoteReplacementOutcome, cx: &mut Context<Self>) {
        let content = self.body.read(cx).value().to_string();
        if self.find.model.commit_replacement_navigation(&content, outcome).is_ok() {
            let selection = outcome.next_match.as_ref().map(|matched| matched.byte_range.clone()).unwrap_or_else(|| {
                let offset = byte_offset_for_char(&content, outcome.caret_char); offset..offset
            });
            self.body.update(cx, |body, cx| body.set_selected_range(selection, cx));
        }
        cx.notify();
    }
    fn find_action(&mut self, receipt: &Receipt, action: FindAction, window: &mut Window, cx: &mut Context<Self>) {
        if !receipt_current(self.find.model.is_open(), receipt, &self.find_receipt(cx)) || !self.find_available(window, cx)
            || self.find_composing(window, cx) || self.find.query.read(cx).value().as_str() != self.find.model.query() { return; }
        match action {
            FindAction::Close => { self.close_find(window, cx); return; }
            FindAction::Previous | FindAction::Next => { self.navigate_find(action == FindAction::Next, cx); return; }
            _ => {}
        }
        let source = self.body.read(cx).value().to_string();
        let replacement = self.find.replacement.read(cx).value().to_string();
        let range = if action == FindAction::ReplaceCurrent { self.find.model.current_match(&source).map(|found| found.byte_range.clone()) } else { None };
        let prepared = if action == FindAction::ReplaceCurrent { self.find.model.prepare_replace_current(&source, &replacement) } else { self.find.model.prepare_replace_all(&source, &replacement) };
        let Some(prepared) = prepared else { return; };
        let Ok(mut outcome) = self.find.model.apply_prepared_replacement(&source, prepared) else { return; };
        if !outcome.changed { self.settle_find_replacement(&mut outcome, cx); return; }
        self.body.update(cx, |body, cx| {
            body.set_selected_range(range.unwrap_or(0..source.len()), cx);
            body.replace(if action == FindAction::ReplaceCurrent { replacement } else { outcome.content.clone() }, window, cx);
        });
        let weak = cx.entity().downgrade(); let mount = self.mount.clone(); let receipt = receipt.clone();
        window.defer(cx, move |_, app| {
            let _ = weak.update(app, |view, cx| {
                if mount.is_current() && mount.matches(&view.mount) && view.presentation_epoch == receipt.presentation && view.find.epoch == receipt.epoch {
                    view.settle_find_replacement(&mut outcome, cx);
                }
            });
        });
    }
    pub(super) fn find_panel(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.find.model.is_open() { return None; }
        let source = self.body.read(cx).value();
        let count = self.find.model.results(source.as_str()).len();
        let current = self.find.model.current_index(source.as_str()).map_or(0, |index| index + 1);
        let receipt = self.find_receipt(cx);
        let blocked = self.pending || self.switch_pending || self.decision.is_some();
        let mut first = div().flex().gap_2().items_center().child(Input::new(&self.find.query).readonly(blocked).aria_label("Find note text")).child(Label::new(format!("{current} / {count}")));
        let mut second = div().flex().gap_2().items_center().child(Input::new(&self.find.replacement).readonly(blocked).aria_label("Replace note text"));
        for (index, action, label) in [(0, FindAction::Previous, "Previous"), (1, FindAction::Next, "Next"), (2, FindAction::ReplaceCurrent, "Replace Current"), (3, FindAction::ReplaceAll, "Replace All"), (4, FindAction::Close, "Close Find")] {
            let captured = receipt.clone();
            let button = Button::new(SharedString::from(format!("note-find-{index}-{}-{}-{}-{}-{}", receipt.epoch, receipt.presentation, receipt.content, receipt.query, receipt.navigation)))
                .label(label).track_focus(&self.find.controls[index]).tab_stop(true).disabled(blocked)
                .on_click(cx.listener(move |view, _, window, cx| view.find_action(&captured, action, window, cx)));
            if index == 2 || index == 3 { second = second.child(button); } else { first = first.child(button); }
        }
        Some(div().id("note-find").flex().flex_col().gap_2().child(first).child(second).into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{byte_offset_for_char, receipt_current, Receipt};
    #[test]
    fn incremental_query_reselects_current_exact_match() {
        let mut model = super::NoteFindState::default();
        model.set_query("n");
        assert_eq!(model.current_match("noise needle").unwrap().byte_range, 0..1);
        model.set_query("needle");
        assert_eq!(model.current_match("noise needle").unwrap().byte_range, 6..12);
    }

    #[test]
    fn note_find_receipts_and_unicode_crlf_caret_are_exact() {
        let receipt = Receipt { epoch: 1, presentation: 2, content: 3, query: 4, navigation: 5, replacement: "x".into() };
        assert!(receipt_current(true, &receipt, &receipt));
        let mut stale = receipt.clone(); stale.presentation += 1;
        assert!(!receipt_current(true, &receipt, &stale));
        stale = receipt.clone(); stale.navigation += 1;
        assert!(!receipt_current(true, &receipt, &stale));
        assert_eq!(byte_offset_for_char("é\r\n文", 3), 4);
        assert_eq!(byte_offset_for_char("é\r\n文", 99), 7);
    }
}
