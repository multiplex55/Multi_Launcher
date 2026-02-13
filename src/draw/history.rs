use crate::draw::model::{CanvasModel, DrawObject};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawHistory {
    undo_stack: Vec<DrawObject>,
    redo_stack: Vec<DrawObject>,
}

impl DrawHistory {
    pub fn commit(&mut self, object: DrawObject) {
        self.undo_stack.push(object);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> Option<DrawObject> {
        let object = self.undo_stack.pop()?;
        self.redo_stack.push(object.clone());
        Some(object)
    }

    pub fn redo(&mut self) -> Option<DrawObject> {
        let object = self.redo_stack.pop()?;
        self.undo_stack.push(object.clone());
        Some(object)
    }

    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    pub fn canvas(&self) -> CanvasModel {
        CanvasModel {
            objects: self.undo_stack.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::model::{Geometry, ObjectStyle, Tool};

    fn sample_object(id: i32) -> DrawObject {
        DrawObject {
            tool: Tool::Line,
            style: ObjectStyle::default(),
            geometry: Geometry::Line {
                start: (id, id),
                end: (id + 1, id + 1),
            },
        }
    }

    #[test]
    fn new_commit_clears_redo_stack() {
        let mut history = DrawHistory::default();
        history.commit(sample_object(0));
        let _ = history.undo();
        assert_eq!(history.redo_len(), 1);

        history.commit(sample_object(1));
        assert_eq!(history.redo_len(), 0);
        assert_eq!(history.undo_len(), 1);
    }

    #[test]
    fn undo_redo_roundtrip_objects() {
        let mut history = DrawHistory::default();
        let first = sample_object(1);
        let second = sample_object(2);

        history.commit(first.clone());
        history.commit(second.clone());

        assert_eq!(history.undo(), Some(second.clone()));
        assert_eq!(history.undo(), Some(first.clone()));
        assert_eq!(history.undo(), None);

        assert_eq!(history.redo(), Some(first));
        assert_eq!(history.redo(), Some(second));
        assert_eq!(history.redo(), None);
    }
}
