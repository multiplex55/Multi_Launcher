use super::calculation_pad_dialog::InspectorPadContext;
use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::{
    HostIdentity, HostedCopyCapture, HostedCopyReceipt, UtilityPresentation, ViewportPassProvenance,
};
use crate::calculation_pad::{
    ComparisonRequest, ComparisonResult, ComparisonSlot, IntegerMode, IntegerWidth,
    ProgrammerInspector,
};
use crate::common::numeric_base::NumericBase;
use eframe::egui;
use std::sync::Arc;

#[derive(Clone)]
struct InspectorDraft {
    model: ProgrammerInspector,
    target_width: IntegerWidth,
    target_signed: bool,
    bit_grouping: u32,
    label_start: String,
    label_end: String,
    label_text: String,
    control_dirty: bool,
    feedback: Option<String>,
    generation: u64,
    comparison_left: ComparisonSlot,
    comparison_right: ComparisonSlot,
    comparison_result: Option<Arc<ComparisonResult>>,
    comparison_selection_generation: Option<u64>,
    formatting_open: bool,
    labels_open: bool,
    comparison_open: bool,
}
impl Default for InspectorDraft {
    fn default() -> Self {
        Self {
            model: ProgrammerInspector::default(),
            target_width: IntegerWidth::Bits64,
            target_signed: false,
            bit_grouping: 4,
            label_start: String::new(),
            label_end: String::new(),
            label_text: String::new(),
            control_dirty: false,
            feedback: None,
            generation: 0,
            comparison_left: ComparisonSlot::Expression(String::new()),
            comparison_right: ComparisonSlot::Expression(String::new()),
            comparison_result: None,
            comparison_selection_generation: None,
            formatting_open: false,
            labels_open: false,
            comparison_open: true,
        }
    }
}

impl InspectorDraft {
    fn comparison_request(&self) -> ComparisonRequest {
        ComparisonRequest {
            left: self.comparison_left.clone(),
            right: self.comparison_right.clone(),
        }
    }

    fn target_mode(&self) -> IntegerMode {
        if self.target_signed {
            IntegerMode::signed(self.target_width)
        } else {
            IntegerMode::unsigned(self.target_width)
        }
    }

    fn select_mode(&mut self, mode: IntegerMode) {
        self.target_width = mode.width();
        self.target_signed = mode.is_signed();
        self.control_dirty = true;
        match self.model.checked_cast(mode) {
            Ok(()) => {
                self.bit_grouping = self.bit_grouping.min(mode.width().bits());
                self.feedback = None;
            }
            Err(error) => {
                self.feedback = Some(format!(
                    "Cannot convert to {}: {error:?}. Working value and mode are unchanged; use Reinterpret bits to apply the attempted mode.",
                    mode.type_name()
                ));
            }
        }
    }

    fn reinterpret_target(&mut self) {
        let mode = self.target_mode();
        self.model.reinterpret_bits(mode);
        self.bit_grouping = self.bit_grouping.min(mode.width().bits());
        self.feedback = None;
        self.control_dirty = true;
    }
}

fn comparison_input_text(
    input: &ComparisonSlot,
    context: Option<&crate::calculation_pad::ComparisonContext>,
) -> String {
    match input {
        ComparisonSlot::Expression(expression) => expression.clone(),
        ComparisonSlot::Result(selected) => context
            .filter(|context| {
                context.incarnation == selected.incarnation
                    && context.document_id == selected.document_id
            })
            .and_then(|context| {
                context.rows.iter().position(|row| row.id == selected.line_id)
            })
            .map_or_else(
                || "Unavailable selected result".to_owned(),
                |index| format!("r{}", index + 1),
            ),
    }
}

fn bit_rows(width: u32, columns: usize, grouping: u32) -> Vec<Vec<u32>> {
    let indices = (0..width).rev().collect::<Vec<_>>();
    let columns = columns.max(1);
    let grouping = grouping.max(1) as usize;
    let row_size = if grouping <= columns {
        (columns / grouping) * grouping
    } else {
        columns
    };
    indices.chunks(row_size).map(<[u32]>::to_vec).collect()
}

fn bit_columns(available: f32, cell: f32, spacing: f32, group_gap: f32, grouping: u32) -> usize {
    let group = grouping.max(1) as usize;
    (1_usize..=16)
        .take_while(|&count| {
            count as f32 * cell
                + count.saturating_sub(1) as f32 * spacing
                + (count.saturating_sub(1) / group) as f32 * group_gap
                <= available
        })
        .last()
        .unwrap_or(1)
}

fn section_toggle(ui: &mut egui::Ui, title: &str, open: &mut bool) -> egui::Response {
    let response = ui.button(format!("{} {title}", if *open { "▾" } else { "▸" }));
    if response.clicked() {
        *open = !*open;
    }
    response
}

fn annotation_fragment(
    row: &[(u32, egui::Rect)],
    range: &std::ops::Range<u32>,
) -> Option<egui::Rangef> {
    let mut cells = row.iter().filter(|(index, _)| range.contains(index));
    let (_, first) = cells.next()?;
    let mut extent = egui::Rangef::new(first.left(), first.right());
    for (_, rect) in cells {
        extent.min = extent.min.min(rect.left());
        extent.max = extent.max.max(rect.right());
    }
    Some(extent)
}

fn annotation_label(annotation: &crate::calculation_pad::InspectorAnnotation) -> String {
    format!(
        "{} [{}:{}]",
        annotation.label,
        annotation.range.end.saturating_sub(1),
        annotation.range.start
    )
}
#[derive(Default)]
pub(crate) struct ProgrammerInspectorDialog {
    pub(super) open: bool,
    epoch: u64,
    close_requested: bool,
    draft: InspectorDraft,
    published: Option<Arc<InspectorPadContext>>,
    preference_error: Option<String>,
    #[cfg(test)]
    test_controls: Vec<(String, egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_clicked_bit: Option<u32>,
    #[cfg(test)]
    test_preference_requests: usize,
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
    compare: bool,
    preferences: Option<crate::settings::ProgrammerInspectorPreferences>,
    copies: Vec<String>,
    ctx: egui::Context,
    receipt: Option<HostedCopyReceipt>,
    #[cfg(test)]
    pub(super) host_button: Option<(egui::Rect, egui::Rect, bool)>,
}
impl ProgrammerInspectorDialog {
    pub(crate) fn initialize_preferences(
        &mut self,
        preferences: crate::settings::ProgrammerInspectorPreferences,
    ) {
        let _ =
            self.draft
                .model
                .set_format_options(crate::calculation_pad::InspectorFormatOptions {
                    prefix: preferences.prefix,
                    pad_to_width: preferences.pad_to_width,
                    grouping: preferences.grouping.min(128),
                });
    }
    pub(crate) fn preference_saved(
        &mut self,
        previous: crate::settings::ProgrammerInspectorPreferences,
        committed: crate::settings::ProgrammerInspectorPreferences,
        preserve_local: bool,
    ) {
        let current = self.draft.model.format_options();
        if !preserve_local
            && current.prefix == previous.prefix
            && current.pad_to_width == previous.pad_to_width
            && current.grouping == previous.grouping
        {
            self.initialize_preferences(committed);
        }
        let current = self.draft.model.format_options();
        if current.prefix == committed.prefix
            && current.pad_to_width == committed.pad_to_width
            && current.grouping == committed.grouping
        {
            self.preference_error = None;
        }
    }
    pub(crate) fn preference_save_failed(&mut self, message: &str) {
        self.preference_error = Some(format!(
            "Preferences were not saved: {message}. Use Retry preferences to try again."
        ));
    }
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
    pub(crate) fn test_format_options(&self) -> crate::calculation_pad::InspectorFormatOptions {
        self.draft.model.format_options()
    }
    #[cfg(test)]
    pub(super) fn test_comparison(&self) -> Option<&ComparisonResult> {
        self.draft.comparison_result.as_deref()
    }
    #[cfg(test)]
    pub(super) fn test_comparison_input(&self, left: bool) -> String {
        let input = if left {
            &self.draft.comparison_left
        } else {
            &self.draft.comparison_right
        };
        comparison_input_text(
            input,
            self.published.as_ref().and_then(|context| context.comparison.as_deref()),
        )
    }
    #[cfg(test)]
    pub(super) fn test_section_open(&self, name: &str) -> bool {
        match name {
            "section-formatting" => self.draft.formatting_open,
            "section-labels" => self.draft.labels_open,
            "section-comparison" => self.draft.comparison_open,
            _ => panic!("unknown Inspector section"),
        }
    }
    #[cfg(test)]
    pub(super) fn test_preference_requests(&self) -> usize {
        self.test_preference_requests
    }
    #[cfg(test)]
    pub(super) fn test_preference_error(&self) -> Option<&str> {
        self.preference_error.as_deref()
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
                self.draft.bit_grouping = self.draft.bit_grouping.min(mode.width().bits());
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
        Option<crate::settings::ProgrammerInspectorPreferences>,
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
            return (None, None, None);
        }
        if let Some(draft) = actions.draft.take() {
            self.draft = draft;
        }
        if actions.close {
            return (None, None, None);
        }
        let preferences = actions.preferences;
        #[cfg(test)]
        if preferences.is_some() {
            self.test_preference_requests += 1;
        }
        if actions.host_request.is_some() {
            return (None, None, preferences);
        }
        if actions.context.as_ref() != current.as_ref() {
            return (None, None, preferences);
        }
        if actions.compare
            && let Some(context) = current.comparison.as_ref()
        {
            let request = self.draft.comparison_request();
            self.draft.comparison_result = Some(Arc::new(context.compare(request)));
            self.draft.comparison_selection_generation = Some(current.selection_generation);
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
        (copy, insert, preferences)
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
        let mut compare = false;
        let mut preferences_changed = false;
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
            let interactive = owner && ui.is_visible() && !ui.is_sizing_pass();
            let active = interactive && first && !ctx.will_discard();
            visible |= active;
            let (host, target) = super::utility_host::presentation_button(ui, presentation, owner);
            #[cfg(test)]
            {
                host_button = Some((host.rect, ui.clip_rect(), host.enabled()));
            }
            if active && host.clicked() {
                request = Some(target);
            }
            ui.add_enabled_ui(interactive, |ui| {
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
                                draft.bit_grouping =
                                    draft.bit_grouping.min(model.value().mode().width().bits());
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
                let active_mode = model.value().mode();
                let mut requested_mode = None;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Mode:");
                    let pending = draft.target_mode() != active_mode;
                    for width in IntegerWidth::ALL {
                        let label = if pending && draft.target_width == width {
                            format!("{}*", width.bits())
                        } else {
                            width.bits().to_string()
                        };
                        let response = ui.selectable_label(active_mode.width() == width, label);
                        #[cfg(test)]
                        test_controls.push((
                            format!("width-{}", width.bits()),
                            response.rect,
                            ui.clip_rect(),
                            response.enabled(),
                        ));
                        if response.clicked() {
                            requested_mode = Some(if draft.target_signed {
                                IntegerMode::signed(width)
                            } else {
                                IntegerMode::unsigned(width)
                            });
                        }
                    }
                    for signedness in [false, true] {
                        let label = if signedness { "Signed" } else { "Unsigned" };
                        let label = if pending && draft.target_signed == signedness {
                            format!("{label}*")
                        } else {
                            label.to_owned()
                        };
                        let response =
                            ui.selectable_label(active_mode.is_signed() == signedness, label);
                        #[cfg(test)]
                        test_controls.push((
                            if signedness { "signed" } else { "unsigned" }.into(),
                            response.rect,
                            ui.clip_rect(),
                            response.enabled(),
                        ));
                        if response.clicked() {
                            requested_mode = Some(if signedness {
                                IntegerMode::signed(draft.target_width)
                            } else {
                                IntegerMode::unsigned(draft.target_width)
                            });
                        }
                    }
                });
                if let Some(mode) = requested_mode
                    && (mode != active_mode || mode != draft.target_mode())
                {
                    draft.select_mode(mode);
                    changed = true;
                }
                let active_mode = draft.model.value().mode();
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Working mode: {}", active_mode.type_name()));
                    ui.monospace(format!("Value: {}", draft.model.value().formatted_decimal()));
                    let pending = draft.target_mode() != active_mode;
                    if pending {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!(
                                "Pending target: {} (not applied)",
                                draft.target_mode().type_name()
                            ),
                        );
                    }
                    let reinterpret =
                        ui.add_enabled(pending, egui::Button::new("Reinterpret bits"));
                    #[cfg(test)]
                    test_controls.push((
                        "reinterpret".into(),
                        reinterpret.rect,
                        ui.clip_rect(),
                        reinterpret.enabled(),
                    ));
                    if reinterpret.clicked() {
                        draft.reinterpret_target();
                        changed = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bit grouping:");
                    let width = draft.model.value().mode().width().bits();
                    for group in [1, 2, 4, 8, 16, 32, 64, 128]
                        .into_iter()
                        .filter(|group| *group <= width)
                    {
                        let response =
                            ui.selectable_label(draft.bit_grouping == group, group.to_string());
                        #[cfg(test)]
                        test_controls.push((
                            format!("bit-group-{group}"),
                            response.rect,
                            ui.clip_rect(),
                            response.enabled(),
                        ));
                        if response.clicked() && draft.bit_grouping != group {
                            draft.bit_grouping = group;
                            changed = true;
                        }
                    }
                });
                let model = &mut draft.model;
                egui::ScrollArea::vertical()
                    .id_salt("inspector-bits")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        const GROUP_GAP: f32 = 8.0;
                        const CELL_GAP: f32 = 1.0;
                        let available = ui.available_width().max(1.0);
                        let font = egui::TextStyle::Monospace.resolve(ui.style());
                        let glyph_width = ui
                            .painter()
                            .layout_no_wrap("0".into(), font.clone(), ui.visuals().text_color())
                            .size()
                            .x;
                        let cell_width = (glyph_width + 2.0 * ui.spacing().button_padding.x)
                            .max(20.0)
                            .min(available);
                        let columns = bit_columns(
                            available,
                            cell_width,
                            CELL_GAP,
                            GROUP_GAP,
                            draft.bit_grouping,
                        );
                        let width = model.value().mode().width().bits();
                        for row in bit_rows(width, columns, draft.bit_grouping) {
                            let mut cells = Vec::with_capacity(row.len());
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = GROUP_GAP;
                                let mut start = 0;
                                while start < row.len() {
                                    let mut end = start + 1;
                                    while end < row.len() && (row[end] + 1) % draft.bit_grouping != 0 {
                                        end += 1;
                                    }
                                    let group = &row[start..end];
                                    let group_width = group.len() as f32 * cell_width
                                        + group.len().saturating_sub(1) as f32 * CELL_GAP;
                                    ui.vertical(|ui| {
                                        let first = group[0];
                                        let last = group[group.len() - 1];
                                        let label = if first == last {
                                            first.to_string()
                                        } else {
                                            format!("{first}–{last}")
                                        };
                                        let label_font = egui::FontId::monospace((font.size * 0.65).max(8.0));
                                        ui.add_sized(
                                            [group_width, label_font.size + 2.0],
                                            egui::Label::new(egui::RichText::new(label).font(label_font))
                                                .truncate(),
                                        );
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = CELL_GAP;
                                            for &index in group {
                                                let bit = model.bit(index).unwrap_or(false);
                                                let response = ui
                                                    .push_id(("inspector-bit", index), |ui| {
                                                        ui.add_sized(
                                                            [
                                                                cell_width,
                                                                font.size + 2.0 * ui.spacing().button_padding.y,
                                                            ],
                                                            egui::Button::selectable(
                                                                bit,
                                                                egui::RichText::new(u8::from(bit).to_string())
                                                                    .font(font.clone()),
                                                            ),
                                                        )
                                                    })
                                                    .inner
                                                    .on_hover_text(format!("Bit {index}"));
                                                #[cfg(test)]
                                                test_controls.push((
                                                    format!("bit-{index}"),
                                                    response.rect,
                                                    ui.clip_rect(),
                                                    response.enabled(),
                                                ));
                                                cells.push((index, response.rect));
                                                if response.clicked() {
                                                    #[cfg(test)]
                                                    {
                                                        test_clicked_bit = Some(index);
                                                    }
                                                    let _ = model.toggle_bit(index);
                                                    changed = true;
                                                }
                                            }
                                        });
                                    });
                                    start = end;
                                }
                            });
                            for (lane, annotation) in model.annotations().iter().enumerate() {
                                if !model.annotation_is_valid(annotation) {
                                    continue;
                                }
                                let Some(span) = annotation_fragment(&cells, &annotation.range) else {
                                    continue;
                                };
                                let color = [
                                    egui::Color32::from_rgb(80, 165, 245),
                                    egui::Color32::from_rgb(235, 150, 70),
                                    egui::Color32::from_rgb(120, 195, 130),
                                    egui::Color32::from_rgb(195, 135, 235),
                                ][lane % 4];
                                let galley = ui.painter().layout(
                                    annotation_label(annotation),
                                    egui::FontId::proportional(10.0),
                                    color,
                                    available,
                                );
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(available, galley.size().y + 5.0),
                                    egui::Sense::hover(),
                                );
                                let y = rect.top() + 2.0;
                                let painter = ui.painter();
                                painter.line_segment(
                                    [egui::pos2(span.min, y), egui::pos2(span.max, y)],
                                    egui::Stroke::new(1.5, color),
                                );
                                painter.line_segment(
                                    [egui::pos2(span.min, y), egui::pos2(span.min, y + 3.0)],
                                    egui::Stroke::new(1.5, color),
                                );
                                painter.line_segment(
                                    [egui::pos2(span.max, y), egui::pos2(span.max, y + 3.0)],
                                    egui::Stroke::new(1.5, color),
                                );
                                let label_x = span
                                    .min
                                    .min((rect.right() - galley.size().x).max(rect.left()));
                                #[cfg(test)]
                                test_controls.push((
                                    "annotation-label".into(),
                                    egui::Rect::from_min_size(egui::pos2(label_x, y + 3.0), galley.size()),
                                    rect,
                                    true,
                                ));
                                painter.galley(egui::pos2(label_x, y + 3.0), galley, color);
                            }
                        }
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
                let formatting = section_toggle(ui, "Formatting", &mut draft.formatting_open);
                #[cfg(test)]
                test_controls.push((
                    "section-formatting".into(),
                    formatting.rect,
                    ui.clip_rect(),
                    formatting.enabled(),
                ));
                if formatting.clicked() {
                    changed = true;
                }
                if draft.formatting_open {
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
                            preferences_changed = true;
                        }
                        if ui
                            .checkbox(&mut options.pad_to_width, "Pad to width")
                            .changed()
                        {
                            changed = true;
                            preferences_changed = true;
                        }
                        ui.label("Group digits:");
                        if ui
                            .add(egui::DragValue::new(&mut options.grouping).range(0..=128))
                            .changed()
                        {
                            changed = true;
                            preferences_changed = true;
                        }
                        if ui.button("Plain digits").clicked() {
                            options = Default::default();
                            changed = true;
                            preferences_changed = true;
                        }
                    });
                    let _ = model.set_format_options(options);
                    if self.preference_error.is_some() && ui.button("Retry preferences").clicked() {
                        preferences_changed = true;
                    }
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
                }
                let labels = section_toggle(ui, "Bit labels", &mut draft.labels_open);
                #[cfg(test)]
                test_controls.push((
                    "section-labels".into(),
                    labels.rect,
                    ui.clip_rect(),
                    labels.enabled(),
                ));
                if labels.clicked() {
                    changed = true;
                }
                if draft.labels_open {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Bit range [start, end):");
                        let label_start =
                            ui.add(egui::TextEdit::singleline(&mut draft.label_start).desired_width(42.0));
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
                        let label_end =
                            ui.add(egui::TextEdit::singleline(&mut draft.label_end).desired_width(42.0));
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
                        let label_text =
                            ui.add(egui::TextEdit::singleline(&mut draft.label_text).desired_width(120.0));
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
                                (Ok(a), Ok(b)) => match model.set_annotation(a..b, draft.label_text.clone()) {
                                    Ok(()) => changed = true,
                                    Err(error) => {
                                        draft.feedback = Some(format!("{error:?}"));
                                        changed = true;
                                    }
                                },
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
                                "{}{}",
                                annotation_label(&annotation),
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
                }
                ui.separator();
                let comparison = section_toggle(ui, "Compare two values", &mut draft.comparison_open);
                #[cfg(test)]
                test_controls.push((
                    "section-comparison".into(),
                    comparison.rect,
                    ui.clip_rect(),
                    comparison.enabled(),
                ));
                if comparison.clicked() {
                    changed = true;
                }
                if draft.comparison_open {
                    for (title, input) in [
                        ("Left", &mut draft.comparison_left),
                        ("Right", &mut draft.comparison_right),
                    ] {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(title);
                            let mut expression =
                                comparison_input_text(input, runtime.context.comparison.as_deref());
                            let field = ui.text_edit_singleline(&mut expression);
                            #[cfg(test)]
                            test_controls.push((
                                format!("comparison-{title}"),
                                field.rect,
                                ui.clip_rect(),
                                field.enabled(),
                            ));
                            if field.changed() {
                                if expression.len() > crate::calculation_pad::MAX_EXPRESSION_BYTES {
                                    let mut end = crate::calculation_pad::MAX_EXPRESSION_BYTES;
                                    while !expression.is_char_boundary(end) {
                                        end -= 1;
                                    }
                                    expression.truncate(end);
                                    draft.feedback = Some("Comparison input was limited to 2048 bytes.".into());
                                }
                                *input = ComparisonSlot::Expression(expression);
                                changed = true;
                            }
                            let select = ui.add_enabled(
                                runtime.context.selected_line.is_some() && runtime.context.comparison.is_some(),
                                egui::Button::new("Use selected result"),
                            );
                            #[cfg(test)]
                            test_controls.push((
                                format!("select-{title}"),
                                select.rect,
                                ui.clip_rect(),
                                select.enabled(),
                            ));
                            if select.clicked() {
                                if let Some(selected) = runtime
                                    .context
                                    .selected_line
                                    .zip(runtime.context.comparison.as_ref())
                                    .map(
                                        |(line_id, context)| crate::calculation_pad::ComparisonResultRef {
                                            incarnation: context.incarnation,
                                            document_id: context.document_id,
                                            line_id,
                                        },
                                    )
                                {
                                    *input = ComparisonSlot::Result(selected);
                                    changed = true;
                                }
                            }
                            if matches!(input, ComparisonSlot::Result(_)) {
                                ui.weak("Selected result");
                            }
                        });
                    }
                    let comparison_button = ui.add_enabled(
                        runtime.context.comparison.is_some(),
                        egui::Button::new("Compare / Refresh"),
                    );
                    #[cfg(test)]
                    test_controls.push((
                        "compare".into(),
                        comparison_button.rect,
                        ui.clip_rect(),
                        comparison_button.enabled(),
                    ));
                    if comparison_button.clicked() {
                        compare = true;
                    }
                    if let Some(result) = &draft.comparison_result {
                        let current_request = draft.comparison_request();
                        let current_context = runtime.context.comparison.as_ref().is_some_and(|context| {
                            context.incarnation == result.context.incarnation
                                && context.document_id == result.context.document_id
                                && context.revision == result.context.revision
                        });
                        if !current_context
                            || result.request != current_request
                            || draft.comparison_selection_generation
                                != Some(runtime.context.selection_generation)
                        {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                "Comparison is stale; refresh to use the current Pad and inputs.",
                            );
                        }
                        for (name, side) in [("Left", &result.left), ("Right", &result.right)] {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!("{name} ({})", side.type_label()));
                                let formatted = side.formatted();
                                ui.monospace(&formatted);
                                if side
                                    .value
                                    .as_ref()
                                    .is_ok_and(crate::calculation_pad::CalculationValue::is_approximate)
                                {
                                    ui.weak("approx.");
                                }
                                let response = ui.small_button("Copy result");
                                #[cfg(test)]
                                test_controls.push((
                                    format!("comparison-copy-{name}"),
                                    response.rect,
                                    ui.clip_rect(),
                                    response.enabled(),
                                ));
                                if response.clicked() {
                                    copy = Some(formatted);
                                }
                            });
                            let representation = match &side.value {
                                Ok(crate::calculation_pad::CalculationValue::Integer(value)) => {
                                    Some((value.bit_pattern(), value.formatted_decimal()))
                                }
                                Ok(crate::calculation_pad::CalculationValue::ExactBase { value, .. }) => {
                                    Some((*value, value.to_string()))
                                }
                                _ => None,
                            };
                            if let Some((bits, decimal)) = representation {
                                ui.monospace(format!(
                                    "dec {decimal}  bin {}  oct {}  hex {}",
                                    crate::common::numeric_base::format_unsigned(bits, NumericBase::Binary),
                                    crate::common::numeric_base::format_unsigned(bits, NumericBase::Octal),
                                    crate::common::numeric_base::format_unsigned(
                                        bits,
                                        NumericBase::Hexadecimal
                                    )
                                ));
                            }
                        }
                        ui.label(result.relation.description());
                    }
                }

                if let Some(message) = &draft.feedback {
                    ui.label(message);
                }
                if let Some(message) = &self.preference_error {
                    ui.colored_label(ui.visuals().error_fg_color, message);
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
        let preferences = preferences_changed.then(|| {
            let value = draft.model.format_options();
            crate::settings::ProgrammerInspectorPreferences {
                prefix: value.prefix,
                pad_to_width: value.pad_to_width,
                grouping: value.grouping,
            }
        });
        InspectorActions {
            host_request: request,
            close: !runtime.pinned && (!window_open || self.close_requested),
            context: Arc::clone(&runtime.context),
            draft: changed.then_some(draft),
            expected_generation: self.draft.generation,
            copy,
            insert,
            compare,
            preferences,
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
    use crate::calculation_pad::{CalculationPadSession, TypedInteger};
    use crate::platform::app_data::AppDataRoot;

    #[test]
    fn selected_comparison_input_tracks_identity_and_reports_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let mut session =
            CalculationPadSession::new(&AppDataRoot::from_path(temp.path().join("data")));
        session.replace_source("123\n345").unwrap();
        session.save_as("selected input").unwrap();
        session.evaluation();
        let initial = session.comparison_context().unwrap();
        let input = ComparisonSlot::Result(crate::calculation_pad::ComparisonResultRef {
            incarnation: initial.incarnation,
            document_id: initial.document_id,
            line_id: initial.rows[0].id,
        });
        assert_eq!(comparison_input_text(&input, Some(&initial)), "r1");
        session.replace_source("0\n123\n345").unwrap();
        session.evaluation();
        let moved = session.comparison_context().unwrap();
        assert_eq!(moved.rows[1].id, initial.rows[0].id);
        assert_eq!(comparison_input_text(&input, Some(&moved)), "r2");
        session.replace_source("0\n345").unwrap();
        session.evaluation();
        let removed = session.comparison_context().unwrap();
        assert_eq!(comparison_input_text(&input, Some(&removed)), "Unavailable selected result");
        session.request_open(initial.document_id).unwrap();
        session.resolve_pending(crate::calculation_pad::TransitionDecision::Discard).unwrap();
        session.evaluation();
        let reopened = session.comparison_context().unwrap();
        assert_eq!(comparison_input_text(&input, Some(&reopened)), "Unavailable selected result");
    }

    #[test]
    fn inspector_mode_selection_checks_and_preserves_pending_target() {
        let mut draft = InspectorDraft::default();
        let format = draft.model.format_options();
        for width in IntegerWidth::ALL {
            for signed in [false, true] {
                let mode = if signed {
                    IntegerMode::signed(width)
                } else {
                    IntegerMode::unsigned(width)
                };
                draft.select_mode(mode);
                assert_eq!(draft.model.value().mode(), mode);
                assert_eq!(draft.target_mode(), mode);
                assert!(draft.feedback.is_none());
                assert_eq!(
                    bit_rows(width.bits(), 3, 4).iter().flatten().count(),
                    width.bits() as usize
                );
            }
        }
        let wide = IntegerMode::unsigned(IntegerWidth::Bits128);
        draft
            .model
            .set_value(TypedInteger::from_bit_pattern((1u128 << 127) | 255, wide).unwrap());
        let before = draft.model.value();
        let narrow = IntegerMode::unsigned(IntegerWidth::Bits8);
        draft.select_mode(narrow);
        assert_eq!(draft.model.value(), before);
        assert_eq!(draft.target_mode(), narrow);
        assert!(draft.feedback.as_ref().unwrap().contains("u8"));
        draft.reinterpret_target();
        assert_eq!(draft.model.value().bit_pattern(), 255);
        let signed = IntegerMode::signed(IntegerWidth::Bits8);
        draft.select_mode(signed);
        assert_eq!(draft.model.value().mode(), narrow);
        assert_eq!(draft.target_mode(), signed);
        draft.reinterpret_target();
        assert_eq!(draft.model.value().mode(), signed);
        assert_eq!(draft.model.value().bit_pattern(), 255);
        assert_eq!(draft.model.format_options(), format);
    }

    #[test]
    fn inspector_group_rows_and_annotation_fragments_preserve_bit_indices() {
        for width in IntegerWidth::ALL {
            for columns in [1, 2, 5, 16] {
                let indices: Vec<_> = bit_rows(width.bits(), columns, 4)
                    .into_iter()
                    .flatten()
                    .collect();
                assert_eq!(indices, (0..width.bits()).rev().collect::<Vec<_>>());
            }
        }
        assert_eq!(bit_rows(16, 9, 4)[0], vec![15, 14, 13, 12, 11, 10, 9, 8]);
        assert_eq!(bit_rows(16, 5, 8)[0], vec![15, 14, 13, 12, 11]);
        assert_eq!(bit_columns(500.0, 25.0, 1.0, 8.0, 4), 16);
        assert!(bit_columns(200.0, 40.0, 1.0, 8.0, 4) < 8);
        let cell = |index, x| {
            (
                index,
                egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(10.0, 10.0)),
            )
        };
        let upper = [cell(7, 0.0), cell(6, 10.0), cell(5, 20.0), cell(4, 30.0)];
        let lower = [cell(3, 0.0), cell(2, 10.0), cell(1, 20.0), cell(0, 30.0)];
        assert_eq!(
            annotation_fragment(&upper, &(2..7)),
            Some(egui::Rangef::new(10.0, 40.0))
        );
        assert_eq!(
            annotation_fragment(&lower, &(2..7)),
            Some(egui::Rangef::new(0.0, 20.0))
        );
        assert!(annotation_fragment(&lower, &(4..8)).is_none());
        let mut draft = InspectorDraft::default();
        draft.select_mode(IntegerMode::unsigned(IntegerWidth::Bits16));
        draft.model.set_annotation(0..8, "Status".into()).unwrap();
        draft.model.set_annotation(4..12, "Overlap".into()).unwrap();
        assert_eq!(
            annotation_label(&draft.model.annotations()[0]),
            "Status [7:0]"
        );
        draft.select_mode(IntegerMode::unsigned(IntegerWidth::Bits8));
        assert_eq!(draft.model.annotations().len(), 2);
        assert!(
            draft
                .model
                .annotation_is_valid(&draft.model.annotations()[0])
        );
        assert!(
            !draft
                .model
                .annotation_is_valid(&draft.model.annotations()[1])
        );
    }

    #[test]
    fn inspector_narrow_grid_renders_all_bits_and_keeps_long_low_label_in_row() {
        let ctx = egui::Context::default();
        let mut dialog = ProgrammerInspectorDialog::default();
        dialog.open();
        for width in IntegerWidth::ALL {
            dialog.draft.select_mode(IntegerMode::unsigned(width));
            dialog.draft.bit_grouping = 4;
            if width == IntegerWidth::Bits8 {
                dialog
                    .draft
                    .model
                    .set_annotation(0..2, "A long label at the low end of the bit grid".into())
                    .unwrap();
            }
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 900.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let runtime = InspectorRuntime {
                    context: Arc::new(InspectorPadContext::default()),
                    root: ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT),
                    host: HostIdentity {
                        host: UtilityPresentation::Docked,
                        generation: 0,
                    },
                    pinned: false,
                };
                dialog.render_host(
                    &ctx,
                    &runtime,
                    true,
                    true,
                    Some(ui),
                    UtilityPresentation::Docked,
                );
            });
            output.textures_delta.clear();
            let bits: Vec<_> = dialog
                .test_controls
                .iter()
                .filter(|(name, _, _, _)| {
                    name.strip_prefix("bit-")
                        .is_some_and(|suffix| suffix.parse::<u32>().is_ok())
                })
                .collect();
            assert_eq!(bits.len(), width.bits() as usize);
            assert!(
                bits.iter()
                    .any(|(name, _, _, _)| name == &format!("bit-{}", width.bits() - 1))
            );
            for (_, rect, clip, _) in bits {
                assert!(
                    rect.left() >= clip.left() && rect.right() <= clip.right(),
                    "{rect:?} outside {clip:?}"
                );
            }
            if width == IntegerWidth::Bits8 {
                let (_, label, row, _) = dialog
                    .test_controls
                    .iter()
                    .find(|(name, _, _, _)| name == "annotation-label")
                    .unwrap();
                assert!(row.contains_rect(*label), "{label:?} outside {row:?}");
            }
        }
    }

    #[test]
    fn inspector_grid_uses_sixteen_bits_at_normal_width_and_adapts_to_text_size() {
        for (available, font_size, expected_first_row) in [(620.0, 14.0, 16), (320.0, 24.0, 4)] {
            let ctx = egui::Context::default();
            ctx.all_styles_mut(|style| {
                style.text_styles.insert(
                    egui::TextStyle::Monospace,
                    egui::FontId::monospace(font_size),
                );
            });
            let mut dialog = ProgrammerInspectorDialog::default();
            dialog.open();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(available, 900.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let runtime = InspectorRuntime {
                    context: Arc::new(InspectorPadContext::default()),
                    root: ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT),
                    host: HostIdentity {
                        host: UtilityPresentation::Docked,
                        generation: 0,
                    },
                    pinned: false,
                };
                dialog.render_host(
                    &ctx,
                    &runtime,
                    true,
                    true,
                    Some(ui),
                    UtilityPresentation::Docked,
                );
            });
            output.textures_delta.clear();
            let bits: Vec<_> = dialog
                .test_controls
                .iter()
                .filter(|(name, _, _, _)| {
                    name.strip_prefix("bit-")
                        .is_some_and(|suffix| suffix.parse::<u32>().is_ok())
                })
                .collect();
            assert_eq!(bits.len(), 64);
            let first_y = bits[0].1.center().y;
            let first_row = bits
                .iter()
                .take_while(|(_, rect, _, _)| (rect.center().y - first_y).abs() < font_size * 0.5)
                .count();
            if font_size == 14.0 {
                assert_eq!(
                    first_row,
                    expected_first_row,
                    "first cells: {:?}",
                    &bits[..20]
                );
            } else {
                assert!(
                    first_row >= expected_first_row && first_row < 16,
                    "first row has {first_row} cells: {:?}",
                    &bits[..20]
                );
            }
            for (_, rect, clip, _) in bits {
                assert!(
                    rect.left() >= clip.left() && rect.right() <= clip.right(),
                    "{rect:?} outside {clip:?}"
                );
            }
        }
    }

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
            ..Default::default()
        });
        let mut dialog = ProgrammerInspectorDialog::default();
        dialog.open();
        dialog.draft.bit_grouping = 128;
        dialog.publish(Arc::clone(&old));
        assert_eq!(dialog.draft.bit_grouping, 8);
        let mut draft = dialog.draft.clone();
        draft.model.toggle_bit(7).unwrap();
        draft.label_text = "unfinished".into();
        draft.control_dirty = true;
        draft.generation += 1;
        let preference = crate::settings::ProgrammerInspectorPreferences {
            prefix: true,
            ..Default::default()
        };
        let actions = InspectorActions {
            host_request: None,
            close: false,
            context: Arc::clone(&old),
            expected_generation: dialog.draft.generation,
            draft: Some(draft),
            copy: Some("stale".into()),
            insert: true,
            compare: false,
            preferences: Some(preference),
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
            ..Default::default()
        });
        assert_eq!(
            dialog.commit(actions, &fresh),
            (None, None, Some(preference))
        );
        dialog.publish(Arc::clone(&fresh));
        assert_eq!(dialog.draft.model.value().bit_pattern(), 129);
        assert_eq!(dialog.draft.model.adopted(), old.candidate.as_ref());
        assert_eq!(dialog.draft.model.candidate(), fresh.candidate.as_ref());
        assert_eq!(dialog.draft.label_text, "unfinished");

        let mut moved = dialog.draft.clone();
        moved.label_text = "retained across Dock".into();
        moved.formatting_open = true;
        moved.labels_open = true;
        moved.comparison_open = false;
        moved.generation += 1;
        let move_action = InspectorActions {
            host_request: Some(UtilityPresentation::Docked),
            close: false,
            context: Arc::clone(&fresh),
            expected_generation: dialog.draft.generation,
            draft: Some(moved),
            copy: Some("suppressed".into()),
            insert: true,
            compare: false,
            preferences: Some(preference),
            copies: Vec::new(),
            ctx: egui::Context::default(),
            receipt: None,
            host_button: None,
        };
        assert_eq!(
            dialog.commit(move_action, &fresh),
            (None, None, Some(preference))
        );
        assert_eq!(dialog.draft.label_text, "retained across Dock");
        assert!(dialog.draft.formatting_open && dialog.draft.labels_open);
        assert!(!dialog.draft.comparison_open);
        let host_only = InspectorActions {
            host_request: Some(UtilityPresentation::Popout),
            close: false,
            context: Arc::clone(&fresh),
            expected_generation: dialog.draft.generation,
            draft: None,
            copy: None,
            insert: false,
            compare: false,
            preferences: None,
            copies: Vec::new(),
            ctx: egui::Context::default(),
            receipt: None,
            host_button: None,
        };
        assert_eq!(dialog.commit(host_only, &fresh), (None, None, None));
        dialog.publish(Arc::clone(&old));
        assert!(dialog.draft.formatting_open && dialog.draft.labels_open);
        assert!(!dialog.draft.comparison_open);

        let mut clean = ProgrammerInspectorDialog::default();
        clean.open();
        clean.publish(Arc::clone(&old));
        clean.draft.label_text = "pending label".into();
        clean.draft.control_dirty = true;
        clean.publish(fresh);
        assert_eq!(clean.draft.model.adopted(), old.candidate.as_ref());
        assert_eq!(clean.draft.label_text, "pending label");
    }

    #[test]
    fn programmer_inspector_preference_retry_clears_only_matching_failure() {
        let mut dialog = ProgrammerInspectorDialog::default();
        let original = crate::settings::ProgrammerInspectorPreferences::default();
        let requested = crate::settings::ProgrammerInspectorPreferences {
            prefix: true,
            ..original
        };
        dialog.initialize_preferences(requested);
        dialog.draft.feedback = Some("Unrelated input error".into());
        dialog.preference_save_failed("disk unavailable");
        dialog.preference_saved(original, requested, false);
        assert!(dialog.preference_error.is_none());
        assert_eq!(
            dialog.draft.feedback.as_deref(),
            Some("Unrelated input error")
        );
        dialog.preference_save_failed("newer failure");
        dialog.preference_saved(original, original, false);
        assert!(dialog.preference_error.is_some());
        assert!(dialog.draft.model.format_options().prefix);

        dialog.initialize_preferences(original);
        dialog.preference_saved(original, requested, true);
        assert!(
            !dialog.draft.model.format_options().prefix,
            "a newer queued choice survives the older acknowledgement"
        );
    }
}
