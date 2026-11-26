//! Renderer-neutral preparation and preview for one snippet invocation.
//!
//! Values, source text and rendered output deliberately have no `Debug` output.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::plugins::snippet_template::{
    RenderErrorKind, RenderIssueKind, RenderedPreview, SnippetInvocationContext,
    TemplateRenderError, render_for_copy, render_for_copy_with_context, render_preview,
    render_preview_with_context,
};
use crate::plugins::snippets::{
    PreparedSnippetTemplate, SnippetEntry, SnippetFieldDefinition, SnippetRunMode,
    prepare_snippet_run,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptError {
    InvalidConfiguration,
    MissingValue,
    RequiredValueEmpty,
    SystemVariableFormatting,
    PreviewOnly,
    Superseded,
    StaleSnippet,
    ClipboardUnavailable,
    AlreadyCompleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptIssueKind {
    InvalidConfiguration,
    MissingValue,
    RequiredValueEmpty,
    SystemVariableFormatting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptIssue {
    pub key: String,
    pub kind: PromptIssueKind,
}

pub struct PromptPreview {
    pub text: String,
    pub issues: Vec<PromptIssue>,
}

pub enum PreparedInvocation {
    Plain(PromptDefinition),
    Automatic(PromptDefinition),
    Prompted(PromptDefinition),
}

#[derive(Clone)]
pub struct PromptDefinition {
    prepared: PreparedSnippetTemplate,
    entry: SnippetEntry,
}

impl PromptDefinition {
    pub fn fields(&self) -> &[SnippetFieldDefinition] { &self.prepared.fields }
    pub fn uses_clipboard(&self) -> bool { self.prepared.parsed.uses_clipboard() }
    pub fn uses_window_title(&self) -> bool { self.prepared.parsed.uses_window_title() }
    pub fn has_system_variables(&self) -> bool { self.prepared.parsed.has_system_variables() }
    pub fn begin(self, context: Option<SnippetInvocationContext>) -> Result<PromptSession, PromptError> {
        if self.has_system_variables() && context.is_none() { return Err(PromptError::SystemVariableFormatting); }
        let (values, initial_focus) = initial_values(&self.prepared);
        Ok(PromptSession { definition: self, values, initial_focus, context, execution: None, completed: false })
    }
    /// Construct an executable session with alias-only bookkeeping attribution.
    pub fn begin_execution(self, path: impl AsRef<Path>, context: Option<SnippetInvocationContext>, attribution: PromptAttribution) -> Result<PromptSession, PromptError> {
        let mut session = self.begin(context)?;
        session.execution = Some(ExecutionBinding {
            path: crate::plugins::snippets::normalized_snippet_path(path.as_ref()),
            attribution,
        });
        Ok(session)
    }
}

pub struct PromptSession {
    definition: PromptDefinition,
    values: HashMap<String, String>,
    initial_focus: Option<String>,
    context: Option<SnippetInvocationContext>,
    execution: Option<ExecutionBinding>,
    completed: bool,
}

struct ExecutionBinding {
    path: PathBuf,
    attribution: PromptAttribution,
}

#[derive(Clone)]
pub struct PromptAttribution {
    pub source: crate::commands::ActivationSource,
    pub history_query: String,
    pub root_policy: crate::universal_actions::RootLauncherPolicy,
}

/// Contains only the alias and original launcher attribution, never rendered text.
pub struct PromptCompletion {
    pub(crate) alias: String,
    pub(crate) safe_action: crate::actions::Action,
    pub(crate) source: crate::commands::ActivationSource,
    pub(crate) history_query: String,
    pub(crate) root_policy: crate::universal_actions::RootLauncherPolicy,
}
impl PromptCompletion {
    pub fn alias(&self) -> &str { &self.alias }
    pub fn safe_action(&self) -> &crate::actions::Action { &self.safe_action }
    pub fn source(&self) -> crate::commands::ActivationSource { self.source }
    pub fn history_query(&self) -> &str { &self.history_query }
}

/// Select the same external origin as the existing launcher and radial paths.
pub fn select_origin_window(
    source: crate::commands::ActivationSource,
    root_visible: bool,
    observed: crate::active_window::ForegroundWindowObservation,
    cached: Option<crate::active_window::ExternalForegroundWindow>,
) -> Option<crate::active_window::ExternalForegroundWindow> {
    use crate::active_window::ForegroundWindowObservation;
    let radial = matches!(source, crate::commands::ActivationSource::RadialRelease
        | crate::commands::ActivationSource::RadialShortcut
        | crate::commands::ActivationSource::RadialHotstring);
    match observed {
        ForegroundWindowObservation::External(window) => Some(window),
        ForegroundWindowObservation::Launcher if root_visible && !radial => cached,
        ForegroundWindowObservation::Launcher | ForegroundWindowObservation::Unavailable => None,
    }
}

/// Capture only automatic values requested by the parsed template.
pub fn capture_protected_context(
    timestamp: chrono::DateTime<chrono::FixedOffset>,
    needs_clipboard: bool,
    needs_window_title: bool,
    capture_origin: impl FnOnce() -> Option<crate::active_window::ExternalForegroundWindow>,
) -> SnippetInvocationContext {
    capture_context_with(timestamp, needs_clipboard, needs_window_title,
        || crate::actions::clipboard::get_text().ok(), capture_origin)
}

fn capture_context_with(
    timestamp: chrono::DateTime<chrono::FixedOffset>,
    needs_clipboard: bool,
    needs_window_title: bool,
    capture_clipboard: impl FnOnce() -> Option<String>,
    capture_origin: impl FnOnce() -> Option<crate::active_window::ExternalForegroundWindow>,
) -> SnippetInvocationContext {
    let origin = needs_window_title.then(capture_origin).flatten();
    let clipboard = needs_clipboard.then(capture_clipboard).flatten();
    let (hwnd, title) = origin.map_or((None, None), |origin| (Some(origin.hwnd), origin.title));
    SnippetInvocationContext::new(timestamp, clipboard, hwnd, title)
}

impl PromptSession {
    pub fn fields(&self) -> &[SnippetFieldDefinition] { self.definition.fields() }
    pub fn initial_focus(&self) -> Option<&str> { self.initial_focus.as_deref() }
    pub fn value(&self, key: &str) -> Option<&str> { self.values.get(key).map(String::as_str) }
    pub fn set_value(&mut self, key: &str, value: String) -> bool {
        let Some(current) = self.values.get_mut(key) else { return false; };
        *current = value;
        true
    }
    /// Pure preview: never resolves a saved entry or accesses the clipboard.
    pub fn preview(&self) -> Result<PromptPreview, PromptError> {
        preview_prepared(&self.definition.prepared, &self.values, self.context.as_ref())
            .map(|preview| PromptPreview {
                text: preview.text,
                issues: preview.validation_errors.into_iter().map(|issue| PromptIssue {
                    key: issue.key,
                    kind: match issue.kind {
                        RenderIssueKind::DuplicateFieldDefinition | RenderIssueKind::MissingFieldDefinition => PromptIssueKind::InvalidConfiguration,
                        RenderIssueKind::MissingValue => PromptIssueKind::MissingValue,
                        RenderIssueKind::RequiredValueEmpty => PromptIssueKind::RequiredValueEmpty,
                        RenderIssueKind::SystemVariableFormatting => PromptIssueKind::SystemVariableFormatting,
                    },
                }).collect(),
            }).map_err(map_error)
    }

    /// Write only after the exact unique persisted entry and current owner have
    /// been checked again at the acquired clipboard boundary.
    pub fn execute_checked(
        &mut self,
        clipboard: &dyn crate::clipboard_modify::clipboard::ClipboardBackend,
        current: &dyn Fn() -> bool,
    ) -> Result<PromptCompletion, PromptError> {
        use crate::common::persistence::LoadState;
        use std::cell::Cell;
        if self.completed { return Err(PromptError::AlreadyCompleted); }
        let binding = self.execution.as_ref().ok_or(PromptError::PreviewOnly)?;
        let attribution = &binding.attribution;
        if !current() { return Err(PromptError::Superseded); }
        let rendered = render_prepared(&self.definition.prepared, &self.values, self.context.as_ref()).map_err(map_error)?;
        if rendered.contains('\0') { return Err(PromptError::ClipboardUnavailable); }
        let path = binding.path.as_path();
        let expected = &self.definition.entry;
        let exact_entry = || {
            match crate::plugins::snippets::load_snippets_typed(path) {
                Ok(LoadState::Loaded(entries)) => crate::plugins::snippets::resolve_snippet_from_entries(&entries, &expected.alias)
                    .is_ok_and(|entry| entry == expected),
                _ => false,
            }
        };
        let result = crate::plugins::snippets::with_snippets_transaction(|| {
            if !current() { return Err(PromptError::Superseded); }
            if !exact_entry() { return Err(PromptError::StaleSnippet); }
            if !current() { return Err(PromptError::Superseded); }
            let refused = Cell::new(None);
            let written = clipboard.write_text_acquired_if(&rendered, &|| {
                if !current() { refused.set(Some(PromptError::Superseded)); return false; }
                if !exact_entry() { refused.set(Some(PromptError::StaleSnippet)); return false; }
                if !current() { refused.set(Some(PromptError::Superseded)); return false; }
                true
            }).map_err(|_| PromptError::ClipboardUnavailable)?;
            if !written { return Err(refused.get().unwrap_or(PromptError::Superseded)); }
            Ok(())
        });
        result?;
        self.completed = true;
        let alias = self.definition.entry.alias.clone();
        Ok(PromptCompletion {
            safe_action: crate::actions::Action { label: alias.clone(), desc: "Snippet".into(), action: crate::plugins::snippets::snippet_run_action(&alias), args: None },
            alias,
            source: attribution.source,
            history_query: attribution.history_query.clone(),
            root_policy: attribution.root_policy,
        })
    }
}

pub fn prepare(entry: &SnippetEntry) -> Result<PreparedInvocation, PromptError> {
    match prepare_snippet_run(entry).map_err(|_| PromptError::InvalidConfiguration)? {
        SnippetRunMode::Plain => {
            let parsed = crate::plugins::snippet_template::parse_plain_system_template(&entry.text)
                .map_err(|_| PromptError::InvalidConfiguration)?;
            Ok(PreparedInvocation::Plain(PromptDefinition {
                prepared: PreparedSnippetTemplate { parsed, fields: Vec::new() }, entry: entry.clone(),
            }))
        }
        SnippetRunMode::SystemOnly(prepared) => Ok(PreparedInvocation::Automatic(PromptDefinition { prepared, entry: entry.clone() })),
        SnippetRunMode::Prompted(prepared) => Ok(PreparedInvocation::Prompted(PromptDefinition { prepared, entry: entry.clone() })),
    }
}

/// Reuse an already-resolved command intent without recapturing automatic values.
pub fn from_intent(path: impl AsRef<Path>, intent: crate::commands::SnippetPromptIntent) -> Result<PromptSession, PromptError> {
    if intent.alias != intent.entry_snapshot.alias
        || intent.safe_action.action != crate::plugins::snippets::snippet_run_action(&intent.alias)
        || intent.safe_action.args.is_some()
    {
        return Err(PromptError::InvalidConfiguration);
    }
    PromptDefinition { prepared: intent.prepared, entry: intent.entry_snapshot }
        .begin_execution(path, intent.context, PromptAttribution {
            source: intent.source,
            history_query: intent.history_query,
            root_policy: intent.root_policy,
        })
}

pub struct EditorDiscovery {
    pub keys: Vec<String>,
    /// Includes previously authored fields that may be restored by re-adding a key.
    pub fields: Vec<SnippetFieldDefinition>,
}

pub fn discover_editor_fields(text: &str, fields: &[SnippetFieldDefinition]) -> Result<EditorDiscovery, PromptError> {
    discover_editor_fields_typed(text, fields).map_err(|_| PromptError::InvalidConfiguration)
}

pub(crate) fn discover_editor_fields_typed(
    text: &str,
    fields: &[SnippetFieldDefinition],
) -> Result<EditorDiscovery, crate::plugins::snippet_template::TemplateError> {
    let parsed = crate::plugins::snippet_template::parse_template(text)?;
    let mut retained = fields.to_vec();
    for key in &parsed.field_keys {
        if !retained.iter().any(|field| field.name == *key) {
            retained.push(SnippetFieldDefinition::new(key.clone()));
        }
    }
    Ok(EditorDiscovery { keys: parsed.field_keys, fields: retained })
}

pub(crate) fn initial_values(prepared: &PreparedSnippetTemplate) -> (HashMap<String, String>, Option<String>) {
    let values = prepared.fields.iter().map(|field| (field.name.clone(), field.default_value.clone())).collect();
    let focus = prepared.fields.iter().find(|field| field.required).or_else(|| prepared.fields.first()).map(|field| field.name.clone());
    (values, focus)
}

pub(crate) fn preview_prepared(prepared: &PreparedSnippetTemplate, values: &HashMap<String, String>, context: Option<&SnippetInvocationContext>) -> Result<RenderedPreview, TemplateRenderError> {
    match context {
        Some(context) => render_preview_with_context(&prepared.parsed, &prepared.fields, values, context),
        None if prepared.parsed.has_system_variables() => Err(missing_context()),
        None => render_preview(&prepared.parsed, &prepared.fields, values),
    }
}

pub(crate) fn render_prepared(prepared: &PreparedSnippetTemplate, values: &HashMap<String, String>, context: Option<&SnippetInvocationContext>) -> Result<String, TemplateRenderError> {
    match context {
        Some(context) => render_for_copy_with_context(&prepared.parsed, &prepared.fields, values, context),
        None if prepared.parsed.has_system_variables() => Err(missing_context()),
        None => render_for_copy(&prepared.parsed, &prepared.fields, values),
    }
}

fn missing_context() -> TemplateRenderError {
    TemplateRenderError { kind: RenderErrorKind::SystemVariableResolution, issues: vec![crate::plugins::snippet_template::RenderIssue { key: "system variable".into(), kind: RenderIssueKind::SystemVariableFormatting }] }
}
fn map_error(error: TemplateRenderError) -> PromptError {
    match error.kind {
        RenderErrorKind::InvalidConfiguration => PromptError::InvalidConfiguration,
        RenderErrorKind::MissingValues => PromptError::MissingValue,
        RenderErrorKind::RequiredValues => PromptError::RequiredValueEmpty,
        RenderErrorKind::SystemVariableResolution => PromptError::SystemVariableFormatting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::snippets::{SnippetEntry, SnippetFieldDefinition};
    use crate::clipboard_modify::clipboard::{ClipboardBackend, ClipboardError};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeClipboard { writes: Mutex<Vec<String>>, before_check: Mutex<Option<Box<dyn Fn() + Send>>> }
    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> { panic!("unexpected clipboard read") }
        fn write_text(&self, _: &str) -> Result<(), ClipboardError> { panic!("unchecked clipboard write") }
        fn write_text_acquired_if(&self, text: &str, current: &dyn Fn() -> bool) -> Result<bool, ClipboardError> {
            if let Some(hook) = self.before_check.lock().unwrap().take() { hook(); }
            if !current() { return Ok(false); }
            self.writes.lock().unwrap().push(text.to_owned());
            Ok(true)
        }
    }

    fn attribution() -> PromptAttribution {
        PromptAttribution { source: crate::commands::ActivationSource::Click, history_query: "cs example".into(), root_policy: crate::universal_actions::RootLauncherPolicy::Legacy }
    }

    fn entry(text: &str, prompted: bool) -> SnippetEntry {
        SnippetEntry { alias: "fixture".into(), text: text.into(), hide_contents: false,
            prompt_for_fields: prompted, fields: Vec::new(), tags: Vec::new() }
    }

    #[test]
    fn shared_preparation_distinguishes_plain_automatic_and_prompted() {
        assert!(matches!(prepare(&entry("plain {{literal}}", false)), Ok(PreparedInvocation::Plain(_))));
        let PreparedInvocation::Automatic(automatic) = prepare(&entry("{{sys.clipboard}}", false)).unwrap() else { panic!("automatic"); };
        assert!(automatic.uses_clipboard());
        assert!(!automatic.uses_window_title());
        assert!(matches!(automatic.begin(None), Err(PromptError::SystemVariableFormatting)));
        let PreparedInvocation::Prompted(prompted) = prepare(&entry("{{name}}", true)).unwrap() else { panic!("prompted"); };
        let mut session = prompted.begin(None).unwrap();
        assert_eq!(session.initial_focus(), Some("name"));
        assert_eq!(session.value("name"), Some(""));
        assert!(session.preview().unwrap().issues.iter().any(|issue| issue.kind == PromptIssueKind::RequiredValueEmpty));
        assert!(session.set_value("name", "Ada".into()));
        assert_eq!(session.preview().unwrap().text, "Ada");
        assert!(!session.set_value("unknown", "ignored".into()));
    }

    #[test]
    fn discovery_retains_authored_orphans_and_uses_unique_source_order() {
        let fields = vec![SnippetFieldDefinition::new("old")];
        let discovered = discover_editor_fields("{{new}} {{new}} {{second}}", &fields).unwrap();
        assert_eq!(discovered.keys, ["new", "second"]);
        assert_eq!(discovered.fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>(), ["old", "new", "second"]);
        assert!(discover_editor_fields("{{unterminated", &fields).is_err());
    }

    #[test]
    fn automatic_context_is_frozen_and_preview_is_repeatable() {
        let PreparedInvocation::Automatic(definition) = prepare(&entry("{{sys.clipboard}} {{sys.window_title}}", false)).unwrap() else { panic!("automatic"); };
        let timestamp = chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05-05:00").unwrap();
        let context = SnippetInvocationContext::new(timestamp, Some("saved".into()), Some(7), Some("window".into()));
        let session = definition.begin(Some(context)).unwrap();
        assert_eq!(session.preview().unwrap().text, "saved window");
        assert_eq!(session.preview().unwrap().text, "saved window");
    }

    #[test]
    fn context_capture_only_calls_referenced_sources() {
        let timestamp = chrono::DateTime::parse_from_rfc3339("2024-01-02T03:04:05-05:00").unwrap();
        let _context = capture_context_with(timestamp, false, false,
            || panic!("clipboard must remain untouched"), || panic!("origin must remain untouched"));
        let calls = std::cell::Cell::new(0);
        let _context = capture_context_with(timestamp, true, false,
            || { calls.set(calls.get() + 1); Some("secret".into()) }, || panic!("origin"));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn completion_bus_preserves_non_root_policy_with_alias_only_action() {
        let completion = PromptCompletion {
            alias: "fixture".into(),
            safe_action: crate::actions::Action { label: "fixture".into(), desc: "Snippet".into(), action: crate::plugins::snippets::snippet_run_action("fixture"), args: None },
            source: crate::commands::ActivationSource::Click,
            history_query: "cs fixture".into(),
            root_policy: crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState,
        };
        let outcome = crate::commands::CommandBus.complete_checked_snippet_run(completion, crate::commands::LauncherRunPreferences {
            clear_query_after_run: true, hide_after_run: true, refocus: true,
        }).unwrap();
        assert_eq!(outcome.history, crate::commands::HistoryPolicy::Record);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Keep);
        assert_eq!(outcome.query, crate::commands::QueryPolicy::Keep);
    }

    #[test]
    fn preview_only_never_copies_and_checked_execution_is_one_shot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let item = entry("hello {{name}}", true);
        std::fs::write(&path, serde_json::to_vec(&vec![item.clone()]).unwrap()).unwrap();
        let PreparedInvocation::Prompted(definition) = prepare(&item).unwrap() else { panic!("prompted"); };
        let clipboard = FakeClipboard::default();
        let mut preview = definition.clone().begin(None).unwrap();
        assert!(preview.set_value("name", "Ada".into()));
        assert!(matches!(preview.execute_checked(&clipboard, &|| true), Err(PromptError::PreviewOnly)));
        assert!(clipboard.writes.lock().unwrap().is_empty());
        let mut execution = definition.begin_execution(&path, None, attribution()).unwrap();
        assert!(matches!(execution.execute_checked(&clipboard, &|| true), Err(PromptError::RequiredValueEmpty)));
        assert!(execution.set_value("name", "Ada".into()));
        let completion = execution.execute_checked(&clipboard, &|| true).unwrap();
        assert_eq!(completion.alias, "fixture");
        assert_eq!(completion.history_query, "cs example");
        assert_eq!(completion.safe_action.label, "fixture");
        assert!(!completion.safe_action.action.contains("Ada"));
        let outcome = crate::commands::CommandBus.complete_checked_snippet_run(completion, crate::commands::LauncherRunPreferences {
            clear_query_after_run: true, hide_after_run: true, refocus: true,
        }).unwrap();
        assert_eq!(outcome.history, crate::commands::HistoryPolicy::Record);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Hide);
        assert_eq!(clipboard.writes.lock().unwrap().as_slice(), ["hello Ada"]);
        assert!(matches!(execution.execute_checked(&clipboard, &|| true), Err(PromptError::AlreadyCompleted)));
        assert_eq!(clipboard.writes.lock().unwrap().len(), 1);
    }

    #[test]
    fn checked_execution_refuses_stale_and_late_revoked_authority() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let item = entry("plain", false);
        std::fs::write(&path, serde_json::to_vec(&vec![item.clone()]).unwrap()).unwrap();
        let PreparedInvocation::Plain(definition) = prepare(&item).unwrap() else { panic!("plain"); };
        let clipboard = FakeClipboard::default();
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let revoked = flag.clone();
        *clipboard.before_check.lock().unwrap() = Some(Box::new(move || { revoked.store(false, std::sync::atomic::Ordering::Release); }));
        let mut execution = definition.clone().begin_execution(&path, None, attribution()).unwrap();
        assert!(matches!(execution.execute_checked(&clipboard, &|| flag.load(std::sync::atomic::Ordering::Acquire)), Err(PromptError::Superseded)));
        assert!(clipboard.writes.lock().unwrap().is_empty());
        let changed = entry("external", false);
        std::fs::write(&path, serde_json::to_vec(&vec![changed]).unwrap()).unwrap();
        let mut stale = definition.begin_execution(&path, None, attribution()).unwrap();
        assert!(matches!(stale.execute_checked(&clipboard, &|| true), Err(PromptError::StaleSnippet)));
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn guarded_clipboard_acquisition_rechecks_the_full_unique_entry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let item = entry("original", false);
        std::fs::write(&path, serde_json::to_vec(&vec![item.clone()]).unwrap()).unwrap();
        let PreparedInvocation::Plain(definition) = prepare(&item).unwrap() else { panic!("plain"); };
        let clipboard = FakeClipboard::default();
        let changed_path = path.clone();
        let changed = entry("changed", false);
        *clipboard.before_check.lock().unwrap() = Some(Box::new(move || {
            std::fs::write(&changed_path, serde_json::to_vec(std::slice::from_ref(&changed)).unwrap()).unwrap();
        }));
        let mut session = definition.begin_execution(&path, None, attribution()).unwrap();
        assert!(matches!(session.execute_checked(&clipboard, &|| true), Err(PromptError::StaleSnippet)));
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn duplicate_alias_refuses_checked_execution() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let item = entry("same", false);
        std::fs::write(&path, serde_json::to_vec(&vec![item.clone(), item.clone()]).unwrap()).unwrap();
        let PreparedInvocation::Plain(definition) = prepare(&item).unwrap() else { panic!("plain"); };
        let mut session = definition.begin_execution(&path, None, attribution()).unwrap();
        let clipboard = FakeClipboard::default();
        assert!(matches!(session.execute_checked(&clipboard, &|| true), Err(PromptError::StaleSnippet)));
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn execution_is_bound_to_original_store_and_unchecked_backends_fail_closed() {
        struct Unchecked;
        impl ClipboardBackend for Unchecked {
            fn read_text(&self) -> Result<String, ClipboardError> { panic!("read") }
            fn write_text(&self, _: &str) -> Result<(), ClipboardError> { panic!("unchecked write") }
        }
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("a.json");
        let second = directory.path().join("b.json");
        let item = entry("original", false);
        let bytes = serde_json::to_vec(&vec![item.clone()]).unwrap();
        std::fs::write(&first, &bytes).unwrap();
        std::fs::write(&second, &bytes).unwrap();
        let PreparedInvocation::Plain(definition) = prepare(&item).unwrap() else { panic!("plain"); };
        let mut session = definition.clone().begin_execution(&first, None, attribution()).unwrap();
        assert!(matches!(session.execute_checked(&Unchecked, &|| true), Err(PromptError::ClipboardUnavailable)));
        std::fs::write(&first, serde_json::to_vec(&vec![entry("changed", false)]).unwrap()).unwrap();
        let clipboard = FakeClipboard::default();
        assert!(matches!(session.execute_checked(&clipboard, &|| true), Err(PromptError::StaleSnippet)));
        assert!(clipboard.writes.lock().unwrap().is_empty());
        assert_eq!(std::fs::read(&second).unwrap(), bytes);
    }
}
