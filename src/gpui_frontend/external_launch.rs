//! Single-flight ownership for ordinary external launcher actions.
use std::sync::{Arc, atomic::Ordering};

use gpui_kit::{Context, FocusHandle, Focusable as _, Window, WindowId};
use multi_launcher::actions::Action;
use multi_launcher::commands::{
    ActivationSource, CommandBus, CommandError, CommandOutcome, ExternalCommand,
    ExternalCommandHost, HistoryPolicy,
};
use multi_launcher::history::HistoryEntry;

use super::{RootWindow, ForegroundIdentity, current_foreground_identity};
use crate::gpui_restart::{EffectPermit, RestartAdmission};
use gpui_kit::component::{
    WindowExt as _,
    notification::{Notification, NotificationDelivery},
};

pub(super) fn saved_external(action: &Action) -> Option<ExternalCommand> {
    ordinary_external(&multi_launcher::commands::parse_action(action).ok()?).cloned()
}

#[cfg(windows)]
pub(super) fn saved_shortcut(action: &Action) -> Option<ExternalCommand> {
    let command = multi_launcher::commands::parse_action(action).ok()?;
    shortcut_external(action, &command).cloned()
}

#[cfg(windows)]
pub(super) fn shortcut_external<'a>(
    action: &Action,
    command: &'a multi_launcher::commands::Command,
) -> Option<&'a ExternalCommand> {
    if action
        .args
        .as_deref()
        .is_some_and(|args| !args.trim().is_empty())
    {
        return None;
    }
    let multi_launcher::commands::Command::External(command) = command else {
        return None;
    };
    (command.namespace.is_none()
        && command.target == action.action
        && command
            .args
            .as_deref()
            .is_none_or(|args| args.trim().is_empty())
        && multi_launcher::actions::exec::shortcut_path_is_admissible(&command.target))
    .then_some(command)
}

pub(super) fn saved_url(action: &Action) -> Option<ExternalCommand> {
    let command = multi_launcher::commands::parse_action(action).ok()?;
    url_external(action, &command).cloned()
}

pub(super) fn url_external<'a>(
    action: &Action,
    command: &'a multi_launcher::commands::Command,
) -> Option<&'a ExternalCommand> {
    let multi_launcher::commands::Command::External(command) = command else {
        return None;
    };
    (action.args.as_deref().is_none_or(|args| args.trim().is_empty())
        && command.namespace.is_none()
        && command.target == action.action
        && command.args.as_deref().is_none_or(|args| args.trim().is_empty())
        && parse_http_target(&command.target, true).is_some())
        .then_some(command)
}

fn parse_http_target(raw: &str, strict: bool) -> Option<url::Url> {
    let authority = raw
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        .then(|| &raw[7..])
        .or_else(|| {
            raw.get(..8)
                .filter(|prefix| prefix.eq_ignore_ascii_case("https://"))
                .map(|_| &raw[8..])
        });
    if raw.trim() != raw
        || raw.chars().any(char::is_control)
        || !authority.is_some_and(|rest| {
            !rest.is_empty() && !rest.starts_with('/') && !rest.starts_with('\\')
        })
    {
        return None;
    }
    if strict {
        if raw.chars().any(|ch| ch.is_whitespace() || ch == '\\') {
            return None;
        }
        let bytes = raw.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'%'
                && !(index + 2 < bytes.len()
                    && bytes[index + 1].is_ascii_hexdigit()
                    && bytes[index + 2].is_ascii_hexdigit())
            {
                return None;
            }
        }
    }
    let violation = std::cell::Cell::new(false);
    let mark_violation = |_| violation.set(true);
    let parsed = url::Url::options()
        .syntax_violation_callback(strict.then_some(&mark_violation as &dyn Fn(_)))
        .parse(raw)
        .ok()?;
    if violation.get()
        || !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
    {
        return None;
    }
    Some(parsed)
}

/// Match the established saved-document final-leaf extension rule.
pub(super) fn document_extension_supported(target: &str) -> bool {
    target.rsplit(['\\', '/']).next()
        .and_then(|leaf| leaf.rsplit_once('.').map(|(_, extension)| extension))
        .is_some_and(|extension| {
            ["txt", "rtf", "pdf", "docx", "xlsx", "pptx"]
                .iter().any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
}

/// Admit only saved document paths whose external arguments are absent or blank.
pub(super) fn saved_document(action: &Action) -> Option<ExternalCommand> {
    let command = multi_launcher::commands::parse_action(action).ok()?;
    document_external(action, &command).cloned()
}

pub(super) fn document_external<'a>(
    action: &Action,
    command: &'a multi_launcher::commands::Command,
) -> Option<&'a ExternalCommand> {
    if action
        .args
        .as_deref()
        .is_some_and(|args| !args.trim().is_empty())
    {
        return None;
    }
    let multi_launcher::commands::Command::External(command) = command else {
        return None;
    };
    if command.namespace.is_some()
        || command.target != action.action
        || command
            .args
            .as_deref()
            .is_some_and(|args| !args.trim().is_empty())
        || !ordinary_path(&command.target, false)
    {
        return None;
    }
    document_extension_supported(&command.target).then_some(command)
}

pub(super) fn ordinary_external(
    command: &multi_launcher::commands::Command,
) -> Option<&ExternalCommand> {
    let multi_launcher::commands::Command::External(command) = command else {
        return None;
    };
    (command.namespace.is_none() && ordinary_executable(&command.target)).then_some(command)
}

fn ordinary_executable(target: &str) -> bool {
    ordinary_path(target, true)
}

fn ordinary_path(target: &str, executable: bool) -> bool {
    if target.is_empty()
        || target.trim() != target
        || target.starts_with("\\\\?\\")
        || target.starts_with("\\\\.\\")
        || target.starts_with("//?/")
        || target.starts_with("//./")
        || target
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '?' | '*'))
    {
        return false;
    }
    let bytes = target.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if target
        .bytes()
        .enumerate()
        .any(|(i, b)| b == b':' && !(drive && i == 1))
    {
        return false;
    }
    let path = if drive { &target[2..] } else { target };
    if path.is_empty() {
        return false;
    }
    let unc = path.starts_with("\\\\") || path.starts_with("//");
    let parts: Vec<_> = path
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect();
    let drive_root = drive && path.chars().all(|c| c == '\\' || c == '/');
    let current_drive_root = !drive && (path == "\\" || path == "/");
    if (parts.is_empty() && (executable || !(drive_root || current_drive_root)))
        || (unc && parts.len() < if executable { 3 } else { 2 })
        || (executable
            && !std::path::Path::new(parts[parts.len() - 1])
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe")))
    {
        return false;
    }
    parts.iter().all(|part| {
        if *part == "." || *part == ".." {
            return true;
        }
        if part.ends_with(' ') || part.ends_with('.') {
            return false;
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        !matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    })
}

fn presentation_owner_matches<T: PartialEq>(
    origin: Option<&T>,
    current: Option<&T>,
    blocked: bool,
) -> bool {
    !blocked && origin.is_some() && origin == current
}

fn deferred_inline_is_current(
    expected_generation: u64,
    generation: u64,
    expected_query: &str,
    results_query: &str,
    live_query: &str,
    owner_current: bool,
) -> bool {
    owner_current
        && generation == expected_generation
        && results_query == expected_query
        && live_query == expected_query
}

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
    pub root: WindowId,
    pub generation: u64,
    pub visibility_revision: u64,
    pub query: String,
    pub focus: Option<FocusHandle>,
}

impl Stamp {
    pub fn presentation_is_current(
        &self,
        root: WindowId,
        generation: u64,
        revision: u64,
        query: &str,
        visible: bool,
    ) -> bool {
        visible
            && self.root == root
            && self.generation == generation
            && self.visibility_revision == revision
            && self.query == query
    }

    pub fn may_refocus(&self, root: WindowId, active: bool) -> bool {
        active && self.root == root
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AssociationKind {
    Folder,
    Bookmark,
    SavedDocument,
    SavedFolder,
    SavedUrl,
    #[cfg(windows)]
    SavedShortcut,
    Web(super::search_providers::WebProvider),
}

impl AssociationKind {
    pub(super) fn from_provider_name(name: &str) -> Option<Self> {
        match name {
            "folders" => Some(Self::Folder),
            "bookmarks" => Some(Self::Bookmark),
            _ => super::search_providers::WebProvider::from_name(name).map(Self::Web),
        }
    }
}

pub(super) fn provider_association(
    origin: &super::LauncherResultOrigin,
    action: &Action,
) -> Option<(AssociationKind, ExternalCommand)> {
    match origin {
        super::LauncherResultOrigin::Provider(name) => provider_association_by_name(name, action),
        _ => None,
    }
}

pub(super) fn provider_association_by_name(
    name: &str,
    action: &Action,
) -> Option<(AssociationKind, ExternalCommand)> {
    let kind = AssociationKind::from_provider_name(name)?;
    association_command(kind, action).map(|command| (kind, command))
}

fn association_command(kind: AssociationKind, action: &Action) -> Option<ExternalCommand> {
    if action.args.is_some() {
        return None;
    }
    let multi_launcher::commands::Command::External(command) =
        multi_launcher::commands::parse_action(action).ok()?
    else {
        return None;
    };
    if command.namespace.is_some() || command.args.is_some() || command.target != action.action {
        return None;
    }
    let valid = match kind {
        AssociationKind::Folder => ordinary_path(&command.target, false),
        AssociationKind::SavedDocument | AssociationKind::SavedFolder => false,
        #[cfg(windows)]
        AssociationKind::SavedShortcut => false,
        AssociationKind::SavedUrl => false,
        AssociationKind::Bookmark | AssociationKind::Web(_) => {
            parse_http_target(&command.target, false).is_some_and(|url| {
                !matches!(kind, AssociationKind::Web(_)) || url.scheme() == "https"
            })
        }
    };
    valid.then_some(command)
}

fn checked_association(
    kind: AssociationKind,
    action: &Action,
    command: &ExternalCommand,
    query: &str,
) -> anyhow::Result<()> {
    #[cfg(windows)]
    if kind == AssociationKind::SavedShortcut {
        anyhow::ensure!(
            saved_shortcut(action).as_ref() == Some(command),
            "Invalid saved shortcut target"
        );
        return multi_launcher::actions::exec::launch_shortcut(&command.target);
    }
    if kind == AssociationKind::SavedFolder {
        return super::saved_folder::checked_association_with(action, command, |target| {
            multi_launcher::actions::exec::launch_association(target)
        });
    }
    if kind == AssociationKind::SavedUrl {
        return checked_saved_url_with(action, command, |target| {
            multi_launcher::actions::exec::launch_association(target)
        });
    }
    if kind == AssociationKind::SavedDocument {
        return checked_document_association_with(action, command, |target| {
            multi_launcher::actions::exec::launch_association(target)
        });
    }
    if let AssociationKind::Web(provider) = kind {
        return checked_web_association_with(provider, action, command, query, |target| {
            multi_launcher::actions::exec::launch_association(target)
        });
    }
    checked_association_with(
        kind,
        action,
        command,
        multi_launcher::plugins::folders::FOLDERS_FILE,
        multi_launcher::plugins::bookmarks::BOOKMARKS_FILE,
        |target| multi_launcher::actions::exec::launch_association(target),
    )
}

fn checked_association_with(
    kind: AssociationKind,
    action: &Action,
    command: &ExternalCommand,
    folder_path: &str,
    bookmark_path: &str,
    launch: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        association_command(kind, action).as_ref() == Some(command),
        "Invalid provider target"
    );
    let current = match kind {
        AssociationKind::Folder => {
            multi_launcher::plugins::folders::folder_search_result_is_current(folder_path, action)?
        }
        AssociationKind::Bookmark => {
            multi_launcher::plugins::bookmarks::bookmark_search_result_is_current(
                bookmark_path,
                action,
            )?
        }
        AssociationKind::Web(_) | AssociationKind::SavedDocument | AssociationKind::SavedFolder | AssociationKind::SavedUrl => false,
        #[cfg(windows)]
        AssociationKind::SavedShortcut => false,
    };
    anyhow::ensure!(current, "The Search result is stale; refresh and try again");
    if kind == AssociationKind::Folder {
        anyhow::ensure!(
            std::path::Path::new(&command.target).is_dir(),
            "The folder is unavailable"
        );
    }
    launch(&command.target)
}

fn checked_document_association_with(
    action: &Action,
    command: &ExternalCommand,
    launch: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        saved_document(action).as_ref() == Some(command),
        "Invalid saved document target"
    );
    let path = std::path::absolute(&command.target)
        .map_err(|error| anyhow::anyhow!("Document path is unavailable: {error}"))?;
    for ancestor in path.ancestors().filter(|path| !path.as_os_str().is_empty()) {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|error| anyhow::anyhow!("Document path is unavailable: {error}"))?;
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        };
        #[cfg(not(windows))]
        let reparse = metadata.file_type().is_symlink();
        anyhow::ensure!(!reparse, "Document path contains a reparse point");
        if ancestor == path.as_path() {
            anyhow::ensure!(metadata.is_file(), "Document target is not a file");
        }
    }
    launch(&command.target)
}

fn checked_saved_url_with(
    action: &Action,
    command: &ExternalCommand,
    launch: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::ensure!(saved_url(action).as_ref() == Some(command), "Invalid saved URL");
    launch(&command.target)
}

fn checked_web_association_with(
    provider: super::search_providers::WebProvider,
    action: &Action,
    command: &ExternalCommand,
    query: &str,
    launch: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        association_command(AssociationKind::Web(provider), action).as_ref() == Some(command),
        "Invalid provider target"
    );
    anyhow::ensure!(
        provider.is_current(query, action, None, None),
        "The Search result is stale; refresh and try again"
    );
    launch(&command.target)
}

#[derive(Clone, Copy)]
pub(super) enum DiagnosticKind {
    Date,
    Unit,
}

#[derive(Clone)]
pub(super) enum RunCommand {
    External { command: ExternalCommand, association: Option<AssociationKind> },
    Indexed { command: ExternalCommand, authority: super::indexed_applications::LaunchAuthority },
    Favorite(ExternalCommand),
    Copy(String),
    WindowSwitch(multi_launcher::window_activation::CheckedWindowSwitch),
    WindowClose(multi_launcher::window_close::CheckedWindowClose),
    Calculator(super::calculator_search::ScalarResult),
    CalculatorHistory(super::calculator_search::HistoryResult),
    Diagnostic { kind: DiagnosticKind, message: String },
}

#[derive(Clone)]
pub(super) struct Request {
    pub action: Action,
    pub history_label: Option<String>,
    pub command: RunCommand,
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
    window_catalog: bool,
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

    pub fn window_catalog_pending(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| pending.window_catalog)
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
            window_catalog: matches!(&request.command, RunCommand::WindowSwitch(_) | RunCommand::WindowClose(_)),
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
    association: Option<AssociationKind>,
    query: String,
}
impl ExternalCommandHost for Host {
    fn execute_external_command(
        &mut self,
        command: &ExternalCommand,
        action: &Action,
    ) -> anyhow::Result<()> {
        if let Some(kind) = self.association {
            return checked_association(kind, action, command, &self.query);
        }
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

struct IndexedHost<'a> {
    host: &'a Host,
    authority: &'a super::indexed_applications::LaunchAuthority,
}

struct FavoriteHost<'a> { host: &'a Host }
impl ExternalCommandHost for FavoriteHost<'_> {
    fn execute_external_command(&mut self, command: &ExternalCommand, action: &Action) -> anyhow::Result<()> {
        anyhow::ensure!(command.namespace.is_none() && command.target == action.action && command.args == action.args, "Invalid favorite command");
        super::favorite_search::checked_launch_with(action, multi_launcher::plugins::fav::FAV_FILE, multi_launcher::actions::exec::launch)
    }
    fn external_clear_query_after_run(&self) -> bool { self.host.external_clear_query_after_run() }
    fn external_hide_after_run(&self) -> bool { self.host.external_hide_after_run() }
    fn external_launcher_should_refocus(&self) -> bool { self.host.external_launcher_should_refocus() }
}
impl ExternalCommandHost for IndexedHost<'_> {
    fn execute_external_command(&mut self, command: &ExternalCommand, action: &Action) -> anyhow::Result<()> {
        anyhow::ensure!(command.namespace.is_none() && command.args.is_none() && command.target == action.action, "Invalid indexed application command");
        super::indexed_applications::checked_launch_with(
            action,
            &self.authority.roots,
            || self.authority.is_current(),
            |target| multi_launcher::actions::exec::launch_with_identity_if_current(
                target, None, || self.authority.is_current(),
            ).map(|_| ()),
            |path| {
                #[cfg(windows)]
                { multi_launcher::actions::exec::launch_shortcut_if_current(path, || self.authority.is_current()) }
                #[cfg(not(windows))]
                { let _ = path; anyhow::bail!("Indexed shortcuts require Windows") }
            },
            |path| multi_launcher::actions::exec::launch_association_if_current(
                path, || self.authority.is_current(),
            ),
        )
    }
    fn external_clear_query_after_run(&self) -> bool { self.host.external_clear_query_after_run() }
    fn external_hide_after_run(&self) -> bool { self.host.external_hide_after_run() }
    fn external_launcher_should_refocus(&self) -> bool { self.host.external_launcher_should_refocus() }
}

fn execute_request(
    request: &Request,
    host: &mut Host,
    copy: impl FnOnce(&str) -> anyhow::Result<()>,
) -> Result<CommandOutcome, CommandError> {
    match &request.command {
        RunCommand::External { command, .. } => CommandBus.dispatch_external(
            command, &request.action, request.source, None, host,
        ),
        RunCommand::Indexed { command, authority } => {
            CommandBus.dispatch_external(command, &request.action, request.source, None,
                &mut IndexedHost { host, authority })
        }
        RunCommand::Favorite(command) => CommandBus.dispatch_external(command, &request.action, request.source, None, &mut FavoriteHost { host }),
        RunCommand::WindowSwitch(checked) => {
            let effect = multi_launcher::window_activation::activate_checked_window_switch(checked.clone())
                .map_err(anyhow::Error::new);
            CommandBus.complete_checked_window_switch(
                checked.selected.native.hwnd as isize, &request.action, request.source, effect,
                multi_launcher::commands::LauncherRunPreferences {
                    clear_query_after_run: request.settings.clear_query_after_run,
                    hide_after_run: request.settings.hide_after_run,
                    refocus: false,
                },
            )
        }
        RunCommand::WindowClose(checked) => {
            let effect = multi_launcher::window_close::post_checked_window_close(checked.clone())
                .map_err(anyhow::Error::new);
            CommandBus.complete_checked_window_close(
                checked.selected.native.hwnd as isize, &request.action, request.source, effect,
                multi_launcher::commands::LauncherRunPreferences {
                    clear_query_after_run: request.settings.clear_query_after_run,
                    hide_after_run: request.settings.hide_after_run,
                    refocus: true,
                },
            )
        }
        RunCommand::Copy(text) => {
            let effect = copy(text);
            CommandBus.complete_checked_clipboard_text(text.clone(), &request.action, request.source, effect,
                multi_launcher::commands::LauncherRunPreferences {
                    clear_query_after_run: request.settings.clear_query_after_run,
                    hide_after_run: request.settings.hide_after_run,
                    refocus: true,
                })
        }
        RunCommand::Calculator(scalar) => {
            let effect = multi_launcher::actions::calc::copy_result(&scalar.result, scalar.expression.as_deref());
            CommandBus.complete_checked_calculator_result(
                multi_launcher::commands::CalculatorCommand::CopyResult {
                    result: scalar.result.clone(),
                    expression: scalar.expression.clone(),
                },
                &request.action, request.source, effect,
                multi_launcher::commands::LauncherRunPreferences {
                    clear_query_after_run: request.settings.clear_query_after_run,
                    hide_after_run: request.settings.hide_after_run,
                    refocus: true,
                },
            )
        }
        RunCommand::CalculatorHistory(history) => {
            let effect = super::calculator_search::copy_checked_history_with(
                history,
                || multi_launcher::plugins::calc_history::load_history(
                    multi_launcher::plugins::calc_history::CALC_HISTORY_FILE,
                ),
                copy,
            );
            CommandBus.complete_checked_calculator_result(
                multi_launcher::commands::CalculatorCommand::CopyHistory { index: history.index },
                &request.action, request.source, effect,
                multi_launcher::commands::LauncherRunPreferences {
                    clear_query_after_run: request.settings.clear_query_after_run,
                    hide_after_run: request.settings.hide_after_run,
                    refocus: true,
                },
            )
        }
        RunCommand::Diagnostic { kind, message } => {
            let domain = match kind {
                DiagnosticKind::Date => "date_arithmetic",
                DiagnosticKind::Unit => "unit_convert",
            };
            Err(CommandError::new(domain, message.clone()).with_gui_failure_policy())
        }
    }
}

pub(super) fn run(request: &Request) -> WorkerResult {
    let mut host = Host {
        settings: request.settings.clone(),
        association: match &request.command {
            RunCommand::External { association, .. } => *association,
            RunCommand::WindowSwitch(_) | RunCommand::WindowClose(_) | RunCommand::Indexed { .. } | RunCommand::Favorite(_) | RunCommand::Copy(_) | RunCommand::Calculator(_) | RunCommand::CalculatorHistory(_) | RunCommand::Diagnostic { .. } => None,
        },
        query: request.query.clone(),
    };
    let outcome = execute_request(request, &mut host, multi_launcher::actions::clipboard::set_text);
    if let Ok(outcome) = &outcome {
        if let multi_launcher::commands::FavoriteLogPolicy::Ran { label, command } =
            &outcome.favorite_log
        {
            tracing::info!(fav = %label, command = %command, "ran favorite");
        }
    } else if let Err(error) = &outcome {
        multi_launcher::toast_log::append_toast_log(&format!("[error:launcher] {}", error.message));
        if let Some(favorite) = &error.favorite {
            tracing::warn!(fav = %favorite, error = %error.message, "favorite failed");
        }
    }
    let result = bookkeep(
        outcome,
        &request,
        |entry, limit| {
            multi_launcher::history::append_history(entry, limit).map_err(|error| error.to_string())
        },
        |key| {
            multi_launcher::usage::record_usage(multi_launcher::usage::USAGE_FILE, key)
                .map_err(|error| error.to_string())
        },
    );
    for error in result
        .history_error
        .iter()
        .chain(result.usage_result.as_ref().and_then(|r| r.as_ref().err()))
    {
        multi_launcher::toast_log::append_toast_log(&format!("[error:launcher] {error}"));
    }
    result
}

fn switch_receipt_foreground_current(
    success: bool, current: bool, foreground: Option<ForegroundIdentity>,
    initiating: ForegroundIdentity, target: ForegroundIdentity, root_active: bool,
) -> bool {
    current && if success { foreground == Some(target) } else { root_active && foreground == Some(initiating) }
}

struct ExternalNotification;

impl RootWindow {
    fn external_owner_is_current(
        &self,
        origin: Option<&FocusHandle>,
        window: &Window,
        cx: &Context<Self>,
    ) -> bool {
        let contexts = window.context_stack();
        let blocked = self.action_sheet.is_some()
            || self.context_favorite_popup.is_some()
            || self.file_menu_open
            || self.settings_menu_open
            || self.help_menu_open
            || super::context_stack_has_primary(&contexts, "PopupMenu")
            || super::context_stack_has_primary(&contexts, "AlertDialog")
            || super::context_stack_has_primary(&contexts, "Dialog");
        let current = window.focused(cx);
        presentation_owner_matches(origin, current.as_ref(), blocked)
    }

    pub(super) fn start_external_launch(
        &mut self,
        action: Action,
        command: ExternalCommand,
        query: String,
        source: ActivationSource,
        association: Option<AssociationKind>,
        history_label: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_run(action, RunCommand::External { command, association }, query, source, history_label, window, cx);
    }

    pub(super) fn start_window_switch(
        &mut self, action: Action, checked: multi_launcher::window_activation::CheckedWindowSwitch,
        query: String, source: ActivationSource, window: &mut Window, cx: &mut Context<Self>,
    ) {
        self.start_run(action, RunCommand::WindowSwitch(checked), query, source, None, window, cx);
    }

    pub(super) fn start_window_close(
        &mut self, action: Action, checked: multi_launcher::window_close::CheckedWindowClose,
        query: String, source: ActivationSource, window: &mut Window, cx: &mut Context<Self>,
    ) {
        self.start_run(action, RunCommand::WindowClose(checked), query, source, None, window, cx);
    }

    pub(super) fn start_favorite_launch(&mut self, action: Action, query: String, source: ActivationSource, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = super::external_launch::saved_external(&action) else { return; };
        self.start_run(action, RunCommand::Favorite(command), query, source, None, window, cx);
    }

    pub(super) fn start_indexed_launch(
        &mut self, action: Action, command: ExternalCommand, authority: super::indexed_applications::LaunchAuthority,
        query: String, source: ActivationSource, history_label: Option<String>,
        window: &mut Window, cx: &mut Context<Self>,
    ) {
        self.start_run(action, RunCommand::Indexed { command, authority }, query, source, history_label, window, cx);
    }

    pub(super) fn start_date_launch(
        &mut self,
        action: Action,
        route: super::date_search::DateRoute,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match route {
            super::date_search::DateRoute::Copy(text) => RunCommand::Copy(text),
            super::date_search::DateRoute::Diagnostic(message) => RunCommand::Diagnostic {
                kind: DiagnosticKind::Date,
                message,
            },
        };
        self.start_run(action, command, query, source, None, window, cx);
    }

    pub(super) fn start_calculator_launch(
        &mut self,
        action: Action,
        scalar: super::calculator_search::ScalarResult,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_run(action, RunCommand::Calculator(scalar), query, source, None, window, cx);
    }

    pub(super) fn start_calculator_history_launch(
        &mut self,
        action: Action,
        history: super::calculator_search::HistoryResult,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_run(action, RunCommand::CalculatorHistory(history), query, source, None, window, cx);
    }

    pub(super) fn start_unit_launch(
        &mut self,
        action: Action,
        route: super::unit_convert_search::UnitRoute,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match route {
            super::unit_convert_search::UnitRoute::Copy(text) => RunCommand::Copy(text),
            super::unit_convert_search::UnitRoute::Diagnostic(message) => RunCommand::Diagnostic {
                kind: DiagnosticKind::Unit,
                message,
            },
        };
        self.start_run(action, command, query, source, None, window, cx);
    }

    pub(super) fn start_copy_launch(
        &mut self,
        action: Action,
        text: String,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_run(action, RunCommand::Copy(text), query, source, None, window, cx);
    }

    fn start_run(
        &mut self,
        action: Action,
        command: RunCommand,
        query: String,
        source: ActivationSource,
        history_label: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root_window else {
            return;
        };
        let stamp = Stamp {
            root: root.window_id(),
            generation: self.launcher_results.generation,
            visibility_revision: self
                .root_visibility
                .request_revision
                .load(Ordering::Acquire),
            query: query.clone(),
            focus: window.focused(cx),
        };
        let request = Request {
            action,
            history_label,
            command,
            query,
            source,
            settings: self.external_launch_preferences.clone(),
        };
        let Some(ticket) = self
            .external_launch
            .begin(&self.restart_admission, stamp, &request)
        else {
            return;
        };
        self.query_history.reset();
        let task = cx.background_executor().spawn(async move {
            let result = run(&request);
            (request, result)
        });
        let root = cx.entity().downgrade();
        let window_handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let (request, result) = task.await;
            let _ = window_handle.update(cx, |_, window, app| {
                let _ = root.update(app, |root, cx| {
                    let Some(receipt) = root.external_launch.finish(ticket, request, result) else {
                        return;
                    };
                    root.apply_external_receipt(ticket, receipt, window, cx);
                    if root.window_search.take_deferred_update() {
                        root.root_visibility.notify_window_search();
                    }
                });
            });
        })
        .detach();
    }

    fn apply_external_receipt(
        &mut self,
        ticket: u64,
        receipt: Receipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_status.as_deref() == Some(CLOSE_STATUS) {
            self.restart_status = None;
        }
        if receipt.result.outcome.is_ok() {
            let count = self
                .search_catalog
                .usage
                .entry(receipt.request.action.action.clone())
                .or_default();
            *count = match &receipt.result.usage_result {
                Some(Ok(canonical)) => *canonical,
                _ => count.saturating_add(1),
            };
        }
        let Some(root) = self.root_window else {
            cx.notify();
            return;
        };
        let revision = self
            .root_visibility
            .request_revision
            .load(Ordering::Acquire);
        let mut visible = self.root_visibility.visible.load(Ordering::Acquire)
            && self.root_visibility.active.load(Ordering::Acquire);
        #[cfg(windows)]
        {
            visible &= self.native_window.is_some_and(|native| {
                self.root_visibility
                    .is_current_visible_revision(revision, native)
            });
        }
        let switch_target = match &receipt.request.command {
            RunCommand::WindowSwitch(checked) => Some(ForegroundIdentity {
                hwnd: checked.selected.native.hwnd,
                process_id: checked.selected.native.pid,
                thread_id: checked.selected.native.tid,
            }),
            _ => None,
        };
        let close_target = match &receipt.request.command {
            RunCommand::WindowClose(checked) => Some(ForegroundIdentity {
                hwnd: checked.selected.native.hwnd,
                process_id: checked.selected.native.pid,
                thread_id: checked.selected.native.tid,
            }),
            _ => None,
        };
        let switch_current = match &receipt.request.command {
            RunCommand::WindowSwitch(checked) => {
                let initiating = ForegroundIdentity {
                    hwnd: checked.initiating.hwnd,
                    process_id: checked.initiating.pid,
                    thread_id: checked.initiating.tid,
                };
                let target = ForegroundIdentity {
                    hwnd: checked.selected.native.hwnd,
                    process_id: checked.selected.native.pid,
                    thread_id: checked.selected.native.tid,
                };
                switch_receipt_foreground_current(
                    receipt.result.outcome.is_ok(), (checked.is_current)(), current_foreground_identity(),
                    initiating, target, window.is_window_active(),
                )
            }
            RunCommand::WindowClose(checked) => {
                let initiating = ForegroundIdentity {
                    hwnd: checked.initiating.hwnd,
                    process_id: checked.initiating.pid,
                    thread_id: checked.initiating.tid,
                };
                (checked.is_current)() && window.is_window_active()
                    && current_foreground_identity() == Some(initiating)
            }
            _ => true,
        };
        let local_focus_owned = self.external_owner_is_current(receipt.stamp.focus.as_ref(), window, cx);
        let current = receipt.stamp.presentation_is_current(
            root.window_id(),
            self.launcher_results.generation,
            revision,
            self.query.read(cx).value().as_str(),
            visible,
        ) && window.window_handle().window_id() == root.window_id()
            && local_focus_owned && switch_current;
        if !current {
            cx.notify();
            return;
        }
        let focus_owned = receipt
            .stamp
            .may_refocus(root.window_id(), window.is_window_active())
            && local_focus_owned;
        let settings = &receipt.request.settings;
        let mut changed_query = None;
        match &receipt.result.outcome {
            Ok(outcome) => {
                self.launcher_search_error = None;
                if let multi_launcher::commands::QueryPolicy::Set(query) = &outcome.query {
                    changed_query = Some(query.clone());
                    self.query_history.reset();
                    self.search_completion.clear_for_query_replacement();
                    self.query
                        .update(cx, |state, cx| state.replace_all(query.clone(), window, cx));
                }
                if outcome.visibility == multi_launcher::commands::VisibilityPolicy::Hide {
                    if let Some(target) = switch_target {
                        self.root_visibility.request_window_switch_hide(
                            revision, target, changed_query.as_deref().unwrap_or(&receipt.request.query),
                        );
                    } else if let Some(target) = close_target {
                        if let RunCommand::WindowClose(checked) = &receipt.request.command {
                            let root = ForegroundIdentity {
                                hwnd: checked.initiating.hwnd,
                                process_id: checked.initiating.pid,
                                thread_id: checked.initiating.tid,
                            };
                            self.root_visibility.request_window_close_hide(
                                revision, root, target, changed_query.as_deref().unwrap_or(&receipt.request.query),
                            );
                        }
                    } else {
                        self.root_visibility.request_external_hide(revision);
                    }
                }
                if outcome.focus && focus_owned && switch_target.is_none() {
                    self.query.focus_handle(cx).focus(window, cx);
                }
                if settings.enable_toasts {
                    for (index, toast) in outcome.toasts.iter().enumerate() {
                        let message = match toast {
                            multi_launcher::commands::ToastPolicy::Launched(label) => {
                                format!("Launched {label}")
                            }
                            multi_launcher::commands::ToastPolicy::Copied(label) => {
                                format!("Copied {label}")
                            }
                            multi_launcher::commands::ToastPolicy::Info(message)
                            | multi_launcher::commands::ToastPolicy::Success(message)
                            | multi_launcher::commands::ToastPolicy::Error(message) => {
                                message.clone()
                            }
                        };
                        self.external_notification(
                            ticket,
                            index,
                            message,
                            false,
                            settings.toast_duration,
                            window,
                            cx,
                        );
                    }
                }
            }
            Err(error) => {
                if settings.show_inline_errors {
                    self.launcher_search_error = Some(error.message.clone());
                }
                if error.toast && settings.enable_toasts && settings.show_error_toasts {
                    self.external_notification(
                        ticket,
                        0,
                        error.message.clone(),
                        true,
                        settings.toast_duration,
                        window,
                        cx,
                    );
                }
                if error.refocus && focus_owned {
                    self.query.focus_handle(cx).focus(window, cx);
                }
            }
        }
        let mut deferred_inline = None;
        for (index, error) in receipt
            .result
            .history_error
            .iter()
            .chain(
                receipt
                    .result
                    .usage_result
                    .as_ref()
                    .and_then(|r| r.as_ref().err()),
            )
            .enumerate()
        {
            let message = if matches!(&receipt.request.command, RunCommand::Copy(_) | RunCommand::Calculator(_) | RunCommand::CalculatorHistory(_)) {
                format!("Copied, but bookkeeping failed: {error}")
            } else {
                format!("Application launched, but bookkeeping failed: {error}")
            };
            if settings.show_inline_errors {
                if changed_query.is_some() {
                    deferred_inline = Some(message.clone());
                } else {
                    self.launcher_search_error = Some(message.clone());
                }
            }
            if settings.enable_toasts && settings.show_error_toasts {
                self.external_notification(
                    ticket,
                    index + 100,
                    message,
                    true,
                    settings.toast_duration,
                    window,
                    cx,
                );
            }
        }
        if let (Some(expected_query), Some(message)) = (changed_query, deferred_inline) {
            let expected_generation = receipt.stamp.generation.wrapping_add(1).max(1);
            let origin_focus = receipt.stamp.focus.clone();
            let root_id = root.window_id();
            let weak_root = cx.entity().downgrade();
            window.defer(cx, move |window, app| {
                let _ = weak_root.update(app, |root, cx| {
                    if root
                        .root_window
                        .is_some_and(|handle| handle.window_id() == root_id)
                        && window.window_handle().window_id() == root_id
                        && root
                            .root_visibility
                            .request_revision
                            .load(Ordering::Acquire)
                            == revision
                        && root.root_visibility.visible.load(Ordering::Acquire)
                        && deferred_inline_is_current(
                            expected_generation,
                            root.launcher_results.generation,
                            &expected_query,
                            &root.launcher_results.query,
                            root.query.read(cx).value().as_str(),
                            root.external_owner_is_current(origin_focus.as_ref(), window, cx),
                        )
                    {
                        root.launcher_search_error = Some(message);
                        cx.notify();
                    }
                });
            });
        }
        cx.notify();
    }

    fn external_notification(
        &self,
        ticket: u64,
        index: usize,
        message: String,
        error: bool,
        duration: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        multi_launcher::toast_log::append_toast_log(&message);
        let key = format!("{ticket}:{index}");
        let notification = if error {
            Notification::error(message)
        } else {
            Notification::success(message)
        };
        window.push_notification(
            notification
                .id1::<ExternalNotification>(key.clone())
                .delivery(NotificationDelivery::InApp)
                .autohide(false),
            cx,
        );
        let window_handle = window.window_handle();
        let delay = std::time::Duration::try_from_secs_f32(duration)
            .unwrap_or(std::time::Duration::from_secs(5));
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(delay).await;
            let _ = window_handle.update(cx, |_, window, app| {
                window.remove_notification1::<ExternalNotification>(key, app)
            });
        })
        .detach();
    }
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
                action: {
                    let mut action = request.action.clone();
                    if let Some(label) = &request.history_label {
                        action.label = label.clone();
                    }
                    action
                },
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
    fn window_switch_receipt_separates_success_and_failure_foreground_ownership() {
        let root = ForegroundIdentity { hwnd: 1, process_id: 2, thread_id: 3 };
        let target = ForegroundIdentity { hwnd: 4, process_id: 5, thread_id: 6 };
        let other = ForegroundIdentity { hwnd: 7, ..target };
        assert!(switch_receipt_foreground_current(true, true, Some(target), root, target, false));
        assert!(!switch_receipt_foreground_current(true, true, Some(root), root, target, true));
        assert!(switch_receipt_foreground_current(false, true, Some(root), root, target, true));
        assert!(!switch_receipt_foreground_current(false, true, Some(other), root, target, true));
        assert!(!switch_receipt_foreground_current(true, false, Some(target), root, target, false));
    }

    #[test]
    fn base_convert_copy_preserves_literal_spaces_and_unicode() {
        use multi_launcher::plugin::Plugin;
        let query = "CONVERT 20436166c3a920 hex to text";
        let action = multi_launcher::plugins::base_convert::BaseConvertPlugin
            .search(query)
            .remove(0);
        let text = super::super::base_convert_search::classify("base_convert", &action).unwrap();
        let settings = Settings {
            clear_query_after_run: false,
            hide_after_run: false,
            history_limit: 10,
            enable_toasts: true,
            show_error_toasts: true,
            show_inline_errors: true,
            toast_duration: 1.0,
        };
        let request = Request {
            history_label: None,
            action: action.clone(),
            command: RunCommand::Copy(text),
            query: query.into(),
            source: ActivationSource::Click,
            settings: settings.clone(),
        };
        let mut host = Host {
            settings,
            association: None,
            query: query.into(),
        };
        let mut copied = None;
        let outcome = execute_request(&request, &mut host, |text| {
            copied = Some(text.to_owned());
            Ok(())
        }).unwrap();
        assert_eq!(copied.as_deref(), Some(" Café "));
        assert!(matches!(
            &outcome.toasts[0],
            multi_launcher::commands::ToastPolicy::Copied(_)
        ));
        let mut history = None;
        let result = bookkeep(Ok(outcome), &request, |entry, _| {
            history = Some(entry);
            Ok(())
        }, |_| Ok(1));
        assert!(result.outcome.is_ok());
        assert_eq!(history.unwrap().action, action);
    }

    #[test]
    fn date_copy_uses_exact_published_text_and_diagnostics_never_copy() {
        use crate::gpui_frontend::date_search::{DateRoute, classify};
        let settings = Settings {
            clear_query_after_run: false,
            hide_after_run: false,
            history_limit: 10,
            enable_toasts: true,
            show_error_toasts: true,
            show_inline_errors: true,
            toast_duration: 1.0,
        };
        let query = "date 1 month after 2026-01-31";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 5)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let action = multi_launcher::plugins::date_arithmetic::DateArithmeticPlugin
            .search_with_reference(query, now)
            .remove(0);
        assert_eq!(action.action, "clipboard:2026-02-28");
        assert_ne!(action.label, "2026-02-28");
        let route = classify("date_arithmetic", &action).unwrap();
        let mut request = Request {
            history_label: None,
            action: action.clone(),
            command: match route {
                DateRoute::Copy(text) => RunCommand::Copy(text),
                DateRoute::Diagnostic(message) => RunCommand::Diagnostic { kind: DiagnosticKind::Date, message },
            },
            query: query.into(),
            source: ActivationSource::Enter,
            settings: settings.clone(),
        };
        let mut host = Host {
            settings,
            association: None,
            query: request.query.clone(),
        };
        let mut copied = Vec::new();
        let outcome = execute_request(&request, &mut host, |text| {
            copied.push(text.to_owned());
            Ok(())
        }).unwrap();
        assert_eq!(copied, ["2026-02-28"]);
        assert_eq!(outcome.history, HistoryPolicy::Record);
        assert!(matches!(&outcome.toasts[0], multi_launcher::commands::ToastPolicy::Copied(_)));
        let mut recorded = None;
        let mut usage_key = None;
        let result = bookkeep(Ok(outcome), &request, |entry, limit| {
            assert_eq!(limit, 10);
            recorded = Some(entry);
            Ok(())
        }, |key| {
            usage_key = Some(key.to_owned());
            Ok(1)
        });
        assert!(result.outcome.is_ok());
        let recorded = recorded.unwrap();
        assert_eq!(recorded.action, action);
        assert_eq!(recorded.query, query);
        assert_eq!(recorded.source.as_deref(), Some("enter"));
        assert_eq!(usage_key.as_deref(), Some("clipboard:2026-02-28"));

        let error = execute_request(&request, &mut host, |_| anyhow::bail!("clipboard unavailable"))
            .unwrap_err();
        assert!(error.message.contains("clipboard unavailable"));
        request.command = RunCommand::Diagnostic { kind: DiagnosticKind::Date, message: "bad date".into() };
        let error = execute_request(&request, &mut host, |_| panic!("diagnostic copied")).unwrap_err();
        assert_eq!(error.message, "bad date");
        let result = bookkeep(
            Err(error),
            &request,
            |_, _| panic!("diagnostic history"),
            |_| panic!("diagnostic usage"),
        );
        assert!(result.outcome.is_err());
        request.command = RunCommand::Diagnostic {
            kind: DiagnosticKind::Unit,
            message: "Cannot convert length to mass".into(),
        };
        let error = execute_request(&request, &mut host, |_| panic!("unit diagnostic copied"))
            .unwrap_err();
        assert_eq!(error.domain, "unit_convert");
        assert_eq!(error.message, "Cannot convert length to mass");
        let result = bookkeep(
            Err(error), &request,
            |_, _| panic!("unit diagnostic history"),
            |_| panic!("unit diagnostic usage"),
        );
        assert!(result.outcome.is_err());
    }

    #[test]
    fn provider_paths_allow_directory_roots_and_require_literal_url_authority() {
        use super::{AssociationKind, association_command, ordinary_executable, ordinary_path};
        assert!(ordinary_path(r"C:\", false));
        assert!(ordinary_path(r"\", false));
        assert!(ordinary_path("/", false));
        assert!(!ordinary_path("C:", false));
        assert!(!ordinary_path(r"\\server", false));
        assert!(ordinary_path(r"\\server\share", false));
        assert!(!ordinary_executable(r"C:\"));
        assert!(!ordinary_executable(r"\\server\share"));
        let mut action = Action {
            label: "URL".into(),
            desc: "Bookmark".into(),
            action: "HTTPS://example.com/x".into(),
            args: None,
        };
        assert!(association_command(AssociationKind::Bookmark, &action).is_some());
        for invalid in [
            "https:example.com",
            "https:///example.com",
            "https:////example.com",
            "https://",
        ] {
            action.action = invalid.into();
            assert!(
                association_command(AssociationKind::Bookmark, &action).is_none(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn saved_url_association_forwards_literal_and_rejects_invalid_shapes() {
        use super::{checked_saved_url_with, saved_url};
        let mut action = Action {
            label: "URL".into(), desc: String::new(),
            action: "HTTPS://Example.COM:443/a.exe?q=Mixed%2fValue#Part".into(),
            args: Some(" \t".into()),
        };
        let command = saved_url(&action).expect("valid literal URL");
        let mut calls = Vec::new();
        checked_saved_url_with(&action, &command, |target| { calls.push(target.to_owned()); Ok(()) }).unwrap();
        assert_eq!(calls, [action.action.clone()]);
        action.args = Some("--extra".into());
        assert!(saved_url(&action).is_none());
        assert!(checked_saved_url_with(&action, &command, |_| panic!("invalid URL launched")).is_err());
        action.args = None;
        for invalid in ["https:example.com", "https:///example.com", "https://", "https://example.com/a b", "https://example.com/\\path", "https://example.com/%", "https://example.com/%2X", "https://example.com/a\"b", "https://user@@example.com/", "ftp://example.com", "https://example.com/\n"] {
            action.action = invalid.into();
            assert!(saved_url(&action).is_none(), "{invalid:?}");
        }
        action.action = "http://127.0.0.1:8123/a%2Fb".into();
        assert!(saved_url(&action).is_some());
    }

    #[test]
    fn document_extension_rule_preserves_final_leaf_and_leading_dot() {
        for path in [r"C:\Docs\a.TXT", "C:/Docs/a.rtf", r"C:\Docs\a.pdf", r"C:\Docs\a.docx", r"C:\Docs\a.xlsx", r"C:\Docs\a.pptx", r"C:\Docs\.txt"] {
            assert!(super::document_extension_supported(path), "{path}");
        }
        for path in [r"C:\Docs.txt\item", r"C:\Docs\a.txt.exe", r"C:\Docs\a", r"C:\Docs\a.", r"C:\Docs\a.lnk"] {
            assert!(!super::document_extension_supported(path), "{path}");
        }
    }

    #[test]
    fn saved_document_association_checks_file_and_forwards_exact_target() {
        use super::{checked_document_association_with, saved_document};
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Mixed Case.PDF");
        std::fs::write(&file, b"document").unwrap();
        let action = Action {
            label: "Document".into(),
            desc: String::new(),
            action: file.to_str().unwrap().to_owned(),
            args: Some("  ".into()),
        };
        let command = saved_document(&action).expect("ordinary document");
        let mut calls = Vec::new();
        checked_document_association_with(&action, &command, |target| {
            calls.push(target.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, [action.action.clone()]);
        std::fs::remove_file(&file).unwrap();
        assert!(
            checked_document_association_with(&action, &command, |_| panic!(
                "missing file launched"
            ))
            .is_err()
        );
        std::fs::create_dir(&file).unwrap();
        assert!(
            checked_document_association_with(&action, &command, |_| panic!("directory launched"))
                .is_err()
        );
        let mut invalid = action.clone();
        for extension in ["txt", "rtf", "pdf", "docx", "xlsx", "pptx"] {
            invalid.action = format!(r"C:\Docs\document.{extension}");
            invalid.args = None;
            assert!(saved_document(&invalid).is_some());
        }
        invalid = action.clone();
        invalid.args = Some("--argument".into());
        assert!(saved_document(&invalid).is_none());
        for path in [
            "https://example.com/file.pdf",
            r"C:\x\file.lnk",
            r"C:\x\file.exe",
            r"\\?\C:\x\file.txt",
        ] {
            invalid.action = path.into();
            invalid.args = None;
            assert!(saved_document(&invalid).is_none());
        }
    }

    #[cfg(unix)]
    #[test]
    fn saved_document_rejects_symlink_components() {
        use super::{checked_document_association_with, saved_document};
        let dir = tempfile::tempdir().unwrap();
        let actual = dir.path().join("actual");
        std::fs::create_dir(&actual).unwrap();
        std::fs::write(actual.join("file.txt"), b"document").unwrap();
        std::os::unix::fs::symlink(&actual, dir.path().join("link")).unwrap();
        let action = Action {
            label: "Document".into(),
            desc: String::new(),
            action: dir
                .path()
                .join("link/file.txt")
                .to_str()
                .unwrap()
                .to_owned(),
            args: None,
        };
        let command = saved_document(&action).unwrap();
        assert!(
            checked_document_association_with(&action, &command, |_| panic!("symlink launched"))
                .is_err()
        );
    }

    #[test]
    fn web_association_requires_exact_current_stock_row_and_forwards_raw_url() {
        use super::{checked_web_association_with, provider_association, AssociationKind};
        use crate::gpui_frontend::{LauncherResultOrigin, search_providers::WebProvider};
        for (provider, name, query) in [
            (WebProvider::Google, "web_search", "g Café+Tea"),
            (WebProvider::Reddit, "reddit", "red snow & ice"),
            (WebProvider::Runescape, "runescape_search", "rs dragon sword"),
            (WebProvider::Runescape, "runescape_search", "osrs dragon sword"),
            (WebProvider::Weather, "weather", "weather New York"),
            (WebProvider::Wikipedia, "wikipedia", "wiki space & time"),
            (WebProvider::Youtube, "youtube", "yt rust language"),
        ] {
            let action = provider.plugin().search(query).remove(0);
            let origin = LauncherResultOrigin::Provider(name.into());
            let (kind, command) = provider_association(&origin, &action).unwrap();
            assert_eq!(kind, AssociationKind::Web(provider));
            let mut forwarded = None;
            checked_web_association_with(provider, &action, &command, query, |target| {
                forwarded = Some(target.to_owned()); Ok(())
            }).unwrap();
            assert_eq!(forwarded.as_deref(), Some(action.action.as_str()));
            assert!(checked_web_association_with(provider, &action, &command, "other query", |_| panic!("stale row launched")).is_err());
            let mut modified = action.clone();
            modified.desc.push('!');
            assert!(checked_web_association_with(provider, &modified, &command, query, |_| panic!("modified row launched")).is_err());
            modified = action.clone();
            modified.args = Some(String::new());
            assert!(provider_association(&origin, &modified).is_none());
            assert!(provider_association(&LauncherResultOrigin::Saved, &action).is_none());
        }
    }

    #[test]
    fn provider_association_revalidates_before_forwarding_exact_targets() {
        use super::{AssociationKind, checked_association_with, provider_association};
        use crate::gpui_frontend::LauncherResultOrigin;
        use std::cell::RefCell;
        let dir = tempfile::tempdir().unwrap();
        let bookmarks = dir.path().join("bookmarks.json");
        let folders = dir.path().join("folders.json");
        let bookmark_path = bookmarks.to_str().unwrap();
        let folder_path = folders.to_str().unwrap();
        let raw = "https://example.com/a.exe?token=Raw%2FValue";
        std::fs::write(
            &bookmarks,
            serde_json::json!([{ "url": raw, "alias": "Alias" }]).to_string(),
        )
        .unwrap();
        let action = Action {
            label: "Alias".into(),
            desc: "Bookmark".into(),
            action: raw.into(),
            args: None,
        };
        let origin = LauncherResultOrigin::Provider("bookmarks".into());
        let (kind, command) = provider_association(&origin, &action).unwrap();
        assert_eq!(kind, AssociationKind::Bookmark);
        let calls = RefCell::new(Vec::new());
        let run = || {
            checked_association_with(
                kind,
                &action,
                &command,
                folder_path,
                bookmark_path,
                |target| {
                    calls.borrow_mut().push(target.to_owned());
                    Ok(())
                },
            )
        };
        run().unwrap();
        assert_eq!(&*calls.borrow(), &[raw.to_owned()]);
        std::fs::write(&bookmarks, "[]").unwrap();
        assert!(run().is_err());
        std::fs::write(&bookmarks, "{broken").unwrap();
        assert!(run().is_err());
        assert_eq!(calls.borrow().len(), 1);
        assert!(provider_association(&LauncherResultOrigin::Saved, &action).is_none());
        let mut changed = action.clone();
        changed.args = Some(String::new());
        assert!(provider_association(&origin, &changed).is_none());

        let folder = dir.path().join("folder.exe");
        std::fs::create_dir(&folder).unwrap();
        let target = folder.to_str().unwrap();
        std::fs::write(
            &folders,
            serde_json::json!([{ "label": "Folder", "path": target }]).to_string(),
        )
        .unwrap();
        let action = Action {
            label: "Folder".into(),
            desc: target.into(),
            action: target.into(),
            args: None,
        };
        let (kind, command) =
            provider_association(&LauncherResultOrigin::Provider("folders".into()), &action)
                .unwrap();
        checked_association_with(
            kind,
            &action,
            &command,
            folder_path,
            bookmark_path,
            |target| {
                calls.borrow_mut().push(target.to_owned());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls.borrow().last().unwrap(), target);
        std::fs::remove_dir(&folder).unwrap();
        assert!(
            checked_association_with(
                kind,
                &action,
                &command,
                folder_path,
                bookmark_path,
                |_| panic!("stale directory launched")
            )
            .is_err()
        );
    }

    #[test]
    fn ordinary_executable_admission_rejects_protocols_and_invalid_windows_paths() {
        for target in [
            r"C:\Apps\Tool.EXE",
            r"..\Tool.exe",
            "tool.exe",
            r"\\server\share\Tool.exe",
        ] {
            assert!(ordinary_executable(target), "{target}");
        }
        for target in [
            "https://host/tool.exe",
            "shell:tool.exe",
            r"\\?\C:\tool.exe",
            r"C:\NUL.exe",
            "tool.exe:stream",
            r"C:\bad?name.exe",
            "tool.exe ",
            "tool.bat",
            ".exe",
        ] {
            assert!(!ordinary_executable(target), "{target}");
        }
        let action = Action {
            label: "same".into(),
            desc: String::new(),
            action: r"C:\app.exe".into(),
            args: Some("--exact value".into()),
        };
        assert_eq!(saved_external(&action).unwrap().args, action.args);
    }

    #[test]
    fn queued_external_hide_requires_current_visible_revision_and_no_new_request() {
        let hide = super::super::RootVisibilityRequest::HideAfterExternalLaunch(3);
        assert!(hide.is_current(3, true, false));
        assert!(!hide.is_current(4, true, false));
        assert!(!hide.is_current(3, false, false));
        assert!(!hide.is_current(3, true, true));
    }

    #[test]
    fn presentation_requires_original_local_owner_without_native_foreground() {
        assert!(presentation_owner_matches(Some(&1), Some(&1), false));
        assert!(!presentation_owner_matches(Some(&1), Some(&2), false));
        assert!(!presentation_owner_matches(Some(&1), Some(&1), true));
        assert!(!presentation_owner_matches::<u8>(None, None, false));
    }

    #[test]
    fn pending_hotkey_supersedes_conditional_hide_but_not_dock_reveal() {
        use super::super::RootVisibilityRequest::{DockHash, HideAfterExternalLaunch};
        use super::super::{ForegroundIdentity, HashWakeSignal, NativeWindow, hash_dock};
        let hide = HideAfterExternalLaunch(2);
        assert!(hide.superseded_by_hotkey(|| true));
        assert!(!hide.superseded_by_hotkey(|| false));
        let reveal = DockHash(hash_dock::Reveal {
            gesture: hash_dock::Gesture {
                source: NativeWindow { identity: ForegroundIdentity { hwnd: 1, process_id: 2, thread_id: 3 } },
                foreground_authorized: true,
                visibility_revision: 2,
            },
            mount_generation: 5,
            acknowledgement: hash_dock::Acknowledgement(Arc::new(HashWakeSignal::default())),
        });
        assert!(reveal.is_current(2, false, false));
        assert!(!reveal.is_current(3, false, false));
        assert!(!reveal.is_current(2, false, true));
        assert!(!reveal.superseded_by_hotkey(|| panic!("explicit reveal precedes the hotkey")));
    }

    #[test]
    fn deferred_inline_error_requires_exact_internal_change_and_owner() {
        assert!(deferred_inline_is_current(5, 5, "", "", "", true));
        assert!(!deferred_inline_is_current(5, 4, "", "old", "", true));
        assert!(!deferred_inline_is_current(5, 6, "", "", "", true));
        assert!(!deferred_inline_is_current(5, 5, "", "", "new", true));
        assert!(!deferred_inline_is_current(5, 5, "", "", "", false));
    }

    #[test]
    fn external_ticket_retires_once_and_stale_presentation_cannot_apply() {
        let admission = Arc::new(RestartAdmission::default());
        let mut state = ExternalLaunch::default();
        let request = Request {
            history_label: None,
            action: Action {
                label: "Tool".into(),
                desc: String::new(),
                action: "tool.exe".into(),
                args: None,
            },
            command: RunCommand::External { command: ExternalCommand {
                target: "tool.exe".into(),
                args: None,
                namespace: None,
            }, association: None },
            query: "tool".into(),
            source: ActivationSource::Enter,
            settings: Settings {
                clear_query_after_run: false,
                hide_after_run: false,
                history_limit: 10,
                enable_toasts: false,
                show_error_toasts: false,
                show_inline_errors: false,
                toast_duration: 1.0,
            },
        };
        let stamp = Stamp {
            root: WindowId::from(1),
            generation: 4,
            visibility_revision: 8,
            query: "tool".into(),
            focus: None,
        };
        let ticket = state.begin(&admission, stamp, &request).unwrap();
        let duplicate = Stamp {
            root: WindowId::from(1),
            generation: 4,
            visibility_revision: 8,
            query: "tool".into(),
            focus: None,
        };
        assert!(state.begin(&admission, duplicate, &request).is_none());
        assert_eq!(admission.pending_effects(), 1);
        let stale_result = WorkerResult {
            outcome: Ok(CommandOutcome::default()),
            history_error: None,
            usage_result: None,
        };
        assert!(
            state
                .finish(ticket + 1, request.clone(), stale_result)
                .is_none()
        );
        assert!(state.is_pending());
        let result = WorkerResult {
            outcome: Ok(CommandOutcome::default()),
            history_error: None,
            usage_result: None,
        };
        let receipt = state.finish(ticket, request.clone(), result).unwrap();
        assert!(!state.is_pending());
        let repeated = WorkerResult {
            outcome: Ok(CommandOutcome::default()),
            history_error: None,
            usage_result: None,
        };
        assert!(state.finish(ticket, request, repeated).is_none());
        assert!(
            !receipt
                .stamp
                .presentation_is_current(WindowId::from(1), 5, 8, "tool", true)
        );
        assert!(
            !receipt
                .stamp
                .presentation_is_current(WindowId::from(1), 4, 9, "tool", true)
        );
        assert!(
            !receipt
                .stamp
                .presentation_is_current(WindowId::from(1), 4, 8, "changed", true)
        );
        assert!(!receipt.stamp.may_refocus(WindowId::from(1), false));
        assert_eq!(admission.pending_effects(), 1);
        drop(receipt);
        assert_eq!(admission.pending_effects(), 0);
    }

    #[test]
    fn blank_saved_launch_history_uses_display_label_and_canonical_usage() {
        let request = Request {
            action: Action {
                label: "Folder".into(), desc: "Saved".into(),
                action: r"C:\Data\Project".into(), args: Some(" ".into()),
            },
            history_label: Some("app Folder".into()),
            command: RunCommand::External {
                command: ExternalCommand {
                    target: r"C:\Data\Project".into(), args: Some(" ".into()), namespace: None,
                },
                association: Some(AssociationKind::SavedFolder),
            },
            query: " \t".into(),
            source: ActivationSource::Click,
            settings: Settings {
                clear_query_after_run: false, hide_after_run: false, history_limit: 10,
                enable_toasts: true, show_error_toasts: true, show_inline_errors: true,
                toast_duration: 2.0,
            },
        };
        let outcome = CommandOutcome { history: HistoryPolicy::Record, ..CommandOutcome::default() };
        let result = bookkeep(Ok(outcome), &request, |entry, _| {
            assert_eq!(entry.query, " \t");
            assert_eq!(entry.action.label, "app Folder");
            assert_eq!(entry.action.action, request.action.action);
            assert_eq!(entry.action.args, request.action.args);
            Ok(())
        }, |key| {
            assert_eq!(key, request.action.action);
            Ok(1)
        });
        assert!(result.outcome.is_ok());
        assert_eq!(request.action.label, "Folder");
    }

    #[test]
    fn bookkeeping_attempts_both_stores_after_success_without_relaunch() {
        let request = Request {
            history_label: None,
            action: Action {
                label: "Tool".into(),
                desc: String::new(),
                action: "tool.exe".into(),
                args: Some("--arg".into()),
            },
            command: RunCommand::External { command: ExternalCommand {
                target: "tool.exe".into(),
                args: Some("--arg".into()),
                namespace: None,
            }, association: None },
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
