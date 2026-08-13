//! Recorder orchestration and optional floating-controller boundary.
use crate::mkmacro::{
    HookCommand, MkMacroDocument, MkStep, RecordedStep, RuntimeSnapshot, to_macro_steps,
};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderState {
    Recording,
    Paused,
    Stopped,
}
#[derive(Debug, Clone)]
pub struct RecorderStatusSnapshot {
    pub state: RecorderState,
    pub elapsed: Duration,
    pub raw_event_count: u64,
    pub produced_step_count: usize,
}
impl Default for RecorderStatusSnapshot {
    fn default() -> Self {
        Self {
            state: RecorderState::Stopped,
            elapsed: Duration::ZERO,
            raw_event_count: 0,
            produced_step_count: 0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerAction {
    Record,
    Pause,
    Resume,
    Stop,
    Play,
    StopPlayback,
}

/// Isolates secondary viewport/window creation from recorder state and commands.
pub trait RecorderControllerView {
    fn set_visible(&mut self, visible: bool);
    fn show(
        &mut self,
        recorder: &RecorderStatusSnapshot,
        runtime: Option<&RuntimeSnapshot>,
    ) -> Option<ControllerAction>;
    fn exclude_from_capture(&mut self) {}
}

#[derive(Debug, Default)]
pub struct DraftTransaction {
    pub macro_id: u64,
    pub inserted: Vec<MkStep>,
}
impl DraftTransaction {
    pub fn apply(self, draft: &mut MkMacroDocument) -> Result<(), String> {
        let m = draft
            .macros
            .iter_mut()
            .find(|m| m.id == self.macro_id)
            .ok_or("selected macro no longer exists")?;
        m.steps.extend(self.inserted);
        Ok(())
    }
    pub fn undo(&self, draft: &mut MkMacroDocument) {
        if let Some(m) = draft.macros.iter_mut().find(|m| m.id == self.macro_id) {
            let ids: std::collections::HashSet<_> = self.inserted.iter().map(|s| s.id).collect();
            m.steps.retain(|s| !ids.contains(&s.id));
        }
    }
}

pub struct RecorderController<V: RecorderControllerView> {
    pub view: V,
    pub status: RecorderStatusSnapshot,
    pub show_floating: bool,
    pub undo: Vec<DraftTransaction>,
}
impl<V: RecorderControllerView> RecorderController<V> {
    pub fn new(view: V) -> Self {
        Self {
            view,
            status: Default::default(),
            show_floating: false,
            undo: vec![],
        }
    }
    pub fn hook_command(&mut self, c: HookCommand) {
        match c {
            HookCommand::Start => {
                self.status = Default::default();
                self.status.state = RecorderState::Recording
            }
            HookCommand::Pause => self.status.state = RecorderState::Paused,
            HookCommand::Resume => self.status.state = RecorderState::Recording,
            HookCommand::Stop | HookCommand::Shutdown => self.status.state = RecorderState::Stopped,
        }
    }
    pub fn render(&mut self, runtime: Option<&RuntimeSnapshot>) -> Option<ControllerAction> {
        self.view.set_visible(self.show_floating);
        if self.show_floating {
            self.view.exclude_from_capture();
            self.view.show(&self.status, runtime)
        } else {
            None
        }
    }
    /// One in-memory, undoable insertion; persistence remains the dialog's explicit Save operation.
    pub fn insert_recording(
        &mut self,
        draft: &mut MkMacroDocument,
        macro_id: u64,
        recorded: &[RecordedStep],
    ) -> Result<(), String> {
        let next = draft
            .macros
            .iter()
            .flat_map(|m| m.steps.iter())
            .map(|s| s.id)
            .max()
            .unwrap_or(0);
        let transaction = DraftTransaction {
            macro_id,
            inserted: to_macro_steps(recorded, next),
        };
        transaction.clone_apply(draft)?;
        self.status.produced_step_count = transaction.inserted.len();
        self.undo.push(transaction);
        Ok(())
    }
}
impl DraftTransaction {
    fn clone_apply(&self, draft: &mut MkMacroDocument) -> Result<(), String> {
        let m = draft
            .macros
            .iter_mut()
            .find(|m| m.id == self.macro_id)
            .ok_or("selected macro no longer exists")?;
        m.steps.extend(self.inserted.clone());
        Ok(())
    }
}

/// Hotkeys/control windows are excluded by explicit commands rather than blanket input suppression;
/// unrelated physical events continue through the hook channel.
pub fn is_control_hotkey(vk: u32, configured: &[u32]) -> bool {
    configured.contains(&vk)
}
