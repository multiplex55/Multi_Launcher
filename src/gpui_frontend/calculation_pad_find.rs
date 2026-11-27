//! Retained Find and Replace controls for the Calculation Pad.
use super::*;

#[derive(Clone)]
struct Receipt {
    basis: EvaluationBasis,
    mount: utility_mount::MountReceipt,
    visibility_revision: u64,
    epoch: u64,
    replace_mode: bool,
    query: String,
    case_sensitive: bool,
    selected: Option<usize>,
    replacement: String,
}

fn mode_current(
    open: bool,
    replace_mode: bool,
    epoch: u64,
    issued_mode: bool,
    issued_epoch: u64,
    needs_replace: bool,
) -> bool {
    open && epoch == issued_epoch && replace_mode == issued_mode && (!needs_replace || replace_mode)
}

fn basis_current(session: Option<&CalculationPadSession>, basis: EvaluationBasis) -> bool {
    session.is_some_and(|session| {
        session.pending_transition().is_none() && session.evaluation_basis() == Some(basis)
    })
}

fn replacement_for(
    model: &mut calculation_pad::FindReplace,
    source: &str,
    all: bool,
) -> Option<String> {
    if !all && model.current.is_none() {
        model.navigate(true);
    }
    model.replacement(source, all)
}

pub(super) struct FindOwner {
    model: calculation_pad::FindReplace,
    query: Entity<InputState>,
    replacement: Entity<InputState>,
    _query_subscription: Subscription,
    _replacement_subscription: Subscription,
    basis: Option<EvaluationBasis>,
    open: bool,
    replace: bool,
    epoch: u64,
}

impl FindOwner {
    pub(super) fn new(window: &mut Window, cx: &mut Context<CalculationPadView>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));
        let query_subscription = cx.subscribe(
            &query,
            |_: &mut CalculationPadView, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        );
        let replacement_subscription = cx.subscribe(
            &replacement,
            |_: &mut CalculationPadView, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        );
        Self {
            model: Default::default(),
            query,
            replacement,
            _query_subscription: query_subscription,
            _replacement_subscription: replacement_subscription,
            basis: None,
            open: false,
            replace: false,
            epoch: 0,
        }
    }
}

impl CalculationPadView {
    pub(super) fn reset_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owned_focus = self.find.query.focus_handle(cx).is_focused(window)
            || self.find.replacement.focus_handle(cx).is_focused(window);
        self.find.open = false;
        self.find.replace = false;
        self.find.epoch = self.find.epoch.wrapping_add(1);
        self.find.basis = None;
        self.find.model = calculation_pad::FindReplace::default();
        self.find
            .query
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.find
            .replacement
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.editor
            .update(cx, |editor, cx| editor.set_selection_focus(None, cx));
        if owned_focus && self.open && window.is_window_active() {
            self.editor.focus_handle(cx).focus(window, cx);
        }
    }

    pub(super) fn reset_find_if_document_changed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session
            .as_ref()
            .and_then(|session| session.evaluation_basis());
        if self.find.basis.is_some_and(|old| {
            current.is_none_or(|new| {
                old.document != new.document || old.incarnation != new.incarnation
            })
        }) {
            self.reset_find(window, cx);
        }
    }

    pub(super) fn open_find(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus.contains_focused(window, cx)
            || !self.current(window, cx)
            || self
                .session
                .as_ref()
                .is_none_or(|session| session.pending_transition().is_some())
        {
            return false;
        }
        self.find.epoch = self.find.epoch.wrapping_add(1);
        self.find.open = true;
        self.find.replace = replace;
        let focus = self.focus.clone();
        self.editor
            .update(cx, |editor, cx| editor.set_selection_focus(Some(focus), cx));
        self.find.query.focus_handle(cx).focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.find.open
            || !self.focus.contains_focused(window, cx)
            || !self.current(window, cx)
            || self
                .session
                .as_ref()
                .is_none_or(|session| session.pending_transition().is_some())
        {
            return false;
        }
        self.find.open = false;
        self.find.epoch = self.find.epoch.wrapping_add(1);
        self.editor
            .update(cx, |editor, cx| editor.set_selection_focus(None, cx));
        self.editor.focus_handle(cx).focus(window, cx);
        cx.notify();
        true
    }

    fn find_receipt_current(
        &self,
        receipt: &Receipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        mode_current(
            self.find.open,
            self.find.replace,
            self.find.epoch,
            receipt.replace_mode,
            receipt.epoch,
            false,
        ) && receipt.mount.is_current()
            && receipt.mount.matches(&self.mount)
            && self.visibility.request_revision.load(Ordering::Acquire)
                == receipt.visibility_revision
            && self.focus.contains_focused(window, cx)
            && self.current(window, cx)
            && basis_current(self.session.as_ref(), receipt.basis)
            && self.find.basis == Some(receipt.basis)
            && self.find.query.read(cx).value().as_ref() == receipt.query.as_str()
            && self.find.model.case_sensitive == receipt.case_sensitive
            && self.find.model.current == receipt.selected
            && self.find.replacement.read(cx).value().as_ref() == receipt.replacement.as_str()
    }

    fn find_action(
        &mut self,
        receipt: &Receipt,
        action: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.find_receipt_current(receipt, window, cx)
            || !mode_current(
                self.find.open,
                self.find.replace,
                self.find.epoch,
                receipt.replace_mode,
                receipt.epoch,
                action >= 2,
            )
        {
            return;
        }
        if action < 2 {
            if let Some(range) = self.find.model.navigate(action == 1) {
                self.editor
                    .update(cx, |editor, cx| editor.set_selected_range(range, cx));
            }
        } else {
            self.find.model.replacement = self.find.replacement.read(cx).value().to_string();
            let replacement = self
                .session
                .as_ref()
                .and_then(|session| session.source())
                .and_then(|source| replacement_for(&mut self.find.model, source, action == 3));
            if let Some(source) = replacement {
                self.revoke_copy();
                if let Some(session) = self.session.as_mut() {
                    match session.replace_source(source) {
                        Ok(_) => {
                            self.message = None;
                            self.set_editor_from_session(window, cx);
                        }
                        Err(error) => {
                            self.message = Some(format!("Could not replace text: {error}"))
                        }
                    }
                }
            } else {
                self.message = Some("No replacement could be applied.".into());
            }
        }
        cx.notify();
    }

    pub(super) fn find_panel(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.find.open {
            return None;
        }
        let session = self.session.as_ref()?;
        let basis = session.evaluation_basis()?;
        let pending = session.pending_transition().is_some();
        if self.find.basis.is_none_or(|old| {
            old.document != basis.document || old.incarnation != basis.incarnation
        }) {
            let case_sensitive = self.find.model.case_sensitive;
            self.find.model = calculation_pad::FindReplace::default();
            self.find.model.case_sensitive = case_sensitive;
        }
        self.find.basis = Some(basis);
        self.find.model.query = self.find.query.read(cx).value().to_string();
        self.find.model.refresh(session.source()?, basis.revision);
        let palette = appearance::current(cx).palette;
        let receipt = Receipt {
            basis,
            mount: self.mount.clone(),
            visibility_revision: self.visibility.request_revision.load(Ordering::Acquire),
            epoch: self.find.epoch,
            replace_mode: self.find.replace,
            query: self.find.model.query.clone(),
            case_sensitive: self.find.model.case_sensitive,
            selected: self.find.model.current,
            replacement: self.find.replacement.read(cx).value().to_string(),
        };
        let mut panel = div()
            .id("calculation-pad-find")
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Input::new(&self.find.query)
                    .aria_label("Find")
                    .readonly(pending),
            )
            .child(
                Checkbox::new("calculation-pad-find-case")
                    .accessibility_label("Match case")
                    .checked(self.find.model.case_sensitive)
                    .disabled(pending)
                    .border_1()
                    .border_color(appearance::to_hsla(palette.panel_surface))
                    .focus(|style| style.border_color(appearance::to_hsla(palette.focus)))
                    .on_change({
                        let receipt = receipt.clone();
                        let weak = cx.entity().downgrade();
                        move |state, _, window, app| {
                            let _ = weak.update(app, |view, cx| {
                                if view.find_receipt_current(&receipt, window, cx) {
                                    view.find.model.case_sensitive =
                                        state == CheckboxState::Checked;
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                CheckboxIndicator::new()
                                    .checked(self.find.model.case_sensitive)
                                    .size(px(18.0))
                                    .rounded(px(3.0))
                                    .border_1()
                                    .border_color(appearance::to_hsla(palette.control_border))
                                    .when(self.find.model.case_sensitive, |indicator| {
                                        indicator
                                            .bg(appearance::to_hsla(palette.accent))
                                            .text_color(appearance::to_hsla(palette.primary_text))
                                    })
                                    .items_center()
                                    .justify_center()
                                    .child(if self.find.model.case_sensitive {
                                        "✓"
                                    } else {
                                        ""
                                    }),
                            )
                            .child(Label::new("Match case")),
                    ),
            )
            .child(Label::new(format!(
                "{}/{}",
                self.find.model.current.map_or(0, |index| index + 1),
                self.find.model.matches.len()
            )));
        if self.find.replace {
            panel = panel.child(
                Input::new(&self.find.replacement)
                    .aria_label("Replace")
                    .readonly(pending),
            );
        }
        let mut controls = div().flex().flex_wrap().gap_2();
        for (label, action) in [
            ("Previous", 0),
            ("Next", 1),
            ("Replace", 2),
            ("Replace All", 3),
        ] {
            if action >= 2 && !self.find.replace {
                continue;
            }
            let receipt = receipt.clone();
            controls = controls.child(
                Button::new(SharedString::from(format!("pad-find-{action}")))
                    .label(label)
                    .disabled(pending)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.find_action(&receipt, action, window, cx)
                    })),
            );
        }
        let close_receipt = receipt.clone();
        controls = controls.child(
            Button::new("pad-find-close")
                .label("Close")
                .disabled(pending)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if view.find_receipt_current(&close_receipt, window, cx) {
                        view.close_find(window, cx);
                    }
                })),
        );
        Some(panel.child(controls).into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{basis_current, mode_current, replacement_for};
    use multi_launcher::{
        calculation_pad::{CalculationPadSession, FindReplace},
        platform::app_data::AppDataRoot,
    };

    #[test]
    fn stale_find_mode_and_reopened_receipts_are_denied() {
        assert!(mode_current(true, true, 7, true, 7, true));
        assert!(!mode_current(true, false, 8, true, 7, true));
        assert!(!mode_current(false, true, 8, true, 7, true));
        assert!(!mode_current(true, true, 9, true, 7, true));
        assert!(!mode_current(true, false, 7, false, 7, true));
    }

    #[test]
    fn reopened_document_rejects_same_revision_receipt_and_replace_is_one_undo() {
        let dir = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let mut session = CalculationPadSession::new(&root);
        session.replace_source("雪A\r\n雪a\r\n").unwrap();
        session.save_as("find test").unwrap();
        let id = session.current_id().unwrap();
        session.request_open(id).unwrap();
        let old = session.evaluation_basis().unwrap();
        session.request_open(id).unwrap();
        let current = session.evaluation_basis().unwrap();
        assert_eq!(old.document, current.document);
        assert_eq!(old.revision, current.revision);
        assert_ne!(old.incarnation, current.incarnation);
        assert!(!basis_current(Some(&session), old));
        assert!(basis_current(Some(&session), current));
        let mut find = FindReplace::default();
        find.query = "雪a".into();
        find.replacement = "λ".into();
        find.refresh(session.source().unwrap(), current.revision);
        let replaced = replacement_for(&mut find, session.source().unwrap(), true).unwrap();
        assert_eq!(replaced, "λ\r\nλ\r\n");
        session.replace_source(replaced).unwrap();
        assert!(session.undo());
        assert_eq!(session.source(), Some("雪A\r\n雪a\r\n"));
    }
}
