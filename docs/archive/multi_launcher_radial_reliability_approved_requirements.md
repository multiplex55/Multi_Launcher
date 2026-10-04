# Multi Launcher — approved radial reliability and action-authoring requirements

Approved in this conversation on 24 September 2026. This file is the behavior contract, not a claim of implementation or validation.

**Authoritative starting archive:** `multi_launcher(20260924-210943).zip`  
**Archive SHA-256:** `f21ca0ee92d1b7572f779b936bb3ce1de7e685314f685ae8711d1734952d2544`

The included Radify/Radial Menu archives and screenshots under `docs/references` are references only. Existing historical plans remain evidence of earlier decisions; this approved contract supersedes any contradictory radial/grid interaction requirements in those plans.

## 1. Goal and priority

Restore fast, predictable launcher-hotkey toggling without sacrificing separate radial hold behavior. Make radial cells a first-class launcher entry point through readable targets, pinned actions, live saved queries, optional Auto Submit, and advanced exact commands. Improve the existing Designer and skin workflows without replacing the subsystem.

Priority order is: hotkey reliability; action/query correctness and authoring; bulk/navigation usability; skins. Separate acceptance gates must prevent appearance work from concealing an unresolved hotkey failure.

## 2. Hotkey contract

| ID | Requirement |
|---|---|
| H-01 | Use the configured `Shift+Alt+Win+End` chord, mapped to one key; release all keys between gestures. Preserve support for other valid configured chords. |
| H-02 | Preserve the user's configured hold threshold. Use 350 ms only when a setting is absent; retain the existing settings validator rather than silently overwriting an invalid explicit value. |
| H-03 | A completed short tap toggles the main grid on release. Do not wait out the remainder of the hold threshold or show the grid speculatively on key-down. |
| H-04 | Focused main grid must hide on a short tap without an outside click; hidden grid must show on the next tap. |
| H-05 | Every admitted gesture has one decision. Repeats, duplicate releases, stale deadlines, and multiple input backends must not create extra toggles. |
| H-06 | A short tap while the runtime radial is open toggles the grid AND dismisses the runtime radial. This deliberately replaces previous keep-radial-open behavior. |
| H-07 | A new hold opens a closed runtime radial or closes an open one without changing the grid's current visibility. Its release never also toggles the grid. |
| H-08 | Preserve existing sticky, release-to-select, and hold-click activation policies. Opening a release-to-select menu does not newly make it sticky. A hotkey-dismiss gesture must not dispatch a hovered item. |
| H-09 | With the Designer focused, a short tap toggles only the main grid; the Designer, its identity, and its unsaved draft survive. Runtime radial, native authoring preview, and Designer are separate owners. |
| H-10 | Repeated gestures require neither pointer movement nor artificially refocusing an external window. Earlier restore/focus/placement work may not undo a later visibility decision. |
| H-11 | Preserve emergency/exclusive-capture priority, direct radial triggers, shutdown/lock/suspend cancellation, main-window placement, and unrelated launcher behavior. A gesture legitimately suppressed by an exclusive owner is not an admitted launcher tap. |

| Grid before | Runtime radial before | Gesture | Grid after | Runtime radial after |
|---|---|---|---|---|
| Hidden | Closed | Short tap | Visible | Closed |
| Visible, including focused | Closed | Short tap | Hidden | Closed |
| Hidden | Open | Short tap | Visible | Closed |
| Visible | Open | Short tap | Hidden | Closed |
| Either | Closed | Hold threshold reached | Unchanged | Open, subject to existing interaction policy |
| Either | Open | New hold threshold reached | Unchanged | Closed |

If a radial open is still preparing, a newer short tap cancels that presentation as well; its late reply must not reopen the menu. Dismissal uses the normal lifecycle, not a fabricated selection. Already committed external side effects cannot be undone; late uncommitted work needs explicit ownership/cancellation handling rather than duplicate dispatch.

## 3. Action and query contract

| ID | Requirement |
|---|---|
| Q-01 | A launcher-style query field is the primary authoring experience. Query results must use the main launcher's matching/ranking behavior. |
| Q-02 | Distinguish `Pin this result/action` from `Save query` in both the UI and persisted model. Do not store a transient row number as identity. |
| Q-03 | A new saved-query cell defaults to Auto Submit OFF. Invocation opens the grid with the query and focuses it for interaction. |
| Q-04 | Auto Submit ON resolves the query at invocation/activation time and activates the current first result through the existing primary-action behavior and safeguards. Do not freeze the first result when the cell is saved or when the menu is opened. |
| Q-05 | Use the same ranking and input query as the main grid. Do not silently skip an unavailable first result to execute a different lower-ranked result. With no executable first result, open the grid with the query and a clear explanation. |
| Q-06 | Pinned actions retain supported target identity and action identity. A missing/unavailable target is visibly unavailable and never falls back to another result. Existing persistent identities, such as note slugs, retain their current semantics; no global identity rewrite is authorized. |
| Q-07 | An Advanced exact-command field uses the existing parser and dispatcher; it is not a second search engine or a shell-language reinterpretation of query text. Preserve structured arguments separately when relevant. |
| Q-08 | Show target title, action, type, and necessary disambiguator directly in chooser rows. Tooltips are supplemental. Identical action labels and identical target titles must still be distinguishable. |
| Q-09 | Searching, assigning, previewing, selecting a skin, and the `would execute` preview never dispatch actions, alter usage/history as execution, or open external programs. Explicit Test is separate and retains existing safeguards. |
| Q-10 | Noninteractive actions do not open or flash the main grid. UI-required actions, confirmations, note editing, dialogs, and manual queries receive the UI they need. Preserve an already-visible grid unless the action explicitly changes launcher visibility. |
| Q-11 | Slow, unavailable, stale, cancelled, or recursive query work cannot cause double execution, a surprise late UI reopen, or action retargeting after selection. |
| Q-12 | Reuse Universal Actions, command parsing/dispatch, query search/ranking, history attribution, runtime dispatch identities, and interaction handoff boundaries. |

A `DynamicSource::LauncherQuery` is still a dynamic menu that yields several cells; it is not replaced by the single-cell saved-query feature. Contextual actions and alternate click bindings remain supported.

## 4. Approved Designer improvements

| ID | Requirement |
|---|---|
| D-01 | Share the action/query editor between Cell Properties and Inspector. Both must use identical search, result labels, assignment validation, and summaries. |
| D-02 | Add `Add to radial menu` from main-grid result/context-menu interaction. Choose target action, destination menu/ring/cell, and commit through authoring; never execute the source result as a side effect. |
| D-03 | Add read-only query preview with `would execute` and `Pin result` choices. Pin only persistable targets; expose contextual/query alternatives for ephemeral results. |
| D-04 | Add multi-select and bulk label/style/after-action editing using existing mutation/history mechanisms. Preserve copying, duplication, ordering, and undo/redo. |
| D-05 | Improve menu/cell search and consistent breadcrumb/back navigation. Keep stable identity through rename, reorder, reload, and history changes. |
| D-06 | Preserve save/apply/cancel/discard/conflict and Designer/native-preview lifecycle semantics, including dirty drafts. Avoid expensive authoring work while the Designer is closed. |

## 5. Approved skin scope

| ID | Requirement |
|---|---|
| S-01 | Modern clean default plus optional classic Radify/RM4-inspired skins. Do not replace existing users' chosen skins on load. |
| S-02 | Thumbnail skin gallery with a small curated preset collection. Use existing rendering/style machinery, not a second renderer. |
| S-03 | Simple accent, scale, spacing, opacity, and label controls; retain advanced inheritance and overrides and show their effect/provenance. |
| S-04 | Compact, comfortable, and high-contrast presets. Warn about dense/unreadable layouts rather than forcing tiny targets or deleting cells. Preserve existing hard geometry/validation limits. |
| S-05 | Preserve package/asset safety and compatibility. Classic-inspired assets must be original or demonstrably licensed; reference archives are not blanket permission to redistribute assets. |

## 6. Explicit exclusions

Conditional cells/status badges, action chains, extensive animation packs, a new radial framework, a GUI framework/dependency upgrade, a second hotkey/search engine, a global persistent-target redesign, a build-system overhaul, and unrelated plugin changes are outside this milestone.

## 7. Verification and workflow

Implement coherent batches and author tests with the code. Use narrow test targets at milestone boundaries; do not rebuild every target or run the full suite after each small edit. Final Cargo Nextest and native Windows acceptance are mandatory for completion. Update tests whose assumptions are intentionally superseded; do not delete legitimate regression coverage.

Use one writer. Preserve existing `.codex` role configuration and `AGENTS.md`. An execution ledger records code status separately from validation, candidate/source identity, commands, test counts, exit codes, artifacts, review findings, and blockers. All rows start pending; none of the planning documents assert that application tests passed.

For genuinely long-running remote jobs, use completion notification when available or observe at approximately 10–20 minute intervals. This is orchestration cadence only, not an application delay, native test polling interval, or timeout. Never start duplicate jobs or treat a quiet log as a failure. Native blocked/unsupported cases are not passing cases.
