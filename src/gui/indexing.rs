use super::event_channel::EventSender;
use super::{LauncherApp, ViewportWake, WatchEvent};
use crate::actions::Action;
use crate::indexer::coordinator::{
    CoordinatorError, IndexCompletion, IndexConfig, IndexCoordinator, NotifierToken, ShutdownError,
    WorkerTermination,
};
use crate::performance::track_c::EventOrigin;
use std::sync::Arc;

/// App-owned lifecycle for replaceable indexed-action requests.
///
/// The supplied action list remains the published source of truth until a
/// completion matches both this desired configuration and its generation.
pub(super) struct IndexingOwner {
    pub(super) desired: IndexConfig,
    pub(super) expected_generation: Option<u64>,
    accepted_config: Option<IndexConfig>,
    terminal_config: Option<IndexConfig>,
    coordinator: Option<IndexCoordinator>,
    notifier: Option<NotifierToken>,
    closed: bool,
    diagnostic: Option<String>,
    terminal_reported: bool,
}

impl IndexingOwner {
    pub(super) fn new(desired: IndexConfig) -> Self {
        Self {
            accepted_config: Some(desired.clone()),
            desired,
            expected_generation: None,
            terminal_config: None,
            coordinator: None,
            notifier: None,
            closed: false,
            diagnostic: None,
            terminal_reported: false,
        }
    }

    pub(super) fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    fn attach(
        &mut self,
        coordinator: IndexCoordinator,
        event_tx: &EventSender,
        wake: ViewportWake,
    ) -> Result<(), CoordinatorError> {
        let tx = event_tx.with_origin(EventOrigin::IndexCoordinator);
        let token = coordinator.attach_notifier(move || {
            if tx.send(WatchEvent::IndexReady).is_ok() {
                wake.wake();
            }
        })?;
        self.notifier = Some(token);
        self.coordinator = Some(coordinator);
        Ok(())
    }

    pub(super) fn install_startup(
        &mut self,
        coordinator: IndexCoordinator,
        config: IndexConfig,
        generation: u64,
        event_tx: &EventSender,
        wake: ViewportWake,
    ) -> Result<(), CoordinatorError> {
        if self.closed {
            return Err(CoordinatorError::Closed);
        }
        if self.coordinator.is_some() || self.desired != config {
            return Err(CoordinatorError::Superseded {
                requested: generation,
                current: self.expected_generation,
            });
        }
        coordinator.validate_acknowledged_result(generation, &config)?;
        self.attach(coordinator, event_tx, wake)?;
        self.expected_generation = Some(generation);
        self.accepted_config = Some(config);
        self.terminal_config = None;
        Ok(())
    }

    fn ensure_coordinator(
        &mut self,
        event_tx: &EventSender,
        wake: ViewportWake,
    ) -> Result<(), CoordinatorError> {
        if self.coordinator.is_none() {
            self.attach(IndexCoordinator::new()?, event_tx, wake)?;
        }
        Ok(())
    }

    pub(super) fn shutdown(&mut self) -> Result<(), ShutdownError> {
        self.closed = true;
        self.expected_generation = None;
        if let (Some(coordinator), Some(token)) = (&self.coordinator, self.notifier.take()) {
            coordinator.revoke_notifier(token);
        }
        match self.coordinator.as_ref() {
            Some(coordinator) => coordinator.shutdown(),
            None => Ok(()),
        }
    }

    fn take_completion(&self) -> Option<Arc<IndexCompletion>> {
        self.coordinator.as_ref()?.take_result()
    }
}

impl Drop for IndexingOwner {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

impl LauncherApp {
    /// Install the coordinator that produced the already-published startup
    /// index. The startup result must have been acknowledged before transfer.
    pub fn install_startup_indexing(
        &mut self,
        coordinator: IndexCoordinator,
        config: IndexConfig,
        generation: u64,
    ) -> Result<(), CoordinatorError> {
        let wake = ViewportWake::root(&self.egui_ctx);
        let result =
            self.indexing
                .install_startup(coordinator, config, generation, &self.event_tx, wake);
        if let Err(error) = &result {
            self.indexing.diagnostic = Some(error.to_string());
        }
        result
    }

    pub(super) fn request_index_config(&mut self, config: IndexConfig) {
        if self.indexing.closed
            || (self.indexing.desired == config
                && (self.indexing.accepted_config.as_ref() == Some(&config)
                    || self.indexing.terminal_config.as_ref() == Some(&config)))
        {
            return;
        }

        // Reject old work before attempting construction/submission, including
        // when the new request cannot be accepted.
        self.indexing.desired = config.clone();
        self.indexing.expected_generation = None;
        self.indexing.accepted_config = None;
        self.indexing.terminal_config = None;
        self.indexing.diagnostic = None;
        self.indexing.terminal_reported = false;

        if config.roots().is_empty() {
            self.publish_indexed_tail_if_changed(&[]);
            if self.indexing.coordinator.is_none() {
                self.indexing.accepted_config = Some(config);
                return;
            }
        }

        let wake = ViewportWake::root(&self.egui_ctx);
        if let Err(error) = self.indexing.ensure_coordinator(&self.event_tx, wake) {
            self.report_indexing_error(error.to_string());
            return;
        }

        let submission = self
            .indexing
            .coordinator
            .as_ref()
            .expect("coordinator was just installed")
            .submit(config);
        match submission {
            Ok(generation) => {
                self.indexing.expected_generation = Some(generation);
                self.indexing.accepted_config = Some(self.indexing.desired.clone());
            }
            Err(error) => {
                // Construction failures leave the config retryable because
                // no coordinator was installed. Submission failures are
                // terminal for this coordinator; keep their diagnostic and
                // coalesce repeats until a different config is committed.
                self.indexing.terminal_config = Some(self.indexing.desired.clone());
                self.indexing.terminal_reported = true;
                self.report_indexing_error(error.to_string());
            }
        }
    }

    pub(super) fn process_index_ready(&mut self) {
        // Taking first acknowledges even stale or post-close notifications.
        let completion = self.indexing.take_completion();
        if self.indexing.closed {
            return;
        }

        if let Some(completion) = completion {
            let is_current = self.indexing.expected_generation == Some(completion.generation())
                && self.indexing.desired == *completion.config();
            if is_current {
                match completion.outcome() {
                    Ok(actions) => {
                        self.publish_indexed_tail_if_changed(actions);
                        self.indexing.diagnostic = None;
                        self.indexing.terminal_reported = false;
                    }
                    Err(error) => {
                        // A later committed save with this same config may
                        // retry a failed scan on the still-live worker.
                        self.indexing.expected_generation = None;
                        self.indexing.accepted_config = None;
                        self.indexing.terminal_config = None;
                        self.report_indexing_error(error.to_string());
                    }
                }
            }
            return;
        }

        if let Some(termination) = self
            .indexing
            .coordinator
            .as_ref()
            .and_then(IndexCoordinator::worker_termination)
            && !self.indexing.terminal_reported
        {
            self.indexing.terminal_reported = true;
            self.indexing.expected_generation = None;
            self.indexing.accepted_config = None;
            self.indexing.terminal_config = Some(self.indexing.desired.clone());
            self.report_indexing_error(format!("index worker terminated: {termination:?}"));
        }
    }

    fn publish_indexed_tail_if_changed(&mut self, indexed: &[Action]) {
        let custom_len = self.custom_len.min(self.actions.len());
        if self.actions[custom_len..] == *indexed {
            return;
        }
        let custom = self.actions[..custom_len].to_vec();
        self.publish_actions(custom, indexed.iter().cloned());
        crate::actions::bump_actions_version();
    }

    fn report_indexing_error(&mut self, message: String) {
        self.indexing.diagnostic = Some(message.clone());
        self.report_error_message("index.refresh", message);
    }

    pub(super) fn shutdown_indexing(&mut self) -> Result<(), ShutdownError> {
        self.indexing.shutdown()
    }

    #[cfg(test)]
    pub(super) fn install_test_index_coordinator(&mut self, coordinator: IndexCoordinator) {
        let wake = ViewportWake::root(&self.egui_ctx);
        if let Err(error) = self.indexing.attach(coordinator, &self.event_tx, wake) {
            self.indexing.diagnostic = Some(error.to_string());
        }
    }

    #[cfg(test)]
    pub(super) fn index_terminal_for_test(&self) -> Option<WorkerTermination> {
        self.indexing
            .coordinator
            .as_ref()
            .and_then(IndexCoordinator::worker_termination)
    }

    #[cfg(test)]
    pub(super) fn wait_for_index_completion_for_test(
        &self,
        generation: u64,
    ) -> Result<Arc<IndexCompletion>, CoordinatorError> {
        self.indexing
            .coordinator
            .as_ref()
            .ok_or(CoordinatorError::Closed)?
            .wait_for_completion(generation)
    }
}
