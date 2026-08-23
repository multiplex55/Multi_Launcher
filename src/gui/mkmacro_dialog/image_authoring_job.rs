//! Background reference-image authoring jobs.
use crate::mkmacro::{MkMacroStore, StagedImageAsset};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
};

/// Boundary between the UI coordinator and execution of the blocking import.
/// Production uses [`ThreadExecutor`]; tests can queue the closure and decide
/// exactly when it runs.
pub trait ImageAuthoringExecutor {
    fn execute(&self, work: Box<dyn FnOnce() + Send>);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadExecutor;

impl ImageAuthoringExecutor for ThreadExecutor {
    fn execute(&self, work: Box<dyn FnOnce() + Send>) {
        std::thread::spawn(work);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraftToken {
    pub macro_id: u64,
    pub draft_generation: u64,
}

#[derive(Debug)]
pub struct ImageAuthoringCompletion {
    pub token: DraftToken,
    pub result: Result<StagedImageAsset, String>,
}

#[derive(Debug)]
pub enum ImageAuthoringJob {
    Idle,
    Importing {
        token: DraftToken,
        previous_asset_id: u64,
        source: PathBuf,
        completion: Receiver<ImageAuthoringCompletion>,
    },
}

impl Default for ImageAuthoringJob {
    fn default() -> Self {
        Self::Idle
    }
}

impl ImageAuthoringJob {
    pub fn is_importing(&self) -> bool {
        matches!(self, Self::Importing { .. })
    }

    pub fn start(
        &mut self,
        store: Arc<MkMacroStore>,
        token: DraftToken,
        previous_asset_id: u64,
        source: PathBuf,
    ) -> Result<(), &'static str> {
        self.start_with_executor(store, token, previous_asset_id, source, &ThreadExecutor)
    }

    pub fn start_with_executor(
        &mut self,
        store: Arc<MkMacroStore>,
        token: DraftToken,
        previous_asset_id: u64,
        source: PathBuf,
        executor: &dyn ImageAuthoringExecutor,
    ) -> Result<(), &'static str> {
        if self.is_importing() {
            return Err("A reference image import is already in progress");
        }
        let (sender, completion) = mpsc::channel();
        *self = Self::Importing {
            token,
            previous_asset_id,
            source: source.clone(),
            completion,
        };
        executor.execute(Box::new(move || {
            let result = crate::mkmacro::ImageAssetAuthoringService::new(&store)
                .import_png(token.macro_id, &source)
                .map_err(|error| format!("Reference image: {error:#}"));
            let _ = sender.send(ImageAuthoringCompletion { token, result });
        }));
        Ok(())
    }

    pub fn try_take(&mut self) -> Option<(DraftToken, u64, ImageAuthoringCompletion)> {
        let Self::Importing {
            token,
            previous_asset_id,
            completion,
            ..
        } = self
        else {
            return None;
        };
        match completion.try_recv() {
            Ok(done) => Some((*token, *previous_asset_id, done)),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some((
                *token,
                *previous_asset_id,
                ImageAuthoringCompletion {
                    token: *token,
                    result: Err("Reference image: import worker stopped unexpectedly".into()),
                },
            )),
        }
    }
}
