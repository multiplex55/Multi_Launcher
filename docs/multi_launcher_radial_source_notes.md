# Multi Launcher radial-menu source notes

## Current authority and evidence status

**Launcher baseline:** The first commit made on the current feature branch after branching from `master`, resolved to a full Git object ID in the actual checkout and pinned once in `docs/plans/radial-menu.md`. The master implementation brief's section **0.2** defines resolution, verification, the no-unique-commit exception, and resume behavior.

**Reference root:** The user-provided Radify/RM4 source, skins/settings examples, and screenshots under the current checkout's **`docs/references/`**. Section **0.3** of the master brief defines discovery, path/hash inventory, safe extraction, and reference-change handling. These files may be added after the Git baseline; their filenames and subfolder layout are not prescribed.

**Implementation authority:** Work against the current feature-branch checkout and preserve legitimate newer code and pre-existing user changes. The pinned baseline is for comparison, not rollback. Read current `AGENTS.md` first.

**Evidence status:** The observations and excerpts below were collected from an earlier uploaded source snapshot and reference package. They have **not** been verified against the user's requested first-branch-commit baseline or the future contents of `docs/references/`. Their original line numbers are historical navigation aids. No baseline SHA or branch name has been invented for this handoff.

### Revalidate before relying on these notes

1. Before adding a plan, reference, or implementation commit, resolve/verify the first feature-branch commit according to master-brief section 0.2. Pin its full object ID and record the branch point separately. Do not use the repository's root commit, a later `HEAD`, or a moving merge base as an unlabeled substitute. On resume, reuse the ledger's pinned SHA.
2. Capture task-start HEAD, the master ref/tip used for discovery, and pre-existing tracked/untracked changes. If no unique branch commit exists yet, follow the brief's explicit evidence-based branch-start rule; never create a commit merely to manufacture a baseline.
3. Inspect relevant baseline files with `git show <BASELINE_COMMIT>:<repo-relative-path>` and compare with the current checkout. Do not check out/reset to the baseline. In PowerShell, a populated baseline variable can be used as `git show "${BaselineCommit}:src/main.rs"`.
4. For each finding below, record `confirmed at baseline`, `confirmed in current checkout`, `changed`, or `not found`, with actual paths/symbols and brief evidence. A path or dependency version that has changed must be corrected in the live source map, not forced back to the historical excerpt.
5. Discover relevant files recursively under `docs/references/`, including user-provided untracked files. Record their paths, selected version/package identity, and SHA-256 hashes. Source ZIPs may remain zipped originals; any safe temporary extraction is only an inspection copy. Do not execute reference AHK scripts or fetch a newer replacement package.
6. Match the reference excerpts below to the actual local reference files. Recheck whether `Preferences.json`, `skin definition.txt`, or additional legacy examples are now present; their absence in an older package is not evidence of current absence.
7. Keep the live source/reference inventory in `docs/plans/radial-menu.md`, not in several independently maintained SHA lists. Preserve this file's provenance; do not relabel old excerpts as newly inspected Git evidence without actually inspecting that commit.

Suggested ledger fields (placeholders, not resolved values):

```text
feature_branch: <actual current branch>
baseline_kind: first-feature-branch-commit | branch-start-no-unique-commit
baseline_commit: <verified full object ID>
baseline_subject: <actual commit subject>
baseline_selection_evidence: <commands/parents/reflog evidence>
branch_point_commit: <separately verified shared starting commit, or unresolved>
master_ref: <actual local or remote-tracking master ref>
master_tip_at_resolution: <verified full object ID>
task_start_commit: <verified HEAD before task edits>
pre_existing_changes: <tracked and untracked status>
references_root: docs/references/
reference_inventory: <repo-relative paths, roles, sizes, hashes/archive members>
source_revalidation: <finding -> baseline evidence + current evidence>
```

Do not perform a full Cargo/Nextest baseline run simply to populate this record. Preserve the slow-machine batching, single-job policy, and 60/120/180/up-to-300-second observation cadence in the master brief.

## Historical architectural observations — pending Git/reference revalidation

1. The Universal Actions surface enum already includes RadialMenu. The existing architecture plan explicitly keeps hold timing and geometry outside providers.
2. The hotkey runtime exposes a boolean trigger and currently polls at 20 ms. The new shared-key lifecycle cannot be inferred from a press notification alone.
3. The main event loop explicitly routes Screen Draw emergency/recovery before ordinary launcher visibility. The existing tests protect consumption of the launcher trigger during drawing.
4. Universal action execution is GUI-owned and includes availability checks and destructive confirmation. Its confirmed-dispatch function currently accepts `_surface` without using it, so merely selecting RadialMenu does not prove parent query/focus/visibility policy is correct.
5. Persistent action target references exclude ephemeral window handles and positional indexes. Native menu configuration should reuse this boundary.
6. Gesture suppression is already owned through an RAII guard. Native input teardown must release this lease on failure as well as success.
7. Screen Draw provides event-driven native session and layered-overlay patterns, but passive overlay styles are not an interactive radial host.
8. Geometry-preserving visibility restoration, typed/atomic persistence, and grouped domain tests already exist and should be retained.
9. The earlier snapshot used eframe/egui 0.27, windows 0.58, image 0.24. Verify the actual pinned Git baseline and current lockfile; do not design against a different framework version accidentally.
10. The earlier Radify package included source, a skin editor, and images, but that earlier inspection did not find a generated Preferences.json. Reinspect docs/references/ before applying this observation to the current reference inputs. Generated/configuration fixtures must be labeled honestly.

## Historical source excerpts — navigation aids, not verified Git-baseline evidence

The numbered lines below retain their original inspected-snapshot provenance. Resolve actual files and line ranges at the pinned baseline and in `docs/references/` before citing them as current evidence.
### Launcher: `Cargo.toml:6–17`

```text
6: 
7: [dependencies]
8: eframe = "0.27"               # egui-based GUI
9: serde = { version = "1.0", features = ["derive"] }
10: serde_json = "1.0"
11: fuzzy-matcher = "0.3"
12: fst = { version = "0.4", features = ["levenshtein"] }
13: open = "5.0"                   # Open files/folders/apps cross-platform
14: anyhow = "1.0"
15: walkdir = "2.4"
16: tracing = "0.1"
17: tracing-subscriber = { version = "0.3.22", features = ["env-filter"] }
```

### Launcher: `Cargo.toml:120–128`

```text
120: 
121: # Group only side-effect-free cases; Cargo auto-discovers every top-level tests/*.rs target.
122: [[test]]
123: name = "plugin_queries"
124: path = "tests/suites/plugin_queries.rs"
125: 
126: [[test]]
127: name = "domain"
128: path = "tests/suites/domain.rs"
```

### Launcher: `src/universal_actions/model.rs:102–115`

```text
102: /// The UI surface presenting actions. This is deliberately distinct from
103: /// `commands::ActivationSource`, which describes the triggering input.
104: #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
105: pub enum ActionSurface {
106:     LauncherList,
107:     LauncherGrid,
108:     ActionSheet,
109:     ContextMenu,
110:     Dashboard,
111:     RadialMenu,
112:     Gesture,
113: }
114: 
115: #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
```

### Launcher: `src/universal_actions/provider.rs:45–55`

```text
45:         }
46:     }
47: }
48: 
49: /// Pure capability discovery for a resolved launcher target.
50: pub trait UniversalActionProvider: Sync {
51:     fn actions(
52:         &self,
53:         resolved: &ResolvedActionTarget,
54:         context: &ActionResolutionContext<'_>,
55:     ) -> Vec<UniversalAction>;
```

### Launcher: `src/universal_actions/target.rs:58–120`

```text
58: impl ActionTarget {
59:     /// Return the stable identity suitable for future saved action bindings.
60:     ///
61:     /// Runtime handles, list indexes, and live timer instances are deliberately
62:     /// not promoted to persistent identities.
63:     pub fn persistent_ref(&self) -> Option<PersistableActionTargetRef> {
64:         match self {
65:             Self::Generic { action } => Some(PersistableActionTargetRef::LegacyAction {
66:                 action: action.clone(),
67:             }),
68:             Self::CustomAction { action, .. } => Some(PersistableActionTargetRef::CustomAction {
69:                 action: action.clone(),
70:             }),
71:             Self::Folder { path } => {
72:                 Some(PersistableActionTargetRef::Folder { path: path.clone() })
73:             }
74:             Self::Bookmark { url } => {
75:                 Some(PersistableActionTargetRef::Bookmark { url: url.clone() })
76:             }
77:             Self::Snippet { alias } => Some(PersistableActionTargetRef::Snippet {
78:                 alias: alias.clone(),
79:             }),
80:             Self::Tempfile { path } => {
81:                 Some(PersistableActionTargetRef::Tempfile { path: path.clone() })
82:             }
83:             Self::Note { slug } => Some(PersistableActionTargetRef::Note { slug: slug.clone() }),
84:             Self::MkMacro { id } => Some(PersistableActionTargetRef::MkMacro { id: *id }),
85:             Self::Timer { .. }
86:             | Self::Stopwatch { .. }
87:             | Self::ClipboardEntry { .. }
88:             | Self::Todo { .. }
89:             | Self::Window { .. }
90:             | Self::BrowserTab { .. } => None,
91:         }
92:     }
93: }
94: 
95: /// Stable subset of [`ActionTarget`] identities that may be stored in future
96: /// Universal Action configuration.
97: #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
98: #[serde(tag = "kind", rename_all = "snake_case")]
99: pub enum PersistableActionTargetRef {
100:     LegacyAction { action: Action },
101:     CustomAction { action: Action },
102:     Folder { path: String },
103:     Bookmark { url: String },
104:     Snippet { alias: String },
105:     Tempfile { path: String },
106:     Note { slug: String },
107:     MkMacro { id: u64 },
108: }
109: 
110: /// Stable reference to a semantic action, optionally scoped to a persistable
111: /// target. A missing target supports future global/root-surface actions.
112: #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
113: pub struct PersistedUniversalActionRef {
114:     pub target: Option<PersistableActionTargetRef>,
115:     pub action_id: ActionId,
116: }
117: 
118: #[cfg(test)]
119: mod tests {
120:     use super::*;
```

### Launcher: `src/universal_actions/target.rs:109–118`

```text
109: 
110: /// Stable reference to a semantic action, optionally scoped to a persistable
111: /// target. A missing target supports future global/root-surface actions.
112: #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
113: pub struct PersistedUniversalActionRef {
114:     pub target: Option<PersistableActionTargetRef>,
115:     pub action_id: ActionId,
116: }
117: 
118: #[cfg(test)]
```

### Launcher: `src/gui/universal_action_executor.rs:24–68`

```text
24: }
25: 
26: impl LauncherApp {
27:     /// Execute a surface-independent action while retaining the input source
28:     /// and presentation surface as distinct invocation context.
29:     pub(crate) fn execute_universal_action(
30:         &mut self,
31:         action: UniversalAction,
32:         surface: ActionSurface,
33:         source: ActivationSource,
34:     ) -> UniversalActionExecution {
35:         if let Some(reason) = action.availability.disabled_reason() {
36:             self.report_error_message("universal_action", reason);
37:             return UniversalActionExecution::Unavailable;
38:         }
39: 
40:         let before = self.launcher_interaction_snapshot();
41:         if self.require_confirm_destructive && action.safety == ActionSafety::Destructive {
42:             let Some(kind) = DestructiveAction::from_universal_action(&action) else {
43:                 self.report_error_message(
44:                     "universal_action",
45:                     format!("Missing confirmation metadata for {}", action.id),
46:                 );
47:                 return UniversalActionExecution::Unavailable;
48:             };
49:             self.pending_universal_confirm = Some(PendingUniversalActionInvocation {
50:                 action,
51:                 surface,
52:                 source,
53:             });
54:             self.confirm_modal.open_for_source(kind, Some(source));
55:             self.restore_for_new_launcher_interaction(&before);
56:             return UniversalActionExecution::ConfirmationRequired;
57:         }
58: 
59:         self.execute_universal_action_confirmed(action, surface, source);
60:         self.restore_for_new_launcher_interaction(&before);
61:         UniversalActionExecution::Executed
62:     }
63: 
64:     pub(super) fn resolve_pending_universal_action_confirmation(
65:         &mut self,
66:         confirmed: bool,
67:     ) -> bool {
68:         let Some(pending) = self.pending_universal_confirm.take() else {
```

### Launcher: `src/gui/universal_action_executor.rs:82–109`

```text
82: 
83:     fn execute_universal_action_confirmed(
84:         &mut self,
85:         action: UniversalAction,
86:         _surface: ActionSurface,
87:         source: ActivationSource,
88:     ) {
89:         let action_id = action.id.clone();
90:         match action.operation {
91:             UniversalActionOperation::InvokePrimary(action) => {
92:                 // This is intentionally the exact legacy primary activation path.
93:                 self.activate_action(action, None, source);
94:             }
95:             UniversalActionOperation::Command {
96:                 command,
97:                 original_action,
98:             } => {
99:                 self.dispatch_universal_secondary_command(
100:                     action_id.as_str(),
101:                     CommandInvocation {
102:                         command,
103:                         original_action,
104:                         query_override: None,
105:                         source,
106:                     },
107:                 );
108:             }
109:             UniversalActionOperation::UiIntent(intent) => {
```

### Launcher: `src/hotkey/runtime.rs:10–26`

```text
10: // Shared signal to open launcher
11: pub struct HotkeyTrigger {
12:     pub open: Arc<Mutex<bool>>,
13:     pub _key: Key,
14:     pub _ctrl: bool,
15:     pub _shift: bool,
16:     pub _alt: bool,
17:     pub _win: bool,
18: }
19: 
20: pub struct HotkeyListener {
21:     stop: Arc<AtomicBool>,
22: }
23: 
24: impl HotkeyTrigger {
25:     pub fn new(hotkey: Hotkey) -> Self {
26:         Self {
```

### Launcher: `src/hotkey/runtime.rs:160–181`

```text
160:                                 && (!need_shift[i] || shift_pressed)
161:                                 && (!need_alt[i] || alt_pressed)
162:                                 && (!need_win[i] || win_pressed)
163:                         };
164:                         if combo {
165:                             if !triggered[i] {
166:                                 triggered[i] = true;
167:                                 if let Ok(mut flag) = open_listeners[i].lock() {
168:                                     *flag = true;
169:                                 }
170:                                 let _ = event_tx.send(());
171:                             }
172:                         } else {
173:                             triggered[i] = false;
174:                         }
175:                     }
176:                 }
177:                 thread::sleep(Duration::from_millis(20));
178:             }
179:         });
180: 
181:         HotkeyListener { stop: stop_flag }
```

### Launcher: `src/hotkey/runtime.rs:184–203`

```text
184:     pub fn take(&self) -> bool {
185:         let mut open = match self.open.lock() {
186:             Ok(g) => g,
187:             Err(e) => {
188:                 tracing::error!("failed to lock hotkey trigger: {e}");
189:                 return false;
190:             }
191:         };
192:         if *open {
193:             *open = false;
194:             tracing::debug!("HotkeyTrigger fired!");
195:             true
196:         } else {
197:             false
198:         }
199:     }
200: }
201: 
202: impl HotkeyListener {
203:     pub fn stop(&self) {
```

### Launcher: `src/main.rs:242–284`

```text
242: fn take_screen_draw_trigger_actions(
243:     launcher: &HotkeyTrigger,
244:     launch: Option<&HotkeyTrigger>,
245:     emergency: Option<&HotkeyTrigger>,
246:     recovery_bridge: &ScreenDrawRecoveryBridge,
247: ) -> ScreenDrawTriggerActions {
248:     let emergency_fired = emergency.is_some_and(HotkeyTrigger::take);
249:     if emergency_fired && recovery_bridge.is_active() {
250:         // Emergency owns this event-loop turn. Consume any defensive co-fire
251:         // so the same physical chord cannot also start or recover Screen Draw.
252:         if let Some(launch) = launch {
253:             let _ = launch.take();
254:         }
255:         let _ = launcher.take();
256:         return ScreenDrawTriggerActions {
257:             emergency: true,
258:             ..Default::default()
259:         };
260:     }
261: 
262:     let launch = launch.is_some_and(HotkeyTrigger::take);
263:     if launch {
264:         // Publish before enqueueing the GUI event. A launcher summon observed
265:         // in this same turn is then routed to recovery, never visibility.
266:         recovery_bridge.stage_start();
267:     }
268:     let recover = take_screen_draw_recovery_trigger(launcher, recovery_bridge.is_active());
269:     ScreenDrawTriggerActions {
270:         launch,
271:         recover,
272:         emergency: false,
273:     }
274: }
275: 
276: fn take_screen_draw_recovery_trigger(trigger: &HotkeyTrigger, screen_draw_active: bool) -> bool {
277:     screen_draw_active && trigger.take()
278: }
279: 
280: pub fn request_hotkey_restart(settings: Settings) {
281:     match RESTART_TX.lock() {
282:         Ok(guard) => {
283:             if let Some(tx) = guard.as_ref() {
284:                 let _ = tx.send(settings);
```

### Launcher: `src/main.rs:797–814`

```text
797: 
798:     #[test]
799:     fn active_screen_draw_consumes_launcher_trigger_exactly_once() {
800:         let trigger = HotkeyTrigger::new(parse_hotkey("F2").unwrap());
801:         *trigger.open.lock().unwrap() = true;
802: 
803:         assert!(take_screen_draw_recovery_trigger(&trigger, true));
804:         assert!(!trigger.take());
805: 
806:         *trigger.open.lock().unwrap() = true;
807:         assert!(!take_screen_draw_recovery_trigger(&trigger, false));
808:         assert!(trigger.take());
809:     }
810: 
811:     #[test]
812:     fn same_primary_conflicts_are_rejected_in_both_modifier_directions() {
813:         for (launcher, emergency) in [("F12", "Ctrl+Shift+F12"), ("Ctrl+Shift+F12", "F12")] {
814:             let mut settings = Settings {
```

### Launcher: `src/mouse_gestures/service.rs:385–420`

```text
385: ///
386: /// The guard is `Send`, so a native input worker can own it and restore mouse
387: /// gestures directly during fail-safe teardown.
388: #[must_use = "dropping the guard immediately releases gesture suppression"]
389: pub struct GestureSuppressionGuard {
390:     service: Arc<Mutex<MouseGestureService>>,
391:     token: Option<GestureSuppressionToken>,
392: }
393: 
394: impl GestureSuppressionGuard {
395:     fn acquire(service: Arc<Mutex<MouseGestureService>>) -> Self {
396:         let token = match service.lock() {
397:             Ok(mut service) => service.acquire_runtime_suppression(),
398:             Err(error) => {
399:                 tracing::error!("mouse gesture service lock was poisoned while suppressing");
400:                 error.into_inner().acquire_runtime_suppression()
401:             }
402:         };
403:         Self {
404:             service,
405:             token: Some(token),
406:         }
407:     }
408: 
409:     pub fn release(&mut self) {
410:         let Some(token) = self.token.take() else {
411:             return;
412:         };
413:         release_guard_token(Arc::clone(&self.service), token);
414:     }
415: }
416: 
417: impl Drop for GestureSuppressionGuard {
418:     fn drop(&mut self) {
419:         self.release();
420:     }
```

### Launcher: `src/mouse_gestures/service.rs:458–466`

```text
458: /// Suppresses the process-wide mouse gesture runtime until the returned guard
459: /// is released or dropped.
460: pub fn acquire_gesture_suppression() -> GestureSuppressionGuard {
461:     GestureSuppressionGuard::acquire(Arc::clone(global_service()))
462: }
463: 
464: pub fn with_service<F>(f: F)
465: where
466:     F: FnOnce(&mut MouseGestureService),
```

### Launcher: `src/commands/model.rs:10–39`

```text
10: 
11: #[derive(Clone, Copy, Debug, PartialEq, Eq)]
12: pub enum ActivationSource {
13:     Enter,
14:     Click,
15:     Dashboard,
16:     Gesture,
17:     Macro,
18: }
19: 
20: impl ActivationSource {
21:     pub fn label(self) -> &'static str {
22:         match self {
23:             Self::Enter => "enter",
24:             Self::Click => "click",
25:             Self::Dashboard => "dashboard",
26:             Self::Gesture => "gesture",
27:             Self::Macro => "macro",
28:         }
29:     }
30: }
31: 
32: #[derive(Clone, Debug, PartialEq)]
33: pub struct CommandInvocation {
34:     pub command: Command,
35:     pub original_action: Action,
36:     pub query_override: Option<String>,
37:     pub source: ActivationSource,
38: }
39: 
```

### Launcher: `src/visibility.rs:26–39`

```text
26: /// placement. Restoring an already-visible launcher must preserve any geometry
27: /// changes made during the current visible session.
28: #[derive(Clone, Copy, Debug, Eq, PartialEq)]
29: pub enum VisiblePlacementPolicy {
30:     ApplyConfiguredPlacement,
31:     PreserveCurrentGeometry,
32: }
33: 
34: /// Process a hotkey trigger and update the minimized state, issuing viewport
35: /// commands when possible. This mirrors the logic from `main.rs`.
36: pub fn handle_visibility_trigger<C: ViewportCtx>(
37:     trigger: &HotkeyTrigger,
38:     visibility: &Arc<AtomicBool>,
39:     restore_flag: &Arc<AtomicBool>,
```

### Launcher: `src/mkmacro/input.rs:7–14`

```text
7: 
8: /// Stable marker used to identify (and ignore while recording) mkmacro input.
9: pub const MKMACRO_EXTRA_INFO: usize = 0x4D4B_4D41_4352_4F01;
10: pub const KEYEVENTF_EXTENDEDKEY_: u32 = 0x0001;
11: pub const KEYEVENTF_KEYUP_: u32 = 0x0002;
12: pub const KEYEVENTF_UNICODE_: u32 = 0x0004;
13: pub const KEYEVENTF_SCANCODE_: u32 = 0x0008;
14: pub const MOUSEEVENTF_MOVE_: u32 = 0x0001;
```

### Launcher: `src/mkmacro/input.rs:66–81`

```text
66: /// Deliberate capability required to construct the backend that can affect the
67: /// user's real desktop. Tests should instead use `FakeBackend` or `with_sink`.
68: /// Keeping this token out of `Default` prevents an acceptance harness from
69: /// accidentally turning a harmless fixture into live `SendInput` calls.
70: #[derive(Debug, Clone, Copy)]
71: pub struct LiveInputOptIn(());
72: impl LiveInputOptIn {
73:     /// Explicitly opts into destructive, production input synthesis.
74:     pub fn production() -> Self {
75:         Self(())
76:     }
77: }
78: impl Win32InputBackend<SystemInputSink> {
79:     pub fn system(_: LiveInputOptIn) -> Self {
80:         Self {
81:             sink: SystemInputSink(()),
```

### Launcher: `src/screen_draw/native_runtime.rs:17–45`

```text
17: const WM_SCREEN_DRAW_COMMAND: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x34;
18: 
19: #[derive(Debug, Clone, PartialEq)]
20: pub enum NativeSessionCommand {
21:     SetTool(ScreenDrawTool),
22:     SetColor(RgbaColor),
23:     SetThickness(f32),
24:     Undo,
25:     Redo,
26:     Clear,
27:     SetAnnotationsVisible(bool),
28:     SetBackground(CanvasBackground),
29:     SetToolbarWindow(Option<ToolbarWindowInfo>),
30:     Ghost,
31:     Resume,
32:     Finish,
33:     PrepareRegionSelection {
34:         generation: ScreenDrawGeneration,
35:         background: ExportBackground,
36:     },
37:     EndRegionSelection,
38:     DisplayChanged,
39:     EmergencyPause,
40:     RenderExport(ExportRenderRequest),
41:     Shutdown,
42: }
43: 
44: #[derive(Debug, Clone, Copy, PartialEq, Eq)]
45: pub struct ExportRenderRequest {
```

### Launcher: `src/screen_draw/window_layers.rs:531–540`

```text
531: -> windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE {
532:     use windows::Win32::UI::WindowsAndMessaging::{
533:         WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
534:     };
535: 
536:     WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE
537: }
538: 
539: #[cfg(windows)]
540: fn interactive_canvas_position_flags()
```

### Launcher: `src/common/persistence.rs:15–29`

```text
15: /// The three file states that generic persistence code can identify safely.
16: #[derive(Clone, Debug, PartialEq, Eq)]
17: #[must_use]
18: pub enum LoadState<T> {
19:     Missing,
20:     Empty,
21:     Loaded(T),
22: }
23: 
24: /// Contextual failures at the shared persistence boundary.
25: #[derive(Debug)]
26: pub enum PersistenceError {
27:     Read {
28:         path: PathBuf,
29:         source: std::io::Error,
```

### Launcher: `src/persistence/catalog.rs:20–57`

```text
20: use std::path::{Component, Path, PathBuf};
21: 
22: #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
23: pub enum PersistentStoreId {
24:     Settings,
25:     Actions,
26:     Bookmarks,
27:     Folders,
28:     Snippets,
29:     Favorites,
30:     Todos,
31:     ShellCommands,
32:     LegacyMacros,
33:     HistoryPins,
34:     CalendarEvents,
35:     Layouts,
36:     DashboardConfig,
37:     MouseGestureDefinitions,
38:     MkMacroDocument,
39:     MkMacroAssets,
40:     MkMacroTemplates,
41:     ClipboardModifiers,
42:     MultiManagerWorkspaces,
43:     Notes,
44:     NotesAssets,
45:     NoteTemplates,
46:     Scratchpad,
47:     QueryHistory,
48:     ClipboardHistory,
49:     CalculatorHistory,
50:     Usage,
51:     CalendarState,
52:     MouseGestureUsage,
53:     MouseGestureState,
54:     NoteUiState,
55:     MultiManagerBindings,
56:     Alarms,
57:     LauncherLog,
```

### Launcher: `docs/plans/universal-action-model.md:70–80`

```text
70: ```
71: 
72: `ActionSurface` says where an action is presented; `ActivationSource` continues
73: to say how an invocation was triggered. They are deliberately independent.
74: 
75: Radial Menu is future work. Universal Actions intentionally contain no radial
76: geometry, wedge placement, hotkey, gesture-duration, or hold-threshold behavior.
77: The existing launcher trigger may eventually be wrapped by a gesture resolver
78: that routes a tap to the launcher and a hold to a radial surface, while custom
79: radial menus may use unrelated triggers. Neither change should require provider
80: or executor redesign, and current hotkey behavior is unchanged by this plan.
```

### Radify: `README.md:381–397`

```text
381: # Options Object Properties
382: 
383: Configuration options for the menu.
384: 
385: Options apply only to the current menu and are not inherited by submenus, except for `Skin` and its associated skin-defined options. To set options for a `Submenu` of a menu item, use the item’s `SubmenuOptions` property.
386: 
387: **Menu options are merged in the following order:**
388: 
389: - User-defined options from the `CreateMenu` method options parameter or menu item `SubmenuOptions` properties.
390: - Skin-defined options.
391: - Global default options.
392: 
393: Some options support multiple scopes: they can be set at the menu level only, or at both menu and item level, with item-level values overriding menu-level ones.
394: 
395: **Scope legend:**
396: 
397: - `Menu`: Can only be set at the menu or submenu level (in the `Options` or `SubmenuOptions` object)
```

### Radify: `README.md:135–167`

```text
135: ## Skin Files
136: 
137: - `ItemGlow.png`
138: - `MenuOuterRim.png`
139: - `MenuBack.png`
140: - `ItemBack.png`
141: - `CenterImage.png`
142: - `SubmenuIndicator.png`
143: 
144: **Note:**
145: 
146: - Only `ItemBack.png` is required for a skin to be considered valid; if this file is missing, the skin will not be loaded.
147: 
148: - The Skins folder requires `.png` files for skin assets. Other image formats can be assigned programmatically or via the **Radify Skin Editor**. See [Supported Image Formats](#supported-image-formats).
149: 
150: **Radial Menu v4 skins**
151: 
152: - Unlike **Radial Menu v4**, which used per-skin `skin definition.txt` files, all settings are loaded from `Preferences.json`.
153: 
154: - To set the submenu indicator image, you can:
155:   - Add a `SubmenuIndicator.png` file to each skin folder.
156:   - In **Radify Skin Editor**, set a default `SubmenuIndicatorImage`, or assign one per skin.
157:   - Set `SubmenuIndicatorImage` programmatically.
158: 
159: ---
160: 
161: ## Set Media Directories
162: 
163: Image and sound files in the configured directories can be referenced by filename only. See [Media Directories Configuration](#media-directories-configuration).
164: 
165: ---
166: 
167: ## Radify Skin Editor Notes
```

### Radify: `README.md:717–738`

```text
717: # Supported Image Formats
718: 
719: - File path to a standard image (`ico, png, jpeg, jpg, gif, bmp, tif`).
720: - Filename with extension (e.g., `downloads.png`) - searches in the [configured image directory](#media-directories-configuration).
721: - Image handles:
722:   - `hIcon`: Icon handle
723:   - `hBitmap`: GDI bitmap handle
724:   - `pBitmap`: GDI+ bitmap pointer
725: - Icons from resource libraries (`.exe`, `.dll`, `.cpl`). Use the format: `fullPath|iconN`, where `N` is the icon index. If `|iconN` is omitted, icon index 1 is used.
726:   - Examples:
727:     - `A_WinDir '\System32\imageres.dll|icon19'`
728:     - `A_ProgramFiles '\Everything\Everything.exe'`
729: 
730: ---
731: 
732: # Supported Sound Formats
733: 
734: - Path to a `.wav` file.
735: - Filename with extension (e.g., `tada.wav`) - searches in both `C:\Windows\Media` and the [configured sound directory](#media-directories-configuration).
736: 
737: ---
738: 
```

### Radify: `README.md:747–766`

```text
747: # License
748: 
749: - MIT License
750: 
751: ---
752: 
753: # Credits
754: 
755: - [AutoHotkey](https://www.autohotkey.com) - Steve Gray, Chris Mallett, portions of the AutoIt Team, and various others.
756: - [Radial Menu v4](https://www.autohotkey.com/boards/viewtopic.php?f=6&t=12078) by Learning one
757: - [GDI+](https://github.com/buliasz/AHKv2-Gdip/blob/master/Gdip_All.ahk)
758:   - tic - Created the original [Gdip.ahk](https://github.com/tariqporter/Gdip) library
759:   - Rseding91, mmikeww, buliasz, and various others.
760: - [JSON](https://github.com/thqby/ahk2_lib/blob/master/JSON.ahk) by thqby, HotKeyIt
761: - Icons and [emojis](https://github.com/microsoft/fluentui-emoji) © Microsoft.
762: 
763: ---
764: 
765: **Radify**
766: 
```

## Evidence limits

The previous inspection supplied source observations and visual references, not compilation, runtime, performance, or test results. It did not execute application/test binaries or reference AHK scripts. These historical excerpts do not establish the contents of the newly requested Git baseline or of files that will be added under `docs/references/`.

Resolve and inspect those local sources before reporting a verified baseline, reference version, settings migration, or compatibility result. Pixel-identical rendering, real Windows input behavior, original RM4 field coverage, runtime resource costs, and passing Nextest results remain implementation/validation gates in the master brief. Do not claim any of them from this document alone.
