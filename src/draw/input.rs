use crate::draw::history::DrawHistory;
use crate::draw::keyboard_hook::{map_key_event_to_command, KeyCommand, KeyEvent};
use crate::draw::messages::ExitReason;
use crate::draw::model::{DrawObject, Geometry, ObjectStyle, Tool};
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
    history: DrawHistory,
    committed_revision: u64,
    dirty_rect: Option<DirtyRect>,
    full_redraw_requested: bool,
}

impl DrawInputState {
    pub fn new(tool: Tool, style: ObjectStyle) -> Self {
        Self {
            tool,
            style,
            active_geometry: None,
            history: DrawHistory::default(),
            committed_revision: 0,
            dirty_rect: None,
            full_redraw_requested: true,
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
        self.request_full_redraw();
    }

    pub fn set_style(&mut self, style: ObjectStyle) {
        self.style = style;
        self.request_full_redraw();
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
        self.dirty_rect = None;
    }

    pub fn committed_canvas(&self) -> crate::draw::model::CanvasModel {
        self.history.canvas()
    }

    pub fn active_object(&self) -> Option<DrawObject> {
        self.active_geometry.as_ref().map(|geometry| DrawObject {
            tool: self.tool,
            style: self.style,
            geometry: geometry.clone(),
        })
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
                    if let Some(last) = points.last().copied() {
                        dirty = Some(DirtyRect::from_points(last, point, pad));
                    }
                    points.push(point);
                }
            }
            Some(Geometry::Line { start, end })
            | Some(Geometry::Rect { start, end })
            | Some(Geometry::Ellipse { start, end }) => {
                let previous = *end;
                let before = DirtyRect::from_points(*start, previous, pad);
                *end = point;
                let after = DirtyRect::from_points(*start, point, pad);
                dirty = Some(before.union(after));
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
            self.committed_revision = self.committed_revision.saturating_add(1);
        }
    }

    pub fn handle_key_event(&mut self, event: KeyEvent) -> Option<InputCommand> {
        match map_key_event_to_command(true, event, None) {
            Some(KeyCommand::Undo) => {
                if self.history.undo().is_some() {
                    self.request_full_redraw();
                    self.committed_revision = self.committed_revision.saturating_add(1);
                }
                Some(InputCommand::Undo)
            }
            Some(KeyCommand::Redo) => {
                if self.history.redo().is_some() {
                    self.request_full_redraw();
                    self.committed_revision = self.committed_revision.saturating_add(1);
                }
                Some(InputCommand::Redo)
            }
            Some(KeyCommand::RequestExit) => Some(InputCommand::RequestExit),
            Some(KeyCommand::ToggleToolbar) => None,
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
