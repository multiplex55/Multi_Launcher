use std::time::Duration;

use super::{
    AnnotationId, AnnotationKind, AnnotationObject, DesktopPoint, Stroke, annotation_hit_test,
    stroke_hit_test,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentError {
    AnnotationIdExhausted,
    TransientStrokeIdExhausted,
}

/// Ordered permanent annotations and their reversible edit history.
///
/// History commands transfer owned objects between the document and the
/// command that represents an edit. Pointer sampling therefore never requires
/// a document snapshot, and undo/redo only moves the objects affected by the
/// transaction.
#[derive(Debug)]
pub struct AnnotationDocument {
    objects: Vec<AnnotationObject>,
    next_id: u64,
    undo: Vec<HistoryEdit>,
    redo: Vec<HistoryEdit>,
    annotations_visible: bool,
}

impl Default for AnnotationDocument {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
            next_id: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            annotations_visible: true,
        }
    }
}

impl AnnotationDocument {
    pub fn objects(&self) -> &[AnnotationObject] {
        &self.objects
    }

    pub fn annotations_visible(&self) -> bool {
        self.annotations_visible
    }

    pub fn set_annotations_visible(&mut self, visible: bool) {
        self.annotations_visible = visible;
    }

    /// Reveals hidden annotations when an input operation begins. Visibility is
    /// presentation state and deliberately does not enter document history.
    /// Returns whether the caller needs to refresh presentation.
    pub fn begin_drawing(&mut self) -> bool {
        let changed = !self.annotations_visible;
        self.annotations_visible = true;
        changed
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn commit(&mut self, kind: AnnotationKind) -> Result<AnnotationId, DocumentError> {
        let id = self.allocate_id()?;
        let index = self.objects.len();
        self.objects.push(AnnotationObject { id, kind });
        self.record_edit(HistoryEdit::Add {
            id,
            index,
            stored: None,
        });
        Ok(id)
    }

    /// Begins a grouped eraser operation. Individual samples mutate the live
    /// document, but the completed drag is recorded as one history command.
    pub fn begin_eraser_drag(&self) -> EraserDrag {
        EraserDrag {
            starting_order: self.objects.iter().map(|object| object.id).collect(),
            removed: Vec::new(),
        }
    }

    pub fn erase_at(
        &mut self,
        drag: &mut EraserDrag,
        point: DesktopPoint,
        tolerance: f32,
    ) -> usize {
        let mut removed_now = 0;
        let mut index = self.objects.len();
        while index > 0 {
            index -= 1;
            if annotation_hit_test(&self.objects[index], point, tolerance) {
                let object = self.objects.remove(index);
                let original_index = drag
                    .starting_order
                    .iter()
                    .position(|id| *id == object.id)
                    .unwrap_or(index);
                drag.removed.push(StoredObject {
                    index: original_index,
                    id: object.id,
                    stored: Some(object),
                });
                removed_now += 1;
            }
        }
        removed_now
    }

    pub fn commit_eraser_drag(&mut self, mut drag: EraserDrag) -> usize {
        let removed = drag.removed.len();
        if removed == 0 {
            return 0;
        }
        drag.removed.sort_unstable_by_key(|entry| entry.index);
        self.record_edit(HistoryEdit::Remove {
            objects: drag.removed,
        });
        removed
    }

    /// Restores objects removed by an unfinished eraser drag without touching
    /// undo/redo state.
    pub fn cancel_eraser_drag(&mut self, mut drag: EraserDrag) -> usize {
        let restored = drag.removed.len();
        restore_removed(&mut self.objects, &mut drag.removed);
        restored
    }

    pub fn clear_all(&mut self) -> usize {
        if self.objects.is_empty() {
            return 0;
        }
        let count = self.objects.len();
        let objects = std::mem::take(&mut self.objects);
        self.record_edit(HistoryEdit::Clear { stored: objects });
        count
    }

    pub fn undo(&mut self) -> bool {
        let Some(mut edit) = self.undo.pop() else {
            return false;
        };
        edit.undo(&mut self.objects);
        self.redo.push(edit);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(mut edit) = self.redo.pop() else {
            return false;
        };
        edit.redo(&mut self.objects);
        self.undo.push(edit);
        true
    }

    fn allocate_id(&mut self) -> Result<AnnotationId, DocumentError> {
        let id = AnnotationId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(DocumentError::AnnotationIdExhausted)?;
        Ok(id)
    }

    fn record_edit(&mut self, edit: HistoryEdit) {
        self.undo.push(edit);
        self.redo.clear();
    }
}

#[derive(Debug)]
pub struct EraserDrag {
    starting_order: Vec<AnnotationId>,
    removed: Vec<StoredObject>,
}

#[derive(Debug)]
enum HistoryEdit {
    Add {
        id: AnnotationId,
        index: usize,
        stored: Option<AnnotationObject>,
    },
    Remove {
        objects: Vec<StoredObject>,
    },
    Clear {
        stored: Vec<AnnotationObject>,
    },
}

impl HistoryEdit {
    fn undo(&mut self, document: &mut Vec<AnnotationObject>) {
        match self {
            Self::Add { id, stored, .. } => {
                if let Some(position) = document.iter().position(|object| object.id == *id) {
                    *stored = Some(document.remove(position));
                }
            }
            Self::Remove { objects } => restore_removed(document, objects),
            Self::Clear { stored } => std::mem::swap(document, stored),
        }
    }

    fn redo(&mut self, document: &mut Vec<AnnotationObject>) {
        match self {
            Self::Add { index, stored, .. } => {
                if let Some(object) = stored.take() {
                    document.insert((*index).min(document.len()), object);
                }
            }
            Self::Remove { objects } => {
                for entry in objects.iter_mut() {
                    if let Some(position) = document.iter().position(|object| object.id == entry.id)
                    {
                        entry.stored = Some(document.remove(position));
                    }
                }
            }
            Self::Clear { stored } => std::mem::swap(document, stored),
        }
    }
}

#[derive(Debug)]
struct StoredObject {
    index: usize,
    id: AnnotationId,
    stored: Option<AnnotationObject>,
}

fn restore_removed(document: &mut Vec<AnnotationObject>, objects: &mut [StoredObject]) {
    objects.sort_unstable_by_key(|entry| entry.index);
    for entry in objects {
        if let Some(object) = entry.stored.take() {
            document.insert(entry.index.min(document.len()), object);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransientStrokeId(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub struct TransientStroke {
    pub id: TransientStrokeId,
    pub stroke: Stroke,
    pub created_at: Duration,
    pub expires_at: Duration,
}

/// Fading ink has its own lifetime and never participates in permanent undo or
/// redo. The worker supplies monotonic timestamps, which makes timer behavior
/// deterministic in tests and avoids requiring a timer while this is empty.
#[derive(Debug, Default)]
pub struct TransientInk {
    strokes: Vec<TransientStroke>,
    next_id: u64,
}

impl TransientInk {
    pub fn strokes(&self) -> &[TransientStroke] {
        &self.strokes
    }

    pub(crate) fn snapshot(&self) -> Vec<TransientStroke> {
        self.strokes.clone()
    }

    pub fn render_strokes_at(&self, now: Duration) -> Vec<Stroke> {
        fading_strokes_at(&self.strokes, now)
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }

    /// The worker's timer seam. `None` means there is no fade timer to arm.
    pub fn next_deadline(&self) -> Option<Duration> {
        self.strokes.iter().map(|stroke| stroke.expires_at).min()
    }

    pub fn add(
        &mut self,
        stroke: Stroke,
        now: Duration,
        lifetime: Duration,
    ) -> Result<TransientStrokeId, DocumentError> {
        let id = TransientStrokeId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(DocumentError::TransientStrokeIdExhausted)?;
        self.strokes.push(TransientStroke {
            id,
            stroke,
            created_at: now,
            expires_at: now.saturating_add(lifetime),
        });
        Ok(id)
    }

    pub fn prune(&mut self, now: Duration) -> usize {
        let before = self.strokes.len();
        self.strokes.retain(|stroke| stroke.expires_at > now);
        before - self.strokes.len()
    }

    pub fn clear(&mut self) -> usize {
        let removed = self.strokes.len();
        self.strokes.clear();
        removed
    }

    pub fn visible_at(&self, now: Duration) -> impl Iterator<Item = &TransientStroke> {
        self.strokes
            .iter()
            .filter(move |stroke| stroke.expires_at > now)
    }

    /// Removes only fading strokes that are still alive at the supplied clock
    /// value. This is intentionally not recorded in permanent history.
    pub fn erase_at(&mut self, now: Duration, point: DesktopPoint, tolerance: f32) -> usize {
        self.prune(now);
        let before = self.strokes.len();
        self.strokes
            .retain(|entry| !stroke_hit_test(&entry.stroke, point, tolerance));
        before - self.strokes.len()
    }
}

/// Produces the transient render layer at an injected monotonic time without
/// pruning or otherwise mutating authoring state.
pub(crate) fn fading_strokes_at(strokes: &[TransientStroke], now: Duration) -> Vec<Stroke> {
    strokes
        .iter()
        .filter(|entry| entry.expires_at > now)
        .map(|entry| {
            let mut stroke = entry.stroke.clone();
            let lifetime = entry.expires_at.saturating_sub(entry.created_at);
            let remaining = entry.expires_at.saturating_sub(now);
            let opacity = if lifetime.is_zero() {
                0.0
            } else {
                remaining.as_secs_f32() / lifetime.as_secs_f32()
            };
            let [r, g, b, alpha] = stroke.color.channels();
            stroke.color = super::RgbaColor::rgba(
                r,
                g,
                b,
                (f32::from(alpha) * opacity.clamp(0.0, 1.0)).round() as u8,
            );
            stroke
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_draw::{LineAnnotation, RgbaColor, ShapeStyle, StrokePoint, TextAnnotation};

    fn line(from_x: i32, to_x: i32) -> AnnotationKind {
        AnnotationKind::Line(LineAnnotation {
            from: DesktopPoint::new(from_x, 0),
            to: DesktopPoint::new(to_x, 0),
            style: ShapeStyle {
                color: RgbaColor::RED,
                thickness: 2.0,
            },
        })
    }

    fn stroke(from_x: i32, to_x: i32) -> Stroke {
        Stroke {
            points: vec![
                StrokePoint::mouse(DesktopPoint::new(from_x, 0)),
                StrokePoint::mouse(DesktopPoint::new(to_x, 0)),
            ],
            color: RgbaColor::RED,
            thickness: 4.0,
        }
    }

    fn ids(document: &AnnotationDocument) -> Vec<AnnotationId> {
        document.objects().iter().map(|object| object.id).collect()
    }

    #[test]
    fn add_undo_redo_preserves_ids_and_order_and_new_edit_clears_redo() {
        let mut document = AnnotationDocument::default();
        let first = document.commit(line(-10, 10)).unwrap();
        let second = document.commit(line(20, 30)).unwrap();
        assert_eq!(ids(&document), vec![first, second]);

        assert!(document.undo());
        assert_eq!(ids(&document), vec![first]);
        assert!(document.redo());
        assert_eq!(ids(&document), vec![first, second]);
        assert!(document.undo());

        let third = document
            .commit(AnnotationKind::Text(TextAnnotation {
                text: "replacement".into(),
                bounds: super::super::DesktopRect::new(-30, -30, 20, 10),
                color: RgbaColor::WHITE,
                font_size: 12.0,
            }))
            .unwrap();
        assert_eq!(ids(&document), vec![first, third]);
        assert!(!document.can_redo());
        assert!(!document.redo());
    }

    #[test]
    fn eraser_drag_is_one_transaction_and_restores_exact_z_order() {
        let mut document = AnnotationDocument::default();
        let first = document.commit(line(-30, -20)).unwrap();
        let second = document.commit(line(0, 10)).unwrap();
        let third = document.commit(line(20, 30)).unwrap();
        let fourth = document.commit(line(40, 50)).unwrap();

        let mut drag = document.begin_eraser_drag();
        assert_eq!(
            document.erase_at(&mut drag, DesktopPoint::new(25, 0), 1.0),
            1
        );
        assert_eq!(
            document.erase_at(&mut drag, DesktopPoint::new(5, 0), 1.0),
            1
        );
        assert_eq!(document.commit_eraser_drag(drag), 2);
        assert_eq!(ids(&document), vec![first, fourth]);

        assert!(document.undo());
        assert_eq!(ids(&document), vec![first, second, third, fourth]);
        assert!(document.redo());
        assert_eq!(ids(&document), vec![first, fourth]);
        assert!(document.undo());
        assert_eq!(ids(&document), vec![first, second, third, fourth]);
    }

    #[test]
    fn empty_operations_are_noops_and_cancelled_eraser_restores_without_history() {
        let mut document = AnnotationDocument::default();
        assert_eq!(document.clear_all(), 0);
        assert!(!document.can_undo());

        let first = document.commit(line(-5, 5)).unwrap();
        let mut missed = document.begin_eraser_drag();
        assert_eq!(
            document.erase_at(&mut missed, DesktopPoint::new(100, 100), 0.0),
            0
        );
        assert_eq!(document.commit_eraser_drag(missed), 0);
        assert!(document.undo());
        assert!(document.objects().is_empty());
        assert!(document.redo());

        let mut cancelled = document.begin_eraser_drag();
        assert_eq!(
            document.erase_at(&mut cancelled, DesktopPoint::new(0, 0), 0.0),
            1
        );
        assert_eq!(document.cancel_eraser_drag(cancelled), 1);
        assert_eq!(ids(&document), vec![first]);
        assert!(document.undo());
        assert!(document.objects().is_empty());
    }

    #[test]
    fn clear_all_is_one_undoable_transaction() {
        let mut document = AnnotationDocument::default();
        let first = document.commit(line(-10, 0)).unwrap();
        let second = document.commit(line(10, 20)).unwrap();
        assert_eq!(document.clear_all(), 2);
        assert!(document.objects().is_empty());

        assert!(document.undo());
        assert_eq!(ids(&document), vec![first, second]);
        assert!(document.redo());
        assert!(document.objects().is_empty());
    }

    #[test]
    fn beginning_to_draw_reveals_without_modifying_history() {
        let mut document = AnnotationDocument::default();
        document.set_annotations_visible(false);
        assert!(document.begin_drawing());
        assert!(document.annotations_visible());
        assert!(!document.begin_drawing());
        assert!(!document.can_undo());
    }

    #[test]
    fn fading_ink_uses_injected_time_and_never_touches_permanent_history() {
        let mut permanent = AnnotationDocument::default();
        let mut fading = TransientInk::default();
        let start = Duration::from_millis(1_000);
        let first = fading
            .add(stroke(-10, 10), start, Duration::from_secs(3))
            .unwrap();
        fading
            .add(
                stroke(20, 30),
                start + Duration::from_secs(1),
                Duration::from_secs(3),
            )
            .unwrap();

        assert_eq!(fading.next_deadline(), Some(Duration::from_secs(4)));
        assert_eq!(fading.visible_at(start).count(), 2);
        assert_eq!(fading.prune(Duration::from_millis(3_999)), 0);
        assert_eq!(fading.prune(Duration::from_secs(4)), 1);
        assert_eq!(
            fading.strokes().iter().any(|stroke| stroke.id == first),
            false
        );
        assert_eq!(fading.next_deadline(), Some(Duration::from_secs(5)));
        assert_eq!(fading.prune(Duration::from_secs(5)), 1);
        assert!(fading.is_empty());
        assert_eq!(fading.next_deadline(), None);
        assert!(!permanent.can_undo());
        assert!(!permanent.undo());
    }

    #[test]
    fn eraser_only_hits_live_transient_strokes() {
        let mut fading = TransientInk::default();
        fading
            .add(stroke(-10, 10), Duration::ZERO, Duration::from_secs(1))
            .unwrap();
        fading
            .add(stroke(90, 110), Duration::ZERO, Duration::from_secs(10))
            .unwrap();
        assert_eq!(
            fading.erase_at(Duration::from_secs(2), DesktopPoint::new(0, 0), 2.0),
            0
        );
        assert_eq!(fading.strokes().len(), 1);
        assert_eq!(
            fading.erase_at(Duration::from_secs(2), DesktopPoint::new(100, 0), 2.0),
            1
        );
        assert!(fading.is_empty());
    }
}
