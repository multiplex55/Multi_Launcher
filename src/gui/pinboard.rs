//! Launcher-owned orchestration for clipboard-free screenshot pin captures.
//!
//! The pinboard owns image memory. This module only coordinates the shared
//! rectangle selector, native exclusion acknowledgements, launcher parking,
//! the bounded capture worker, and the native pin-window owner.

use super::LauncherApp;
use crate::gui::mkmacro_dialog::visual_capture_workflow::SharedVisualOverlayController;
use crate::gui::mkmacro_dialog::visual_overlay::{
    OperationId, RectanglePurpose, VisualOverlayEvent,
};
use crate::gui::pinboard_windows::PinWindowPresenter;
use crate::launcher_parking::LauncherParkingTransaction;
use crate::mkmacro::{ExecutionDiagnostic, ScreenCaptureBackend, ScreenRect};
use crate::pinboard::{
    PinCaptureGeneration, PinCaptureIdentity, PinCaptureOperationId, PinVisibilityAck,
    PinWindowVisibilityCoordinator, Pinboard,
};
use crate::visibility::{RootFocusIntent, VisibilityRevision};
use image::RgbaImage;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

trait LauncherCaptureParking: Send {
    fn verify_capture_safe(&mut self) -> Result<bool, String>;
    fn restore(&mut self) -> Result<(), String>;
    fn repark(&mut self) -> Result<(), String>;
    fn commit_hidden(&mut self);
}

trait LauncherCaptureParkingFactory: Send + Sync {
    fn begin(
        &self,
        generation: PinCaptureGeneration,
        hwnd: usize,
        virtual_desktop: ScreenRect,
    ) -> Result<Box<dyn LauncherCaptureParking>, String>;
}

struct NativeLauncherCaptureParking(LauncherParkingTransaction<PinCaptureGeneration>);

impl LauncherCaptureParking for NativeLauncherCaptureParking {
    fn verify_capture_safe(&mut self) -> Result<bool, String> {
        self.0.verify()
    }

    fn restore(&mut self) -> Result<(), String> {
        self.0.restore()
    }

    fn repark(&mut self) -> Result<(), String> {
        self.0.repark_after_stale_restore(|| Ok(()))
    }

    fn commit_hidden(&mut self) {
        self.0.commit_hidden();
    }
}

#[derive(Default)]
struct NativeLauncherCaptureParkingFactory;

impl LauncherCaptureParkingFactory for NativeLauncherCaptureParkingFactory {
    fn begin(
        &self,
        generation: PinCaptureGeneration,
        hwnd: usize,
        virtual_desktop: ScreenRect,
    ) -> Result<Box<dyn LauncherCaptureParking>, String> {
        Ok(Box::new(NativeLauncherCaptureParking(
            LauncherParkingTransaction::begin(generation, hwnd, virtual_desktop)?,
        )))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinCapturePhase {
    AwaitRootAcknowledgement,
    AwaitPinHideAcknowledgement,
    Selecting,
    AwaitCancelledSelectionTerminal,
    Capturing,
    Restoring,
}

struct PinCaptureSession {
    identity: PinCaptureIdentity,
    overlay_operation_id: OperationId,
    hwnd: usize,
    hwnd_generation: u64,
    virtual_desktop: ScreenRect,
    owned_revision: u64,
    focus_intent: RootFocusIntent,
    invocation_id: Option<u64>,
    phase: PinCapturePhase,
    parking: Box<dyn LauncherCaptureParking>,
    hide_requested: bool,
    restore_requested: bool,
    overlay_dispatched: bool,
    overlay_terminal_consumed: bool,
    overlay_reservation_released: bool,
    cancel_sent: bool,
    capture_job: Option<CaptureJob>,
    selected_rect: Option<ScreenRect>,
    staged: bool,
    superseded: bool,
    cancelled: bool,
    error: Option<String>,
    error_reported: bool,
    cleanup_complete: bool,
}

struct CaptureJob {
    identity: PinCaptureIdentity,
    rect: ScreenRect,
    cancelled: Arc<AtomicBool>,
    join: JoinHandle<Result<RgbaImage, String>>,
}

impl CaptureJob {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn take_result(
        self,
        wait: bool,
    ) -> Option<(PinCaptureIdentity, ScreenRect, Result<RgbaImage, String>)> {
        if !wait && !self.join.is_finished() {
            return None;
        }
        Some(self.join_result())
    }

    fn join_result(self) -> (PinCaptureIdentity, ScreenRect, Result<RgbaImage, String>) {
        let result = self
            .join
            .join()
            .map_err(|_| "screen capture worker terminated unexpectedly".to_owned())
            .and_then(|result| result);
        (self.identity, self.rect, result)
    }

    fn is_finished(&self) -> bool {
        self.join.is_finished()
    }
}

#[derive(Clone, Copy)]
struct RootVisibilitySnapshot {
    revision: u64,
    visible: bool,
    focus_intent: RootFocusIntent,
    invocation_id: Option<u64>,
}

pub(super) struct PinboardCaptureLifecycle {
    board: Pinboard,
    overlay: SharedVisualOverlayController,
    capture_backend: Arc<dyn ScreenCaptureBackend>,
    parking_factory: Arc<dyn LauncherCaptureParkingFactory>,
    windows: PinWindowPresenter,
    pin_windows: Box<dyn PinWindowVisibilityCoordinator>,
    session: Option<PinCaptureSession>,
}

impl PinboardCaptureLifecycle {
    pub(super) fn new(
        overlay: SharedVisualOverlayController,
        capture_backend: Arc<dyn ScreenCaptureBackend>,
    ) -> Self {
        let windows = PinWindowPresenter::new();
        let pin_windows = windows.visibility_coordinator();
        Self {
            board: Pinboard::default(),
            overlay,
            capture_backend,
            parking_factory: Arc::new(NativeLauncherCaptureParkingFactory),
            windows,
            pin_windows,
            session: None,
        }
    }

    pub(super) fn is_busy(&self) -> bool {
        self.session.is_some() || self.board.active_capture().is_some()
    }

    pub(super) fn cancel(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.cancelled = true;
        if let Some(job) = session.capture_job.as_ref() {
            job.cancel();
        }
    }

    pub(super) fn pinboard(&self) -> &Pinboard {
        &self.board
    }

    pub(super) fn poll_pin_windows(
        &mut self,
        ctx: &eframe::egui::Context,
        root_identity: (usize, u64),
    ) {
        self.windows.set_root_native_identity(root_identity);
        self.windows.poll(ctx, &mut self.board);
    }

    pub(super) fn show_pin_windows(
        &mut self,
        ctx: &eframe::egui::Context,
        root_identity: (usize, u64),
    ) {
        self.windows.set_root_native_identity(root_identity);
        self.windows.show(ctx, &self.board);
    }

    pub(super) fn show_pin_list(&mut self, ctx: &eframe::egui::Context, open: &mut bool) {
        self.windows.show_pin_list(ctx, &mut self.board, open);
    }

    pub(super) fn pin_windows_need_bootstrap(&self) -> bool {
        self.windows.needs_viewport_bootstrap(&self.board)
    }

    pub(super) fn pin_windows_need_hidden_root_ui(&self) -> bool {
        self.windows.needs_root_ui_when_hidden(&self.board)
    }

    pub(super) fn close_pin(&mut self, id: crate::pinboard::PinId) -> bool {
        self.windows.close(id, &mut self.board)
    }

    pub(super) fn shutdown_pin_windows(&mut self) -> Result<(), String> {
        self.windows.shutdown()
    }

    pub(super) fn set_pin_window_coordinator(
        &mut self,
        coordinator: Box<dyn PinWindowVisibilityCoordinator>,
    ) -> Result<(), String> {
        if self.is_busy() {
            return Err("cannot replace pin visibility owner during a capture".into());
        }
        self.pin_windows = coordinator;
        Ok(())
    }

    /// Test and later native-window owner seam. Production M11-A starts with
    /// an actually empty native registry; replacement is rejected in-flight.
    #[cfg(test)]
    fn set_adapters_for_test(
        &mut self,
        parking_factory: Arc<dyn LauncherCaptureParkingFactory>,
        pin_windows: Box<dyn PinWindowVisibilityCoordinator>,
    ) {
        assert!(self.session.is_none());
        self.parking_factory = parking_factory;
        self.pin_windows = pin_windows;
    }

    pub(super) fn begin(
        &mut self,
        visible: bool,
        last_visible: bool,
        revision: u64,
        focus_intent: RootFocusIntent,
        invocation_id: Option<u64>,
        launcher_hwnd: Option<usize>,
        root_identity: (usize, u64),
    ) -> Result<bool, String> {
        if self.is_busy() {
            return Ok(false);
        }
        if !visible || !last_visible {
            return Err("Show the launcher before pinning a screen region".into());
        }
        let hwnd = launcher_hwnd.ok_or("Launcher HWND is unavailable for screenshot pinning")?;
        if hwnd == 0 || root_identity.0 != hwnd || root_identity.1 == 0 {
            return Err("ROOT native window identity is unavailable or changed".into());
        }
        let virtual_desktop = self
            .capture_backend
            .virtual_desktop()
            .map_err(|error| format!("could not resolve the virtual desktop: {error}"))?;
        virtual_desktop
            .validate_capture()
            .map_err(|error| format!("invalid virtual desktop geometry: {error}"))?;

        // Register the owner before reserving resources or starting parking so
        // generic editor polling cannot steal startup/terminal failures.
        let overlay_operation_id = self
            .overlay
            .reserve_pinboard_rectangle_pick()
            .map_err(|error| error.to_string())?;
        let operation_id = PinCaptureOperationId::new(overlay_operation_id).ok_or_else(|| {
            self.overlay
                .abort_reserved_pinboard_rectangle_pick(overlay_operation_id);
            "visual overlay operation identity was invalid".to_owned()
        })?;
        let identity = match self.board.reserve_capture(operation_id) {
            Ok(identity) => identity,
            Err(error) => {
                self.overlay
                    .abort_reserved_pinboard_rectangle_pick(overlay_operation_id);
                return Err(error.to_string());
            }
        };
        let parking = match self
            .parking_factory
            .begin(identity.generation, hwnd, virtual_desktop)
        {
            Ok(parking) => parking,
            Err(error) => {
                self.board.cancel_capture(identity);
                self.overlay
                    .abort_reserved_pinboard_rectangle_pick(overlay_operation_id);
                return Err(format!(
                    "could not park ROOT for screenshot capture: {error}"
                ));
            }
        };
        self.session = Some(PinCaptureSession {
            identity,
            overlay_operation_id,
            hwnd,
            hwnd_generation: root_identity.1,
            virtual_desktop,
            owned_revision: revision,
            focus_intent,
            invocation_id,
            phase: PinCapturePhase::AwaitRootAcknowledgement,
            parking,
            hide_requested: false,
            restore_requested: false,
            overlay_dispatched: false,
            overlay_terminal_consumed: false,
            overlay_reservation_released: false,
            cancel_sent: false,
            capture_job: None,
            selected_rect: None,
            staged: false,
            superseded: false,
            cancelled: false,
            error: None,
            error_reported: false,
            cleanup_complete: false,
        });
        Ok(true)
    }

    pub(super) fn poll(
        &mut self,
        visibility_revision: &VisibilityRevision,
        visible_flag: &Arc<std::sync::atomic::AtomicBool>,
        restore_flag: &Arc<std::sync::atomic::AtomicBool>,
        last_visible: &mut bool,
        root_identity: (usize, u64),
        egui_ctx: &eframe::egui::Context,
    ) -> Option<String> {
        let Some(mut session) = self.session.take() else {
            return None;
        };

        let current_revision = visibility_revision.current();
        if current_revision != session.owned_revision {
            session.superseded = true;
            if session.phase != PinCapturePhase::Restoring {
                session.cancelled = true;
            }
        }

        match session.phase {
            PinCapturePhase::AwaitRootAcknowledgement => {
                if session.cancelled {
                    session.phase = PinCapturePhase::Restoring;
                } else if root_identity != (session.hwnd, session.hwnd_generation) {
                    set_capture_error(
                        &mut session,
                        "ROOT native identity changed during pin capture",
                    );
                    session.phase = PinCapturePhase::Restoring;
                } else {
                    match session.parking.verify_capture_safe() {
                        Err(error) => {
                            set_capture_error(
                                &mut session,
                                format!("ROOT parking acknowledgement failed: {error}"),
                            );
                            session.phase = PinCapturePhase::Restoring;
                        }
                        Ok(false) => {}
                        Ok(true) => {
                            match publish_root_hidden(
                                visibility_revision,
                                visible_flag,
                                restore_flag,
                                last_visible,
                                &mut session,
                            ) {
                                Ok(()) => {
                                    session.phase = PinCapturePhase::AwaitPinHideAcknowledgement
                                }
                                Err(error) => {
                                    set_capture_error(&mut session, error);
                                    session.phase = PinCapturePhase::Restoring;
                                }
                            }
                        }
                    }
                }
            }
            PinCapturePhase::AwaitPinHideAcknowledgement => {
                if session.cancelled {
                    session.phase = PinCapturePhase::Restoring;
                } else if !session.hide_requested {
                    match self.pin_windows.request_hide_all(session.identity) {
                        Ok(()) => session.hide_requested = true,
                        Err(error) => {
                            set_capture_error(
                                &mut session,
                                format!("could not request hiding screenshot pins: {error}"),
                            );
                            session.phase = PinCapturePhase::Restoring;
                        }
                    }
                }
                if session.phase == PinCapturePhase::AwaitPinHideAcknowledgement
                    && session.hide_requested
                {
                    match self.pin_windows.poll_hidden(session.identity) {
                        PinVisibilityAck::Pending => {}
                        PinVisibilityAck::Failed(error) => {
                            set_capture_error(
                                &mut session,
                                format!("screenshot pin windows did not hide: {error}"),
                            );
                            session.phase = PinCapturePhase::Restoring;
                        }
                        PinVisibilityAck::Acknowledged => {
                            match self.overlay.begin_reserved_pinboard_rectangle_pick(
                                session.overlay_operation_id,
                                session.virtual_desktop,
                            ) {
                                Ok(()) => {
                                    session.overlay_dispatched = true;
                                    session.phase = PinCapturePhase::Selecting;
                                }
                                Err(error) => {
                                    set_capture_error(&mut session, error);
                                    session.phase = PinCapturePhase::Restoring;
                                }
                            }
                        }
                    }
                }
            }
            PinCapturePhase::Selecting => {
                if session.cancelled {
                    self.overlay
                        .cancel_pinboard_rectangle_pick(session.overlay_operation_id);
                    session.cancel_sent = true;
                    session.phase = PinCapturePhase::AwaitCancelledSelectionTerminal;
                } else if let Some(event) = self
                    .overlay
                    .poll_pinboard_rectangle_event(session.overlay_operation_id)
                {
                    self.consume_selection_terminal(&mut session, event);
                }
            }
            PinCapturePhase::AwaitCancelledSelectionTerminal => {
                if !session.cancel_sent {
                    self.overlay
                        .cancel_pinboard_rectangle_pick(session.overlay_operation_id);
                    session.cancel_sent = true;
                }
                if let Some(event) = self
                    .overlay
                    .poll_pinboard_rectangle_event(session.overlay_operation_id)
                {
                    self.consume_selection_terminal(&mut session, event);
                    session.cancelled = true;
                    session.phase = PinCapturePhase::Restoring;
                }
            }
            PinCapturePhase::Capturing => {
                if let Some(job) = session.capture_job.as_ref()
                    && session.cancelled
                {
                    job.cancel();
                }
                if let Some(job) = session.capture_job.take() {
                    if job.is_finished() {
                        let (identity, rect, result) = job.join_result();
                        if identity == session.identity && !session.cancelled {
                            match result {
                                Ok(image) => {
                                    match self.board.stage_capture(identity, rect, image) {
                                        Ok(()) => session.staged = true,
                                        Err(error) => {
                                            set_capture_error(&mut session, error.to_string())
                                        }
                                    }
                                }
                                Err(error) => set_capture_error(&mut session, error),
                            }
                        } else if let Err(error) = result {
                            if !session.cancelled {
                                set_capture_error(&mut session, error);
                            }
                        }
                        session.phase = PinCapturePhase::Restoring;
                    } else {
                        session.capture_job = Some(job);
                    }
                } else {
                    session.phase = PinCapturePhase::Restoring;
                }
            }
            PinCapturePhase::Restoring => {}
        }

        if session.phase == PinCapturePhase::Restoring {
            if session.overlay_dispatched && !session.overlay_terminal_consumed {
                // A dispatched selection reaches this phase only after its
                // terminal event has been consumed. Keep the slot if it has
                // not, then drain it operation-scoped before releasing it.
                if let Some(event) = self
                    .overlay
                    .poll_pinboard_rectangle_event(session.overlay_operation_id)
                {
                    self.consume_selection_terminal(&mut session, event);
                } else {
                    self.overlay
                        .cancel_pinboard_rectangle_pick(session.overlay_operation_id);
                    session.cancel_sent = true;
                    session.phase = PinCapturePhase::AwaitCancelledSelectionTerminal;
                }
            }
            if session.phase == PinCapturePhase::Restoring {
                self.restore_capture_session(
                    &mut session,
                    visibility_revision,
                    visible_flag,
                    restore_flag,
                    last_visible,
                    root_identity,
                    egui_ctx,
                );
            }
        }

        if session.phase == PinCapturePhase::AwaitCancelledSelectionTerminal
            && let Some(event) = self
                .overlay
                .poll_pinboard_rectangle_event(session.overlay_operation_id)
        {
            self.consume_selection_terminal(&mut session, event);
            session.cancelled = true;
            session.phase = PinCapturePhase::Restoring;
            self.restore_capture_session(
                &mut session,
                visibility_revision,
                visible_flag,
                restore_flag,
                last_visible,
                root_identity,
                egui_ctx,
            );
        }

        let report = session.error.as_ref().and_then(|error| {
            if session.error_reported {
                None
            } else {
                session.error_reported = true;
                Some(error.clone())
            }
        });
        let completed = session.cleanup_complete;
        if !completed {
            self.session = Some(session);
        }
        if self.session.is_some() {
            egui_ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        report
    }

    fn consume_selection_terminal(
        &mut self,
        session: &mut PinCaptureSession,
        event: VisualOverlayEvent,
    ) {
        if session.overlay_terminal_consumed {
            return;
        }
        session.overlay_terminal_consumed = self
            .overlay
            .acknowledge_pinboard_rectangle_closed(session.overlay_operation_id);
        if !session.overlay_terminal_consumed {
            set_capture_error(
                session,
                "pinboard selector terminal did not acknowledge closure",
            );
            session.phase = PinCapturePhase::Restoring;
            return;
        }
        let matching_id = overlay_event_id(&event) == session.overlay_operation_id;
        if session.cancelled {
            session.phase = PinCapturePhase::Restoring;
            return;
        }
        match event {
            VisualOverlayEvent::RectangleConfirmed {
                purpose: RectanglePurpose::PinboardCapture,
                rect,
                ..
            } if matching_id && !session.cancelled => {
                match self.board.reserve_selected_rect(session.identity, rect) {
                    Err(error) => {
                        set_capture_error(session, error.to_string());
                        session.phase = PinCapturePhase::Restoring;
                    }
                    Ok(_) => match spawn_capture_job(
                        Arc::clone(&self.capture_backend),
                        session.identity,
                        rect,
                    ) {
                        Ok(job) => {
                            session.selected_rect = Some(rect);
                            session.capture_job = Some(job);
                            session.phase = PinCapturePhase::Capturing;
                        }
                        Err(error) => {
                            set_capture_error(session, error);
                            session.phase = PinCapturePhase::Restoring;
                        }
                    },
                }
            }
            VisualOverlayEvent::Cancelled { .. } | VisualOverlayEvent::Expired { .. } => {
                session.cancelled = true;
                session.phase = PinCapturePhase::Restoring;
            }
            VisualOverlayEvent::Error { error, .. } => {
                set_capture_error(session, format!("screen selection failed: {error}"));
                session.phase = PinCapturePhase::Restoring;
            }
            _ => {
                session.cancelled = true;
                set_capture_error(
                    session,
                    "pinboard selector returned an unrelated terminal event",
                );
                session.phase = PinCapturePhase::Restoring;
            }
        }
    }

    fn restore_capture_session(
        &mut self,
        session: &mut PinCaptureSession,
        visibility_revision: &VisibilityRevision,
        visible_flag: &Arc<std::sync::atomic::AtomicBool>,
        restore_flag: &Arc<std::sync::atomic::AtomicBool>,
        last_visible: &mut bool,
        root_identity: (usize, u64),
        egui_ctx: &eframe::egui::Context,
    ) {
        if let Some(job) = session.capture_job.as_ref() {
            job.cancel();
        }
        if let Some(job) = session.capture_job.take() {
            if job.is_finished() {
                let (identity, rect, result) = job.join_result();
                if identity == session.identity && !session.cancelled {
                    match result {
                        Ok(image) => match self.board.stage_capture(identity, rect, image) {
                            Ok(()) => session.staged = true,
                            Err(error) => set_capture_error(session, error.to_string()),
                        },
                        Err(error) => set_capture_error(session, error),
                    }
                }
            } else {
                session.capture_job = Some(job);
                return;
            }
        }
        if !session.restore_requested {
            if let Err(error) = self.pin_windows.cancel_and_restore_all(session.identity) {
                set_capture_error(
                    session,
                    format!("could not restore screenshot pin windows: {error}"),
                );
                return;
            }
            session.restore_requested = true;
        }
        match self.pin_windows.poll_restored(session.identity) {
            PinVisibilityAck::Pending => return,
            PinVisibilityAck::Failed(error) => {
                set_capture_error(
                    session,
                    format!("screenshot pin visibility restoration failed: {error}"),
                );
                session.restore_requested = false;
                return;
            }
            PinVisibilityAck::Acknowledged => {}
        }
        if root_identity == (session.hwnd, session.hwnd_generation) {
            if let Err(error) = session.parking.restore() {
                set_capture_error(
                    session,
                    format!("could not restore ROOT after pin capture: {error}"),
                );
                return;
            }
            restore_root_visibility_intent(
                visibility_revision,
                visible_flag,
                restore_flag,
                last_visible,
                session,
            );
            let latest = root_visibility_snapshot(visibility_revision, visible_flag);
            if !latest.visible {
                if let Err(error) = session.parking.repark() {
                    set_capture_error(
                        session,
                        format!("could not preserve a newer hidden ROOT request: {error}"),
                    );
                    egui_ctx.request_repaint();
                    return;
                }
                session.parking.commit_hidden();
            }
        } else {
            // The identity fence prevents a stale HWND from being restored if
            // Windows reuses its numeric handle for another window.
            session.parking.commit_hidden();
            set_capture_error(session, "ROOT native identity changed before restoration");
        }

        if session.staged && !session.cancelled && !session.superseded {
            match self.board.commit_capture(session.identity) {
                Ok(_) => {}
                Err(error) => {
                    set_capture_error(session, error.to_string());
                    self.board.cancel_capture(session.identity);
                }
            }
        } else {
            self.board.cancel_capture(session.identity);
        }
        if !session.overlay_reservation_released {
            if session.overlay_dispatched {
                if !session.overlay_terminal_consumed {
                    // The caller keeps the session until the selector closes.
                    return;
                }
                session.overlay_reservation_released = self
                    .overlay
                    .finish_pinboard_rectangle_pick(session.overlay_operation_id);
            } else {
                session.overlay_reservation_released = self
                    .overlay
                    .abort_reserved_pinboard_rectangle_pick(session.overlay_operation_id);
            }
            if !session.overlay_reservation_released {
                set_capture_error(
                    session,
                    "pinboard selection reservation could not be released",
                );
                return;
            }
        }
        session.cleanup_complete = true;
    }

    pub(super) fn shutdown(
        &mut self,
        visibility_revision: &VisibilityRevision,
        visible_flag: &Arc<std::sync::atomic::AtomicBool>,
        restore_flag: &Arc<std::sync::atomic::AtomicBool>,
        last_visible: &mut bool,
        root_identity: (usize, u64),
    ) -> Result<(), String> {
        let Some(mut session) = self.session.take() else {
            return Ok(());
        };
        session.cancelled = true;
        if let Some(job) = session.capture_job.as_ref() {
            job.cancel();
        }
        if let Some(job) = session.capture_job.take() {
            let _ = job.take_result(true);
        }
        if session.overlay_dispatched && !session.overlay_terminal_consumed {
            let _ = self
                .overlay
                .cancel_and_drain_pinboard_rectangle_pick(session.overlay_operation_id);
            session.overlay_terminal_consumed = self
                .overlay
                .acknowledge_pinboard_rectangle_closed(session.overlay_operation_id);
        }
        let mut errors = Vec::new();
        if let Err(error) = self.pin_windows.cancel_and_restore_all(session.identity) {
            errors.push(format!(
                "could not request screenshot pin restoration: {error}"
            ));
        }
        if let Err(error) = self.pin_windows.shutdown_and_drain(session.identity) {
            errors.push(format!(
                "could not drain screenshot pin visibility: {error}"
            ));
        }
        if root_identity == (session.hwnd, session.hwnd_generation) {
            if let Err(error) = session.parking.restore() {
                errors.push(format!(
                    "could not restore ROOT during pin shutdown: {error}"
                ));
            } else {
                restore_root_visibility_intent(
                    visibility_revision,
                    visible_flag,
                    restore_flag,
                    last_visible,
                    &mut session,
                );
                if !root_visibility_snapshot(visibility_revision, visible_flag).visible {
                    if let Err(error) = session.parking.repark() {
                        errors.push(format!(
                            "could not retain newer hidden ROOT request: {error}"
                        ));
                    } else {
                        session.parking.commit_hidden();
                    }
                }
            }
        } else {
            session.parking.commit_hidden();
            errors.push("ROOT native identity changed before pin shutdown restoration".into());
        }
        self.board.cancel_capture(session.identity);
        if !session.overlay_reservation_released {
            if session.overlay_dispatched && session.overlay_terminal_consumed {
                session.overlay_reservation_released = self
                    .overlay
                    .finish_pinboard_rectangle_pick(session.overlay_operation_id);
            } else if !session.overlay_dispatched {
                session.overlay_reservation_released = self
                    .overlay
                    .abort_reserved_pinboard_rectangle_pick(session.overlay_operation_id);
            }
        }
        if !session.overlay_reservation_released {
            errors.push("pinboard selection reservation did not finish shutdown cleanup".into());
        }
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        Ok(())
    }
}

impl LauncherApp {
    pub(super) fn pinboard_capture_busy(&self) -> bool {
        self.pinboard_capture.is_busy()
    }

    pub(super) fn pinboard_capture_owns_root(&self) -> bool {
        self.pinboard_capture.is_busy()
    }

    pub(super) fn begin_pinboard_capture(&mut self) -> Result<bool, String> {
        if self.pinboard_capture_busy() {
            return Ok(false);
        }
        self.ensure_no_coordinate_pick()?;
        if self.color_pick_owns_root() || self.color_pick.controller.is_active() {
            return Err("Finish or cancel the screen color picker before pinning a region".into());
        }
        if self.ocr_owns_root() {
            return Err("Finish or cancel Screen Region OCR before pinning a region".into());
        }
        if self.screen_draw_launcher_parking.is_some()
            || !matches!(
                self.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::NoSession
                    | crate::screen_draw::ScreenDrawState::Failed { .. }
            )
        {
            return Err("Close the active Screen Draw session before pinning a region".into());
        }
        if self.crop_screenshot_operation.is_some() {
            return Err("Finish or cancel the screenshot crop before pinning a region".into());
        }
        let (revision, (focus_intent, invocation_id)) = self.visibility_revision.inspect(|| {
            (
                self.visibility_revision.focus_intent(),
                self.visibility_revision.invocation_id(),
            )
        });
        self.pinboard_capture.begin(
            self.visible_flag.load(Ordering::SeqCst),
            self.last_visible,
            revision,
            focus_intent,
            invocation_id,
            self.launcher_hwnd,
            self.root_window_bridge.identity(),
        )
    }

    pub(super) fn poll_pinboard_capture(&mut self, ctx: &eframe::egui::Context) {
        let root_identity = self.root_window_bridge.identity();
        self.pinboard_capture.poll_pin_windows(ctx, root_identity);
        if let Some(error) = self.pinboard_capture.poll(
            &self.visibility_revision,
            &self.visible_flag,
            &self.restore_flag,
            &mut self.last_visible,
            self.root_window_bridge.identity(),
            ctx,
        ) {
            self.report_error_message("pinboard.capture", error);
        }
    }

    pub(super) fn shutdown_pinboard_capture(&mut self) {
        let mut errors = Vec::new();
        if let Err(error) = self.pinboard_capture.shutdown(
            &self.visibility_revision,
            &self.visible_flag,
            &self.restore_flag,
            &mut self.last_visible,
            self.root_window_bridge.identity(),
        ) {
            errors.push(error);
        }
        if let Err(error) = self.pinboard_capture.shutdown_pin_windows() {
            errors.push(error);
        }
        if !errors.is_empty() {
            self.report_error_message("pinboard.shutdown", errors.join("; "));
        }
    }

    pub(super) fn show_screenshot_pins(&mut self, ctx: &eframe::egui::Context) {
        let root_identity = self.root_window_bridge.identity();
        self.pinboard_capture.show_pin_windows(ctx, root_identity);
    }

    pub(super) fn show_screenshot_pin_list(
        &mut self,
        ctx: &eframe::egui::Context,
        is_open: bool,
    ) -> bool {
        let mut open = is_open;
        self.pinboard_capture.show_pin_list(ctx, &mut open);
        open
    }

    pub(super) fn pin_windows_need_bootstrap(&self) -> bool {
        self.pinboard_capture.pin_windows_need_bootstrap()
    }

    pub(super) fn pin_windows_need_hidden_root_ui(&self) -> bool {
        self.pinboard_capture.pin_windows_need_hidden_root_ui()
    }

    pub(super) fn cancel_pinboard_capture(&mut self) {
        self.pinboard_capture.cancel();
    }

    pub(super) fn close_screenshot_pin(&mut self, id: crate::pinboard::PinId) -> bool {
        self.pinboard_capture.close_pin(id)
    }
}

fn set_capture_error(session: &mut PinCaptureSession, error: impl Into<String>) {
    if session.error.is_none() {
        session.error = Some(error.into());
    }
    session.cancelled = true;
}

fn overlay_event_id(event: &VisualOverlayEvent) -> OperationId {
    match event {
        VisualOverlayEvent::PointConfirmed { operation_id, .. }
        | VisualOverlayEvent::RectangleConfirmed { operation_id, .. }
        | VisualOverlayEvent::Cancelled { operation_id }
        | VisualOverlayEvent::Expired { operation_id }
        | VisualOverlayEvent::Error { operation_id, .. } => *operation_id,
    }
}

fn spawn_capture_job(
    backend: Arc<dyn ScreenCaptureBackend>,
    identity: PinCaptureIdentity,
    rect: ScreenRect,
) -> Result<CaptureJob, String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let join = thread::Builder::new()
        .name(format!("pin-capture-{}", identity.generation.get()))
        .spawn(move || {
            let result = backend
                .capture_rect(rect, &|| worker_cancelled.load(Ordering::Acquire))
                .map_err(|error: ExecutionDiagnostic| error.to_string());
            if worker_cancelled.load(Ordering::Acquire) {
                Err("screen capture was cancelled".to_owned())
            } else {
                result
            }
        })
        .map_err(|error| format!("could not start screen capture worker: {error}"))?;
    Ok(CaptureJob {
        identity,
        rect,
        cancelled,
        join,
    })
}

fn root_visibility_snapshot(
    revision: &VisibilityRevision,
    visible_flag: &Arc<std::sync::atomic::AtomicBool>,
) -> RootVisibilitySnapshot {
    let (revision, (visible, focus_intent, invocation_id)) = revision.inspect(|| {
        (
            visible_flag.load(Ordering::SeqCst),
            revision.focus_intent(),
            revision.invocation_id(),
        )
    });
    RootVisibilitySnapshot {
        revision,
        visible,
        focus_intent,
        invocation_id,
    }
}

fn publish_root_hidden(
    revision: &VisibilityRevision,
    visible_flag: &Arc<std::sync::atomic::AtomicBool>,
    restore_flag: &Arc<std::sync::atomic::AtomicBool>,
    last_visible: &mut bool,
    session: &mut PinCaptureSession,
) -> Result<(), String> {
    let result = revision.request_if_current_with_focus_intent_and_invocation(
        session.owned_revision,
        session.focus_intent,
        session.invocation_id,
        || {
            visible_flag.store(false, Ordering::SeqCst);
            restore_flag.store(false, Ordering::SeqCst);
        },
    );
    let Some((owned_revision, ())) = result else {
        session.superseded = true;
        session.cancelled = true;
        return Err("a newer launcher visibility request superseded pin capture".into());
    };
    session.owned_revision = owned_revision;
    *last_visible = false;
    Ok(())
}

fn restore_root_visibility_intent(
    revision: &VisibilityRevision,
    visible_flag: &Arc<std::sync::atomic::AtomicBool>,
    restore_flag: &Arc<std::sync::atomic::AtomicBool>,
    last_visible: &mut bool,
    session: &mut PinCaptureSession,
) {
    let current = root_visibility_snapshot(revision, visible_flag);
    if current.revision == session.owned_revision && !session.superseded {
        if current.visible {
            *last_visible = true;
            return;
        }
        if let Some((new_revision, ())) = revision
            .request_if_current_with_focus_intent_and_invocation(
                current.revision,
                current.focus_intent,
                current.invocation_id,
                || {
                    visible_flag.store(true, Ordering::SeqCst);
                    restore_flag.store(true, Ordering::SeqCst);
                },
            )
        {
            session.owned_revision = new_revision;
            *last_visible = true;
            return;
        }
    }
    *last_visible = current.visible;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::mkmacro_dialog::visual_capture_workflow::SharedVisualOverlayController;
    use crate::gui::mkmacro_dialog::visual_overlay::RectanglePurpose;
    use crate::mkmacro::{ExecResult, SearchRegion};
    use crate::visibility::VisibilityRevision;
    use std::sync::{Mutex, mpsc};
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct CaptureFlowLog {
        parking_checks: usize,
        parking_restores: usize,
        parking_reparks: usize,
        parking_commits: usize,
        pin_hide_requests: usize,
        pin_hide_polls: usize,
        pin_restore_requests: usize,
        pin_restore_polls: usize,
        capture_rects: Vec<ScreenRect>,
    }

    struct FakeParkingFactory {
        log: Arc<Mutex<CaptureFlowLog>>,
        acknowledge_after: Option<usize>,
    }

    impl LauncherCaptureParkingFactory for FakeParkingFactory {
        fn begin(
            &self,
            _: PinCaptureGeneration,
            _: usize,
            _: ScreenRect,
        ) -> Result<Box<dyn LauncherCaptureParking>, String> {
            Ok(Box::new(FakeParking {
                log: Arc::clone(&self.log),
                acknowledge_after: self.acknowledge_after,
            }))
        }
    }

    struct FakeParking {
        log: Arc<Mutex<CaptureFlowLog>>,
        acknowledge_after: Option<usize>,
    }

    impl LauncherCaptureParking for FakeParking {
        fn verify_capture_safe(&mut self) -> Result<bool, String> {
            let mut log = self.log.lock().unwrap();
            log.parking_checks += 1;
            Ok(self
                .acknowledge_after
                .is_some_and(|count| log.parking_checks >= count))
        }

        fn restore(&mut self) -> Result<(), String> {
            self.log.lock().unwrap().parking_restores += 1;
            Ok(())
        }

        fn repark(&mut self) -> Result<(), String> {
            self.log.lock().unwrap().parking_reparks += 1;
            Ok(())
        }

        fn commit_hidden(&mut self) {
            self.log.lock().unwrap().parking_commits += 1;
        }
    }

    struct FakePinWindows {
        log: Arc<Mutex<CaptureFlowLog>>,
        hide_ack_after: usize,
        restore_ack_after: usize,
        hide_failure: Option<&'static str>,
    }

    impl PinWindowVisibilityCoordinator for FakePinWindows {
        fn request_hide_all(&mut self, _: PinCaptureIdentity) -> Result<(), String> {
            self.log.lock().unwrap().pin_hide_requests += 1;
            Ok(())
        }

        fn poll_hidden(&mut self, _: PinCaptureIdentity) -> PinVisibilityAck {
            if let Some(error) = self.hide_failure.take() {
                return PinVisibilityAck::Failed(error.to_owned());
            }
            let mut log = self.log.lock().unwrap();
            log.pin_hide_polls += 1;
            if log.pin_hide_polls >= self.hide_ack_after {
                PinVisibilityAck::Acknowledged
            } else {
                PinVisibilityAck::Pending
            }
        }

        fn request_restore_all(&mut self, _: PinCaptureIdentity) -> Result<(), String> {
            self.log.lock().unwrap().pin_restore_requests += 1;
            Ok(())
        }

        fn poll_restored(&mut self, _: PinCaptureIdentity) -> PinVisibilityAck {
            let mut log = self.log.lock().unwrap();
            log.pin_restore_polls += 1;
            if log.pin_restore_polls >= self.restore_ack_after {
                PinVisibilityAck::Acknowledged
            } else {
                PinVisibilityAck::Pending
            }
        }

        fn cancel_and_restore_all(&mut self, _: PinCaptureIdentity) -> Result<(), String> {
            self.log.lock().unwrap().pin_restore_requests += 1;
            Ok(())
        }

        fn shutdown_and_drain(&mut self, _: PinCaptureIdentity) -> Result<(), String> {
            Ok(())
        }
    }

    struct GatedCaptureBackend {
        log: Arc<Mutex<CaptureFlowLog>>,
        started: mpsc::Sender<ScreenRect>,
        release: Mutex<mpsc::Receiver<()>>,
        completed: mpsc::Sender<()>,
        desktop: ScreenRect,
    }

    impl ScreenCaptureBackend for GatedCaptureBackend {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(self.desktop)
        }

        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<ScreenRect> {
            Ok(self.desktop)
        }

        fn capture_rect(
            &self,
            rect: ScreenRect,
            cancelled: &dyn Fn() -> bool,
        ) -> ExecResult<RgbaImage> {
            self.log.lock().unwrap().capture_rects.push(rect);
            self.started.send(rect).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            if cancelled() {
                return Err(crate::mkmacro::cancelled_error());
            }
            self.completed.send(()).unwrap();
            Ok(RgbaImage::from_pixel(
                rect.width,
                rect.height,
                image::Rgba([17, 29, 43, 255]),
            ))
        }
    }

    fn poll_lifecycle(
        context: &eframe::egui::Context,
        lifecycle: &mut PinboardCaptureLifecycle,
        visibility: &VisibilityRevision,
        visible: &Arc<std::sync::atomic::AtomicBool>,
        restore: &Arc<std::sync::atomic::AtomicBool>,
        last_visible: &mut bool,
        identity: (usize, u64),
    ) -> Option<String> {
        let mut error = None;
        context
            .run_ui(eframe::egui::RawInput::default(), |ctx| {
                error = lifecycle.poll(visibility, visible, restore, last_visible, identity, ctx);
            })
            .drop_without_applying_deltas();
        error
    }

    #[test]
    fn fake_capture_waits_for_both_native_acknowledgements_and_restores_before_commit() {
        let overlay_fixture = SharedVisualOverlayController::test_fixture();
        let log = Arc::new(Mutex::new(CaptureFlowLog::default()));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let desktop = ScreenRect::new(-1_920, -240, 3_840, 1_320);
        let backend: Arc<dyn ScreenCaptureBackend> = Arc::new(GatedCaptureBackend {
            log: Arc::clone(&log),
            started: started_tx,
            release: Mutex::new(release_rx),
            completed: completed_tx,
            desktop,
        });
        let mut lifecycle =
            PinboardCaptureLifecycle::new(overlay_fixture.controller.clone(), backend);
        lifecycle.set_adapters_for_test(
            Arc::new(FakeParkingFactory {
                log: Arc::clone(&log),
                acknowledge_after: Some(2),
            }),
            Box::new(FakePinWindows {
                log: Arc::clone(&log),
                hide_ack_after: 2,
                restore_ack_after: 2,
                hide_failure: None,
            }),
        );
        let visibility = VisibilityRevision::default();
        let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let restore = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut last_visible = true;
        let native_identity = (42, 9);
        assert!(
            lifecycle
                .begin(
                    true,
                    true,
                    visibility.current(),
                    RootFocusIntent::PreserveForeground,
                    Some(71),
                    Some(native_identity.0),
                    native_identity,
                )
                .unwrap()
        );
        let operation_id = lifecycle.session.as_ref().unwrap().overlay_operation_id;
        let context = eframe::egui::Context::default();

        poll_lifecycle(
            &context,
            &mut lifecycle,
            &visibility,
            &visible,
            &restore,
            &mut last_visible,
            native_identity,
        );
        assert_eq!(log.lock().unwrap().parking_checks, 1);
        assert_eq!(log.lock().unwrap().pin_hide_requests, 0);
        assert!(log.lock().unwrap().capture_rects.is_empty());

        poll_lifecycle(
            &context,
            &mut lifecycle,
            &visibility,
            &visible,
            &restore,
            &mut last_visible,
            native_identity,
        );
        assert!(!visible.load(Ordering::SeqCst));
        assert_eq!(log.lock().unwrap().pin_hide_requests, 0);
        assert!(log.lock().unwrap().capture_rects.is_empty());

        poll_lifecycle(
            &context,
            &mut lifecycle,
            &visibility,
            &visible,
            &restore,
            &mut last_visible,
            native_identity,
        );
        assert_eq!(log.lock().unwrap().pin_hide_polls, 1);
        assert!(overlay_fixture.observer.commands.lock().unwrap().is_empty());
        assert!(log.lock().unwrap().capture_rects.is_empty());

        poll_lifecycle(
            &context,
            &mut lifecycle,
            &visibility,
            &visible,
            &restore,
            &mut last_visible,
            native_identity,
        );
        overlay_fixture.observer.wait_for_commands(1);
        assert_eq!(log.lock().unwrap().pin_hide_polls, 2);
        assert!(log.lock().unwrap().capture_rects.is_empty());
        let rect = ScreenRect::new(-1_800, -100, 1_200, 700);
        overlay_fixture.controller.inject_pinboard_event_for_test(
            VisualOverlayEvent::RectangleConfirmed {
                operation_id,
                purpose: RectanglePurpose::PinboardCapture,
                rect,
            },
        );
        assert!(overlay_fixture.controller.poll().is_empty());
        poll_lifecycle(
            &context,
            &mut lifecycle,
            &visibility,
            &visible,
            &restore,
            &mut last_visible,
            native_identity,
        );
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            rect
        );
        assert_eq!(log.lock().unwrap().parking_checks, 2);
        assert_eq!(log.lock().unwrap().pin_hide_polls, 2);
        assert_eq!(log.lock().unwrap().capture_rects, [rect]);
        assert!(
            overlay_fixture
                .controller
                .reserve_pinboard_rectangle_pick()
                .is_err()
        );

        release_tx.send(()).unwrap();
        completed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let restore_deadline = Instant::now() + Duration::from_secs(2);
        while log.lock().unwrap().pin_restore_polls == 0 && Instant::now() < restore_deadline {
            poll_lifecycle(
                &context,
                &mut lifecycle,
                &visibility,
                &visible,
                &restore,
                &mut last_visible,
                native_identity,
            );
            thread::yield_now();
        }
        assert!(lifecycle.is_busy());
        assert_eq!(log.lock().unwrap().pin_restore_polls, 1);
        assert!(
            overlay_fixture
                .controller
                .reserve_pinboard_rectangle_pick()
                .is_err()
        );
        poll_lifecycle(
            &context,
            &mut lifecycle,
            &visibility,
            &visible,
            &restore,
            &mut last_visible,
            native_identity,
        );
        assert!(!lifecycle.is_busy());
        assert!(visible.load(Ordering::SeqCst));
        assert!(restore.load(Ordering::SeqCst));
        assert_eq!(lifecycle.pinboard().len(), 1);
        let pin = lifecycle.pinboard().pins().next().unwrap();
        assert_eq!(
            pin.snapshot().physical_dimensions(),
            (rect.width, rect.height)
        );
        assert_eq!(pin.snapshot().capture_screen_rect(), rect);
        let completed = log.lock().unwrap();
        assert_eq!(completed.capture_rects, [rect]);
        assert_eq!(completed.parking_restores, 1);
        assert_eq!(completed.pin_restore_polls, 2);
        assert_eq!(completed.pin_restore_requests, 1);
        assert!(overlay_fixture.controller.poll().is_empty());
        let next_operation = overlay_fixture
            .controller
            .reserve_pinboard_rectangle_pick()
            .unwrap();
        assert!(
            overlay_fixture
                .controller
                .abort_reserved_pinboard_rectangle_pick(next_operation)
        );
    }

    #[test]
    fn missing_root_ack_never_dispatches_selection_or_captures_pixels() {
        let overlay_fixture = SharedVisualOverlayController::test_fixture();
        let log = Arc::new(Mutex::new(CaptureFlowLog::default()));
        let (started_tx, _started_rx) = mpsc::channel();
        let (_release_tx, release_rx) = mpsc::channel();
        let (completed_tx, _completed_rx) = mpsc::channel();
        let backend: Arc<dyn ScreenCaptureBackend> = Arc::new(GatedCaptureBackend {
            log: Arc::clone(&log),
            started: started_tx,
            release: Mutex::new(release_rx),
            completed: completed_tx,
            desktop: ScreenRect::new(-100, -50, 640, 480),
        });
        let mut lifecycle =
            PinboardCaptureLifecycle::new(overlay_fixture.controller.clone(), backend);
        lifecycle.set_adapters_for_test(
            Arc::new(FakeParkingFactory {
                log: Arc::clone(&log),
                acknowledge_after: None,
            }),
            Box::new(FakePinWindows {
                log: Arc::clone(&log),
                hide_ack_after: 1,
                restore_ack_after: 1,
                hide_failure: None,
            }),
        );
        let visibility = VisibilityRevision::default();
        let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let restore = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut last_visible = true;
        let native_identity = (42, 9);
        assert!(
            lifecycle
                .begin(
                    true,
                    true,
                    visibility.current(),
                    RootFocusIntent::PreserveForeground,
                    None,
                    Some(native_identity.0),
                    native_identity,
                )
                .unwrap()
        );
        let context = eframe::egui::Context::default();
        for _ in 0..3 {
            poll_lifecycle(
                &context,
                &mut lifecycle,
                &visibility,
                &visible,
                &restore,
                &mut last_visible,
                native_identity,
            );
        }
        assert!(overlay_fixture.observer.commands.lock().unwrap().is_empty());
        assert_eq!(log.lock().unwrap().pin_hide_requests, 0);
        assert!(log.lock().unwrap().capture_rects.is_empty());
        lifecycle
            .shutdown(
                &visibility,
                &visible,
                &restore,
                &mut last_visible,
                native_identity,
            )
            .unwrap();
        assert_eq!(lifecycle.pinboard().pending_bytes(), 0);
        assert_eq!(log.lock().unwrap().parking_restores, 1);
    }

    #[test]
    fn failed_pin_hide_ack_never_dispatches_selection_or_captures_pixels() {
        let overlay_fixture = SharedVisualOverlayController::test_fixture();
        let log = Arc::new(Mutex::new(CaptureFlowLog::default()));
        let (started_tx, _started_rx) = mpsc::channel();
        let (_release_tx, release_rx) = mpsc::channel();
        let (completed_tx, _completed_rx) = mpsc::channel();
        let backend: Arc<dyn ScreenCaptureBackend> = Arc::new(GatedCaptureBackend {
            log: Arc::clone(&log),
            started: started_tx,
            release: Mutex::new(release_rx),
            completed: completed_tx,
            desktop: ScreenRect::new(-100, -50, 640, 480),
        });
        let mut lifecycle =
            PinboardCaptureLifecycle::new(overlay_fixture.controller.clone(), backend);
        lifecycle.set_adapters_for_test(
            Arc::new(FakeParkingFactory {
                log: Arc::clone(&log),
                acknowledge_after: Some(1),
            }),
            Box::new(FakePinWindows {
                log: Arc::clone(&log),
                hide_ack_after: 1,
                restore_ack_after: 1,
                hide_failure: Some("native hide acknowledgement unavailable"),
            }),
        );
        let visibility = VisibilityRevision::default();
        let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let restore = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut last_visible = true;
        let native_identity = (42, 9);
        assert!(
            lifecycle
                .begin(
                    true,
                    true,
                    visibility.current(),
                    RootFocusIntent::PreserveForeground,
                    None,
                    Some(native_identity.0),
                    native_identity,
                )
                .unwrap()
        );
        let context = eframe::egui::Context::default();
        for _ in 0..2 {
            poll_lifecycle(
                &context,
                &mut lifecycle,
                &visibility,
                &visible,
                &restore,
                &mut last_visible,
                native_identity,
            );
        }
        assert!(!lifecycle.is_busy());
        assert_eq!(log.lock().unwrap().pin_hide_requests, 1);
        assert!(log.lock().unwrap().capture_rects.is_empty());
        assert!(overlay_fixture.observer.commands.lock().unwrap().is_empty());
        assert_eq!(lifecycle.pinboard().pending_bytes(), 0);
        assert_eq!(log.lock().unwrap().parking_restores, 1);
    }
}
