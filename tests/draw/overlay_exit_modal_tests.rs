use multi_launcher::draw::controller::{ControllerLifecycle, OverlayController};
use multi_launcher::draw::messages::{
    ExitDialogMode, ExitReason, MainToOverlay, OverlayLifecycleEvent, OverlayToMain,
};

#[test]
fn request_exit_enters_modal_mode_without_immediate_termination() {
    let (main_tx, main_rx) = std::sync::mpsc::channel::<MainToOverlay>();
    let (overlay_tx, overlay_rx) = std::sync::mpsc::channel::<OverlayToMain>();
    let mut controller = OverlayController::new(main_rx, overlay_tx);

    main_tx
        .send(MainToOverlay::RequestExit {
            reason: ExitReason::UserRequest,
        })
        .expect("send request exit");

    controller.pump_runtime_messages(|| {}, |_| {});

    assert_eq!(controller.lifecycle(), ControllerLifecycle::Starting);
    assert_eq!(controller.exit_reason(), None);
    assert_eq!(controller.exit_dialog_mode(), ExitDialogMode::PromptVisible);
    assert_eq!(
        overlay_rx.recv().expect("lifecycle event"),
        OverlayToMain::LifecycleEvent(OverlayLifecycleEvent::ExitRequested {
            reason: ExitReason::UserRequest,
        })
    );
}
