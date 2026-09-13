use serde::{Deserialize, Serialize};

use super::geometry::{DesktopPoint, DesktopRect};
use crate::mkmacro::screen::ScreenRect;

/// An unpremultiplied sRGBA color suitable for persistence and native drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RgbaColor(pub [u8; 4]);

impl RgbaColor {
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    pub const WHITE: Self = Self([255, 255, 255, 255]);
    pub const RED: Self = Self([255, 0, 0, 255]);
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self([red, green, blue, alpha])
    }

    pub const fn channels(self) -> [u8; 4] {
        self.0
    }
}

impl Default for RgbaColor {
    fn default() -> Self {
        Self::RED
    }
}

/// Stable identity for a permanent annotation object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnnotationId(pub u64);

/// A sampled point in a freehand stroke, in signed desktop coordinates.
///
/// Pressure and timestamp are optional so today's mouse input does not need to
/// invent values while future pointer input can preserve them without changing
/// the document shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokePoint {
    pub position: DesktopPoint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_millis: Option<u64>,
    /// Suppresses the segment from the preceding sample to this sample.
    #[serde(default)]
    pub break_before: bool,
}

impl StrokePoint {
    pub const fn mouse(position: DesktopPoint) -> Self {
        Self {
            position,
            pressure: None,
            timestamp_millis: None,
            break_before: false,
        }
    }

    pub const fn mouse_break(position: DesktopPoint) -> Self {
        Self {
            position,
            pressure: None,
            timestamp_millis: None,
            break_before: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StrokeSegment {
    pub(crate) from: DesktopPoint,
    pub(crate) to: DesktopPoint,
}

/// An in-progress or committed freehand stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub points: Vec<StrokePoint>,
    pub color: RgbaColor,
    pub thickness: f32,
}

impl Stroke {
    pub fn new(color: RgbaColor, thickness: f32) -> Self {
        Self {
            points: Vec::new(),
            color,
            thickness,
        }
    }

    pub fn push_mouse_point(&mut self, position: DesktopPoint) {
        self.points.push(StrokePoint::mouse(position));
    }

    pub(crate) fn push_mouse_break(&mut self, position: DesktopPoint) {
        self.points.push(StrokePoint::mouse_break(position));
    }

    /// Iterates the drawable subpaths in this stroke. A break marker belongs
    /// to the current point and suppresses only its incoming segment.
    pub(crate) fn segments(&self) -> impl Iterator<Item = StrokeSegment> + '_ {
        self.points.windows(2).filter_map(|pair| {
            (!pair[1].break_before).then_some(StrokeSegment {
                from: pair[0].position,
                to: pair[1].position,
            })
        })
    }
}

/// Shared appearance for line and shape primitives.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShapeStyle {
    pub color: RgbaColor,
    pub thickness: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineAnnotation {
    pub from: DesktopPoint,
    pub to: DesktopPoint,
    pub style: ShapeStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArrowAnnotation {
    pub from: DesktopPoint,
    pub to: DesktopPoint,
    pub style: ShapeStyle,
}

/// Opposing corners are retained so an active drag does not need to normalize
/// or lose its direction before commit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShapeAnnotation {
    pub from: DesktopPoint,
    pub to: DesktopPoint,
    pub style: ShapeStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextAnnotation {
    pub text: String,
    pub bounds: DesktopRect,
    pub color: RgbaColor,
    pub font_size: f32,
}

/// Geometry and appearance of an annotation, independent of document identity.
/// This makes the same values suitable for an unfinished active operation and
/// for assignment of a stable id at commit time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Pen(Stroke),
    Highlighter(Stroke),
    Line(LineAnnotation),
    Arrow(ArrowAnnotation),
    Rectangle(ShapeAnnotation),
    Ellipse(ShapeAnnotation),
    Text(TextAnnotation),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationObject {
    pub id: AnnotationId,
    pub kind: AnnotationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenDrawTool {
    Pen,
    Highlighter,
    StraightLine,
    Arrow,
    Rectangle,
    Ellipse,
    Text,
    Eraser,
    FadingInk,
    Eyedropper,
}

impl Default for ScreenDrawTool {
    fn default() -> Self {
        Self::Pen
    }
}

/// The selected canvas source. Ghost mode is intentionally not a background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasBackground {
    FrozenDesktop,
    White,
    Black,
    Solid(RgbaColor),
}

/// The pixels selected for an export, independently of how they are painted
/// and where they are delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    FullDesktop,
    Region(ScreenRect),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportBackground {
    FrozenDesktop,
    Transparent,
    White,
    Black,
    Solid(RgbaColor),
}

impl Default for ExportBackground {
    fn default() -> Self {
        Self::FrozenDesktop
    }
}

impl From<CanvasBackground> for ExportBackground {
    fn from(value: CanvasBackground) -> Self {
        match value {
            CanvasBackground::FrozenDesktop => Self::FrozenDesktop,
            CanvasBackground::White => Self::White,
            CanvasBackground::Black => Self::Black,
            CanvasBackground::Solid(color) => Self::Solid(color),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportDestination {
    Clipboard,
    File,
    ScreenshotEditor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportRequest {
    pub scope: ExportScope,
    pub background: ExportBackground,
    pub destination: ExportDestination,
}

impl Default for CanvasBackground {
    fn default() -> Self {
        Self::FrozenDesktop
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarOrientation {
    Vertical,
    Horizontal,
}

impl Default for ToolbarOrientation {
    fn default() -> Self {
        Self::Vertical
    }
}

/// High-level session state mirrored outside the native worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenDrawMode {
    NoSession,
    AwaitingLauncherParking,
    Capturing,
    Drawing,
    Ghost,
    Finish,
    SelectingRegion,
    DisplayChanged,
    Failed,
}

impl Default for ScreenDrawMode {
    fn default() -> Self {
        Self::NoSession
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_enums_have_stable_snake_case_serde_names() {
        assert_eq!(
            serde_json::to_string(&ScreenDrawTool::FadingInk).unwrap(),
            r#""fading_ink""#
        );
        assert_eq!(
            serde_json::from_str::<ToolbarOrientation>(r#""horizontal""#).unwrap(),
            ToolbarOrientation::Horizontal
        );
        assert_eq!(
            serde_json::to_value(CanvasBackground::Solid(RgbaColor::rgba(1, 2, 3, 4))).unwrap(),
            serde_json::json!({ "solid": [1, 2, 3, 4] })
        );
    }

    #[test]
    fn defaults_start_with_pen_on_the_frozen_desktop() {
        assert_eq!(ScreenDrawTool::default(), ScreenDrawTool::Pen);
        assert_eq!(CanvasBackground::default(), CanvasBackground::FrozenDesktop);
        assert_eq!(ToolbarOrientation::default(), ToolbarOrientation::Vertical);
        assert_eq!(ScreenDrawMode::default(), ScreenDrawMode::NoSession);
    }

    #[test]
    fn legacy_stroke_points_default_to_an_unbroken_subpath() {
        let point: StrokePoint = serde_json::from_value(serde_json::json!({
            "position": { "x": -42, "y": 17 }
        }))
        .unwrap();
        assert_eq!(point, StrokePoint::mouse(DesktopPoint::new(-42, 17)));
        assert!(!point.break_before);
    }

    #[test]
    fn stroke_point_breaks_round_trip_and_suppress_only_the_incoming_segment() {
        let points = vec![
            StrokePoint::mouse(DesktopPoint::new(0, 0)),
            StrokePoint::mouse_break(DesktopPoint::new(10, 0)),
            StrokePoint::mouse(DesktopPoint::new(20, 0)),
        ];
        let restored: Vec<StrokePoint> =
            serde_json::from_str(&serde_json::to_string(&points).unwrap()).unwrap();
        assert_eq!(restored, points);

        let stroke = Stroke {
            points: restored,
            color: RgbaColor::RED,
            thickness: 2.0,
        };
        assert_eq!(
            stroke.segments().collect::<Vec<_>>(),
            vec![StrokeSegment {
                from: DesktopPoint::new(10, 0),
                to: DesktopPoint::new(20, 0),
            }]
        );
    }
}
