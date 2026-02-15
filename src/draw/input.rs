use crate::draw::history::DrawHistory;
use crate::draw::keyboard_hook::{map_key_event_to_command, KeyCommand, KeyEvent};
use crate::draw::messages::ExitReason;
use crate::draw::model::{CanvasModel, Color, DrawObject, Geometry, ObjectStyle, Tool};
use crate::draw::render::DirtyRect;
use crate::draw::runtime;

const MIN_POINT_DIST_SQ: i64 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointerModifiers {
    pub ctrl: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputCommand {
    Undo,
    Redo,
    RequestExit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawInputState {
    tool: Tool,
    style: ObjectStyle,
    active_geometry: Option<Geometry>,
    preview_origin: Option<(i32, i32)>,
    previous_preview_bounds: Option<DirtyRect>,
    history: DrawHistory,
    committed_buffer: CanvasModel,
    preview_buffer: Option<DrawObject>,
    committed_revision: u64,
    dirty_rect: Option<DirtyRect>,
    full_redraw_requested: bool,
    full_redraw_request_count: u64,
}

impl DrawInputState {
    pub fn new(tool: Tool, style: ObjectStyle) -> Self {
        Self {
            tool,
            style,
            active_geometry: None,
            preview_origin: None,
            previous_preview_bounds: None,
            history: DrawHistory::default(),
            committed_buffer: CanvasModel::default(),
            preview_buffer: None,
            committed_revision: 0,
            dirty_rect: None,
            full_redraw_requested: true,
            full_redraw_request_count: 1,
        }
    }

    pub fn history(&self) -> &DrawHistory {
        &self.history
    }

    pub fn current_tool(&self) -> Tool {
        self.tool
    }

    pub fn current_style(&self) -> ObjectStyle {
        self.style
    }

    pub fn committed_revision(&self) -> u64 {
        self.committed_revision
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
    }

    pub fn set_style(&mut self, style: ObjectStyle) {
        self.style = style;
    }

    pub fn apply_quick_color(&mut self, color: Color) {
        self.style.stroke.color = color;
    }

    pub fn take_dirty_rect(&mut self) -> Option<DirtyRect> {
        self.dirty_rect.take()
    }

    pub fn take_full_redraw_request(&mut self) -> bool {
        std::mem::take(&mut self.full_redraw_requested)
    }

    fn mark_dirty(&mut self, rect: DirtyRect) {
        self.dirty_rect = Some(match self.dirty_rect.take() {
            Some(existing) => existing.union(rect),
            None => rect,
        });
    }

    fn request_full_redraw(&mut self) {
        self.full_redraw_requested = true;
        self.full_redraw_request_count = self.full_redraw_request_count.saturating_add(1);
        self.dirty_rect = None;
    }

    pub fn full_redraw_request_count(&self) -> u64 {
        self.full_redraw_request_count
    }

    pub fn committed_canvas(&self) -> &CanvasModel {
        &self.committed_buffer
    }

    pub fn active_object(&self) -> Option<&DrawObject> {
        self.preview_buffer.as_ref()
    }

    pub fn handle_left_down(
        &mut self,
        point: (i32, i32),
        modifiers: PointerModifiers,
    ) -> Option<InputCommand> {
        if modifiers.shift || modifiers.ctrl {
            return Some(InputCommand::RequestExit);
        }

        self.active_geometry = Some(match self.tool {
            Tool::Pen => Geometry::Pen {
                points: vec![point],
            },
            Tool::Line => Geometry::Line {
                start: point,
                end: point,
            },
            Tool::Rect => Geometry::Rect {
                start: point,
                end: point,
            },
            Tool::Ellipse => Geometry::Ellipse {
                start: point,
                end: point,
            },
            Tool::Eraser => Geometry::Eraser {
                points: vec![point],
            },
        });

        self.preview_origin = None;
        self.previous_preview_bounds = None;
        self.preview_buffer = None;

        if matches!(self.tool, Tool::Line | Tool::Rect | Tool::Ellipse) {
            self.preview_origin = Some(point);
            let geometry = match self.tool {
                Tool::Line => Geometry::Line {
                    start: point,
                    end: point,
                },
                Tool::Rect => Geometry::Rect {
                    start: point,
                    end: point,
                },
                Tool::Ellipse => Geometry::Ellipse {
                    start: point,
                    end: point,
                },
                Tool::Pen | Tool::Eraser => unreachable!("shape tools only"),
            };
            self.preview_buffer = Some(DrawObject {
                tool: self.tool,
                style: self.style,
                geometry,
            });
            self.previous_preview_bounds = Some(DirtyRect::from_points(
                point,
                point,
                (self.style.stroke.width.max(1) as i32) + 2,
            ));
        }

        self.mark_dirty(DirtyRect::from_points(
            point,
            point,
            (self.style.stroke.width.max(1) as i32) + 2,
        ));

        None
    }

    pub fn handle_move(&mut self, point: (i32, i32)) {
        let pad = (self.style.stroke.width.max(1) as i32) + 2;
        let mut dirty: Option<DirtyRect> = None;
        match self.active_geometry.as_mut() {
            Some(Geometry::Pen { points }) | Some(Geometry::Eraser { points }) => {
                if should_append_point(points.last().copied(), point) {
                    let previous_segment = if points.len() >= 2 {
                        Some(DirtyRect::from_points(
                            points[points.len() - 2],
                            points[points.len() - 1],
                            pad,
                        ))
                    } else {
                        None
                    };
                    if let Some(last) = points.last().copied() {
                        dirty = Some(DirtyRect::from_points(last, point, pad));
                    }
                    points.push(point);

                    let mut preview_updated = false;
                    if let Some(preview) = self.preview_buffer.as_mut() {
                        if preview.tool == self.tool && preview.style == self.style {
                            match &mut preview.geometry {
                                Geometry::Pen {
                                    points: preview_points,
                                }
                                | Geometry::Eraser {
                                    points: preview_points,
                                } => {
                                    preview_points.push(point);
                                    preview_updated = true;
                                }
                                Geometry::Line { .. }
                                | Geometry::Rect { .. }
                                | Geometry::Ellipse { .. } => {}
                            }
                        }
                    }
                    if !preview_updated {
                        let geometry = match self.tool {
                            Tool::Pen => Geometry::Pen {
                                points: points.clone(),
                            },
                            Tool::Eraser => Geometry::Eraser {
                                points: points.clone(),
                            },
                            Tool::Line | Tool::Rect | Tool::Ellipse => {
                                unreachable!("freehand preview tools only")
                            }
                        };
                        self.preview_buffer = Some(DrawObject {
                            tool: self.tool,
                            style: self.style,
                            geometry,
                        });
                    }

                    if let Some(current_bounds) = self.preview_buffer.as_ref().and_then(|preview| {
                        geometry_dirty_rect(&preview.geometry, self.style.stroke.width.max(1))
                    }) {
                        let previous_bounds = self.previous_preview_bounds;
                        self.previous_preview_bounds = Some(current_bounds);
                        let dirty_with_preview = previous_bounds
                            .map(|bounds| bounds.union(current_bounds))
                            .unwrap_or(current_bounds);
                        dirty = Some(match dirty.take() {
                            Some(base) => base.union(dirty_with_preview),
                            None => dirty_with_preview,
                        });
                    }

                    if let Some(previous) = previous_segment {
                        dirty = Some(match dirty.take() {
                            Some(base) => base.union(previous),
                            None => previous,
                        });
                    }
                }
            }
            Some(Geometry::Line { start, end })
            | Some(Geometry::Rect { start, end })
            | Some(Geometry::Ellipse { start, end }) => {
                let origin = self.preview_origin.unwrap_or(*start);
                let previous = self
                    .previous_preview_bounds
                    .unwrap_or_else(|| DirtyRect::from_points(origin, *end, pad));
                *end = point;
                let after = DirtyRect::from_points(origin, point, pad);
                self.previous_preview_bounds = Some(after);
                let geometry = match self.tool {
                    Tool::Line => Geometry::Line {
                        start: origin,
                        end: point,
                    },
                    Tool::Rect => Geometry::Rect {
                        start: origin,
                        end: point,
                    },
                    Tool::Ellipse => Geometry::Ellipse {
                        start: origin,
                        end: point,
                    },
                    Tool::Pen | Tool::Eraser => unreachable!("shape preview tools only"),
                };
                self.preview_buffer = Some(DrawObject {
                    tool: self.tool,
                    style: self.style,
                    geometry,
                });
                dirty = Some(previous.union(after));
            }
            None => {}
        }
        if let Some(rect) = dirty {
            self.mark_dirty(rect);
        }
    }

    pub fn handle_left_up(&mut self, point: (i32, i32)) {
        self.handle_move(point);
        if let Some(geometry) = self.active_geometry.take() {
            if let Some(bounds) = geometry_dirty_rect(&geometry, self.style.stroke.width.max(1)) {
                self.mark_dirty(bounds);
            }
            self.history.commit(DrawObject {
                tool: self.tool,
                style: self.style,
                geometry,
            });
            self.committed_buffer = self.history.canvas();
            self.preview_buffer = None;
            self.preview_origin = None;
            self.previous_preview_bounds = None;
            self.committed_revision = self.committed_revision.saturating_add(1);
        }
    }

    pub fn handle_key_event(&mut self, event: KeyEvent) -> Option<InputCommand> {
        let command = map_key_event_to_command(true, event, None);
        self.handle_key_command(command)
    }

    pub fn handle_key_command(&mut self, command: Option<KeyCommand>) -> Option<InputCommand> {
        match command {
            Some(KeyCommand::Undo) => {
                if self.history.undo().is_some() {
                    self.committed_buffer = self.history.canvas();
                    self.request_full_redraw();
                    self.committed_revision = self.committed_revision.saturating_add(1);
                }
                Some(InputCommand::Undo)
            }
            Some(KeyCommand::Redo) => {
                if self.history.redo().is_some() {
                    self.committed_buffer = self.history.canvas();
                    self.request_full_redraw();
                    self.committed_revision = self.committed_revision.saturating_add(1);
                }
                Some(InputCommand::Redo)
            }
            Some(KeyCommand::RequestExit) => Some(InputCommand::RequestExit),
            Some(KeyCommand::ToggleToolbar) => None,
            Some(KeyCommand::SelectQuickColor(_)) => None,
            None => None,
        }
    }

    pub fn request_exit(&self) -> InputCommand {
        InputCommand::RequestExit
    }
}

pub(crate) fn geometry_dirty_rect(geometry: &Geometry, stroke_width: u32) -> Option<DirtyRect> {
    let pad = (stroke_width as i32) + 2;
    match geometry {
        Geometry::Pen { points } | Geometry::Eraser { points } => {
            let first = points.first().copied()?;
            let mut rect = DirtyRect::from_points(first, first, pad);
            let mut last = first;
            for point in points.iter().copied().skip(1) {
                rect = rect.union(DirtyRect::from_points(last, point, pad));
                last = point;
            }
            Some(rect)
        }
        Geometry::Line { start, end }
        | Geometry::Rect { start, end }
        | Geometry::Ellipse { start, end } => Some(DirtyRect::from_points(*start, *end, pad)),
    }
}

pub fn route_command_to_runtime(command: Option<InputCommand>, exit_reason: ExitReason) {
    route_command(command, exit_reason, |reason| {
        if let Err(err) = runtime().request_exit(reason) {
            tracing::warn!(?err, "failed to request draw exit from input command");
        }
    });
}

fn route_command<F>(command: Option<InputCommand>, exit_reason: ExitReason, mut request_exit: F)
where
    F: FnMut(ExitReason),
{
    if matches!(command, Some(InputCommand::RequestExit)) {
        request_exit(exit_reason);
    }
}

pub fn bridge_left_down_to_runtime(
    state: &mut DrawInputState,
    point: (i32, i32),
    modifiers: PointerModifiers,
) {
    route_command_to_runtime(
        state.handle_left_down(point, modifiers),
        ExitReason::UserRequest,
    );
}

pub fn bridge_mouse_move_to_runtime(state: &mut DrawInputState, point: (i32, i32)) {
    state.handle_move(point);
}

pub fn bridge_left_up_to_runtime(state: &mut DrawInputState, point: (i32, i32)) {
    state.handle_left_up(point);
}

pub fn bridge_key_event_to_runtime(
    state: &mut DrawInputState,
    event: KeyEvent,
) -> Option<InputCommand> {
    let command = state.handle_key_event(event);
    route_command_to_runtime(command.clone(), ExitReason::UserRequest);
    command
}

fn should_append_point(last: Option<(i32, i32)>, point: (i32, i32)) -> bool {
    let Some((last_x, last_y)) = last else {
        return true;
    };

    let dx = point.0 as i64 - last_x as i64;
    let dy = point.1 as i64 - last_y as i64;
    dx * dx + dy * dy >= MIN_POINT_DIST_SQ
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::keyboard_hook::{KeyCode, KeyModifiers};

    fn draw_state(tool: Tool) -> DrawInputState {
        DrawInputState::new(tool, ObjectStyle::default())
    }

    #[test]
    fn pen_commit_creates_single_history_object() {
        let mut state = draw_state(Tool::Pen);

        assert_eq!(
            state.handle_left_down((10, 10), PointerModifiers::default()),
            None
        );
        state.handle_move((10, 11));
        state.handle_move((14, 14));
        state.handle_left_up((18, 18));

        assert_eq!(state.history().undo_len(), 1);
        assert_eq!(state.history().canvas().objects.len(), 1);
    }

    #[test]
    fn every_tool_commits_single_object_on_left_down_move_up() {
        for tool in [
            Tool::Pen,
            Tool::Line,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Eraser,
        ] {
            let mut state = draw_state(tool);
            assert_eq!(
                state.handle_left_down((3, 4), PointerModifiers::default()),
                None
            );
            state.handle_move((9, 10));
            state.handle_left_up((12, 14));

            assert_eq!(
                state.history().undo_len(),
                1,
                "unexpected undo count for {tool:?}"
            );
            assert_eq!(
                state.history().canvas().objects.len(),
                1,
                "unexpected object count for {tool:?}"
            );
        }
    }

    #[test]
    fn pen_and_eraser_respect_movement_threshold() {
        for tool in [Tool::Pen, Tool::Eraser] {
            let mut state = draw_state(tool);
            let _ = state.handle_left_down((0, 0), PointerModifiers::default());
            state.handle_move((1, 1));
            state.handle_move((2, 2));
            state.handle_left_up((3, 0));

            let history = state.history();
            let canvas = history.canvas();
            let object = canvas.objects.first().expect("single committed object");
            match &object.geometry {
                Geometry::Pen { points } | Geometry::Eraser { points } => {
                    assert_eq!(
                        points,
                        &vec![(0, 0), (3, 0)],
                        "unexpected points for {tool:?}"
                    );
                }
                other => panic!("unexpected geometry for {tool:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn freehand_active_object_exists_after_move_before_commit() {
        for tool in [Tool::Pen, Tool::Eraser] {
            let mut state = draw_state(tool);
            let _ = state.handle_left_down((0, 0), PointerModifiers::default());
            state.handle_move((4, 0));
            assert!(
                state.active_object().is_some(),
                "missing preview for {tool:?}"
            );
        }
    }

    #[test]
    fn borrowed_accessors_track_preview_and_commit_transitions() {
        let mut state = draw_state(Tool::Pen);
        assert!(state.active_object().is_none());
        assert!(state.committed_canvas().objects.is_empty());

        let _ = state.handle_left_down((0, 0), PointerModifiers::default());
        state.handle_move((5, 0));

        let preview = state.active_object().expect("preview exists after move");
        match &preview.geometry {
            Geometry::Pen { points } => assert_eq!(points, &vec![(0, 0), (5, 0)]),
            other => panic!("unexpected preview geometry: {other:?}"),
        }

        state.handle_left_up((10, 0));

        assert!(state.active_object().is_none());
        let committed = state.committed_canvas();
        assert_eq!(committed.objects.len(), 1);
        match &committed.objects[0].geometry {
            Geometry::Pen { points } => assert_eq!(points, &vec![(0, 0), (5, 0), (10, 0)]),
            other => panic!("unexpected committed geometry: {other:?}"),
        }
    }

    #[test]
    fn borrowed_preview_accessor_is_stable_and_non_allocating_for_reads() {
        let mut state = draw_state(Tool::Pen);
        let _ = state.handle_left_down((0, 0), PointerModifiers::default());
        state.handle_move((4, 0));

        let first = state.active_object().expect("first preview");
        let second = state.active_object().expect("second preview");
        assert!(std::ptr::eq(first, second));
        assert_eq!(first, second);
    }

    #[test]
    fn shift_click_requests_exit() {
        let mut state = draw_state(Tool::Line);
        let command = state.handle_left_down(
            (0, 0),
            PointerModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(command, Some(InputCommand::RequestExit));
        assert_eq!(state.history().undo_len(), 0);
    }

    #[test]
    fn ctrl_click_requests_exit() {
        let mut state = draw_state(Tool::Rect);
        let command = state.handle_left_down(
            (0, 0),
            PointerModifiers {
                shift: false,
                ctrl: true,
            },
        );
        assert_eq!(command, Some(InputCommand::RequestExit));
        assert_eq!(state.history().undo_len(), 0);
    }

    #[test]
    fn esc_dispatches_exit() {
        let mut state = draw_state(Tool::Ellipse);
        let command = state.handle_key_event(KeyEvent {
            key: KeyCode::Escape,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(command, Some(InputCommand::RequestExit));
    }

    #[test]
    fn key_u_dispatches_undo() {
        let mut state = draw_state(Tool::Line);

        let _ = state.handle_left_down((0, 0), PointerModifiers::default());
        state.handle_left_up((10, 10));
        assert_eq!(state.history().undo_len(), 1);

        let undo_command = state.handle_key_event(KeyEvent {
            key: KeyCode::U,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(undo_command, Some(InputCommand::Undo));
        assert_eq!(state.history().undo_len(), 0);
        assert_eq!(state.history().redo_len(), 1);
    }

    #[test]
    fn ctrl_r_dispatches_redo() {
        let mut state = draw_state(Tool::Line);

        let _ = state.handle_left_down((0, 0), PointerModifiers::default());
        state.handle_left_up((10, 10));
        let _ = state.handle_key_event(KeyEvent {
            key: KeyCode::U,
            modifiers: KeyModifiers::default(),
        });

        let redo_command = state.handle_key_event(KeyEvent {
            key: KeyCode::KeyR,
            modifiers: KeyModifiers {
                ctrl: true,
                shift: false,
                alt: false,
                win: false,
            },
        });
        assert_eq!(redo_command, Some(InputCommand::Redo));
        assert_eq!(state.history().undo_len(), 1);
        assert_eq!(state.history().redo_len(), 0);
    }

    #[test]
    fn bridge_key_event_routes_exit_command_into_runtime_exit_path() {
        let rt = crate::draw::runtime();
        rt.reset_for_test();
        rt.force_lifecycle_for_test(crate::draw::state::DrawLifecycle::Active);

        let mut state = draw_state(Tool::Pen);
        bridge_key_event_to_runtime(
            &mut state,
            KeyEvent {
                key: KeyCode::Escape,
                modifiers: KeyModifiers::default(),
            },
        );

        assert_eq!(rt.lifecycle(), crate::draw::state::DrawLifecycle::Exiting);
        assert_eq!(
            rt.exit_prompt_state().map(|prompt| prompt.reason),
            Some(ExitReason::UserRequest)
        );
        rt.reset_for_test();
    }

    #[test]
    fn bridge_left_down_routes_shift_and_ctrl_failsafe_exit_commands_into_runtime() {
        let rt = crate::draw::runtime();
        for modifiers in [
            PointerModifiers {
                shift: true,
                ctrl: false,
            },
            PointerModifiers {
                shift: false,
                ctrl: true,
            },
        ] {
            rt.reset_for_test();
            rt.force_lifecycle_for_test(crate::draw::state::DrawLifecycle::Active);

            let mut state = draw_state(Tool::Pen);
            bridge_left_down_to_runtime(&mut state, (20, 20), modifiers);

            assert_eq!(rt.lifecycle(), crate::draw::state::DrawLifecycle::Exiting);
            assert_eq!(
                rt.exit_prompt_state().map(|prompt| prompt.reason),
                Some(ExitReason::UserRequest)
            );
        }
        rt.reset_for_test();
    }

    #[test]
    fn pen_incremental_moves_only_mark_segment_dirty_without_full_redraw() {
        let mut state = draw_state(Tool::Pen);
        assert!(state.take_full_redraw_request());

        let _ = state.handle_left_down((10, 10), PointerModifiers::default());
        let _ = state.take_dirty_rect();

        state.handle_move((20, 20));
        let dirty = state.take_dirty_rect().expect("segment dirty rect");
        assert!(dirty.width > 0 && dirty.height > 0);
        assert!(!state.take_full_redraw_request());
    }

    #[test]
    fn shape_preview_move_unions_previous_and_new_preview_bounds() {
        let mut state = draw_state(Tool::Rect);
        let _ = state.handle_left_down((4, 4), PointerModifiers::default());
        let _ = state.take_dirty_rect();

        state.handle_move((10, 10));
        let first = state.take_dirty_rect().expect("first preview dirty");
        state.handle_move((20, 20));
        let second = state.take_dirty_rect().expect("second preview dirty");

        assert!(second.x <= first.x);
        assert!(second.y <= first.y);
        assert!(second.width >= first.width);
        assert!(second.height >= first.height);
    }

    #[test]
    fn cancel_gestures_route_request_exit_via_same_runtime_path() {
        let mut state = draw_state(Tool::Pen);
        let mut reasons = Vec::new();

        route_command(
            state.handle_key_event(KeyEvent {
                key: KeyCode::Escape,
                modifiers: KeyModifiers::default(),
            }),
            ExitReason::UserRequest,
            |reason| reasons.push(reason),
        );

        route_command(
            state.handle_left_down(
                (10, 10),
                PointerModifiers {
                    shift: true,
                    ctrl: false,
                },
            ),
            ExitReason::UserRequest,
            |reason| reasons.push(reason),
        );

        route_command(
            state.handle_left_down(
                (10, 10),
                PointerModifiers {
                    shift: false,
                    ctrl: true,
                },
            ),
            ExitReason::UserRequest,
            |reason| reasons.push(reason),
        );

        assert_eq!(
            reasons,
            vec![
                ExitReason::UserRequest,
                ExitReason::UserRequest,
                ExitReason::UserRequest,
            ]
        );
    }

    #[test]
    fn tool_and_style_changes_do_not_request_full_redraw() {
        let mut state = draw_state(Tool::Pen);
        assert!(state.take_full_redraw_request());
        let baseline = state.full_redraw_request_count();

        state.set_tool(Tool::Rect);
        let mut style = state.current_style();
        style.stroke.color = crate::draw::model::Color::rgba(9, 8, 7, 255);
        state.set_style(style);

        assert_eq!(state.full_redraw_request_count(), baseline);
        assert!(!state.take_full_redraw_request());
    }

    #[test]
    fn quick_color_style_mutation_does_not_change_tool_or_history() {
        let mut state = draw_state(Tool::Rect);
        assert!(state.take_full_redraw_request());
        let initial_tool = state.current_tool();
        let initial_undo_len = state.history().undo_len();
        let initial_redo_len = state.history().redo_len();
        let initial_revision = state.committed_revision();

        state.apply_quick_color(crate::draw::model::Color::rgba(12, 34, 56, 255));

        assert_eq!(state.current_tool(), initial_tool);
        assert_eq!(state.history().undo_len(), initial_undo_len);
        assert_eq!(state.history().redo_len(), initial_redo_len);
        assert_eq!(state.committed_revision(), initial_revision);
        assert_eq!(
            state.current_style().stroke.color,
            crate::draw::model::Color::rgba(12, 34, 56, 255)
        );
        assert!(!state.take_full_redraw_request());
    }

    #[test]
    fn quick_color_key_command_is_non_destructive_when_unhandled_locally() {
        let mut state = draw_state(Tool::Pen);
        let baseline_style = state.current_style();
        let baseline_revision = state.committed_revision();

        let command = state.handle_key_command(Some(KeyCommand::SelectQuickColor(3)));
        assert_eq!(command, None);
        assert_eq!(state.current_style(), baseline_style);
        assert_eq!(state.committed_revision(), baseline_revision);
        assert_eq!(state.history().undo_len(), 0);
        assert_eq!(state.history().redo_len(), 0);
    }

    #[test]
    fn undo_redo_are_the_only_hotkey_paths_requesting_full_redraw() {
        let mut state = draw_state(Tool::Line);
        assert!(state.take_full_redraw_request());

        let _ = state.handle_left_down((0, 0), PointerModifiers::default());
        state.handle_left_up((10, 10));
        let baseline = state.full_redraw_request_count();

        let _ = state.handle_key_event(KeyEvent {
            key: KeyCode::U,
            modifiers: KeyModifiers::default(),
        });
        assert!(state.take_full_redraw_request());
        assert!(state.full_redraw_request_count() > baseline);

        let after_undo = state.full_redraw_request_count();
        let _ = state.handle_key_event(KeyEvent {
            key: KeyCode::KeyR,
            modifiers: KeyModifiers {
                ctrl: true,
                shift: false,
                alt: false,
                win: false,
            },
        });
        assert!(state.take_full_redraw_request());
        assert!(state.full_redraw_request_count() > after_undo);
    }

    #[test]
    fn geometry_dirty_rect_covers_segment_and_shapes() {
        let line = geometry_dirty_rect(
            &Geometry::Line {
                start: (10, 10),
                end: (20, 14),
            },
            2,
        )
        .expect("line rect");
        assert_eq!(line, DirtyRect::from_points((10, 10), (20, 14), 4));

        let rect = geometry_dirty_rect(
            &Geometry::Rect {
                start: (5, 8),
                end: (12, 18),
            },
            3,
        )
        .expect("rect bounds");
        assert_eq!(rect, DirtyRect::from_points((5, 8), (12, 18), 5));
    }

    #[test]
    fn committed_revision_advances_only_on_history_mutation() {
        let mut state = draw_state(Tool::Line);
        assert_eq!(state.committed_revision(), 0);

        state.handle_move((50, 50));
        assert_eq!(state.committed_revision(), 0);

        let _ = state.handle_left_down((0, 0), PointerModifiers::default());
        state.handle_move((10, 10));
        assert_eq!(state.committed_revision(), 0);

        state.handle_left_up((20, 20));
        assert_eq!(state.committed_revision(), 1);

        let _ = state.handle_key_event(KeyEvent {
            key: KeyCode::U,
            modifiers: KeyModifiers::default(),
        });
        assert_eq!(state.committed_revision(), 2);

        let _ = state.handle_key_event(KeyEvent {
            key: KeyCode::KeyR,
            modifiers: KeyModifiers {
                ctrl: true,
                shift: false,
                alt: false,
                win: false,
            },
        });
        assert_eq!(state.committed_revision(), 3);
    }
}
