#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CanvasStroke {
    pub points: Vec<(i32, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CanvasModel {
    pub strokes: Vec<CanvasStroke>,
}
