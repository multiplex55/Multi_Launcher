# Multi Launcher — Settings Dialog Reorganization + Mouse Gesture Timing Controls

## Status

**State:** Pending implementation

**Source of truth:** Current repository checkout corresponding to:

`multi_launcher(20261005-195918).zip`

This plan is authoritative for this feature branch.

Historical Multi Launcher plans, radial reliability investigations, native-input
diagnostics, candidate reports, acceptance matrices, and previous remediation
methodologies are reference material only.

They are not requirements for this task unless this plan explicitly adopts
them.

---

# 1. Goal

Implement two tightly scoped improvements.

## Goal A — Reorganize the Main Settings Dialog

Reorganize ONLY the main Multi Launcher Settings dialog into logical,
collapsible categories.

The objective is to make the Settings window:

- easier to scan;
- easier to navigate;
- less vertically overwhelming;
- more logically grouped;
- easier to extend later.

This is primarily a **view-layer reorganization**.

Do not change the meaning, persistence, runtime behavior, or ownership of the
settings being reorganized.

---

## Goal B — Expose Mouse Gesture Timing Controls

Expose the two timing values that already exist in the Mouse Gesture runtime:

- trail refresh interval;
- recognition interval.

Add them to the existing dedicated Mouse Gesture Settings dialog.

Preserve the existing runtime algorithm and current defaults.

---

# 2. Engineering Philosophy

This is intentionally a **surgical feature branch**.

The required approach is:

1. inspect the directly relevant current source;
2. establish exact current control ownership;
3. implement the UI reorganization;
4. expose the two existing Mouse Gesture timing values;
5. perform narrowly targeted verification;
6. perform one focused review;
7. stop.

This is NOT:

- a Settings architecture rewrite;
- a serialization redesign;
- a new Settings navigation framework;
- a launcher behavior change;
- a radial behavior change;
- a plugin architecture refactor;
- a Mouse Gesture rendering optimization;
- a Mouse Gesture algorithm rewrite;
- a broad UI redesign;
- a repository-wide regression effort.

Do enough work to implement the feature correctly and preserve existing
behavior.

Then stop.

---

# 3. Agent Orchestration

The parent Codex agent is the **orchestrator**.

Use the configured specialized agents where useful:

- planner;
- implementer;
- reviewer.

Use only one source-writing implementation agent at a time.

Do not run overlapping implementation agents against the same repository state.

The expected flow is:

```text
Orchestrator
    ↓
Focused planner reconnaissance
    ↓
Implementation in coherent checkpoints
    ↓
Frequent checkpoint commits
    ↓
Targeted verification
    ↓
Focused reviewer
    ↓
Targeted remediation if needed
    ↓
Final diff audit
    ↓
Done
```

Do not create:

- candidate cycles;
- repeated qualification rounds;
- repeated independent review loops;
- broad regression campaigns;
- historical radial acceptance workflows.

---

# 4. Main Settings Navigation Model

Retain the existing:

- single Settings window;
- single vertically scrolling main content area;
- Save control at the bottom.

Do NOT introduce:

- sidebar navigation;
- tabs;
- separate category windows;
- custom navigation widgets.

The main Settings dialog should use top-level collapsible sections.

Use the existing egui `CollapsingState` / persistent-ID approach already used
elsewhere in the application.

Do not create a new custom collapse-state framework.

---

# 5. Approved Top-Level Settings Categories

Use exactly these seven top-level categories, in exactly this order:

1. **Hotkeys**
2. **Launcher Window & Appearance**
3. **Search & Results**
4. **Actions, Safety & Feedback**
5. **Dashboard**
6. **Radial Menus**
7. **Plugin Settings**

Radial Menus should no longer dominate the top of the Settings dialog.

Plugin Settings remains toward the bottom.

---

# 6. Default Expansion State

At a fresh application/session state, use:

```text
▼ Hotkeys
▼ Launcher Window & Appearance
▼ Search & Results
▸ Actions, Safety & Feedback
▸ Dashboard
▸ Radial Menus
▸ Plugin Settings
```

Therefore:

```text
Hotkeys                       = expanded
Launcher Window & Appearance  = expanded
Search & Results              = expanded
Actions, Safety & Feedback    = collapsed
Dashboard                     = collapsed
Radial Menus                  = collapsed
Plugin Settings               = collapsed
```

Use egui session memory so open/closed state naturally survives:

- subsequent frames;
- closing and reopening the Settings window during the same application run.

Do NOT add serialized application-settings fields solely to remember section
expansion across application restarts.

---

# 7. Expand / Collapse All

Add compact controls near the top of the main Settings content:

```text
Expand all
Collapse all
```

These controls affect ONLY the seven new top-level Settings sections.

They must NOT recursively expand or collapse the individual plugin settings
subsections.

The existing plugin-specific:

```text
Expand plugin sections
Collapse plugin sections
```

behavior remains independent.

Example:

```text
Collapse all
    ↓
Hotkeys                    collapsed
Launcher Window            collapsed
Search & Results           collapsed
Actions & Feedback         collapsed
Dashboard                  collapsed
Radial Menus               collapsed
Plugin Settings            collapsed
```

This must NOT mutate:

```text
mouse_gestures plugin subsection
notes subsection
clipboard_modify subsection
other plugin subsection states
```

When Plugin Settings is reopened, its previously remembered individual plugin
section states should remain intact in egui memory.

---

# 8. Internal Section Presentation

Inside each major section, use simple labels and separators where useful.

Example:

```text
Launcher Window & Appearance

Window behavior
    Always on top

Position
    Follow mouse
    Use static position
    X / Y / W / H
    Snapshot

Appearance
    Query scale
    List scale
    Open Theme Settings...
```

These internal labels are NOT additional collapsible levels.

Do NOT create a nested tree of collapsible sections.

Use only one primary collapse level for the main Settings categories.

Preserve the current egui visual language.

Do not substantially restyle existing controls.

---

# 9. Hotkeys Category

Move/render the following controls under **Hotkeys**:

- Launcher hotkey
- Enable quit hotkey
- Quit hotkey
- Enable help hotkey
- Help hotkey

Preserve exactly:

- current hotkey parsing;
- validation;
- valid/invalid indicators;
- optional hotkey behavior;
- stored values;
- callbacks.

No hotkey behavior changes are permitted.

---

# 10. Launcher Window & Appearance Category

Render the following under **Launcher Window & Appearance**.

## Window behavior

- Always on top

## Position

- Off-screen X
- Off-screen Y
- Follow mouse
- Use static position
- Static X
- Static Y
- Static W
- Static H
- Snapshot

## Appearance

- Query scale
- List scale
- Open Theme Settings...

Preserve the existing interaction between:

```text
Follow mouse
Use static position
```

If enabling Follow Mouse currently disables Static Position, preserve that
behavior exactly.

Preserve the existing Snapshot behavior exactly.

Do not modify Theme Settings itself.

---

# 11. Search & Results Category

Render the following under **Search & Results**.

## Search behavior

- Enable query autocomplete
- Fuzzy weight
- Usage weight
- Match exact
- Page jump

## Results layout

- Display results in grid layout
- Grid rows
- Grid columns
- Respect plugin list/grid capability
- Force list for plugins

Preserve all existing dependent enablement.

For example, grid-specific controls should remain gated by the current
`query_results_layout_enabled` behavior.

Do not alter:

- fuzzy search behavior;
- usage weighting;
- exact matching;
- pagination;
- plugin list/grid capability behavior;
- result layout runtime behavior.

---

# 12. Actions, Safety & Feedback Category

Render the following under **Actions, Safety & Feedback**.

## After running an action

- Hide window after running action
- Preserve command after run
- Clear query after run

## Safety

- Require confirm for destructive actions

## Notifications and errors

- Enable toast notifications
- Toast duration
- Show inline errors
- Show error toasts

## Diagnostics and refresh

- Debug logging
- Disable timer auto refresh
- Timer refresh rate

Preserve all current dependencies.

Examples:

- Toast duration remains enabled only according to existing toast behavior.
- Timer refresh remains enabled/disabled according to the current
  `disable_timer_updates` logic.

Do not change runtime behavior.

---

# 13. Dashboard Category

Keep Dashboard settings together under **Dashboard**.

Retain the current controls, including:

- Enable dashboard when query is empty
- Dashboard config path
- Default location
- Show dashboard when the search box is blank
- Reduce dashboard work when not focused
- Show dashboard diagnostics (dev)
- Customize Dashboard...

Preserve current conditional visibility for developer/debug-only controls.

Do not redesign the Dashboard editor.

Remove a redundant internal:

```text
Dashboard
```

heading if the new top-level collapsing header already provides that title.

Do not change Dashboard behavior.

---

# 14. Radial Menus Category

Keep all current Radial Menu settings together.

The section should include the existing controls for:

- Enable radial menus
- Share the launcher hotkey between tap and hold
- Hold threshold
- Tooltip scope
- Tooltip delay
- Expected label-layout diagnostics
- Default menu
- New-menu interaction behavior
- New-menu submenu behavior
- Destructive action safety
- Process-wide radial item inputs
- New-item input scope
- Shared-trigger information
- Conflict/validation messages
- Back / Esc explanatory text
- Edit menus...
- Edit skins...
- Any existing migration/diagnostic controls currently owned by this section

The section defaults collapsed.

Do not change:

- radial enablement semantics;
- shared tap/hold behavior;
- radial hotkey behavior;
- hold threshold behavior;
- radial input scope behavior;
- radial runtime resources;
- Radial Designer behavior;
- radial skin behavior.

This is presentation-only.

---

# 15. Plugin Settings Category

Wrap the existing plugin-settings area in the new top-level:

**Plugin Settings**

category.

Preserve:

- enabled-plugin filtering;
- deferred plugin reload messages;
- current plugin enumeration;
- plugin settings serialization;
- individual plugin `CollapsingState`;
- Notes special handling;
- Clipboard Modify special handling;
- Mouse Gestures special handling;
- existing "Expand plugin sections" / "Collapse plugin sections" controls.

The Plugin Settings top-level category defaults collapsed.

Its top-level open/closed state must be independent from individual plugin
subsection state.

Do not redesign plugin settings in this branch.

---

# 16. Main Settings Behavior / Persistence Constraints

This Settings reorganization is primarily a VIEW-LAYER change.

Do NOT:

- rename persisted main Settings keys;
- modify main Settings serialization;
- introduce replacement persistence structures;
- change Settings load semantics;
- change Settings save semantics;
- change when settings take effect;
- change runtime launcher behavior;
- change runtime radial behavior;
- change plugin behavior;
- redesign dedicated sub-settings dialogs.

The existing `SettingsEditor` remains the working edit buffer.

Collapsing a section must NOT:

- reset its values;
- reload values from disk;
- discard unsaved edits;
- trigger Save;
- mutate runtime behavior merely due to collapsing.

Reordering controls must not affect their values or dependencies.

Visual category ownership does NOT need to mirror Rust serialization structs.

---

# 17. Expected Current Main Settings Architecture

Confirm the current checkout before editing.

Expected relevant source:

```text
src/settings_editor/render.rs
src/settings_editor/state.rs
src/settings_editor/mapping.rs
```

The current Settings render path is expected to include approximately:

```text
render_hotkey_section
render_radial_section
render_general_section
render_layout_section
render_dashboard_section
render_plugin_sections
```

The plugin-settings renderer already uses:

```rust
egui::collapsing_header::CollapsingState
```

with persistent IDs.

Reuse this pattern.

Do not invent a second collapse-state mechanism.

---

# 18. Recommended Top-Level Section Helper

Prefer one small UI-only helper for top-level Settings sections.

Conceptually:

```rust
show_settings_section(
    ui,
    stable_id,
    title,
    default_open,
    top_level_expand_request,
    |ui| {
        // existing settings controls
    },
);
```

The exact signature is implementation-owned.

Requirements:

- IDs are stable across frames.
- IDs clearly distinguish the seven categories.
- Default-open state follows this plan.
- A one-frame Expand/Collapse All request can override the current state.
- No serialized state is required.

Example stable IDs:

```text
settings_section_hotkeys
settings_section_launcher_window
settings_section_search_results
settings_section_actions_feedback
settings_section_dashboard
settings_section_radial
settings_section_plugins
```

Exact strings may differ.

A tiny enum/helper describing the seven sections is acceptable if it improves
clarity.

Do not over-abstract this.

---

# 19. Top-Level Expand / Collapse Request

Prefer a one-frame UI request:

```text
None
Some(true)   -> open all seven top-level sections
Some(false)  -> close all seven top-level sections
```

Do not reuse the existing plugin subsection `expand_request` if doing so would
couple top-level and plugin-subsection expansion.

Top-level section expansion and plugin subsection expansion are distinct
concepts.

---

# 20. Main Settings Renderer Refactor

The current General and Layout renderers contain controls that belong to several
new categories.

Refactor them into clearer content renderers.

A reasonable resulting structure is approximately:

```text
render_hotkey_section
render_launcher_window_appearance_section
render_search_results_section
render_actions_safety_feedback_section
render_dashboard_section
render_radial_section
render_plugin_sections
```

Exact private function names are implementation-owned.

Do not retain obsolete duplicate render paths after migration.

Every pre-existing main Settings control must appear exactly once.

---

# 21. Control Preservation Audit

Before modifying the render layout, establish an inventory of all existing
controls currently rendered by the relevant Settings functions.

The purpose is to avoid accidentally dropping a setting during the move.

After implementation, compare the new rendering against that inventory.

Verify:

- every old control still exists;
- no control exists twice;
- callbacks still call the same methods;
- dependencies remain intact;
- buttons still perform the same action;
- conditional developer/debug controls remain conditional.

This is a review activity.

Do NOT build a UI automation framework simply to inventory widgets.

---

# 22. Mouse Gesture Timing Goal

Add two controls to the EXISTING dedicated:

```text
Mouse Gesture Settings
```

dialog.

Add:

```text
Timing / Performance

Trail refresh interval (ms)       16
Recognition interval (ms)         40

Lower intervals update more frequently but may use more CPU.
```

Do NOT add these controls to the main Settings dialog.

Do NOT reorganize the rest of the Mouse Gesture Settings dialog.

---

# 23. Existing Mouse Gesture Timing Architecture

Current source already contains:

```rust
pub struct MouseGestureConfig {
    pub trail_interval_ms: u64,
    pub recognition_interval_ms: u64,
    ...
}
```

Current defaults are:

```text
trail_interval_ms       = 16
recognition_interval_ms = 40
```

The worker derives its poll interval from the smaller timing value and
independently gates recognition work using the recognition interval.

Preserve this algorithm.

This branch exposes configuration.

It does NOT redesign cadence processing.

---

# 24. MouseGestureSettings Persistence

Extend the existing:

```text
src/plugins/mouse_gestures.rs
MouseGestureSettings
```

with:

```rust
trail_interval_ms: u64
recognition_interval_ms: u64
```

Use serde defaults.

Existing configurations that do not contain these fields must load
successfully.

Required default values:

```text
trail_interval_ms       = 16
recognition_interval_ms = 40
```

Prefer a shared source of truth for the default timing constants so
`MouseGestureSettings` and `MouseGestureConfig` cannot silently drift apart.

For example:

```rust
pub(crate) const DEFAULT_TRAIL_INTERVAL_MS: u64 = 16;
pub(crate) const DEFAULT_RECOGNITION_INTERVAL_MS: u64 = 40;
```

Exact visibility/location is implementation-owned.

A tiny corresponding change in:

```text
src/mouse_gestures/service.rs
```

is allowed if necessary to centralize those defaults.

Do not otherwise modify the worker loop.

---

# 25. Mouse Gesture Runtime Mapping

Current `MouseGestureRuntime::apply` constructs a `MouseGestureConfig` from
`MouseGestureSettings`.

Add:

```text
MouseGestureSettings.trail_interval_ms
    -> MouseGestureConfig.trail_interval_ms

MouseGestureSettings.recognition_interval_ms
    -> MouseGestureConfig.recognition_interval_ms
```

Preserve every existing mapping.

A small private pure conversion helper is permitted if it cleanly centralizes
the mapping and improves testability.

Conceptually:

```rust
fn runtime_config_from_settings(
    settings: &MouseGestureSettings,
    plugin_enabled: bool,
) -> MouseGestureConfig
```

The exact signature is implementation-owned.

If extracted:

- production uses it;
- tests use the same helper;
- no duplicate mapping is created;
- it does not become public API merely for testing.

---

# 26. Mouse Gesture Timing UI

Modify only the existing dedicated dialog:

```text
src/gui/mouse_gesture_settings_dialog.rs
```

Add a small non-collapsible:

```text
Timing / Performance
```

area near the existing visual Trail / Hint settings.

Add:

```text
Trail refresh interval (ms)
Recognition interval (ms)
```

Prefer `egui::DragValue`.

Use these limits:

```text
Trail refresh interval:
    minimum = 4 ms
    maximum = 250 ms
    default = 16 ms

Recognition interval:
    minimum = 4 ms
    maximum = 500 ms
    default = 40 ms
```

Recommended adjustment speed:

```text
1 ms
```

Allow the values to be configured independently.

Do NOT force:

```text
recognition_interval_ms >= trail_interval_ms
```

The current worker design already supports independent values.

Add concise help text:

```text
Lower intervals update more frequently but may use more CPU.
```

Equivalent precise wording is acceptable.

---

# 27. Mouse Gesture Live Apply

Preserve the dedicated dialog's existing live-apply path.

The two new controls must participate in the same change handling as existing
Mouse Gesture settings.

Changing either timing value should:

1. update the local `MouseGestureSettings`;
2. mark the dialog dirty;
3. use the existing runtime apply mechanism;
4. update the currently running Mouse Gesture service configuration.

Do not add a new timing-specific runtime channel.

Saving continues through the existing plugin-settings persistence path.

---

# 28. Mouse Gesture Backward Compatibility

An old configuration such as:

```json
{
  "enabled": true,
  "show_trail": true
}
```

must continue to deserialize successfully.

The resulting timing values must be:

```text
trail_interval_ms       = 16
recognition_interval_ms = 40
```

No explicit migration is required.

Normal serialization after the user saves may naturally include the new
fields.

---

# 29. Mouse Gesture Non-Goals

Do NOT implement in this branch:

- dirty-rectangle invalidation;
- trail rendering optimization;
- mouse interpolation;
- smoothing/splines;
- antialiasing changes;
- raw mouse input;
- alternate Windows mouse hooks;
- higher-frequency hook callbacks;
- recognition algorithm changes;
- direction classification changes;
- deadzone changes;
- gesture thresholds;
- hint redesign;
- general Mouse Gesture Settings reorganization.

Expose the existing timing values only.

Rendering optimization can be evaluated separately after the user experiments
with these controls.

---

# 30. Settings Search / Filter

Do NOT add a Settings search/filter field in this branch.

Evaluate the collapsible organization first.

Search can be considered later if Settings continues to grow.

---

# 31. Visual Redesign Non-Goals

Keep the current egui visual language.

Do NOT add:

- a theme overhaul;
- new icon infrastructure;
- animations;
- custom category cards requiring new infrastructure;
- window sizing redesign;
- Settings position redesign;
- Theme Settings changes.

The goal is organization, not visual reinvention.

---

# 32. Commit Cadence — Active Checkpoint Commits

This feature branch should use **smaller, more frequent commits** than previous
large initiatives.

Do not wait for an entire large milestone to finish before committing when that
milestone contains multiple independently understandable changes.

The objective is:

> each commit represents one coherent checkpoint that leaves repository history
> easy to follow.

Do NOT:

- commit every few lines;
- commit knowingly broken intermediate code;
- create meaningless "WIP" commits;
- manufacture commits with no coherent purpose.

But also do NOT allow multiple substantial subsections to accumulate into one
very large commit merely because they belong to the same milestone.

---

# 33. Commit Stage Identifiers

Use plan subsection identifiers in commit subjects.

Preferred format:

```text
<type>(<scope>): [M#-X] <clear description>
```

Examples:

```text
refactor(settings): [M1-A] add top-level collapsible section framework

feat(settings): [M1-B] add top-level expand and collapse controls

refactor(settings): [M2-A] move hotkey controls into their settings section

refactor(settings): [M2-B] organize launcher window and appearance controls

refactor(settings): [M2-C] organize search and result layout settings

refactor(settings): [M2-D] group action safety and feedback settings

refactor(settings): [M2-E] place dashboard settings in their top-level section

refactor(settings): [M2-F] place radial settings in their top-level section

refactor(settings): [M2-G] wrap plugin settings without coupling collapse state

feat(mouse-gestures): [M3-A] persist configurable gesture timing intervals

refactor(mouse-gestures): [M3-B] map timing settings into runtime config

feat(mouse-gestures): [M3-C] expose trail and recognition timing controls

test(mouse-gestures): [M4-A] cover timing defaults and runtime mapping

fix(settings): [M6-A] preserve plugin section state during top-level collapse
```

These are examples of natural boundaries.

They are not a requirement to produce exactly this number of commits.

However:

- do not collapse all of M2 into one giant commit;
- do not collapse all Settings work into one giant commit;
- do not collapse all Mouse Gesture work into one giant commit if persistence,
  runtime mapping, and UI are independently coherent.

---

# 34. Commit Message Detail

Commit subjects should clearly describe what changed.

Do not optimize commit messages for minimum length.

When the reason or preserved behavior is not obvious, include a short body.

Example:

```text
refactor(settings): [M2-C] organize search and result layout settings

Move query autocomplete, ranking, matching, paging, and result-grid controls
into the Search & Results top-level section.

Preserve existing SettingsEditor ownership and grid-dependent enablement; this
checkpoint changes presentation only.
```

Example:

```text
feat(mouse-gestures): [M3-A] persist configurable gesture timing intervals

Add backward-compatible trail and recognition timing fields to
MouseGestureSettings.

Existing configurations continue to deserialize with the current 16 ms and
40 ms defaults.
```

Avoid vague messages such as:

```text
updates
changes
more work
cleanup
wip
checkpoint
codex changes
```

---

# 35. Practical Commit Rule

After completing a coherent subsection:

1. inspect the relevant source diff;
2. perform the smallest inexpensive check that is useful at that point;
3. ensure the repository is in a coherent state;
4. create the checkpoint commit;
5. record it compactly in the execution ledger;
6. continue to the next subsection.

Do not require expensive final verification before every checkpoint commit.

Example:

```text
implement Hotkeys migration
    ↓
inspect diff
    ↓
small/local check if useful
    ↓
commit M2-A
    ↓
implement Launcher Window & Appearance migration
    ↓
commit M2-B
```

Then later:

```text
all coherent implementation complete
    ↓
focused tests
    ↓
normal launcher build
    ↓
focused review
```

---

# 36. Avoid Long-Lived Uncommitted Diffs

Before beginning a materially different subsection, strongly prefer committing
the previous coherent subsection.

Examples:

- finish Hotkeys -> commit before substantial Launcher Window work;
- finish Search & Results -> commit before Actions & Feedback;
- finish Radial section migration -> commit before Plugin Settings integration;
- finish timing persistence -> commit before timing UI if both are meaningful
  independent checkpoints.

If two adjacent pieces are genuinely tiny and inseparable, combining them is
acceptable.

If a subsection grows unexpectedly large, split it at a natural boundary.

The goal is visible, active progress rather than long-lived giant diffs.

---

# 37. Commit Verification Expectations

Do not run:

```text
cargo build --bin multi_launcher
```

before every commit.

Do not run expensive Nextest filters before every UI-only checkpoint unless the
checkpoint changes logic covered by those tests.

Checkpoint commits may precede final verification as long as they are coherent
and do not knowingly leave the repository broken.

Final targeted verification remains concentrated in the dedicated verification
milestones.

If final verification reveals a defect:

- fix it in a new descriptive commit;
- do not silently rewrite unrelated earlier checkpoints;
- do not squash the checkpoint history unless the user explicitly requests it.

The checkpoint history is intentional and useful.

---

# 38. Commit Safety

Before every checkpoint commit, inspect:

```text
git status
git diff
```

Ensure the commit contains only intended task changes.

Do not commit:

- unrelated user changes;
- generated runtime data;
- build artifacts;
- temporary diagnostics;
- accidental formatting churn;
- historical plan outputs unrelated to this feature.

Preserve pre-existing user changes.

---

# 39. Commit Ledger

When a checkpoint commit is created, add a compact entry to the execution
ledger.

Format:

```text
- M1-A complete — `<hash>` refactor(settings): [M1-A] add top-level collapsible section framework
- M1-B complete — `<hash>` feat(settings): [M1-B] add top-level expand and collapse controls
- M2-A complete — `<hash>` refactor(settings): [M2-A] move hotkey controls into their settings section
```

Do not write verbose per-command or candidate history into the ledger.

Git history is the detailed chronological record.

The ledger should identify:

- milestone/checkpoint;
- commit hash;
- commit subject;
- current next stage.

---

# 40. Milestone 0 — Focused Reconnaissance

## Objective

Confirm exact current ownership before source modification.

## Planner assignment

Read:

```text
AGENTS.md
docs/plans/settings-dialog-reorganization-and-mouse-gesture-timing.md
```

Inspect only directly relevant source:

```text
src/settings_editor/render.rs
src/settings_editor/state.rs
src/settings_editor/mapping.rs
src/plugins/mouse_gestures.rs
src/gui/mouse_gesture_settings_dialog.rs
src/mouse_gestures/service.rs
```

Inspect directly relevant tests where necessary.

Confirm:

1. every currently rendered main Settings control;
2. current render function ownership;
3. existing plugin `CollapsingState` pattern;
4. existing plugin expand/collapse state behavior;
5. current MouseGestureSettings persistence;
6. current MouseGestureRuntime settings mapping;
7. current Mouse Gesture live-apply/save behavior;
8. current 16 ms / 40 ms runtime defaults.

Do not:

- inspect unrelated plugins;
- inspect radial input internals;
- resume native-input investigations;
- run broad tests;
- pre-implement the feature.

## Planner deliverable

Provide one bounded implementation handoff containing:

- exact current relevant symbols;
- current control inventory;
- any source mismatch with this plan;
- exact source files expected to change;
- recommended implementation order;
- exact focused test locations;
- explicit non-goals.

## Done criteria

Implementation can begin without rediscovering architecture.

No commit is required for a read-only planning milestone unless the plan itself
is intentionally updated with a material source correction.

---

# 41. Milestone 1 — Top-Level Settings Infrastructure

## M1-A — Collapsible Section Framework

### Objective

Add the reusable top-level Settings collapsing mechanism.

### Required changes

Implement:

- stable persistent IDs;
- default-open values;
- top-level section helper;
- one-frame forced open/close support.

Do not yet perform unnecessary behavior changes.

### Checkpoint

Create a commit after this infrastructure is coherent.

Suggested subject:

```text
refactor(settings): [M1-A] add top-level collapsible section framework
```

---

## M1-B — Expand / Collapse All

### Objective

Add:

```text
Expand all
Collapse all
```

for the seven top-level sections.

Ensure plugin subsection state remains independent.

### Checkpoint

Create a separate coherent commit.

Suggested subject:

```text
feat(settings): [M1-B] add top-level expand and collapse controls
```

Do not wait until the entire Settings reorganization is complete before
committing M1.

---

# 42. Milestone 2 — Main Settings Content Reorganization

This milestone should deliberately produce several checkpoint commits.

Do not implement all seven categories and then commit one giant M2 diff.

---

## M2-A — Hotkeys

Move the current hotkey controls into the Hotkeys section.

Preserve behavior exactly.

Checkpoint commit:

```text
refactor(settings): [M2-A] move hotkey controls into their settings section
```

---

## M2-B — Launcher Window & Appearance

Move:

- Always on top;
- positioning;
- follow mouse;
- static position;
- snapshot;
- offscreen position;
- query/list scale;
- Theme Settings button.

Preserve all dependencies.

Checkpoint commit:

```text
refactor(settings): [M2-B] organize launcher window and appearance controls
```

---

## M2-C — Search & Results

Move:

- query autocomplete;
- fuzzy weight;
- usage weight;
- exact matching;
- page jump;
- grid mode;
- grid rows/columns;
- capability behavior;
- force-list plugins.

Checkpoint commit:

```text
refactor(settings): [M2-C] organize search and result layout settings
```

---

## M2-D — Actions, Safety & Feedback

Move:

- post-action behavior;
- destructive confirmation;
- toast settings;
- error settings;
- debug logging;
- timer refresh controls.

Checkpoint commit:

```text
refactor(settings): [M2-D] group action safety and feedback settings
```

---

## M2-E — Dashboard

Wrap the existing Dashboard controls in the new top-level section.

Avoid unnecessary changes to the Dashboard contents.

Checkpoint commit:

```text
refactor(settings): [M2-E] place dashboard settings in their top-level section
```

If this diff is truly trivial and tightly coupled with M2-F, it may be combined,
but the default preference is a separate checkpoint.

---

## M2-F — Radial Menus

Wrap the complete existing radial settings body in the new top-level section.

Do not alter radial behavior.

Checkpoint commit:

```text
refactor(settings): [M2-F] place radial settings in their top-level section
```

---

## M2-G — Plugin Settings

Wrap Plugin Settings in the new top-level section.

Ensure:

- plugin subsection collapse state is independent;
- top-level Expand/Collapse does not recursively affect plugins;
- existing plugin Expand/Collapse controls continue to work.

Checkpoint commit:

```text
refactor(settings): [M2-G] isolate plugin settings under top-level section
```

---

# 43. Milestone 3 — Mouse Gesture Timing Controls

This milestone should normally be split into multiple commits.

---

## M3-A — Persistence and Defaults

### Objective

Add the two backward-compatible fields to `MouseGestureSettings`.

Implement:

```text
trail_interval_ms
recognition_interval_ms
```

with defaults:

```text
16
40
```

Prefer centralized default constants.

Do not modify the worker algorithm.

Checkpoint commit:

```text
feat(mouse-gestures): [M3-A] persist configurable gesture timing intervals
```

---

## M3-B — Runtime Mapping

### Objective

Pass persisted timing values into `MouseGestureConfig`.

If useful, extract a small private pure mapping helper.

Preserve all other config fields.

Checkpoint commit:

```text
refactor(mouse-gestures): [M3-B] map timing settings into runtime config
```

If M3-A and M3-B are genuinely inseparable in the current source, they may be
combined, but only if the resulting commit remains clear and reasonably sized.

---

## M3-C — Dedicated Dialog Controls

### Objective

Expose the timing values in the dedicated Mouse Gesture Settings dialog.

Add:

```text
Timing / Performance
Trail refresh interval (ms)
Recognition interval (ms)
Lower intervals update more frequently but may use more CPU.
```

Ranges:

```text
Trail        4..=250
Recognition  4..=500
```

Preserve the existing dirty/live-apply/save path.

Checkpoint commit:

```text
feat(mouse-gestures): [M3-C] expose trail and recognition timing controls
```

---

# 44. Milestone 4 — Targeted Automated Verification

## Verification Budget

This is a hard scope constraint.

Do NOT automatically run:

```text
cargo nextest run
```

for the whole repository.

Do NOT run:

- all plugin tests;
- radial native/input suites;
- historical radial acceptance;
- candidate cycles;
- screenshot tests;
- pixel-perfect UI tests;
- Multi Manager tests;
- MkMacro tests;
- unrelated launcher tests.

---

## M4-A — Mouse Gesture Timing Tests

Add focused coverage for:

### Test 1 — Defaults / Backward Compatibility / Round Trip

Prove that a settings JSON value without the new fields loads as:

```text
trail_interval_ms       = 16
recognition_interval_ms = 40
```

Then prove custom values such as:

```text
trail_interval_ms       = 8
recognition_interval_ms = 20
```

round-trip correctly.

### Test 2 — Runtime Mapping

Prove custom persisted values become the corresponding runtime
`MouseGestureConfig` fields.

Prefer pure mapping tests.

Do not start process-wide hooks merely to verify field assignment.

Run only the exact relevant tests.

Checkpoint commit if the tests are a meaningful independent change:

```text
test(mouse-gestures): [M4-A] cover timing defaults and runtime mapping
```

If tests were appropriately included with their implementation commits, a
separate test-only commit is not mandatory.

---

# 45. Milestone 5 — Build and Static Checks

After coherent implementation is complete and focused tests pass:

Run:

```text
cargo build --bin multi_launcher
```

Run scoped formatting checks for changed Rust files.

Run:

```text
git diff --check
```

Do not repeatedly rebuild unchanged areas.

If no source changes are required from these checks, no empty "verification
commit" is needed.

If remediation is required, create a focused commit describing the actual
correction.

---

# 46. Milestone 6 — Focused Review

Use one focused reviewer.

The reviewer should inspect:

- final diff;
- directly relevant source;
- Settings control preservation;
- Mouse Gesture timing mapping;
- commit scope/history where useful.

Do not initiate broader testing.

---

## Main Settings Review Checklist

Confirm:

1. Exactly seven top-level categories exist.
2. Category order matches the plan.
3. Hotkeys defaults open.
4. Launcher Window & Appearance defaults open.
5. Search & Results defaults open.
6. Remaining four categories default closed.
7. Expand all affects all seven top-level categories.
8. Collapse all affects all seven top-level categories.
9. Plugin subsection expansion remains independent.
10. Every previous main Settings control still exists exactly once.
11. No Settings persistence keys changed.
12. Save/load semantics remain unchanged.
13. Existing enable/disable dependencies remain intact.
14. Save footer behavior remains unchanged.
15. No dedicated sub-settings dialog was redesigned.

---

## Mouse Gesture Review Checklist

Confirm:

1. Defaults remain exactly 16 / 40.
2. Missing JSON fields receive defaults.
3. Custom values serialize correctly.
4. Runtime config receives custom values.
5. UI ranges are exactly 4–250 and 4–500.
6. Values remain independent.
7. Existing dirty/live-apply path is used.
8. Existing save path is used.
9. Worker cadence algorithm is unchanged.
10. No trail-rendering optimization slipped into scope.

---

## Scope Review

Confirm no:

- unrelated refactor;
- hotkey behavior modification;
- radial input modification;
- Settings architecture rewrite;
- native input work;
- historical acceptance work;
- broad regression campaign.

---

# 47. Reviewer Remediation

If the reviewer finds a concrete issue:

1. report the exact finding;
2. return it to the same implementation writer;
3. fix only the supported issue;
4. rerun only directly affected verification;
5. create a clearly described remediation commit.

Example:

```text
fix(settings): [M6-A] preserve plugin collapse state across top-level toggles
```

Do not fold reviewer corrections silently into unrelated previous commits.

Do not restart the implementation.

Do not create repeated reviewer cycles for trivial changes.

---

# 48. Final Diff Audit

Before completion:

```text
git status
git diff
git diff --check
```

Confirm:

- no settings disappeared;
- no settings are duplicated;
- no temporary debug code;
- no commented-out old renderers;
- no generated runtime data accidentally included;
- no build artifacts;
- no Mouse Gesture worker algorithm modifications;
- no main Settings serialization changes;
- no unrelated formatting churn;
- no unrelated user changes committed.

Preserve pre-existing user/application changes.

---

# 49. Completion Criteria

## Main Settings

- [ ] Main Settings remains one vertically scrolling window.
- [ ] Seven approved top-level collapsible categories exist.
- [ ] Category order is correct.
- [ ] Hotkeys defaults open.
- [ ] Launcher Window & Appearance defaults open.
- [ ] Search & Results defaults open.
- [ ] Actions, Safety & Feedback defaults closed.
- [ ] Dashboard defaults closed.
- [ ] Radial Menus defaults closed.
- [ ] Plugin Settings defaults closed.
- [ ] Top-level Expand all works.
- [ ] Top-level Collapse all works.
- [ ] Top-level controls do not recursively alter plugin subsections.
- [ ] Existing plugin subsection expansion remains functional.
- [ ] Every previous main Settings control exists exactly once.
- [ ] Unsaved values survive collapse/expand.
- [ ] Save behavior remains unchanged.
- [ ] Main Settings serialization/schema remains unchanged.
- [ ] Launcher behavior remains unchanged.
- [ ] Radial behavior remains unchanged.
- [ ] Plugin behavior remains unchanged.

## Mouse Gestures

- [ ] Trail refresh interval is exposed.
- [ ] Recognition interval is exposed.
- [ ] Defaults remain 16 ms / 40 ms.
- [ ] UI ranges are 4–250 / 4–500 ms.
- [ ] Values remain independently configurable.
- [ ] Existing configs missing the fields load with defaults.
- [ ] Custom values round-trip correctly.
- [ ] Values live-apply through the existing mechanism.
- [ ] Runtime config receives configured values.
- [ ] Worker cadence algorithm is unchanged.
- [ ] No dirty-rectangle/render optimization was added.

## Verification

- [ ] Focused Mouse Gesture tests pass.
- [ ] `cargo build --bin multi_launcher` passes.
- [ ] Scoped formatting checks pass.
- [ ] `git diff --check` passes.
- [ ] Focused reviewer has no unresolved concrete findings.
- [ ] No full repository Nextest run was performed merely for ceremony.
- [ ] No manual UI acceptance gate was introduced.

## Git History

- [ ] Major implementation subsections have clear checkpoint commits.
- [ ] M2 was not accumulated into one giant settings-reorganization commit.
- [ ] Commit subjects include plan-stage identifiers.
- [ ] Commit messages clearly describe meaningful changes.
- [ ] No meaningless WIP/checkpoint commits were created.
- [ ] Commit history was not squashed unless explicitly requested by the user.

---

# 50. Stop Condition

Once the completion criteria are satisfied:

**STOP.**

Do not continue into:

- Settings search;
- sidebar navigation;
- tabs;
- icon infrastructure;
- theme work;
- Mouse Gesture dirty-rect rendering;
- Mouse Gesture smoothing;
- additional gesture timing controls;
- radial skin implementation;
- unrelated UI cleanup;
- repository-wide testing.

Record potentially valuable future work separately.

---

# 51. Final Codex Report

At completion report:

## Implemented

Summarize:

- Settings category reorganization;
- collapsible behavior;
- Expand/Collapse All;
- Mouse Gesture timing configuration.

## Architecture

State explicitly:

- main Settings persistence remained unchanged;
- category organization is a view-layer concern;
- Mouse Gesture plugin settings intentionally gained two backward-compatible
  timing fields;
- Mouse Gesture worker cadence algorithm remained unchanged.

## Commits

List checkpoint commits in chronological order:

```text
<hash> <subject>
<hash> <subject>
...
```

Briefly identify the plan subsection represented by each.

## Files Changed

List meaningful files only.

## Tests

List exact targeted test commands and results.

## Build / Static Checks

Report actual results for:

```text
cargo build --bin multi_launcher
```

and formatting/diff checks.

## Review

Summarize concrete reviewer findings and remediation commits.

## Explicitly Not Run

State that broad/full regression and historical native/radial suites were
intentionally outside scope.

## Remaining Issues

List only genuine known issues inside this feature scope.

Do not manufacture follow-up work.

---

# 52. Compact Execution Ledger

Maintain a compact ledger at the bottom of this plan.

Do not create candidate/pass histories.

Initial state:

```text
- M0 Focused reconnaissance — complete
- M1-A complete — `cd44fb95` refactor(settings): [M1-A] add top-level collapsible section framework
- M1-B complete — `5d8210b6` feat(settings): [M1-B] add top-level expand and collapse controls
- M2-A complete — `153e8a67` refactor(settings): [M2-A] move hotkey controls into their settings section
- M2-B complete — `b0e0d1d7` refactor(settings): [M2-B] organize launcher window and appearance controls
- M2-C complete — `59c9beea` refactor(settings): [M2-C] organize search and result layout settings
- M2-D complete — `00895e0e` refactor(settings): [M2-D] group action safety and feedback settings
- Next checkpoint: M2-E Dashboard
- M2-E Dashboard — pending
- M2-F Radial Menus — pending
- M2-G Plugin Settings — pending
- M3-A Mouse Gesture timing persistence/defaults — pending
- M3-B Mouse Gesture runtime mapping — pending
- M3-C Mouse Gesture timing UI — pending
- M4 Targeted automated verification — pending
- M5 Build/static checks — pending
- M6 Focused review/remediation — pending
- Final diff/completion — pending
```

When a checkpoint commit is created, update compactly:

```text
- M2-B complete — `<hash>` refactor(settings): [M2-B] organize launcher window and appearance controls
```

Do not append repetitive command logs or per-attempt debugging history.
