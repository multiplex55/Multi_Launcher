//! Hash Generator view composition; the entity owns state and handlers.
use super::*;
use gpui_kit::component::ActiveTheme as _;

#[derive(Debug, PartialEq, Eq)]
enum ComparisonFeedback {
    Match,
    Mismatch,
    Invalid(String),
}

fn comparison_feedback(comparison: Option<Result<HashComparison, String>>) -> Option<ComparisonFeedback> {
    match comparison? {
        Ok(HashComparison::NotRequested) => None,
        Ok(HashComparison::Match) => Some(ComparisonFeedback::Match),
        Ok(HashComparison::Mismatch) => Some(ComparisonFeedback::Mismatch),
        Err(error) => Some(ComparisonFeedback::Invalid(error)),
    }
}

#[inline(never)]
fn render_hash_digest(
    digest: String,
    mono_family: SharedString,
    mono_size: Pixels,
    palette: multi_launcher::gui::theme::SemanticPalette,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .child(
            Label::new("Digest")
                .text_size(px(metrics.small_text_size))
                .line_height(px(metrics.line_height)),
        )
        .child(
            Label::new(digest)
                .id("hash-digest-result")
                .w_full()
                .min_w(px(0.0))
                .whitespace_normal()
                .font_family(mono_family)
                .text_size(mono_size)
                .line_height(px(metrics.line_height))
                .text_color(appearance::to_hsla(palette.success)),
        )
        .into_any_element()
}

impl HashGeneratorWindow {
    #[inline(never)]
    fn render_source_row(
        &self,
        jump_render: &hash_jump::HashJumpRender<'_>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let metrics = jump_render.metrics;
        let eligibility = jump_render.eligibility;
        let selector_width = px(metrics.body_text_size * 10.0);
        let source = jump_render.wrap(
            Select::new(&self.source)
                .accessibility_label("Hash input source")
                .placeholder("Choose source")
                .disabled(!eligibility.allows(hash_jump::HashControl::Source)),
            hash_jump::HashControl::Source,
        );
        let load = jump_render.wrap(
            Button::new("load-hash-clipboard")
                .max_w_full()
                .min_w(px(0.0))
                .track_focus(&self.jump.clipboard)
                .label("Load Clipboard")
                .tab_stop(true)
                .disabled(!eligibility.allows(hash_jump::HashControl::Clipboard))
                .on_click(cx.listener(|this, _, _, cx| this.load_clipboard(cx))),
            hash_jump::HashControl::Clipboard,
        );
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(metrics.item_spacing_x))
            .gap_y(px(metrics.item_spacing_y))
            .child(
                div()
                    .w(selector_width)
                    .flex_auto()
                    .min_w(px(0.0))
                    .max_w_full()
                    .child(source),
            )
            .child(div().max_w_full().min_w(px(0.0)).child(load))
            .into_any_element()
    }

    #[inline(never)]
    fn render_calculation_row(
        &self,
        jump_render: &hash_jump::HashJumpRender<'_>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let metrics = jump_render.metrics;
        let eligibility = jump_render.eligibility;
        let selector_width = px(metrics.body_text_size * 10.0);
        let algorithm_control = jump_render.wrap(
            Select::new(&self.algorithm)
                .accessibility_label("Hash algorithm")
                .disabled(!eligibility.allows(hash_jump::HashControl::Algorithm)),
            hash_jump::HashControl::Algorithm,
        );
        let calculate = jump_render.wrap(
            Button::new("calculate-hash")
                .max_w_full()
                .min_w(px(0.0))
                .track_focus(&self.jump.calculate)
                .label("Calculate")
                .tab_stop(true)
                .disabled(!eligibility.allows(hash_jump::HashControl::Calculate))
                .on_click(cx.listener(|this, _, _, cx| this.calculate_hash(cx))),
            hash_jump::HashControl::Calculate,
        );
        let cancel_enabled = eligibility.allows(hash_jump::HashControl::Cancel);
        let copy_enabled = eligibility.allows(hash_jump::HashControl::Copy);
        let cancel = {
            jump_render.wrap(
                Button::new("cancel-hash")
                    .max_w_full()
                    .min_w(px(0.0))
                    .track_focus(&self.jump.cancel)
                    .label("Cancel")
                    .tab_stop(cancel_enabled)
                    .disabled(!cancel_enabled)
                    .when(cancel_enabled, |button| button.on_click(cx.listener(|this, _, _, cx| this.cancel_hash(cx)))),
                hash_jump::HashControl::Cancel,
            )
        };
        let copy = {
            jump_render.wrap(
                Button::new("copy-hash-digest")
                    .max_w_full()
                    .min_w(px(0.0))
                    .track_focus(&self.jump.copy)
                    .label("Copy Digest")
                    .tab_stop(copy_enabled)
                    .disabled(!copy_enabled)
                    .when(copy_enabled, |button| button.on_click(cx.listener(|this, _, _, cx| this.copy_digest(cx)))),
                hash_jump::HashControl::Copy,
            )
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(metrics.item_spacing_x))
            .gap_y(px(metrics.item_spacing_y))
            .child(
                div()
                    .w(selector_width)
                    .flex_auto()
                    .min_w(px(0.0))
                    .max_w_full()
                    .child(algorithm_control),
            )
            .child(div().max_w_full().min_w(px(0.0)).child(calculate))
            .child(div().max_w_full().min_w(px(0.0)).child(cancel))
            .child(div().max_w_full().min_w(px(0.0)).child(copy))
            .into_any_element()
    }

    #[inline(never)]
    fn render_host_row(
        &self,
        jump_render: &hash_jump::HashJumpRender<'_>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let metrics = jump_render.metrics;
        let eligibility = jump_render.eligibility;
        let dock = jump_render.wrap(
            Button::new("dock-hash-generator")
                .max_w_full()
                .min_w(px(0.0))
                .track_focus(&self.jump.dock)
                .label("Dock (Ctrl+Shift+D)")
                .tab_stop(true)
                .disabled(!eligibility.allows(hash_jump::HashControl::Dock))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.request_rehost(HashPresentationMode::Docked, cx)
                })),
            hash_jump::HashControl::Dock,
        );
        let popout = jump_render.wrap(
            Button::new("popout-hash-generator")
                .max_w_full()
                .min_w(px(0.0))
                .track_focus(&self.jump.popout)
                .label("Pop Out (Ctrl+Shift+P)")
                .tab_stop(true)
                .disabled(!eligibility.allows(hash_jump::HashControl::Popout))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.request_rehost(HashPresentationMode::Popout, cx)
                })),
            hash_jump::HashControl::Popout,
        );
        let close = jump_render.wrap(
            Button::new("close-hash-generator")
                .max_w_full()
                .min_w(px(0.0))
                .track_focus(&self.jump.close)
                .label("Close Hash Generator")
                .tab_stop(true)
                .disabled(!eligibility.allows(hash_jump::HashControl::Close))
                .on_click(cx.listener(|this, _, _, cx| this.request_close(super::hash_pin::CloseOrigin::Button, cx))),
            hash_jump::HashControl::Close,
        );
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(metrics.item_spacing_x))
            .gap_y(px(metrics.item_spacing_y))
            .child(div().max_w_full().min_w(px(0.0)).child(dock))
            .child(div().max_w_full().min_w(px(0.0)).child(popout))
            .child(div().max_w_full().min_w(px(0.0)).child(close))
            .into_any_element()
    }
}

impl Render for HashGeneratorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let resolved = appearance::current(cx);
        let metrics = resolved.metrics;
        let palette = resolved.palette;
        let mono_family = cx.theme().mono_font_family.clone();
        let mono_size = cx.theme().mono_font_size;
        let (
            has_job,
            is_running,
            is_stopping,
            progress,
            digest,
            error,
            comparison,
            mode,
            algorithm,
            path,
            clipboard_byte_len,
            clipboard_feedback,
            can_copy_digest,
        ) = {
            let session = self
                .session
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                session.has_job(),
                session.is_running(),
                session.is_stopping_stale_job(),
                session.progress(),
                session
                    .result()
                    .map(|result| result.digest.clone()),
                session.error().map(str::to_owned),
                session.comparison(),
                session.mode(),
                session.algorithm(),
                session.file_path().map(|path| path.display().to_string()),
                session.clipboard_byte_len(),
                session.clipboard_feedback().map(str::to_owned),
                session.can_copy_digest(),
            )
        };
        let can_interact = self.is_current_mount();
        let file_picker_pending = self.file_picker_pending.load(Ordering::Acquire);
        let clipboard_pending_kind = self.clipboard_pending.kind();
        let clipboard_copy_pending = clipboard_pending_kind == HashClipboardPendingKind::Copy;
        let clipboard_pending = clipboard_pending_kind != HashClipboardPendingKind::Idle;
        let eligibility = hash_jump::HashEligibility {
            current: can_interact,
            mode,
            presentation: self.mode,
            has_job,
            is_running,
            picker_pending: file_picker_pending,
            clipboard_pending,
            clipboard_copy_pending,
            can_copy_digest: can_copy_digest && digest.is_some(),
        };
        let clipboard_pending_status = hash_clipboard_pending_feedback(clipboard_pending_kind);
        let picker_feedback = if file_picker_pending {
            Some("Choose a file to hash…".to_owned())
        } else {
            self.file_picker_feedback.clone()
        };
        let presentation_feedback = self
            .presentation_feedback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let comparison_label = comparison_feedback(comparison);
        if let Some(scope) = self.jump.scope {
            let scope_current = match scope {
                hash_jump::HashJumpScope::Main => self.hash_jump_owns_input(window, cx),
                hash_jump::HashJumpScope::Popup(kind, epoch) => self
                    .hash_popup_scope(window, cx)
                    .is_some_and(|(current, opening, _)| current == kind && opening == epoch),
            };
            if !can_interact || !scope_current {
                self.jump.retire();
            }
        }
        let generation = self.mount_generation;
        let jump_draft = self
            .jump
            .controller
            .enabled
            .then(|| Rc::new(RefCell::new(JumpGeometryDraft::new(generation))));
        let jump_frame = Rc::clone(&self.jump.frame);
        let expected_generation = self.jump.controller.session_generation();
        let expected_ids = Rc::new(self.jump.controller.target_ids().to_vec());
        let alphabet = self.jump.controller.alphabet.clone();
        let jump_render = hash_jump::HashJumpRender {
            jump: &self.jump,
            generation,
            draft: jump_draft.clone(),
            eligibility,
            palette,
            metrics,
        };

        let source_row = self.render_source_row(&jump_render, cx);
        let calculation_row = self.render_calculation_row(&jump_render, cx);
        let host_row = self.render_host_row(&jump_render, cx);

        div()
            .flex()
            .flex_col()
            .size_full()
            .p(px(metrics.window_padding))
            .track_focus(&self.focus_handle)
            .key_context("HashGeneratorWindow")
            .when(self.jump.controller.is_active(), |this| {
                this.capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                    if this.jump.retire() {
                        cx.notify();
                    }
                }))
            })
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default()
                    && ((this.hash_popup_scope(window, cx).is_some() && this.jump.controller.owns_key(0x1b))
                        || this.handle_escape(window, cx))
                {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &gpui_kit::base::input::Escape, window, cx| {
                if this.handle_escape(window, cx) {
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .capture_action(cx.listener(|this, confirm: &gpui_kit::base::actions::Confirm, window, cx| {
                // The stock closed Select maps Ctrl+Enter to secondary Confirm
                // before the containing Hash action can receive its shortcut.
                let contexts = window.context_stack();
                if confirm.secondary
                    && context_stack_has_primary(&contexts, "Select")
                    && !context_stack_has_primary(&contexts, "List")
                    && this.focus_handle.contains_focused(window, cx)
                    && this.is_current_mount()
                {
                    this.calculate_hash(cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &CalculateHash, _, cx| {
                this.calculate_hash(cx);
            }))
            .on_action(cx.listener(|this, _: &DockHashGenerator, _, cx| {
                this.request_rehost(HashPresentationMode::Docked, cx);
            }))
            .on_action(cx.listener(|this, _: &PopoutHashGenerator, _, cx| {
                this.request_rehost(HashPresentationMode::Popout, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseHashGenerator, _, cx| {
                this.request_close(super::hash_pin::CloseOrigin::Keyboard, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .h_auto()
                    .min_h_full()
                    .gap_y(px(metrics.item_spacing_y))
                    .w_full()
                    .p(px(metrics.panel_padding))
                    .bg(appearance::to_hsla(palette.panel_surface))
                    .rounded(px(metrics.corner_radius))
                    .text_color(appearance::to_hsla(palette.primary_text))
                    .text_size(px(metrics.body_text_size))
                    .line_height(px(metrics.line_height))
                    .child(source_row)
                    .when(mode == HashInputMode::Text, |this| {
                        this.child(
                            jump_render.wrap(Textarea::new(&self.text)
                                .font_family(mono_family.clone())
                                .text_size(mono_size)
                                .aria_label("Text to hash")
                                .tab_index(0)
                                .readonly(!eligibility.allows(hash_jump::HashControl::Text)), hash_jump::HashControl::Text),
                        )
                    })
                    .when(mode == HashInputMode::Clipboard, |this| {
                        let snapshot_status = clipboard_byte_len.map_or_else(
                            || "No Clipboard snapshot loaded".to_owned(),
                            |bytes| format!("Loaded Clipboard snapshot · {bytes} bytes"),
                        );
                        this.child(
                            Label::new(snapshot_status)
                                .id("hash-clipboard-snapshot-status")
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height)),
                        )
                    })
                    .when(mode == HashInputMode::File, |this| {
                        let selected_path = path
                            .clone()
                            .unwrap_or_else(|| "No file selected".to_owned());
                        this.child(
                            Label::new(format!("File: {selected_path}"))
                                .w_full()
                                .min_w(px(0.0))
                                .whitespace_normal()
                                .font_family(mono_family.clone())
                                .text_size(mono_size)
                                .line_height(px(metrics.line_height)),
                        )
                            .child(
                                jump_render.wrap(Button::new("choose-hash-file").track_focus(&self.jump.file)
                                    .label("Choose File")
                                    .tab_stop(true)
                                    .disabled(!eligibility.allows(hash_jump::HashControl::File))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_file(window, cx);
                                    })), hash_jump::HashControl::File),
                            )
                    })
                    .child(Label::new("Expected digest (optional)")
                        .text_size(px(metrics.small_text_size))
                        .line_height(px(metrics.line_height)))
                    .child(
                        jump_render.wrap(Textarea::new(&self.expected)
                            .font_family(mono_family.clone())
                            .text_size(mono_size)
                            .aria_label("Expected digest")
                            .tab_index(0)
                            .readonly(!eligibility.allows(hash_jump::HashControl::Expected)), hash_jump::HashControl::Expected),
                    )
                    .child(calculation_row)
                    .children(
                        algorithm.security_notice().map(|notice| {
                            Label::new(notice)
                                .id("hash-algorithm-warning")
                                .text_color(appearance::to_hsla(palette.warning))
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height))
                        }),
                    )
                    .children(progress.map(|progress| hash_progress::render(progress, metrics)))
                    .children(is_stopping.then(|| {
                        Label::new("Stopping the previous calculation…")
                            .id("hash-stopping-status")
                            .text_color(appearance::to_hsla(palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(digest.map(|digest| {
                        render_hash_digest(digest, mono_family.clone(), mono_size, palette, metrics)
                    }))
                    .children(comparison_label.map(|feedback| {
                        let (label, color, role) = match feedback {
                            ComparisonFeedback::Match => ("Match".to_owned(), palette.success, Role::Status),
                            ComparisonFeedback::Mismatch => ("Mismatch".to_owned(), palette.error, Role::Status),
                            ComparisonFeedback::Invalid(error) => (format!("Invalid expected digest: {error}"), palette.error, Role::Alert),
                        };
                        div()
                            .id("hash-comparison-status")
                            .role(role)
                            .aria_label(label.clone())
                            .text_color(appearance::to_hsla(color))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                            .child(label)
                    }))
                    .children(error.map(|error| {
                        div()
                            .id("hash-error-status")
                            .role(Role::Alert)
                            .aria_label(error.clone())
                            .text_color(appearance::to_hsla(palette.error))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                            .child(error)
                    }))
                    .children(picker_feedback.map(|feedback| {
                        Label::new(feedback)
                            .id("hash-picker-status")
                            .text_color(appearance::to_hsla(palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(clipboard_pending_status.map(|feedback| {
                        div()
                            .id("hash-clipboard-operation-status")
                            .role(Role::Status)
                            .aria_label(feedback)
                            .text_color(appearance::to_hsla(palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                            .child(feedback)
                    }))
                    .children(clipboard_feedback.filter(|_| !clipboard_pending).map(|feedback| {
                        div()
                            .id("hash-clipboard-feedback-status")
                            .role(Role::Status)
                            .aria_label(feedback.clone())
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                            .child(feedback)
                    }))
                    .children(
                        presentation_feedback.map(|feedback| {
                            Label::new(feedback).id("hash-presentation-status")
                                .text_size(px(metrics.small_text_size))
                                .line_height(px(metrics.line_height))
                        }),
                    )
                    .children(self.native_geometry_error.as_ref().map(|error| {
                        Label::new(error.clone())
                            .id("hash-native-geometry-status")
                            .text_color(appearance::to_hsla(palette.warning))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .children(self.size_write_error.as_ref().map(|error| {
                        Label::new(error.clone())
                            .id("hash-size-write-status")
                            .text_color(appearance::to_hsla(palette.error))
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height))
                    }))
                    .child(host_row)
                    .child(
                        Label::new("Tab moves through controls · Space or Enter opens a selector")
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height)),
                    )
                    .child(
                        Label::new("Ctrl+Enter: calculate · Ctrl+Shift+D: dock · Ctrl+Shift+P: pop out · Ctrl+W: close")
                            .text_size(px(metrics.small_text_size))
                            .line_height(px(metrics.line_height)),
                    ),
            )
            .children(jump_draft.map(|draft| jump_geometry_publisher(
                draft, jump_frame, cx.entity().downgrade(), expected_generation, expected_ids, alphabet, metrics,
                |owner, frame, window, cx, geometry_changed| {
                    if matches!(owner.jump.scope, Some(hash_jump::HashJumpScope::Popup(..))) { return; }
                    let retired = if owner.is_current_mount() && owner.hash_jump_owns_input(window, cx) {
                        owner.jump.reconcile_frame(Some(frame))
                    } else { owner.jump.retire() };
                    if retired || (geometry_changed && owner.jump.controller.is_active() && frame.matches_active_session) { cx.notify(); }
                },
            )))
            .overflow_y_scrollbar()
            .id(format!(
                "hash-generator-tool-scroll-{}",
                self.mount_generation
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::{comparison_feedback, ComparisonFeedback};
    use multi_launcher::hash_generator::HashComparison;

    #[test]
    fn comparison_feedback_matches_stock_wording_and_semantics() {
        assert_eq!(comparison_feedback(None), None);
        assert_eq!(comparison_feedback(Some(Ok(HashComparison::NotRequested))), None);
        assert_eq!(comparison_feedback(Some(Ok(HashComparison::Match))), Some(ComparisonFeedback::Match));
        assert_eq!(comparison_feedback(Some(Ok(HashComparison::Mismatch))), Some(ComparisonFeedback::Mismatch));
        assert_eq!(comparison_feedback(Some(Err("bad length".into()))), Some(ComparisonFeedback::Invalid("bad length".into())));
    }
}
