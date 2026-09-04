//! One-shot background image-search attempts for the macro editor.

use super::image_authoring_destination::ConditionPath;
use super::image_authoring_job::ImageAuthoringExecutor;
use crate::mkmacro::{
    DiagnosticKind, ExecutionDiagnostic, ImageSearchMatch, MkImagePayload, VisualSearch,
};
use std::sync::{
    Arc,
    mpsc::{self, Receiver},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ImageSearchTestTarget {
    pub macro_id: u64,
    pub step_id: Option<u64>,
    pub draft_generation: u64,
    pub condition_path: Option<ConditionPath>,
}

#[derive(Debug)]
pub(crate) struct ImageSearchTestCompletion {
    pub target: ImageSearchTestTarget,
    pub result: Result<Option<ImageSearchMatch>, ExecutionDiagnostic>,
}

#[derive(Debug)]
pub(crate) enum ImageSearchTestJob {
    Idle,
    Testing {
        target: ImageSearchTestTarget,
        completion: Receiver<ImageSearchTestCompletion>,
    },
}

impl Default for ImageSearchTestJob {
    fn default() -> Self {
        Self::Idle
    }
}

impl ImageSearchTestJob {
    pub(crate) fn is_testing(&self) -> bool {
        matches!(self, Self::Testing { .. })
    }

    pub(crate) fn start(
        &mut self,
        search: Arc<dyn VisualSearch>,
        target: ImageSearchTestTarget,
        payload: MkImagePayload,
    ) -> Result<(), &'static str> {
        self.start_with_executor(
            search,
            target,
            payload,
            &super::image_authoring_job::ThreadExecutor,
        )
    }

    pub(crate) fn start_with_executor(
        &mut self,
        search: Arc<dyn VisualSearch>,
        target: ImageSearchTestTarget,
        payload: MkImagePayload,
        executor: &dyn ImageAuthoringExecutor,
    ) -> Result<(), &'static str> {
        if self.is_testing() {
            return Err("An image search test is already in progress");
        }
        let (sender, completion) = mpsc::channel();
        let worker_target = target.clone();
        *self = Self::Testing { target, completion };
        executor.execute(Box::new(move || {
            let result = search
                .find_image_match(worker_target.macro_id, &payload)
                .map_err(|error| {
                    error
                        .context("operation", "test image search")
                        .context("macro_id", worker_target.macro_id.to_string())
                        .context("image", payload.image.filename())
                });
            let _ = sender.send(ImageSearchTestCompletion {
                target: worker_target,
                result,
            });
        }));
        Ok(())
    }

    pub(crate) fn cancel(&mut self) {
        *self = Self::Idle;
    }

    pub(crate) fn try_take(&mut self) -> Option<ImageSearchTestCompletion> {
        let Self::Testing { target, completion } = self else {
            return None;
        };
        match completion.try_recv() {
            Ok(done) => {
                *self = Self::Idle;
                Some(done)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                let target = target.clone();
                *self = Self::Idle;
                Some(ImageSearchTestCompletion {
                    target: target.clone(),
                    result: Err(ExecutionDiagnostic::new(
                        DiagnosticKind::Backend,
                        "Image search test worker stopped unexpectedly",
                    )
                    .context("operation", "test image search")
                    .context("macro_id", target.macro_id.to_string())),
                })
            }
        }
    }
}
