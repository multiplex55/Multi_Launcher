//! Input and review shared by the two Markdown outlines.
use super::utility_host::HostIdentity;
use crate::notes_markdown::MarkdownHeading;
use crate::notes_markdown::section_move::{
    PreparedSectionMove, SectionPlacement, prepare_section_move, section_move_available,
};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutlineStamp {
    pub(super) document: egui::Id,
    pub(super) revision: u64,
    pub(super) host: HostIdentity,
    pub(super) viewport: egui::ViewportId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MoveRequest {
    pub(super) stamp: OutlineStamp,
    pub(super) selected: usize,
    pub(super) destination: usize,
    pub(super) placement: SectionPlacement,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum OutlineAction {
    Prepare(MoveRequest),
    Confirm(MoveRequest),
    Cancel(MoveRequest),
}

#[derive(Clone, Copy)]
struct DragSection {
    stamp: OutlineStamp,
    heading: usize,
}

#[derive(Default)]
pub(super) struct OutlineMoves {
    review: Option<OutlineReview>,
    message: Option<String>,
    availability: Vec<(MoveRequest, bool)>,
    reveal: Option<(OutlineStamp, usize)>,
}

struct OutlineReview {
    request: MoveRequest,
    prepared: PreparedSectionMove,
}

impl OutlineMoves {
    pub(super) fn has_review(&self, stamp: OutlineStamp) -> bool {
        self.review
            .as_ref()
            .is_some_and(|review| review.request.stamp == stamp)
    }

    pub(super) fn clear(&mut self) {
        self.review = None;
        self.message = None;
        self.availability.clear();
        self.reveal = None;
    }

    pub(super) fn record_move(&mut self, stamp: OutlineStamp, heading_start: usize) {
        self.clear();
        self.reveal = Some((stamp, heading_start));
    }

    pub(super) fn reveal_moved_row(
        &mut self,
        stamp: OutlineStamp,
        heading_start: usize,
        row: &egui::Response,
    ) {
        if self.reveal == Some((stamp, heading_start)) {
            row.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
            self.reveal = None;
        }
    }

    pub(super) fn retire_stale(&mut self, stamp: OutlineStamp) {
        if self.reveal.is_some_and(|(owner, _)| owner != stamp) {
            self.reveal = None;
        }
        if self
            .review
            .as_ref()
            .is_some_and(|review| review.request.stamp != stamp)
        {
            self.review = None;
        }
    }

    pub(super) fn can_move(&mut self, request: MoveRequest, source: &str) -> bool {
        self.availability
            .retain(|(cached, _)| cached.stamp == request.stamp);
        if let Some((_, available)) = self
            .availability
            .iter()
            .find(|(cached, _)| *cached == request)
        {
            return *available;
        }
        let available = section_move_available(
            source,
            request.selected,
            request.destination,
            request.placement,
        );
        if self.availability.len() == 12 {
            self.availability.remove(0);
        }
        self.availability.push((request, available));
        available
    }

    pub(super) fn ui(
        &self,
        ui: &mut egui::Ui,
        stamp: OutlineStamp,
        headings: &[MarkdownHeading],
        owns_input: bool,
    ) -> Option<OutlineAction> {
        if let Some(message) = &self.message {
            ui.label(message);
        }
        let review = self
            .review
            .as_ref()
            .filter(|review| review.request.stamp == stamp)?;
        ui.group(|ui| {
            ui.strong("Review the section move");
            if let (Some(selected), Some(destination)) = (
                headings.get(review.request.selected),
                headings.get(review.request.destination),
            ) {
                let placement = match review.request.placement {
                    SectionPlacement::Before => "before",
                    SectionPlacement::After => "after",
                };
                ui.label(format!(
                    "Move “{}” on line {} {placement} “{}” on line {}.",
                    selected.title,
                    selected.line_index + 1,
                    destination.title,
                    destination.line_index + 1
                ));
            }
            egui::ScrollArea::vertical()
                .max_height(100.0)
                .show(ui, |ui| {
                    for change in &review.prepared.anchor_changes {
                        if change.before == change.after {
                            ui.label(format!("#{} will open a different section.", change.after));
                        } else {
                            ui.label(format!("#{} → #{}", change.before, change.after));
                        }
                    }
                });
            let mut result = None;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(owns_input, egui::Button::new("Confirm section move"))
                    .clicked()
                {
                    result = Some(OutlineAction::Confirm(review.request));
                }
                if ui
                    .add_enabled(owns_input, egui::Button::new("Cancel move"))
                    .clicked()
                {
                    result = Some(OutlineAction::Cancel(review.request));
                }
            });
            result
        })
        .inner
    }

    pub(super) fn apply_action(
        &mut self,
        current: OutlineStamp,
        source: &str,
        action: OutlineAction,
    ) -> Option<PreparedSectionMove> {
        self.retire_stale(current);
        match action {
            OutlineAction::Prepare(request) if request.stamp == current => {
                self.message = None;
                match prepare_section_move(
                    source,
                    request.selected,
                    request.destination,
                    request.placement,
                ) {
                    Ok(prepared) if prepared.anchor_changes.is_empty() => Some(prepared),
                    Ok(prepared) => {
                        self.review = Some(OutlineReview { request, prepared });
                        None
                    }
                    Err(error) => {
                        self.message = Some(error.to_string());
                        None
                    }
                }
            }
            OutlineAction::Confirm(request)
                if self
                    .review
                    .as_ref()
                    .is_some_and(|review| review.request == request) =>
            {
                self.review.take().map(|review| review.prepared)
            }
            OutlineAction::Cancel(request)
                if self
                    .review
                    .as_ref()
                    .is_some_and(|review| review.request == request) =>
            {
                self.clear();
                None
            }
            _ => None,
        }
    }

    pub(super) fn report_error(&mut self, error: impl std::fmt::Display) {
        self.message = Some(error.to_string());
    }
}

pub(super) fn owns_outline_input(ui: &egui::Ui) -> bool {
    let ctx = ui.ctx();
    ui.is_enabled()
        && ui.input(|input| input.focused)
        && !egui::Popup::is_any_open(ctx)
        && ctx.memory(|memory| memory.top_modal_layer().is_none())
        && super::jump_mode::ui::visible_top_middle_layer(ctx)
            .is_none_or(|layer| layer == ui.layer_id())
}

pub(super) fn outline_row(
    ui: &mut egui::Ui,
    title: &str,
    selected: bool,
    stamp: OutlineStamp,
    index: usize,
    allow_moves: bool,
    slot_can_move: impl FnOnce(MoveRequest) -> bool,
) -> (egui::Response, Option<OutlineAction>) {
    let mut button = egui::Button::selectable(selected, title);
    if allow_moves {
        button = button.sense(egui::Sense::click_and_drag());
    }
    let response = ui.push_id(index, |ui| ui.add(button)).inner;
    if response.gained_focus() {
        response.scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
    }
    if !allow_moves || !response.enabled() {
        return (response, None);
    }
    response.dnd_set_drag_payload(DragSection {
        stamp,
        heading: index,
    });
    let Some(drag) = response
        .dnd_hover_payload::<DragSection>()
        .filter(|drag| drag.stamp == stamp)
    else {
        return (response, None);
    };
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return (response, None);
    };
    let placement = if pointer.y < response.rect.center().y {
        SectionPlacement::Before
    } else {
        SectionPlacement::After
    };
    let request = MoveRequest {
        stamp,
        selected: drag.heading,
        destination: index,
        placement,
    };
    if slot_can_move(request) {
        let y = match placement {
            SectionPlacement::Before => response.rect.top(),
            SectionPlacement::After => response.rect.bottom(),
        };
        let right = ui
            .available_rect_before_wrap()
            .right()
            .min(ui.clip_rect().right());
        ui.painter().line_segment(
            [egui::pos2(response.rect.left(), y), egui::pos2(right, y)],
            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
        );
    }
    let action = response
        .dnd_release_payload::<DragSection>()
        .map(|_| OutlineAction::Prepare(request));
    (response, action)
}

pub(super) fn move_buttons(
    ui: &mut egui::Ui,
    up: Option<MoveRequest>,
    down: Option<MoveRequest>,
) -> Option<OutlineAction> {
    let mut selected_action = None;
    ui.horizontal_wrapped(|ui| {
        for (heading, action) in [("Move up", up), ("Move down", down)] {
            if ui
                .add_enabled(action.is_some(), egui::Button::new(heading))
                .clicked()
            {
                selected_action = action.map(OutlineAction::Prepare);
            }
        }
    });
    selected_action
}

pub(super) fn sibling_requests(
    headings: &[MarkdownHeading],
    selected: Option<usize>,
    stamp: OutlineStamp,
) -> [Option<MoveRequest>; 2] {
    let Some(selected) = selected.filter(|&index| index < headings.len()) else {
        return [None, None];
    };
    let (parents, ends) = crate::notes_markdown::sections::heading_hierarchy(headings);
    let parent = parents[selected];
    let before = (0..selected).rev().find(|&index| parents[index] == parent);
    let after = (ends[selected] < headings.len() && parents[ends[selected]] == parent)
        .then_some(ends[selected]);
    [
        before.map(|destination| MoveRequest {
            stamp,
            selected,
            destination,
            placement: SectionPlacement::Before,
        }),
        after.map(|destination| MoveRequest {
            stamp,
            selected,
            destination,
            placement: SectionPlacement::After,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes_markdown::analyze_markdown;

    fn stamp(revision: u64) -> OutlineStamp {
        OutlineStamp {
            document: egui::Id::new("markdown-outline-fixture"),
            revision,
            host: HostIdentity {
                host: super::super::utility_host::UtilityPresentation::Docked,
                generation: 1,
            },
            viewport: egui::ViewportId::ROOT,
        }
    }

    #[test]
    fn outline_move_review_fences_revision_and_requires_confirmation() {
        let source = "# Same\nfirst\n# Same\nsecond";
        let headings = analyze_markdown(source).headings;
        let [_, Some(request)] = sibling_requests(&headings, Some(0), stamp(1)) else {
            panic!("the next heading is a sibling");
        };
        let mut moves = OutlineMoves::default();
        assert!(moves.can_move(request, source));
        assert!(
            moves
                .apply_action(stamp(1), source, OutlineAction::Prepare(request))
                .is_none()
        );
        assert!(moves.has_review(stamp(1)));
        assert!(
            moves
                .apply_action(stamp(2), source, OutlineAction::Confirm(request))
                .is_none()
        );
        assert!(!moves.has_review(stamp(2)));
        assert!(
            moves
                .apply_action(stamp(1), source, OutlineAction::Prepare(request))
                .is_none()
        );
        assert!(
            moves
                .apply_action(stamp(1), source, OutlineAction::Cancel(request))
                .is_none()
        );
        assert!(!moves.has_review(stamp(1)));
        assert!(
            moves
                .apply_action(stamp(1), source, OutlineAction::Prepare(request))
                .is_none()
        );
        let prepared = moves
            .apply_action(stamp(1), source, OutlineAction::Confirm(request))
            .unwrap();
        assert_eq!(
            prepared.edit.apply(source).unwrap().source,
            "# Same\nsecond\n# Same\nfirst"
        );
        assert!(!moves.has_review(stamp(1)));
    }

    #[test]
    fn outline_move_direction_uses_the_complete_hierarchy() {
        let source = "# First\n## Child\ntext\n# Second\n## Deeper\n# Third\n";
        let headings = analyze_markdown(source).headings;
        let [before, after] = sibling_requests(&headings, Some(2), stamp(1));
        assert_eq!(before.unwrap().destination, 0);
        assert_eq!(after.unwrap().destination, 4);
        assert_eq!(sibling_requests(&headings, Some(1), stamp(1)), [None, None]);
    }

    #[test]
    fn outline_move_drag_uses_exact_current_owner_and_slot() {
        use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
        let ctx = egui::Context::default();
        let frame = |revision, events| {
            let mut rects = Vec::new();
            let mut action = None;
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(350.0, 280.0))),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    for (index, title) in
                        ["First section", "Second section"].into_iter().enumerate()
                    {
                        let (response, request) = outline_row(
                            ui,
                            title,
                            false,
                            stamp(revision),
                            index,
                            true,
                            |candidate| {
                                candidate.selected == 0
                                    && candidate.destination == 1
                                    && candidate.placement == SectionPlacement::After
                            },
                        );
                        rects.push(response.rect);
                        if request.is_some() {
                            action = request;
                        }
                    }
                },
            );
            output.drop_without_applying_deltas();
            (rects, action)
        };
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let (rects, _) = frame(1, Vec::new());
        let from = rects[0].center();
        let to = rects[1].center_bottom() - Vec2::new(0.0, 2.0);
        let drag = |drop_revision, to| {
            frame(1, vec![Event::PointerMoved(from)]);
            frame(1, vec![button(from, true)]);
            let (_, interim) = frame(1, vec![Event::PointerMoved(to)]);
            assert!(interim.is_none());
            frame(drop_revision, vec![button(to, false)]).1
        };
        let Some(OutlineAction::Prepare(request)) = drag(1, to) else {
            panic!("a retained drag to the eligible slot should stage a move");
        };
        assert_eq!(
            (request.selected, request.destination, request.placement),
            (0, 1, SectionPlacement::After)
        );
        assert!(drag(2, to).is_none());
        let before = rects[1].center_top() + Vec2::new(0.0, 2.0);
        let rejected = drag(1, before).expect("invalid owned slot should provide feedback");
        let mut moves = OutlineMoves::default();
        assert!(
            moves
                .apply_action(stamp(1), "# A\nbody\n# B\nother", rejected)
                .is_none()
        );
        assert!(
            moves
                .message
                .as_deref()
                .is_some_and(|message| message.contains("no effect"))
        );
    }
}
