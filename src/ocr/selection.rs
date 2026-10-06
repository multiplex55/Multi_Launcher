//! Transient selection transitions. Native parking and overlay effects belong to
//! the GUI adapter; generation identity is independent of native operation IDs.
use super::job::{OcrJobEvent, OcrJobEventKind};
use crate::mkmacro::ScreenRect;
use crate::mkmacro::{DiagnosticKind, ExecResult, ExecutionDiagnostic};
use std::time::{Duration, Instant};

const PARK_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OcrGeneration(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SelectionOutcome {
    Cancelled,
    Failed(String),
    Continue,
}

#[derive(Clone, Debug)]
pub(crate) enum OcrPresentation {
    Recognizing,
    Result(String),
    NoText,
    Error(ExecutionDiagnostic),
}

#[derive(Clone, Debug)]
enum Phase {
    PendingParking,
    ApplyingParking,
    VerifyParking,
    VerifyingParking,
    Selecting(u64),
    Cancelling(u64),
    Confirmed(ScreenRect),
    Capturing {
        cancelled: bool,
    },
    RestoringJob {
        cancelled: bool,
        presentation: OcrPresentation,
    },
    Presented(OcrPresentation),
    Restoring(SelectionOutcome),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionEffect {
    Park(OcrGeneration),
    Verify(OcrGeneration),
}

#[derive(Default)]
pub(crate) struct OcrSelectionController {
    next_generation: u64,
    session: Option<(OcrGeneration, Instant, Phase)>,
}

impl OcrSelectionController {
    pub(crate) fn request(&mut self, now: Instant) -> Result<Option<OcrGeneration>, String> {
        if self.session.is_some() {
            return Ok(None);
        }
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or("OCR generation identity exhausted")?;
        let generation = OcrGeneration(self.next_generation);
        self.session = Some((generation, now, Phase::PendingParking));
        Ok(Some(generation))
    }

    pub(crate) fn poll(&mut self, now: Instant, native_ready: bool) -> Option<SelectionEffect> {
        let (generation, started, phase) = self.session.as_mut()?;
        if matches!(
            phase,
            Phase::PendingParking
                | Phase::ApplyingParking
                | Phase::VerifyParking
                | Phase::VerifyingParking
        ) && now.saturating_duration_since(*started) >= PARK_TIMEOUT
        {
            *phase = Phase::Restoring(SelectionOutcome::Failed(
                "Timed out waiting for capture-safe launcher parking".into(),
            ));
            return None;
        }
        match phase {
            Phase::PendingParking if native_ready => {
                *phase = Phase::ApplyingParking;
                Some(SelectionEffect::Park(*generation))
            }
            Phase::VerifyParking => {
                *phase = Phase::VerifyingParking;
                Some(SelectionEffect::Verify(*generation))
            }
            _ => None,
        }
    }

    pub(crate) fn parking_applied(
        &mut self,
        generation: OcrGeneration,
        result: Result<(), String>,
    ) {
        if let Some((current, _, phase)) = &mut self.session
            && *current == generation
            && matches!(phase, Phase::ApplyingParking)
        {
            *phase = match result {
                Ok(()) => Phase::VerifyParking,
                Err(error) => Phase::Restoring(SelectionOutcome::Failed(format!(
                    "Could not park launcher for OCR: {error}"
                ))),
            };
        }
    }

    /// Returns true only after a later poll has requested verification.
    pub(crate) fn parking_verified(
        &mut self,
        generation: OcrGeneration,
        result: Result<bool, String>,
    ) -> bool {
        if let Some((current, _, phase)) = &mut self.session
            && *current == generation
            && matches!(phase, Phase::VerifyingParking)
        {
            match result {
                Ok(true) => return true,
                Ok(false) => *phase = Phase::VerifyParking,
                Err(error) => {
                    *phase = Phase::Restoring(SelectionOutcome::Failed(format!(
                        "Could not verify OCR launcher parking: {error}"
                    )))
                }
            }
        }
        false
    }

    pub(crate) fn selector_started(
        &mut self,
        generation: OcrGeneration,
        result: Result<u64, String>,
    ) {
        if let Some((current, _, phase)) = &mut self.session
            && *current == generation
            && matches!(phase, Phase::VerifyingParking)
        {
            *phase = match result {
                Ok(id) => Phase::Selecting(id),
                Err(error) => Phase::Restoring(SelectionOutcome::Failed(error)),
            };
        }
    }

    pub(crate) fn operation(&self) -> Option<(OcrGeneration, u64)> {
        self.session
            .as_ref()
            .and_then(|(generation, _, phase)| match phase {
                Phase::Selecting(id) | Phase::Cancelling(id) => Some((*generation, *id)),
                _ => None,
            })
    }

    /// Invalidates success immediately, but keeps native identity until terminal
    /// acknowledgement. A racing confirmation can only acknowledge cancellation.
    pub(crate) fn cancel(&mut self) -> Option<u64> {
        let (_, _, phase) = self.session.as_mut()?;
        match phase {
            Phase::Selecting(id) => {
                let id = *id;
                *phase = Phase::Cancelling(id);
                Some(id)
            }
            Phase::Capturing { cancelled } | Phase::RestoringJob { cancelled, .. } => {
                *cancelled = true;
                None
            }
            Phase::Presented(_) => {
                self.release();
                None
            }
            Phase::Cancelling(_) | Phase::Restoring(_) => None,
            _ => {
                *phase = Phase::Restoring(SelectionOutcome::Cancelled);
                None
            }
        }
    }

    pub(crate) fn terminal(
        &mut self,
        generation: OcrGeneration,
        id: u64,
        result: Result<Option<ScreenRect>, String>,
    ) {
        if let Some((current, _, phase)) = &mut self.session
            && *current == generation
        {
            match *phase {
                Phase::Cancelling(expected) if expected == id => {
                    *phase = Phase::Restoring(SelectionOutcome::Cancelled)
                }
                Phase::Selecting(expected) if expected == id => {
                    *phase = match result {
                        Ok(Some(rect)) => Phase::Confirmed(rect),
                        Ok(None) => Phase::Restoring(SelectionOutcome::Cancelled),
                        Err(error) => Phase::Restoring(SelectionOutcome::Failed(error)),
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn restore_outcome(&self) -> Option<&SelectionOutcome> {
        self.session.as_ref().and_then(|(_, _, phase)| match phase {
            Phase::Restoring(outcome) => Some(outcome),
            Phase::RestoringJob {
                cancelled: true, ..
            } => Some(&SelectionOutcome::Cancelled),
            Phase::RestoringJob { .. } => Some(&SelectionOutcome::Continue),
            _ => None,
        })
    }
    pub(crate) fn confirmed(&self) -> Option<(OcrGeneration, ScreenRect)> {
        self.session
            .as_ref()
            .and_then(|(generation, _, phase)| match phase {
                Phase::Confirmed(rect) => Some((*generation, *rect)),
                _ => None,
            })
    }
    pub(crate) fn release(&mut self) {
        self.session = None;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.session.is_some()
    }

    /// Consumes confirmation exactly once, before the GUI starts a worker.
    pub(crate) fn take_capture(&mut self) -> Option<(OcrGeneration, ScreenRect)> {
        let (generation, rect) = self.confirmed()?;
        self.session.as_mut()?.2 = Phase::Capturing { cancelled: false };
        Some((generation, rect))
    }

    pub(crate) fn worker_event(&mut self, event: OcrJobEvent) {
        let Some((generation, _, phase)) = &mut self.session else {
            return;
        };
        if *generation != event.generation {
            return;
        }
        let presentation = |result: ExecResult<String>| match result {
            Ok(text) if text.trim().is_empty() => OcrPresentation::NoText,
            Ok(text) => OcrPresentation::Result(text),
            Err(error) => OcrPresentation::Error(error),
        };
        match (phase.clone(), event.kind) {
            (Phase::Capturing { cancelled }, OcrJobEventKind::Captured) => {
                *phase = Phase::RestoringJob {
                    cancelled,
                    presentation: OcrPresentation::Recognizing,
                };
            }
            (Phase::Capturing { cancelled }, OcrJobEventKind::Finished(result))
            | (Phase::RestoringJob { cancelled, .. }, OcrJobEventKind::Finished(result)) => {
                let cancelled = cancelled
                    || result
                        .as_ref()
                        .is_err_and(|error| error.kind == DiagnosticKind::Cancelled);
                *phase = Phase::RestoringJob {
                    cancelled,
                    presentation: presentation(result),
                };
            }
            (Phase::Presented(OcrPresentation::Recognizing), OcrJobEventKind::Finished(result)) => {
                if result
                    .as_ref()
                    .is_err_and(|error| error.kind == DiagnosticKind::Cancelled)
                {
                    self.release();
                } else {
                    *phase = Phase::Presented(presentation(result));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn restored(&mut self) {
        if let Some((
            _,
            _,
            Phase::RestoringJob {
                cancelled: false,
                presentation,
            },
        )) = &self.session
        {
            let presentation = presentation.clone();
            self.session.as_mut().unwrap().2 = Phase::Presented(presentation);
        } else {
            self.release();
        }
    }

    pub(crate) fn presentation(&self) -> Option<&OcrPresentation> {
        match &self.session.as_ref()?.2 {
            Phase::Presented(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn presented_generation(&self) -> Option<OcrGeneration> {
        self.presentation()?;
        Some(self.session.as_ref()?.0)
    }

    pub(crate) fn presentation_mut(&mut self) -> Option<&mut OcrPresentation> {
        match &mut self.session.as_mut()?.2 {
            Phase::Presented(state) => Some(state),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selecting() -> (OcrSelectionController, OcrGeneration) {
        let mut controller = OcrSelectionController::default();
        let now = Instant::now();
        let generation = controller.request(now).unwrap().unwrap();
        assert_eq!(
            controller.poll(now, true),
            Some(SelectionEffect::Park(generation))
        );
        controller.parking_applied(generation, Ok(()));
        assert!(!controller.parking_verified(generation, Ok(true)));
        assert_eq!(
            controller.poll(now, true),
            Some(SelectionEffect::Verify(generation))
        );
        assert!(controller.parking_verified(generation, Ok(true)));
        controller.selector_started(generation, Ok(77));
        (controller, generation)
    }
    #[test]
    fn ocr_selection_stale_and_cancel_racing_confirmation_cannot_stage_geometry() {
        let (mut controller, generation) = selecting();
        controller.terminal(generation, 78, Ok(Some(ScreenRect::new(-5, -3, 20, 40))));
        assert_eq!(controller.operation(), Some((generation, 77)));
        assert_eq!(controller.cancel(), Some(77));
        assert!(controller.restore_outcome().is_none());
        controller.terminal(generation, 77, Ok(Some(ScreenRect::new(-5, -3, 20, 40))));
        assert_eq!(
            controller.restore_outcome(),
            Some(&SelectionOutcome::Cancelled)
        );
        assert!(controller.confirmed().is_none());
    }
    #[test]
    fn ocr_selection_confirmation_retains_exact_geometry_and_duplicate_is_noop() {
        let (mut controller, generation) = selecting();
        let rect = ScreenRect::new(-100, -30, 230, 300);
        controller.terminal(generation, 77, Ok(Some(rect)));
        assert_eq!(controller.confirmed(), Some((generation, rect)));
        assert!(controller.request(Instant::now()).unwrap().is_none());
        controller.release();
        let next = controller.request(Instant::now()).unwrap().unwrap();
        assert_ne!(generation, next);
        controller.terminal(generation, 77, Ok(Some(rect)));
        assert!(controller.confirmed().is_none());
    }
    #[test]
    fn ocr_selection_parking_wait_is_bounded() {
        let mut controller = OcrSelectionController::default();
        let now = Instant::now();
        controller.request(now).unwrap();
        assert!(controller.poll(now, false).is_none());
        controller.poll(now + PARK_TIMEOUT, false);
        assert!(matches!(
            controller.restore_outcome(),
            Some(SelectionOutcome::Failed(_))
        ));
    }

    #[test]
    fn ocr_workflow_capture_is_once_and_fast_completion_waits_for_restore() {
        let (mut controller, generation) = selecting();
        let rect = ScreenRect::new(-100, -30, 230, 300);
        controller.terminal(generation, 77, Ok(Some(rect)));
        assert_eq!(controller.take_capture(), Some((generation, rect)));
        assert!(controller.take_capture().is_none());
        assert!(controller.restore_outcome().is_none());
        controller.worker_event(OcrJobEvent {
            generation,
            kind: OcrJobEventKind::Captured,
        });
        controller.worker_event(OcrJobEvent {
            generation,
            kind: OcrJobEventKind::Finished(Ok("exact\ntext".into())),
        });
        assert_eq!(
            controller.restore_outcome(),
            Some(&SelectionOutcome::Continue)
        );
        assert!(controller.presentation().is_none());
        controller.restored();
        assert!(
            matches!(controller.presentation(),Some(OcrPresentation::Result(text)) if text=="exact\ntext")
        );
        assert!(controller.request(Instant::now()).unwrap().is_none());
    }

    #[test]
    fn ocr_workflow_cancel_capture_retains_owner_and_old_generation_cannot_publish() {
        let (mut controller, generation) = selecting();
        controller.terminal(generation, 77, Ok(Some(ScreenRect::new(-10, -20, 30, 40))));
        controller.take_capture();
        controller.cancel();
        assert!(controller.is_active());
        assert!(controller.restore_outcome().is_none());
        controller.worker_event(OcrJobEvent {
            generation,
            kind: OcrJobEventKind::Captured,
        });
        controller.worker_event(OcrJobEvent {
            generation,
            kind: OcrJobEventKind::Finished(Ok("cancelled text".into())),
        });
        assert_eq!(
            controller.restore_outcome(),
            Some(&SelectionOutcome::Cancelled)
        );
        controller.restored();
        assert!(!controller.is_active());
        controller.request(Instant::now()).unwrap();
        controller.worker_event(OcrJobEvent {
            generation,
            kind: OcrJobEventKind::Finished(Ok("old text".into())),
        });
        assert!(controller.presentation().is_none());
        assert!(controller.restore_outcome().is_none());
    }
}
