use super::calculation_pad_dialog::InspectorPadContext;
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::{
    HostIdentity, HostedCopyCapture, HostedCopyReceipt, UtilityPresentation, ViewportPassProvenance,
};
use crate::calculation_pad::{IntegerMode, IntegerWidth, ProgrammerInspector};
use crate::common::numeric_base::NumericBase;
use eframe::egui;
use std::sync::Arc;

#[derive(Clone)]
struct InspectorDraft {
    model: ProgrammerInspector,
    target_width: IntegerWidth,
    target_signed: bool,
    label_start: String,
    label_end: String,
    label_text: String,
    control_dirty: bool,
    feedback: Option<String>,
    generation: u64,
}
impl Default for InspectorDraft {
    fn default() -> Self {
        Self {
            model: ProgrammerInspector::default(),
            target_width: IntegerWidth::Bits64,
            target_signed: false,
            label_start: String::new(),
            label_end: String::new(),
            label_text: String::new(),
            control_dirty: false,
            feedback: None,
            generation: 0,
        }
    }
}
#[derive(Default)]
pub(super) struct ProgrammerInspectorDialog {
    pub(super) open: bool,
    epoch: u64,
    close_requested: bool,
    draft: InspectorDraft,
    published: Option<Arc<InspectorPadContext>>,
    #[cfg(test)]
    test_controls: Vec<(String, egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_clicked_bit: Option<u32>,
}
pub(super) struct InspectorRuntime {
    pub(super) context: Arc<InspectorPadContext>,
    pub(super) root: ViewportPassProvenance,
    pub(super) host: HostIdentity,
    pub(super) pinned: bool,
}
pub(super) struct InspectorActions {
    pub(super) host_request: Option<UtilityPresentation>,
    pub(super) close: bool,
    pub(super) context: Arc<InspectorPadContext>,
    draft: Option<InspectorDraft>,
    expected_generation: u64,
    pub(super) copy: Option<String>,
    pub(super) insert: bool,
    copies: Vec<String>,
    ctx: egui::Context,
    receipt: Option<HostedCopyReceipt>,
    #[cfg(test)]
    pub(super) host_button: Option<(egui::Rect, egui::Rect, bool)>,
}
impl ProgrammerInspectorDialog {
    pub(super) fn open(&mut self) {
        if !self.open {
            self.epoch = self.epoch.wrapping_add(1).max(1);
        }
        self.open = true;
        self.close_requested = false;
    }
    pub(super) fn request_close(&mut self) {
        if self.open {
            self.close_requested = true;
        }
    }
    pub(super) fn finish_close(&mut self) {
        self.open = false;
        self.close_requested = false;
    }
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }
    #[cfg(test)]
    pub(super) fn test_control(&self, name: &str) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_controls
            .iter()
            .find(|(id, _, _, _)| id == name)
            .map(|(_, rect, clip, enabled)| (*rect, *clip, *enabled))
    }
    #[cfg(test)]
    pub(super) fn test_value(&self) -> crate::calculation_pad::TypedInteger {
        self.draft.model.value()
    }
    #[cfg(test)]
    pub(super) fn test_clicked_bit(&self) -> Option<u32> {
        self.test_clicked_bit
    }
    #[cfg(test)]
    pub(super) fn test_annotation_count(&self) -> usize {
        self.draft.model.annotations().len()
    }
    #[cfg(test)]
    pub(super) fn test_format_options(&self) -> crate::calculation_pad::InspectorFormatOptions {
        self.draft.model.format_options()
    }
    pub(super) fn publish(&mut self, context: Arc<InspectorPadContext>) {
        if self
            .published
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &context))
        {
            return;
        }
        if self.published.as_ref().map(|old| &old.candidate) != Some(&context.candidate) {
            if !self.draft.control_dirty || self.draft.model.has_local_work() {
                self.draft.model.observe(context.candidate.clone());
            }
            if !self.draft.control_dirty {
                let mode = self.draft.model.value().mode();
                self.draft.target_width = mode.width();
                self.draft.target_signed = mode.is_signed();
            }
            self.draft.generation = self.draft.generation.wrapping_add(1);
        }
        self.published = Some(context);
    }
    pub(super) fn commit(
        &mut self,
        mut actions: InspectorActions,
        current: &Arc<InspectorPadContext>,
    ) -> (
        Option<String>,
        Option<crate::calculation_pad::InspectorInsertRequest>,
    ) {
        let mut captured = Vec::new();
        if let Some(receipt) = actions.receipt {
            actions.ctx.with_plugin(|plugin: &mut HostedCopyCapture| {
                plugin.captured.retain(|(candidate, copies)| {
                    let same = candidate.host == receipt.host
                        && candidate.epoch == receipt.epoch
                        && candidate.root == receipt.root
                        && candidate.owner == receipt.owner;
                    if same {
                        captured.extend(copies.iter().cloned());
                    }
                    !same
                });
            });
        }
        if self.draft.generation != actions.expected_generation {
            return (None, None);
        }
        if let Some(draft) = actions.draft.take() {
            self.draft = draft;
        }
        if actions.close
            || actions.host_request.is_some()
            || actions.context.as_ref() != current.as_ref()
        {
            return (None, None);
        }
        let copy = actions
            .copy
            .take()
            .or_else(|| actions.copies.into_iter().chain(captured).last());
        let insert = if actions.insert && self.draft.model.adopted() == current.candidate.as_ref() {
            current
                .caret
                .as_ref()
                .and_then(|caret| self.draft.model.insert_request(caret.range.clone()).ok())
        } else {
            None
        };
        (copy, insert)
    }
    pub(super) fn render_host(
        &mut self,
        ctx: &egui::Context,
        runtime: &InspectorRuntime,
        owner: bool,
        first: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> InspectorActions {
        let mut draft = self.draft.clone();
        let mut changed = false;
        let mut copy = None;
        let mut insert = false;
        let mut request = None;
        let mut window_open = self.open;
        let mut visible = false;
        #[cfg(test)]
        let mut host_button = None;
        #[cfg(test)]
        let mut test_controls = Vec::new();
        #[cfg(test)]
        let mut test_clicked_bit = None;
        let start = ctx.output(|o| o.commands.len());
        let mut body = |ui: &mut egui::Ui| {
            if ctx.viewport_id() == egui::ViewportId::ROOT {
                ui.style_mut().interaction.selectable_labels = false;
            }
            let active =
                owner && first && ui.is_visible() && !ui.is_sizing_pass() && !ctx.will_discard();
            visible |= active;
            let (host, target) = super::utility_host::presentation_button(ui, presentation, owner);
            #[cfg(test)]
            {
                host_button = Some((host.rect, ui.clip_rect(), host.enabled()));
            }
            if active && host.clicked() {
                request = Some(target);
            }
            ui.add_enabled_ui(active, |ui| {
                let model = &mut draft.model;
                ui.horizontal_wrapped(|ui| {
                    ui.label(match model.adopted() {
                        Some(source) => format!("Adopted line ID {}", source.line_id.get()),
                        None => "No integer result selected".into(),
                    });
                    if model.has_local_work() || draft.control_dirty {
                        ui.strong("Modified");
                    }
                    match runtime.context.candidate.as_ref() {
                        Some(source) => {
                            ui.weak(format!(
                                "Selected line ID {} (revision {})",
                                source.line_id.get(),
                                source.revision
                            ));
                        }
                        None => {
                            ui.weak("Current selection unavailable");
                        }
                    }
                    if model.adopted() != runtime.context.candidate.as_ref() {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            "Source changed or unavailable",
                        );
                    }
                    let refresh = ui.button("Refresh from selected result");
                    #[cfg(test)]
                    test_controls.push((
                        "refresh".into(),
                        refresh.rect,
                        ui.clip_rect(),
                        refresh.enabled(),
                    ));
                    if refresh.clicked() {
                        model.observe(runtime.context.candidate.clone());
                        match model.refresh() {
                            Ok(()) => {
                                draft.control_dirty = !draft.label_start.is_empty()
                                    || !draft.label_end.is_empty()
                                    || !draft.label_text.is_empty();
                                draft.target_width = model.value().mode().width();
                                draft.target_signed = model.value().mode().is_signed();
                                changed = true;
                            }
                            Err(error) => {
                                draft.feedback = Some(format!("{error:?}"));
                                changed = true;
                            }
                        }
                    }
                });
                let ready = model.adopted().is_some()
                    && model.adopted() == runtime.context.candidate.as_ref()
                    && runtime.context.caret.is_some()
                    && runtime.context.editable;
                let insert_button =
                    ui.add_enabled(ready, egui::Button::new("Insert at Pad selection"));
                #[cfg(test)]
                test_controls.push((
                    "insert".into(),
                    insert_button.rect,
                    ui.clip_rect(),
                    insert_button.enabled(),
                ));
                if insert_button.clicked() {
                    insert = true;
                }
                if !ready {
                    ui.weak("Select an integer result and a current Pad insertion range.");
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Mode:");
                    for width in IntegerWidth::ALL {
                        let response = ui.selectable_value(
                            &mut draft.target_width,
                            width,
                            width.bits().to_string(),
                        );
                        #[cfg(test)]
                        test_controls.push((
                            format!("width-{}", width.bits()),
                            response.rect,
                            ui.clip_rect(),
                            response.enabled(),
                        ));
                        if response.changed() {
                            changed = true;
                            draft.control_dirty = true;
                        }
                    }
                    if ui
                        .selectable_value(&mut draft.target_signed, false, "Unsigned")
                        .changed()
                    {
                        changed = true;
                        draft.control_dirty = true;
                    }
                    let signed = ui.selectable_value(&mut draft.target_signed, true, "Signed");
                    #[cfg(test)]
                    test_controls.push((
                        "signed".into(),
                        signed.rect,
                        ui.clip_rect(),
                        signed.enabled(),
                    ));
                    if signed.changed() {
                        changed = true;
                        draft.control_dirty = true;
                    }
                    let mode = if draft.target_signed {
                        IntegerMode::signed(draft.target_width)
                    } else {
                        IntegerMode::unsigned(draft.target_width)
                    };
                    let cast = ui.button("Checked cast");
                    #[cfg(test)]
                    test_controls.push((
                        "checked-cast".into(),
                        cast.rect,
                        ui.clip_rect(),
                        cast.enabled(),
                    ));
                    if cast.clicked() {
                        match model.checked_cast(mode) {
                            Ok(()) => changed = true,
                            Err(error) => {
                                draft.feedback = Some(format!("{error:?}"));
                                changed = true;
                            }
                        }
                    }
                    if ui.button("Reinterpret bits").clicked() {
                        model.reinterpret_bits(mode);
                        changed = true;
                    }
                });
                ui.label(format!(
                    "Working mode: {}",
                    model.value().mode().type_name()
                ));
                egui::ScrollArea::vertical()
                    .id_salt("inspector-bits")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        let columns = ((ui.available_width() / 70.0).floor() as usize).clamp(2, 8);
                        egui::Grid::new("inspector-bit-grid")
                            .num_columns(columns)
                            .show(ui, |ui| {
                                let width = model.value().mode().width().bits();
                                for (position, index) in (0..width).rev().enumerate() {
                                    let bit = model.bit(index).unwrap_or(false);
                                    let bit_response = ui.selectable_label(
                                        bit,
                                        format!("{index}: {}", u8::from(bit)),
                                    );
                                    #[cfg(test)]
                                    test_controls.push((
                                        format!("bit-{index}"),
                                        bit_response.rect,
                                        ui.clip_rect(),
                                        bit_response.enabled(),
                                    ));
                                    if bit_response.clicked() {
                                        #[cfg(test)]
                                        {
                                            test_clicked_bit = Some(index);
                                        }
                                        let _ = model.toggle_bit(index);
                                        changed = true;
                                    }
                                    if (position + 1) % columns == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("NOT").clicked() {
                        model.bit_not();
                        changed = true;
                    }
                    if ui.button("Shift left").clicked() {
                        if model.shift_left(1).is_ok() {
                            changed = true;
                        }
                    }
                    if ui.button("Shift right").clicked() {
                        if model.shift_right(1).is_ok() {
                            changed = true;
                        }
                    }
                    if ui.button("Rotate left").clicked() {
                        if model.rotate_left(1).is_ok() {
                            changed = true;
                        }
                    }
                    if ui.button("Rotate right").clicked() {
                        if model.rotate_right(1).is_ok() {
                            changed = true;
                        }
                    }
                    ui.label(format!("Set bits: {}", model.popcount()));
                });
                let mut options = model.format_options();
                ui.horizontal_wrapped(|ui| {
                    let prefix = ui.checkbox(&mut options.prefix, "Prefix");
                    #[cfg(test)]
                    test_controls.push((
                        "prefix".into(),
                        prefix.rect,
                        ui.clip_rect(),
                        prefix.enabled(),
                    ));
                    if prefix.changed() {
                        changed = true;
                    }
                    if ui
                        .checkbox(&mut options.pad_to_width, "Pad to width")
                        .changed()
                    {
                        changed = true;
                    }
                    ui.label("Group digits:");
                    if ui
                        .add(egui::DragValue::new(&mut options.grouping).range(0..=128))
                        .changed()
                    {
                        changed = true;
                    }
                    if ui.button("Plain digits").clicked() {
                        options = Default::default();
                        changed = true;
                    }
                });
                let _ = model.set_format_options(options);
                for (title, base) in [
                    ("Binary", NumericBase::Binary),
                    ("Octal", NumericBase::Octal),
                    ("Decimal", NumericBase::Decimal),
                    ("Hexadecimal", NumericBase::Hexadecimal),
                ] {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(title);
                        let text = model.format(base);
                        let _value = ui.monospace(&text);
                        #[cfg(test)]
                        test_controls.push((
                            format!("value-{title}"),
                            _value.rect,
                            ui.clip_rect(),
                            _value.enabled(),
                        ));
                        let response = ui.small_button("Copy");
                        #[cfg(test)]
                        test_controls.push((
                            format!("copy-{title}"),
                            response.rect,
                            ui.clip_rect(),
                            response.enabled(),
                        ));
                        if response.clicked() {
                            copy = Some(text);
                        }
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bit range [start, end):");
                    let label_start = ui.add(
                        egui::TextEdit::singleline(&mut draft.label_start).desired_width(42.0),
                    );
                    #[cfg(test)]
                    test_controls.push((
                        "label-start".into(),
                        label_start.rect,
                        ui.clip_rect(),
                        label_start.enabled(),
                    ));
                    if label_start.changed() {
                        changed = true;
                        draft.control_dirty = true;
                    }
                    let label_end = ui
                        .add(egui::TextEdit::singleline(&mut draft.label_end).desired_width(42.0));
                    #[cfg(test)]
                    test_controls.push((
                        "label-end".into(),
                        label_end.rect,
                        ui.clip_rect(),
                        label_end.enabled(),
                    ));
                    if label_end.changed() {
                        changed = true;
                        draft.control_dirty = true;
                    }
                    let label_text = ui.add(
                        egui::TextEdit::singleline(&mut draft.label_text).desired_width(120.0),
                    );
                    #[cfg(test)]
                    test_controls.push((
                        "label-text".into(),
                        label_text.rect,
                        ui.clip_rect(),
                        label_text.enabled(),
                    ));
                    if label_text.changed() {
                        changed = true;
                        draft.control_dirty = true;
                    }
                    let add_label = ui.button("Add label");
                    #[cfg(test)]
                    test_controls.push((
                        "add-label".into(),
                        add_label.rect,
                        ui.clip_rect(),
                        add_label.enabled(),
                    ));
                    if add_label.clicked() {
                        match (
                            draft.label_start.parse::<u32>(),
                            draft.label_end.parse::<u32>(),
                        ) {
                            (Ok(a), Ok(b)) => {
                                match model.set_annotation(a..b, draft.label_text.clone()) {
                                    Ok(()) => changed = true,
                                    Err(error) => {
                                        draft.feedback = Some(format!("{error:?}"));
                                        changed = true;
                                    }
                                }
                            }
                            _ => {
                                draft.feedback = Some("Enter a valid bit range.".into());
                                changed = true;
                            }
                        }
                    }
                });
                for annotation in model.annotations().to_vec() {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{}..{}: {}{}",
                            annotation.range.start,
                            annotation.range.end,
                            annotation.label,
                            if model.annotation_is_valid(&annotation) {
                                ""
                            } else {
                                " (outside current width)"
                            }
                        ));
                        if ui.small_button("Remove").clicked() {
                            model.remove_annotation(annotation.range);
                            changed = true;
                        }
                    });
                }
                if let Some(message) = &draft.feedback {
                    ui.label(message);
                }
            });
        };
        if let Some(ui) = child_ui {
            body(ui);
        } else {
            egui::Window::new("Programmer Inspector")
                .id(egui::Id::new("programmer-inspector-window"))
                .default_size([720.0, 650.0])
                .min_size([360.0, 280.0])
                .show_utility(
                    ctx,
                    "programmer-inspector",
                    egui::Id::new("programmer-inspector-window"),
                    Some(&mut window_open),
                    super::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        super::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::vec2(
                            720.0, 650.0,
                        )),
                        egui::vec2(360.0, 280.0),
                        [None, None],
                    ),
                    |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| body(ui));
                    },
                );
        }
        let mut copies = Vec::new();
        ctx.output_mut(|output| {
            let tail = output.commands.split_off(start.min(output.commands.len()));
            for command in tail {
                match command {
                    egui::OutputCommand::CopyText(text) => copies.push(text),
                    other => output.commands.push(other),
                }
            }
        });
        let receipt = if ctx.viewport_id() != egui::ViewportId::ROOT {
            ctx.add_plugin(HostedCopyCapture::default());
            let receipt = HostedCopyReceipt {
                root: runtime.root,
                owner: ViewportPassProvenance::current(ctx, ctx.viewport_id()),
                host: runtime.host,
                epoch: self.epoch,
                eligible: first && owner && visible && !ctx.will_discard(),
            };
            ctx.with_plugin(|plugin: &mut HostedCopyCapture| {
                plugin
                    .captured
                    .retain(|(old, _)| old.owner.viewport_id != ctx.viewport_id());
                plugin.armed.insert(ctx.viewport_id(), receipt);
            });
            Some(receipt)
        } else {
            None
        };
        if changed {
            draft.generation = draft.generation.wrapping_add(1);
        }
        #[cfg(test)]
        {
            self.test_controls = test_controls;
            self.test_clicked_bit = test_clicked_bit;
        }
        InspectorActions {
            host_request: request,
            close: !runtime.pinned && (!window_open || self.close_requested),
            context: Arc::clone(&runtime.context),
            draft: changed.then_some(draft),
            expected_generation: self.draft.generation,
            copy,
            insert,
            copies,
            ctx: ctx.clone(),
            receipt,
            #[cfg(test)]
            host_button,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculation_pad::CalculationPadSession;
    use crate::platform::app_data::AppDataRoot;

    #[test]
    fn programmer_inspector_retains_private_draft_across_new_context_and_move() {
        let temp = tempfile::tempdir().unwrap();
        let mut session =
            CalculationPadSession::new(&AppDataRoot::from_path(temp.path().join("data")));
        session.replace_source("u8(1)").unwrap();
        let line = session.line_id(0).unwrap();
        session.evaluation();
        let source = session.inspector_source(line).unwrap();
        let old = Arc::new(InspectorPadContext {
            caret: Some(super::super::calculation_pad_dialog::InspectorCaret {
                incarnation: source.incarnation,
                document: source.document_id,
                revision: source.revision,
                range: 0..5,
                generation: 1,
            }),
            candidate: Some(source),
            selection_generation: 1,
            editable: true,
        });
        let mut dialog = ProgrammerInspectorDialog::default();
        dialog.open();
        dialog.publish(Arc::clone(&old));
        let mut draft = dialog.draft.clone();
        draft.model.toggle_bit(7).unwrap();
        draft.label_text = "unfinished".into();
        draft.control_dirty = true;
        draft.generation += 1;
        let actions = InspectorActions {
            host_request: None,
            close: false,
            context: Arc::clone(&old),
            expected_generation: dialog.draft.generation,
            draft: Some(draft),
            copy: Some("stale".into()),
            insert: true,
            copies: vec![],
            ctx: egui::Context::default(),
            receipt: None,
            host_button: None,
        };
        session.replace_source("u8(4)").unwrap();
        session.evaluation();
        let fresh = Arc::new(InspectorPadContext {
            candidate: session.inspector_source(line),
            caret: None,
            selection_generation: 1,
            editable: true,
        });
        assert_eq!(dialog.commit(actions, &fresh), (None, None));
        dialog.publish(Arc::clone(&fresh));
        assert_eq!(dialog.draft.model.value().bit_pattern(), 129);
        assert_eq!(dialog.draft.model.adopted(), old.candidate.as_ref());
        assert_eq!(dialog.draft.model.candidate(), fresh.candidate.as_ref());
        assert_eq!(dialog.draft.label_text, "unfinished");

        let mut moved = dialog.draft.clone();
        moved.label_text = "retained across Dock".into();
        moved.generation += 1;
        let move_action = InspectorActions {
            host_request: Some(UtilityPresentation::Docked),
            close: false,
            context: Arc::clone(&fresh),
            expected_generation: dialog.draft.generation,
            draft: Some(moved),
            copy: Some("suppressed".into()),
            insert: true,
            copies: Vec::new(),
            ctx: egui::Context::default(),
            receipt: None,
            host_button: None,
        };
        assert_eq!(dialog.commit(move_action, &fresh), (None, None));
        assert_eq!(dialog.draft.label_text, "retained across Dock");

        let mut clean = ProgrammerInspectorDialog::default();
        clean.open();
        clean.publish(Arc::clone(&old));
        clean.draft.label_text = "pending label".into();
        clean.draft.control_dirty = true;
        clean.publish(fresh);
        assert_eq!(clean.draft.model.adopted(), old.candidate.as_ref());
        assert_eq!(clean.draft.label_text, "pending label");
    }
}
