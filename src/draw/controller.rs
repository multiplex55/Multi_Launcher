use crate::draw::messages::{ExitReason, MainToOverlay, OverlayLifecycleEvent, OverlayToMain};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerLifecycle {
    Starting,
    Active,
    ExitRequested,
    Exited,
}

pub struct OverlayController {
    main_to_overlay_rx: Receiver<MainToOverlay>,
    overlay_to_main_tx: Sender<OverlayToMain>,
    lifecycle: ControllerLifecycle,
    exit_reason: Option<ExitReason>,
}

impl OverlayController {
    pub fn new(
        main_to_overlay_rx: Receiver<MainToOverlay>,
        overlay_to_main_tx: Sender<OverlayToMain>,
    ) -> Self {
        Self {
            main_to_overlay_rx,
            overlay_to_main_tx,
            lifecycle: ControllerLifecycle::Starting,
            exit_reason: None,
        }
    }

    pub fn lifecycle(&self) -> ControllerLifecycle {
        self.lifecycle
    }

    pub fn exit_reason(&self) -> Option<ExitReason> {
        self.exit_reason.clone()
    }

    pub fn pump_runtime_messages<F, C>(&mut self, mut on_update_settings: F, mut on_command: C)
    where
        F: FnMut(),
        C: FnMut(crate::draw::messages::OverlayCommand),
    {
        loop {
            match self.main_to_overlay_rx.try_recv() {
                Ok(MainToOverlay::Start) => {
                    self.lifecycle = ControllerLifecycle::Active;
                    let _ = self.overlay_to_main_tx.send(OverlayToMain::LifecycleEvent(
                        OverlayLifecycleEvent::Started,
                    ));
                }
                Ok(MainToOverlay::UpdateSettings) => {
                    on_update_settings();
                    let _ = self.overlay_to_main_tx.send(OverlayToMain::LifecycleEvent(
                        OverlayLifecycleEvent::SettingsApplied,
                    ));
                }
                Ok(MainToOverlay::DispatchCommand { command }) => {
                    on_command(command);
                }
                Ok(MainToOverlay::RequestExit { reason }) => {
                    self.lifecycle = ControllerLifecycle::ExitRequested;
                    self.exit_reason = Some(reason.clone());
                    let _ = self.overlay_to_main_tx.send(OverlayToMain::LifecycleEvent(
                        OverlayLifecycleEvent::ExitRequested { reason },
                    ));
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.lifecycle = ControllerLifecycle::ExitRequested;
                    self.exit_reason = Some(ExitReason::OverlayFailure);
                    break;
                }
            }
        }
    }

    pub fn mark_exited(&mut self) {
        self.lifecycle = ControllerLifecycle::Exited;
    }
}

#[cfg(test)]
mod tests {
    use super::{ControllerLifecycle, OverlayController};
    use crate::draw::messages::{ExitReason, MainToOverlay, OverlayLifecycleEvent, OverlayToMain};

    #[test]
    fn request_exit_message_transitions_controller_state() {
        let (main_tx, main_rx) = std::sync::mpsc::channel::<MainToOverlay>();
        let (overlay_tx, overlay_rx) = std::sync::mpsc::channel::<OverlayToMain>();
        let mut controller = OverlayController::new(main_rx, overlay_tx);

        main_tx
            .send(MainToOverlay::RequestExit {
                reason: ExitReason::UserRequest,
            })
            .expect("request exit send");

        controller.pump_runtime_messages(|| {}, |_| {});
        assert_eq!(controller.lifecycle(), ControllerLifecycle::ExitRequested);
        assert_eq!(controller.exit_reason(), Some(ExitReason::UserRequest));
        assert_eq!(
            overlay_rx.recv().expect("exit ack"),
            OverlayToMain::LifecycleEvent(OverlayLifecycleEvent::ExitRequested {
                reason: ExitReason::UserRequest
            })
        );
    }

    #[test]
    fn update_settings_message_invokes_controller_update_path() {
        let (main_tx, main_rx) = std::sync::mpsc::channel::<MainToOverlay>();
        let (overlay_tx, overlay_rx) = std::sync::mpsc::channel::<OverlayToMain>();
        let mut controller = OverlayController::new(main_rx, overlay_tx);
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();

        main_tx
            .send(MainToOverlay::UpdateSettings)
            .expect("update settings send");

        controller.pump_runtime_messages(
            move || called_clone.store(true, std::sync::atomic::Ordering::SeqCst),
            |_| {},
        );

        assert!(called.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            overlay_rx.recv().expect("settings ack"),
            OverlayToMain::LifecycleEvent(OverlayLifecycleEvent::SettingsApplied)
        );
    }
}
