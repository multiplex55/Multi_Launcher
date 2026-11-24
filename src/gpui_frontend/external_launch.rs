//! Single-flight ownership for ordinary external launcher actions.
use std::sync::Arc;

use super::AnyWindowHandle;
use multi_launcher::actions::Action;
use multi_launcher::commands::{
    ActivationSource, CommandBus, CommandError, CommandOutcome, ExternalCommand,
    ExternalCommandHost, HistoryPolicy,
};
use multi_launcher::history::HistoryEntry;

use crate::gpui_restart::{EffectPermit, RestartAdmission};

pub(super) const CLOSE_STATUS: &str =
    "Wait for the application launch to finish before closing Multi Launcher.";

#[derive(Clone)]
pub(crate) struct Settings {
    pub(crate) clear_query_after_run: bool,
    pub(crate) hide_after_run: bool,
    pub(crate) history_limit: usize,
    pub(crate) enable_toasts: bool,
    pub(crate) show_error_toasts: bool,
    pub(crate) show_inline_errors: bool,
    pub(crate) toast_duration: f32,
}

pub(super) struct Stamp {
    pub root: AnyWindowHandle,
    pub generation: u64,
    pub visibility_revision: u64,
    pub query: String,
}

impl Stamp {
    pub fn presentation_is_current(
        &self,
        root: AnyWindowHandle,
        generation: u64,
        revision: u64,
        query: &str,
        visible: bool,
    ) -> bool {
        visible
            && self.root.window_id() == root.window_id()
            && self.generation == generation
            && self.visibility_revision == revision
            && self.query == query
    }

    pub fn may_refocus(&self, root: AnyWindowHandle, active: bool) -> bool {
        active && self.root.window_id() == root.window_id()
    }
}

pub(super) struct Request {
    pub action: Action,
    pub command: ExternalCommand,
    pub query: String,
    pub source: ActivationSource,
    pub settings: Settings,
}

pub(super) struct WorkerResult {
    pub outcome: Result<CommandOutcome, CommandError>,
    pub history_error: Option<String>,
    pub usage_result: Option<Result<u32, String>>,
}

pub(super) struct Receipt {
    pub stamp: Stamp,
    pub request: Request,
    pub result: WorkerResult,
    // The restart fence remains held until the caller consumes this receipt.
    _permit: EffectPermit,
}

struct Pending {
    ticket: u64,
    stamp: Stamp,
    permit: EffectPermit,
}

#[derive(Default)]
pub(super) struct ExternalLaunch {
    pending: Option<Pending>,
    next_ticket: u64,
}

impl ExternalLaunch {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn begin(
        &mut self,
        admission: &Arc<RestartAdmission>,
        stamp: Stamp,
        request: &Request,
    ) -> Option<u64> {
        if self.pending.is_some() || request.query != stamp.query {
            return None;
        }
        let permit = admission.admit()?;
        self.next_ticket = self.next_ticket.wrapping_add(1).max(1);
        let ticket = self.next_ticket;
        self.pending = Some(Pending {
            ticket,
            stamp,
            permit,
        });
        Some(ticket)
    }

    pub fn finish(
        &mut self,
        ticket: u64,
        request: Request,
        result: WorkerResult,
    ) -> Option<Receipt> {
        if self.pending.as_ref()?.ticket != ticket {
            return None;
        }
        let pending = self.pending.take()?;
        Some(Receipt {
            stamp: pending.stamp,
            request,
            result,
            _permit: pending.permit,
        })
    }
}

struct Host {
    settings: Settings,
}
impl ExternalCommandHost for Host {
    fn execute_external_command(
        &mut self,
        command: &ExternalCommand,
        _: &Action,
    ) -> anyhow::Result<()> {
        multi_launcher::actions::exec::launch(&command.target, command.args.as_deref())
    }
    fn external_clear_query_after_run(&self) -> bool {
        self.settings.clear_query_after_run
    }
    fn external_hide_after_run(&self) -> bool {
        self.settings.hide_after_run
    }
    fn external_launcher_should_refocus(&self) -> bool {
        true
    }
}

pub(super) fn run(request: &Request) -> WorkerResult {
    let mut host = Host {
        settings: request.settings.clone(),
    };
    let outcome = CommandBus.dispatch_external(
        &request.command,
        &request.action,
        request.source,
        None,
        &mut host,
    );
    bookkeep(
        outcome,
        &request,
        |entry, limit| {
            multi_launcher::history::append_history(entry, limit).map_err(|error| error.to_string())
        },
        |key| {
            multi_launcher::usage::record_usage(multi_launcher::usage::USAGE_FILE, key)
                .map_err(|error| error.to_string())
        },
    )
}

fn bookkeep(
    outcome: Result<CommandOutcome, CommandError>,
    request: &Request,
    history: impl FnOnce(HistoryEntry, usize) -> Result<(), String>,
    usage: impl FnOnce(&str) -> Result<u32, String>,
) -> WorkerResult {
    let mut history_error = None;
    let mut usage_result = None;
    if outcome
        .as_ref()
        .is_ok_and(|outcome| outcome.history == HistoryPolicy::Record)
    {
        history_error = history(
            HistoryEntry {
                query: request.query.clone(),
                query_lc: String::new(),
                action: request.action.clone(),
                source: Some(request.source.label().to_owned()),
                timestamp: 0,
            },
            request.settings.history_limit,
        )
        .err();
        usage_result = Some(usage(&request.action.action));
    }
    WorkerResult {
        outcome,
        history_error,
        usage_result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookkeeping_attempts_both_stores_after_success_without_relaunch() {
        let request = Request {
            action: Action {
                label: "Tool".into(),
                desc: String::new(),
                action: "tool.exe".into(),
                args: Some("--arg".into()),
            },
            command: ExternalCommand {
                target: "tool.exe".into(),
                args: Some("--arg".into()),
                namespace: None,
            },
            query: "tool".into(),
            source: ActivationSource::Click,
            settings: Settings {
                clear_query_after_run: false,
                hide_after_run: false,
                history_limit: 10,
                enable_toasts: true,
                show_error_toasts: true,
                show_inline_errors: true,
                toast_duration: 2.0,
            },
        };
        let outcome = CommandOutcome {
            history: HistoryPolicy::Record,
            ..CommandOutcome::default()
        };
        let result = bookkeep(
            Ok(outcome),
            &request,
            |entry, limit| {
                assert_eq!(entry.query, "tool");
                assert_eq!(entry.action, request.action);
                assert_eq!(entry.source.as_deref(), Some("click"));
                assert_eq!(limit, 10);
                Err("history failed".into())
            },
            |key| {
                assert_eq!(key, "tool.exe");
                Ok(4)
            },
        );
        assert!(result.outcome.is_ok());
        assert_eq!(result.history_error.as_deref(), Some("history failed"));
        assert_eq!(result.usage_result, Some(Ok(4)));
    }
}
