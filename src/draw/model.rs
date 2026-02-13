#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Line,
    Rect,
    Ellipse,
    Eraser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub width: f32,
    pub color: Color,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 2.0,
            color: Color::rgba(255, 255, 255, 255),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillStyle {
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ObjectStyle {
    pub stroke: StrokeStyle,
    pub fill: Option<FillStyle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Geometry {
    Pen { points: Vec<(i32, i32)> },
    Line { start: (i32, i32), end: (i32, i32) },
    Rect { start: (i32, i32), end: (i32, i32) },
    Ellipse { start: (i32, i32), end: (i32, i32) },
    Eraser { points: Vec<(i32, i32)> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawObject {
    pub tool: Tool,
    pub style: ObjectStyle,
    pub geometry: Geometry,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CanvasModel {
    pub objects: Vec<DrawObject>,
}
