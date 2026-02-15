use crate::draw::model::Tool;
use crate::draw::toolbar::ToolbarHitTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarIcon {
    Collapse,
    Expand,
    Pen,
    Line,
    Rect,
    Ellipse,
    Eraser,
    StrokeWidthDown,
    StrokeWidthUp,
    FillToggle,
    Undo,
    Redo,
    Save,
    Exit,
}

pub fn icon_for_hit_target(target: ToolbarHitTarget, collapsed: bool) -> Option<ToolbarIcon> {
    match target {
        ToolbarHitTarget::Header => None,
        ToolbarHitTarget::ToggleCollapse => Some(if collapsed {
            ToolbarIcon::Expand
        } else {
            ToolbarIcon::Collapse
        }),
        ToolbarHitTarget::Tool(tool) => Some(icon_for_tool(tool)),
        ToolbarHitTarget::QuickColor(_) | ToolbarHitTarget::FillColor(_) => None,
        ToolbarHitTarget::StrokeWidthDown => Some(ToolbarIcon::StrokeWidthDown),
        ToolbarHitTarget::StrokeWidthUp => Some(ToolbarIcon::StrokeWidthUp),
        ToolbarHitTarget::FillToggle => Some(ToolbarIcon::FillToggle),
        ToolbarHitTarget::Undo => Some(ToolbarIcon::Undo),
        ToolbarHitTarget::Redo => Some(ToolbarIcon::Redo),
        ToolbarHitTarget::Save => Some(ToolbarIcon::Save),
        ToolbarHitTarget::Exit => Some(ToolbarIcon::Exit),
    }
}

pub fn icon_for_tool(tool: Tool) -> ToolbarIcon {
    match tool {
        Tool::Pen => ToolbarIcon::Pen,
        Tool::Line => ToolbarIcon::Line,
        Tool::Rect => ToolbarIcon::Rect,
        Tool::Ellipse => ToolbarIcon::Ellipse,
        Tool::Eraser => ToolbarIcon::Eraser,
    }
}

pub fn icon_bitmap(icon: ToolbarIcon) -> &'static [&'static str] {
    match icon {
        ToolbarIcon::Collapse => &[
            "10001", "01010", "00100", "00100", "00100", "00100", "00100",
        ],
        ToolbarIcon::Expand => &[
            "00100", "00100", "00100", "00100", "00100", "01010", "10001",
        ],
        ToolbarIcon::Pen => &[
            "0000011", "0000110", "0001100", "0011000", "0110000", "1100000", "1000000",
        ],
        ToolbarIcon::Line => &[
            "1000001", "0100010", "0010100", "0001000", "0010100", "0100010", "1000001",
        ],
        ToolbarIcon::Rect => &[
            "1111111", "1000001", "1000001", "1000001", "1000001", "1000001", "1111111",
        ],
        ToolbarIcon::Ellipse => &[
            "0111110", "1100011", "1000001", "1000001", "1000001", "1100011", "0111110",
        ],
        ToolbarIcon::Eraser => &[
            "0011000", "0111100", "1111110", "1111110", "0111110", "0011110", "0001110",
        ],
        ToolbarIcon::StrokeWidthDown => &[
            "1111111", "0001000", "0011100", "0101010", "0001000", "0001000", "0001000",
        ],
        ToolbarIcon::StrokeWidthUp => &[
            "0001000", "0001000", "0001000", "0101010", "0011100", "0001000", "1111111",
        ],
        ToolbarIcon::FillToggle => &[
            "0011000", "0111100", "1111110", "1111110", "0111100", "0011000", "0001000",
        ],
        ToolbarIcon::Undo => &[
            "0011100", "0110010", "0100000", "1111110", "0100010", "0100010", "0011100",
        ],
        ToolbarIcon::Redo => &[
            "0011100", "0100110", "0000010", "0111111", "0100010", "0100010", "0011100",
        ],
        ToolbarIcon::Save => &[
            "1111111", "1000001", "1011101", "1011101", "1011101", "1000001", "1111111",
        ],
        ToolbarIcon::Exit => &[
            "1000001", "0100010", "0010100", "0001000", "0010100", "0100010", "1000001",
        ],
    }
}
