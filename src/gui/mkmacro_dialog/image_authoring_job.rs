//! Background reference-image authoring jobs.
use super::image_authoring_destination::ConditionOperationDestination;
use crate::mkmacro::{ImageImportChoice, ImageImportResult, MkImageRef, MkMacroStore};
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

#[cfg(test)]
mod tests {
    use super::*;

    struct DropExecutor;
    impl ImageAuthoringExecutor for DropExecutor {
        fn execute(&self, _work: Box<dyn FnOnce() + Send>) {}
    }

    #[test]
    fn busy_guard_and_disconnect_preserve_job_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let token = DraftToken {
            macro_id: 8,
            draft_generation: 3,
        };
        let destination =
            ImageAuthoringDestination::ConditionImage(ConditionOperationDestination {
                macro_id: 8,
                step_id: Some(21),
                draft_generation: 3,
                path: super::super::image_authoring_destination::ConditionPath::from_indexes(vec![
                    0, 1,
                ]),
                operation:
                    super::super::image_authoring_destination::ConditionImageOperation::ImportPng,
            });
        let mut job = ImageAuthoringJob::default();
        job.start_with_executor(
            Arc::new(store),
            token,
            destination.clone(),
            MkImageRef::from_filename("old.png"),
            "secret/example.png".into(),
            &DropExecutor,
        )
        .unwrap();
        assert!(
            job.start_with_executor(
                Arc::new(MkMacroStore::open(dir.path()).unwrap().0),
                token,
                ImageAuthoringDestination::ImageActionReference,
                MkImageRef::default(),
                "other.png".into(),
                &DropExecutor
            )
            .is_err()
        );
        let (actual_token, actual_destination, previous, source, completion) =
            job.try_take().unwrap();
        assert_eq!(actual_token, token);
        assert_eq!(actual_destination, destination);
        assert_eq!(completion.destination, destination);
        assert_eq!(previous, MkImageRef::from_filename("old.png"));
        assert_eq!(source.file_name().unwrap(), "example.png");
        assert!(
            completion
                .result
                .unwrap_err()
                .contains("stopped unexpectedly")
        );
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageAuthoringDestination {
    ImageActionReference,
    ConditionImage(ConditionOperationDestination),
}

#[derive(Debug)]
pub struct ImageAuthoringCompletion {
    pub token: DraftToken,
    pub destination: ImageAuthoringDestination,
    pub result: Result<ImageImportResult, String>,
}

#[derive(Debug)]
pub enum ImageAuthoringJob {
    Idle,
    Importing {
        token: DraftToken,
        destination: ImageAuthoringDestination,
        previous_image: MkImageRef,
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
        destination: ImageAuthoringDestination,
        previous_image: MkImageRef,
        source: PathBuf,
    ) -> Result<(), &'static str> {
        self.start_with_executor(
            store,
            token,
            destination,
            previous_image,
            source,
            &ThreadExecutor,
        )
    }

    pub fn start_with_executor(
        &mut self,
        store: Arc<MkMacroStore>,
        token: DraftToken,
        destination: ImageAuthoringDestination,
        previous_image: MkImageRef,
        source: PathBuf,
        executor: &dyn ImageAuthoringExecutor,
    ) -> Result<(), &'static str> {
        self.start_with_choice_with_executor(
            store,
            token,
            destination,
            previous_image,
            source,
            None,
            executor,
        )
    }

    pub fn start_with_choice_with_executor(
        &mut self,
        store: Arc<MkMacroStore>,
        token: DraftToken,
        destination: ImageAuthoringDestination,
        previous_image: MkImageRef,
        source: PathBuf,
        choice: Option<ImageImportChoice>,
        executor: &dyn ImageAuthoringExecutor,
    ) -> Result<(), &'static str> {
        if self.is_importing() {
            return Err("A reference image import is already in progress");
        }
        let (sender, completion) = mpsc::channel();
        *self = Self::Importing {
            token,
            destination: destination.clone(),
            previous_image,
            source: source.clone(),
            completion,
        };
        executor.execute(Box::new(move || {
            let service = crate::mkmacro::ImageAssetAuthoringService::new(&store);
            let result = match choice {
                Some(choice) => service.import_png_with_choice(&source, choice),
                None => service.import_png(&source),
            }
            .map_err(|error| format!("Reference image: {error:#}"));
            let _ = sender.send(ImageAuthoringCompletion {
                token,
                destination,
                result,
            });
        }));
        Ok(())
    }

    pub fn try_take(
        &mut self,
    ) -> Option<(
        DraftToken,
        ImageAuthoringDestination,
        MkImageRef,
        PathBuf,
        ImageAuthoringCompletion,
    )> {
        let Self::Importing {
            token,
            destination,
            previous_image,
            source,
            completion,
            ..
        } = self
        else {
            return None;
        };
        match completion.try_recv() {
            Ok(done) => Some((
                *token,
                destination.clone(),
                previous_image.clone(),
                source.clone(),
                done,
            )),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some((
                *token,
                destination.clone(),
                previous_image.clone(),
                source.clone(),
                ImageAuthoringCompletion {
                    token: *token,
                    destination: destination.clone(),
                    result: Err("Reference image: import worker stopped unexpectedly".into()),
                },
            )),
        }
    }
}
