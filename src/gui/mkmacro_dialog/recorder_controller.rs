//! Recorder orchestration and optional floating-controller boundary.
use crate::mkmacro::{HookCommand, RuntimeSnapshot};
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

pub struct RecorderController<V: RecorderControllerView> {
    pub view: V,
    pub status: RecorderStatusSnapshot,
    pub show_floating: bool,
}
impl<V: RecorderControllerView> RecorderController<V> {
    pub fn new(view: V) -> Self {
        Self {
            view,
            status: Default::default(),
            show_floating: false,
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
            HookCommand::Fence => {}
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
}

/// Hotkeys/control windows are excluded by explicit commands rather than blanket input suppression;
/// unrelated physical events continue through the hook channel.
pub fn is_control_hotkey(vk: u32, configured: &[u32]) -> bool {
    configured.contains(&vk)
}
