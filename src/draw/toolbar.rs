use crate::draw::model::{Color, ObjectStyle, Tool};
use crate::draw::settings::DrawColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarCommand {
    SelectTool(Tool),
    SetStrokeWidth(u32),
    SetColor(Color),
    SetFillEnabled(bool),
    SetFillColor(Color),
    Undo,
    Redo,
    Save,
    Exit,
    ToggleVisibility,
    ToggleCollapsed,
    SetPosition { x: i32, y: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarPointerEvent {
    LeftDown,
    Move,
    LeftUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarState {
    pub visible: bool,
    pub collapsed: bool,
    pub focused: bool,
    pub position: (i32, i32),
    pub dragging: bool,
    pub drag_anchor: (i32, i32),
}

impl ToolbarState {
    pub fn new(visible: bool, collapsed: bool, position: (i32, i32)) -> Self {
        Self {
            visible,
            collapsed,
            focused: false,
            position,
            dragging: false,
            drag_anchor: (0, 0),
        }
    }

    pub fn apply_command(&mut self, command: ToolbarCommand) {
        reduce_toolbar_state(self, command);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl ToolbarRect {
    pub fn contains(self, point: (i32, i32)) -> bool {
        point.0 >= self.x
            && point.0 < self.x + self.w
            && point.1 >= self.y
            && point.1 < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarHitTarget {
    Header,
    ToggleCollapse,
    Tool(Tool),
    QuickColor(usize),
    StrokeWidthDown,
    StrokeWidthUp,
    FillToggle,
    FillColor(usize),
    Undo,
    Redo,
    Save,
    Exit,
}

#[derive(Debug, Clone)]
pub struct ToolbarLayout {
    pub panel: ToolbarRect,
    pub header: ToolbarRect,
    pub collapse_toggle: ToolbarRect,
    pub tool_rects: Vec<(Tool, ToolbarRect)>,
    pub quick_color_rects: Vec<(usize, ToolbarRect)>,
    pub fill_color_rects: Vec<(usize, ToolbarRect)>,
    pub width_down_rect: ToolbarRect,
    pub width_up_rect: ToolbarRect,
    pub fill_toggle_rect: ToolbarRect,
    pub undo_rect: ToolbarRect,
    pub redo_rect: ToolbarRect,
    pub save_rect: ToolbarRect,
    pub exit_rect: ToolbarRect,
}

impl ToolbarLayout {
    pub fn for_state(size: (u32, u32), state: &ToolbarState, quick_colors: usize) -> Option<Self> {
        if !state.visible || size.0 < 320 || size.1 < 150 {
            return None;
        }
        let panel = ToolbarRect {
            x: state.position.0.max(0),
            y: state.position.1.max(0),
            w: 430,
            h: if state.collapsed { 28 } else { 124 },
        };
        let header = ToolbarRect {
            x: panel.x,
            y: panel.y,
            w: panel.w,
            h: 24,
        };
        let collapse_toggle = ToolbarRect {
            x: panel.x + panel.w - 22,
            y: panel.y + 3,
            w: 18,
            h: 18,
        };

        let tools = [
            Tool::Pen,
            Tool::Line,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Eraser,
        ];
        let tool_rects = tools
            .into_iter()
            .enumerate()
            .map(|(idx, tool)| {
                (
                    tool,
                    ToolbarRect {
                        x: panel.x + 8 + (idx as i32 * 28),
                        y: panel.y + 30,
                        w: 24,
                        h: 22,
                    },
                )
            })
            .collect();

        let quick_color_rects = (0..quick_colors.min(8))
            .map(|idx| {
                (
                    idx,
                    ToolbarRect {
                        x: panel.x + 8 + (idx as i32 * 22),
                        y: panel.y + 58,
                        w: 18,
                        h: 18,
                    },
                )
            })
            .collect();

        let fill_color_rects = (0..quick_colors.min(4))
            .map(|idx| {
                (
                    idx,
                    ToolbarRect {
                        x: panel.x + 8 + (idx as i32 * 22),
                        y: panel.y + 82,
                        w: 18,
                        h: 18,
                    },
                )
            })
            .collect();

        Some(Self {
            panel,
            header,
            collapse_toggle,
            tool_rects,
            quick_color_rects,
            fill_color_rects,
            width_down_rect: ToolbarRect {
                x: panel.x + 168,
                y: panel.y + 30,
                w: 20,
                h: 22,
            },
            width_up_rect: ToolbarRect {
                x: panel.x + 190,
                y: panel.y + 30,
                w: 20,
                h: 22,
            },
            fill_toggle_rect: ToolbarRect {
                x: panel.x + 216,
                y: panel.y + 30,
                w: 36,
                h: 22,
            },
            undo_rect: ToolbarRect {
                x: panel.x + 258,
                y: panel.y + 30,
                w: 36,
                h: 22,
            },
            redo_rect: ToolbarRect {
                x: panel.x + 298,
                y: panel.y + 30,
                w: 36,
                h: 22,
            },
            save_rect: ToolbarRect {
                x: panel.x + 338,
                y: panel.y + 30,
                w: 36,
                h: 22,
            },
            exit_rect: ToolbarRect {
                x: panel.x + 378,
                y: panel.y + 30,
                w: 36,
                h: 22,
            },
        })
    }

    pub fn hit_test(&self, point: (i32, i32), collapsed: bool) -> Option<ToolbarHitTarget> {
        if !self.panel.contains(point) {
            return None;
        }
        if self.collapse_toggle.contains(point) {
            return Some(ToolbarHitTarget::ToggleCollapse);
        }
        if self.header.contains(point) {
            return Some(ToolbarHitTarget::Header);
        }
        if collapsed {
            return Some(ToolbarHitTarget::Header);
        }
        for (tool, rect) in &self.tool_rects {
            if rect.contains(point) {
                return Some(ToolbarHitTarget::Tool(*tool));
            }
        }
        for (idx, rect) in &self.quick_color_rects {
            if rect.contains(point) {
                return Some(ToolbarHitTarget::QuickColor(*idx));
            }
        }
        for (idx, rect) in &self.fill_color_rects {
            if rect.contains(point) {
                return Some(ToolbarHitTarget::FillColor(*idx));
            }
        }
        if self.width_down_rect.contains(point) {
            return Some(ToolbarHitTarget::StrokeWidthDown);
        }
        if self.width_up_rect.contains(point) {
            return Some(ToolbarHitTarget::StrokeWidthUp);
        }
        if self.fill_toggle_rect.contains(point) {
            return Some(ToolbarHitTarget::FillToggle);
        }
        if self.undo_rect.contains(point) {
            return Some(ToolbarHitTarget::Undo);
        }
        if self.redo_rect.contains(point) {
            return Some(ToolbarHitTarget::Redo);
        }
        if self.save_rect.contains(point) {
            return Some(ToolbarHitTarget::Save);
        }
        if self.exit_rect.contains(point) {
            return Some(ToolbarHitTarget::Exit);
        }
        Some(ToolbarHitTarget::Header)
    }
}

pub fn map_hit_to_command(
    target: ToolbarHitTarget,
    current_style: ObjectStyle,
    quick_colors: &[DrawColor],
) -> Option<ToolbarCommand> {
    match target {
        ToolbarHitTarget::Header => None,
        ToolbarHitTarget::ToggleCollapse => Some(ToolbarCommand::ToggleCollapsed),
        ToolbarHitTarget::Tool(tool) => Some(ToolbarCommand::SelectTool(tool)),
        ToolbarHitTarget::QuickColor(index) => quick_colors
            .get(index)
            .copied()
            .map(draw_color_to_model)
            .map(ToolbarCommand::SetColor),
        ToolbarHitTarget::StrokeWidthDown => Some(ToolbarCommand::SetStrokeWidth(
            current_style.stroke.width.saturating_sub(1).max(1),
        )),
        ToolbarHitTarget::StrokeWidthUp => Some(ToolbarCommand::SetStrokeWidth(
            current_style.stroke.width.saturating_add(1),
        )),
        ToolbarHitTarget::FillToggle => {
            Some(ToolbarCommand::SetFillEnabled(current_style.fill.is_none()))
        }
        ToolbarHitTarget::FillColor(index) => quick_colors
            .get(index)
            .copied()
            .map(draw_color_to_model)
            .map(ToolbarCommand::SetFillColor),
        ToolbarHitTarget::Undo => Some(ToolbarCommand::Undo),
        ToolbarHitTarget::Redo => Some(ToolbarCommand::Redo),
        ToolbarHitTarget::Save => Some(ToolbarCommand::Save),
        ToolbarHitTarget::Exit => Some(ToolbarCommand::Exit),
    }
}

pub fn reduce_toolbar_state(state: &mut ToolbarState, command: ToolbarCommand) {
    match command {
        ToolbarCommand::ToggleVisibility => state.visible = !state.visible,
        ToolbarCommand::ToggleCollapsed => state.collapsed = !state.collapsed,
        ToolbarCommand::SetPosition { x, y } => state.position = (x.max(0), y.max(0)),
        _ => {}
    }
}

fn draw_color_to_model(color: DrawColor) -> Color {
    Color::rgba(color.r, color.g, color.b, color.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_mapping_from_hit_targets_is_correct() {
        let style = ObjectStyle::default();
        let palette = [DrawColor::rgba(1, 2, 3, 255)];

        assert_eq!(
            map_hit_to_command(ToolbarHitTarget::Tool(Tool::Rect), style, &palette),
            Some(ToolbarCommand::SelectTool(Tool::Rect))
        );
        assert_eq!(
            map_hit_to_command(ToolbarHitTarget::QuickColor(0), style, &palette),
            Some(ToolbarCommand::SetColor(Color::rgba(1, 2, 3, 255)))
        );
        assert_eq!(
            map_hit_to_command(ToolbarHitTarget::Undo, style, &palette),
            Some(ToolbarCommand::Undo)
        );
    }

    #[test]
    fn reducer_updates_visibility_and_position_state() {
        let mut state = ToolbarState::new(true, false, (16, 16));

        reduce_toolbar_state(&mut state, ToolbarCommand::ToggleVisibility);
        assert!(!state.visible);

        reduce_toolbar_state(&mut state, ToolbarCommand::SetPosition { x: 200, y: 120 });
        assert_eq!(state.position, (200, 120));

        reduce_toolbar_state(&mut state, ToolbarCommand::ToggleCollapsed);
        assert!(state.collapsed);
    }

    #[test]
    fn visibility_toggle_can_be_driven_by_hotkey_or_ui_command() {
        let mut state = ToolbarState::new(true, false, (16, 16));

        reduce_toolbar_state(&mut state, ToolbarCommand::ToggleVisibility);
        assert!(!state.visible);

        reduce_toolbar_state(&mut state, ToolbarCommand::ToggleVisibility);
        assert!(state.visible);
    }
}
