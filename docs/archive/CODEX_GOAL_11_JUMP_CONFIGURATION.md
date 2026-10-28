# Codex Goal 11 — Configure Jump Mode activation and ergonomic hint letters

**Companion execution prompt:** [CODEX_KICKSTART_11_JUMP_CONFIGURATION.md](CODEX_KICKSTART_11_JUMP_CONFIGURATION.md)  
**Requested coverage:** #22  
**Program position:** 11 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/jump-configuration` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish jump-configuration branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
6. **Attempt the initial upstream push once.** After the baseline commit exists, perform one normal non-force push to this task branch when an appropriate remote and platform approvals permit, for example `git push -u origin HEAD` when `origin` is verified as the intended repository. If denied/unavailable, record the real reason and keep working locally. Do not retry in a loop, bypass protections, or require the user to resolve ordinary push failure.
7. **Verify the bootstrap and remain on this branch.** Record branch, original base, baseline SHA, pre-existing work and initial push result in the execution ledger. A ledger created by the baseline commit can record its resulting SHA immediately afterward and include that update in the next legitimate checkpoint; do not create recursive commits merely to record their own hashes. Do not silently return to the prior branch.

### M0-A acceptance and commit contract

**Objective:** A unique active task branch with its own committed baseline and an honest initial push result.

**Code ownership:** Git state and an optional task execution ledger only; no application code or test execution yet.

**Tasks:** Complete steps 1–7 above, check the dependency contract below read-only, and keep existing user work untouched.

**Behavior after this checkpoint:** All later feature work and verification happen on the new branch and are measured against the new baseline commit.

**Edge cases:** Missing remote/upstream, denied fetch/push, existing branch name, dirty checkout, stale local integration tip, missing prerequisite or unsafe transition.

**Regression constraints:** No user change staged/lost, no destructive Git command, no force push/history rewrite, and no implementation on `master`, `main` or the old feature branch.

**Verification:** Inspect branch/HEAD, baseline commit, worktree/staged diff, base provenance and actual push output. No application build is needed to prove bootstrap.

**Acceptance:** Unique branch is active; baseline commit exists on it; original base and baseline SHA are recorded; push was attempted or its policy/access unavailability is recorded; unrelated changes are preserved. If branch/baseline cannot be established safely, stop with a precise blocker. Push failure alone is nonblocking.

**Commit boundary:** The first commit is the M0-A baseline. Every later checkpoint stays on this new branch; normal best-effort push follows each commit.


## A. Goal and desired outcome

The user can choose one Jump Mode activation chord and an ordered hint-letter set in Settings, then use them throughout existing Jump-enabled launcher surfaces. Old settings continue to use Shift+Space and A–Z, and navigation retains its existing focus, ownership and execution behavior.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [05 — Keep Plugin Settings usable with long descriptions](CODEX_GOAL_05_PLUGIN_SETTINGS_SCROLL.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/gui/jump_mode.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/jump_mode.rs)** — `HINT_ALPHABET; Hint::new; HintMap::assign; allocate_hints; HintSession::push_char`. The alphabet is fixed to A–Z. HintMap sorts targets deterministically and freezes a prefix-free hint assignment; the allocator's lexical ordering and Hint assertions embed the current alphabet. Input is already ASCII case-insensitive.
- **[src/gui/jump_mode/ui.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/jump_mode/ui.rs)** — `JumpModeUi; process_root_input_events; toggle_session; is_leader_key`. The app-owned controller hardcodes Shift+Space. It already pairs key/text events, suppresses held repeats, tracks consumed key releases, fences invalidated input and respects navigation ownership. This is a local egui input path, not a global hotkey registration.
- **[src/settings/model.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/settings/model.rs)** — `Settings::experimental_jump_mode_enabled; default and round-trip tests`. Only the enabled flag is persisted for Jump; legacy settings and the default leave it disabled.
- **[src/settings_editor/state.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/settings_editor/state.rs)** — `SettingsEditor fields and validate_before_save`. The editor has explicit fields, validation and committed baseline handling; new preferences must join this path rather than bypass settings transactions.
- **[src/settings_editor/mapping.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/settings_editor/mapping.rs)** — `SettingsEditor::from_settings and to_settings`. The enabled flag is explicitly copied both ways and has a round-trip regression test.
- **[src/settings_editor/save.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/settings_editor/save.rs)** — `apply_saved_settings`. Committed settings are applied to LauncherApp; the Jump enabled-state change is applied here.
- **[src/settings_editor/catalog.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/settings_editor/catalog.rs)** — `control::LAUNCHER_JUMP_MODE`. Settings Search already indexes the Jump enabled control, giving the new controls an established navigation pattern.
- **[src/hotkey/parse.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotkey/parse.rs)** — `parse_hotkey and Hotkey`. A shared chord parser exists, but runtime key representability and strict single-main-key validation still need to be enforced for this local feature.
- **[src/hotkey/directory.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotkey/directory.rs)** — `RouteKind::Local; Scope::LocalSession; MainSetting; Candidate and applied receipts`. The directory supports local routes and separates configured candidates from exact applied runtime receipts. Jump is not currently a MainSetting entry.
- **[src/main.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/main.rs)** — `hotkey_directory_source_snapshots; publish_hotkey_directory_if_changed`. Main-settings directory candidates and receipt publication are assembled here; new local Jump reporting must be integrated without altering existing global route registration.
- **[src/gui/render.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/render.rs)** — `dispatch_hotkey_directory_edit`. MainSetting edit destinations route to Settings Search controls. A new Jump destination must use the same current-identity checks.

## C. Approved functional requirements

1. Retain experimental disabled-by-default behavior. Missing new fields load as Shift+Space and ABCDEFGHIJKLMNOPQRSTUVWXYZ. Preserve existing custom configuration and all unrelated settings.
2. Offer one configurable activation chord, represented as exactly one supported non-modifier key plus modifiers, using canonical existing spelling. Reject malformed/multiple-main-key entries and keys/modifier combinations that the local event path cannot distinguish. Do not silently accept an unsupported rdev key or collapse AltGr into a different chord.
3. Prevent plain printable typing keys from becoming the launcher-wide leader without modifiers. Preserve dedicated Escape cancellation, Enter execution and Backspace prefix editing; explain invalid/reserved conflicts inline. Do not silently change an existing launcher, macro or other shortcut to accommodate Jump.
4. Allow an ordered set of 2–26 distinct ASCII letters; display uppercase, accept lowercase, preserve the user's order for ergonomic priority and reject duplicates/nonletters with a useful message. Trim harmless surrounding whitespace, but do not silently reorder the set.
5. Keep compact deterministic prefix-free assignments, stable during an active session. The unchanged default alphabet must preserve the current assignment order for the same target snapshot.
6. Activation remains local to an eligible launcher-owned surface. The configured chord starts Jump or cancels the active session on a fresh press; auto-repeat must not toggle repeatedly. Other applications and focused native child controls keep their existing ownership.
7. Apply configuration only after successful settings commit. A real chord/alphabet change retires the current hint session and safely fences already captured keys until release; stale prefixes cannot target a new map.
8. Expose both preferences in the normal settings UI and Settings Search. Report the chord in Hotkey Directory as a local route with an edit link and truthful enabled/applied status; it is not an OS-registered global binding.
9. Use the configured chord in relevant help/status text. Keep full-hint focus/select, separate Enter activation, text preservation, popup handling and existing surface coverage unchanged.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Global Jump activation, new keyboard hooks, multi-chord sequences, non-ASCII alphabets, automatic keyboard-layout detection, expanded Jump target coverage or badge styling controls.
- A general shortcut-remapping redesign or changing the semantics of unrelated directory candidates, settings or input ownership.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Add a small typed validated Jump configuration at the existing domain/controller boundary. New persisted fields are backward-compatible additions alongside the existing enabled flag; field/type names are implementation choices, not existing APIs.
2. Parameterize the current allocator and Hint validation with the validated alphabet. Preserve target ordering and use alphabet rank rather than incidental Unicode/lexical order when honoring the custom priority. Handle a two-letter alphabet without fixed-26 assumptions.
3. Replace is_leader_key's fixed predicate through the controller's committed configuration. Reuse paired-event capture, repeat suppression and ownership fences; do not add a competing input observer.
4. Integrate SettingsEditor state/mapping/validation/render/catalog, apply_saved_settings and LauncherApp controller application. Inspect existing settings-save merge/conflict behavior so concurrent unrelated updates survive.
5. Extend the existing directory candidate/edit/publication path with a local Jump entry. An applied local configuration receipt must refer to its actual current revision; never manufacture a successful global registration.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Model validated configuration and parameterize hint assignment

**Objective:** Custom chords and ordered alphabets have one validated representation while default navigation stays identical.

**Code ownership:**

- src/gui/jump_mode.rs
- src/settings/model.rs
- src/hotkey/parse.rs — reuse; change only if a shared validation defect requires it

**Implementation tasks:**

1. Define the backward-compatible persisted preference fields and a validated runtime representation with defaults. Specify invalid-file recovery per new field using established loading diagnostics; keep unrelated settings loadable.
2. Validate exactly one supported main key, representable modifiers and reserved typing/navigation constraints. Keep local chord conversion separate from global registration.
3. Parameterize HintMap allocation/validation and use custom alphabet rank for deterministic compact hints. Preserve the default map byte-for-byte for representative existing target sets.
4. Add focused pure tests for legacy/default settings, settings round trips, invalid configurations, 2/8/26-letter sets, uniqueness, prefix freedom, custom order and default compatibility.

**Behavioral requirements:**

- Valid settings can express the selected chord/alphabet without changing existing defaults.
- Pure allocation supports small ergonomic alphabets and many targets without collisions or nontermination.

**Edge cases:**

- Empty/one-letter/duplicate/non-ASCII alphabets; lowercase; invalid chord tokens; multiple main keys; unsupported modifier distinctions; zero and large target counts.

**Regression constraints:**

- Legacy enabled flag remains authoritative and defaults off.
- Target identity/sort order and default A–Z assignments remain stable.

**Verification:**

- Run the selected Jump-domain and settings serialization tests; include the existing compactness/prefix-free tests with the default configuration.
- Use deterministic generated target sets rather than full application startup for pure properties.

**Acceptance criteria:**

- All specified invalid configurations fail usefully and valid configurations round-trip.
- Default assignments match previous expected behavior; custom order and two-letter maps are deterministic and prefix-free.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(jump): [M1-A] validate preferences and parameterize hint allocation` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Apply configured input through the existing session controller

**Objective:** The real input path uses committed preferences without leaking text or disturbing focus ownership.

**Code ownership:**

- src/gui/jump_mode/ui.rs
- src/gui/mod.rs
- src/settings_editor/save.rs

**Implementation tasks:**

1. Pass validated settings into JumpModeUi during construction and committed reload, replacing the fixed leader predicate.
2. Keep chord recognition before general shortcut filtering so supported modifier chords work, while preserving ownership checks and consumed key/text pairing.
3. Retire active sessions on actual configuration changes and retain release fences for captured keys. Ensure a stale pending focus intent cannot execute after reconfiguration.
4. Adapt focused controller tests for the original and an alternate chord, same-chord cancellation, repeated presses, paired text, text-only input, native-owner rejection and key-up after settings changes.

**Behavioral requirements:**

- The new chord works on existing supported surfaces and the old chord becomes ordinary input when no longer assigned.
- Active session/configuration transitions are deterministic and do not type hint or leader text into editors.

**Edge cases:**

- Changing settings while a hint prefix is active or a key is held; settings save failure; lost foreground/owner; no targets; a chord paired with text; control/alt shortcuts during a session.

**Regression constraints:**

- Full hint only focuses/selects; Enter remains a separate execution step.
- Foreign/native editor ownership, root focus recovery, popup handling and existing disabled behavior are preserved.

**Verification:**

- Run the exact controller tests for leader/toggle/repeat/capture and directly affected owner-fence cases.
- Use a bounded event sequence to verify no duplicate toggle, stale activation or text leak.

**Acceptance criteria:**

- Default and alternate chords pass activation/cancellation and text-preservation tests.
- Failed/pending saves leave active committed preferences unchanged; successful changes invalidate stale sessions safely.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(jump): [M1-B] apply configured leader through session input ownership` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Expose preferences and finish settings/directory integration

**Objective:** Users can discover, edit, persist and verify the new configuration through existing UI paths.

**Code ownership:**

- src/settings_editor/state.rs
- src/settings_editor/mapping.rs
- src/settings_editor/render.rs
- src/settings_editor/catalog.rs
- src/settings_editor/save.rs
- src/hotkey/directory.rs
- src/main.rs — directory publication only
- src/gui/render.rs — directory edit dispatch
- src/gui/hotkey_directory_dialog.rs — only if the existing generic display needs adaptation
- README.md and current Jump help text

**Implementation tasks:**

1. Add editable chord/alphabet fields, inline validation, defaults/reset affordances consistent with nearby controls, and Settings Search metadata/control observation.
2. Extend settings merge/mapping/application tests so editing Jump preserves unrelated and concurrent settings. Show committed configuration after reopening.
3. Publish one local Jump directory candidate with exact applied configuration status and a current edit target. Update exhaustive MainSetting handling and test stale revision rejection.
4. Replace user-facing hardcoded leader instructions where they describe active configuration; retain documentation of the default.
5. Build once and run a disposable native session: change to an alternate chord and a small ordered alphabet, use root and one existing dialog/text field, cancel, save/reopen and return to defaults. Capture settings plus custom hints.

**Behavioral requirements:**

- Preferences are discoverable through settings search and directory editing; directory labeling distinguishes local scope.
- Saved configuration survives restart and default reset reproduces current behavior.

**Edge cases:**

- Invalid field while other settings are edited; save-pending/failed save; disabled Jump with stored preferences; stale directory edit request; narrow settings layout.

**Regression constraints:**

- No global hotkey re-registration or conflict-policy change for unrelated features.
- Settings Search observes actual controls and uses current identities; existing keyboard navigation remains usable.

**Verification:**

- Run focused settings mapping/catalog/navigation and local directory candidate/edit tests affected by the change.
- Perform one native smoke covering default/custom/reset, a text field and one existing dialog; verify custom letter order and exported screenshots visually.

**Acceptance criteria:**

- Both fields persist and are searchable/editable from established paths.
- Local directory status is based on current applied configuration, and native evidence verifies alternate chord/letters without text leakage.
- Updated help and final report accurately state supported chord/key limits.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(jump): [M2-A] expose preferences in settings and hotkey directory` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Compile the necessary application/test targets once per coherent code increment, reusing the selected profile; log compilation separately and allow its measured normal duration.
- After compilation, allow up to 10 minutes for the combined targeted domain/controller/settings/directory selection and up to 10 minutes for one native smoke session. Re-run only failed/touched cases, with at most two focused remediation attempts.
- Use isolated settings/data and disable Paste Sequence next/cancel shortcuts before application fixtures. Pure allocator tests must not construct LauncherApp. No full-suite or expanded surface acceptance campaign.
- Search touched production/help paths for remaining hardcoded activation text, distinguishing intentional default examples from stale active instructions. Final first-party lint work follows the package's end-only policy.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Custom letter order changes allocator behavior or a one-symbol alphabet makes expansion impossible. | Require at least two distinct ASCII symbols and test prefix freedom, deterministic rank ordering and exact default compatibility. |
| Recognizing modifier chords outside the existing owner gate steals shortcuts or inserts text. | Modify the current controller only, retain event pairing/release fences, and test a text field and foreign/native ownership. |
| Directory status reports configured intent as applied runtime behavior. | Use local configuration receipts/revisions through the established candidate/publication model; do not represent a global OS registration. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Configured chord and ordered letters work on existing Jump-enabled surfaces and survive settings reload.
- Defaults and all stated focus/input invariants remain verified.
- Settings Search, Hotkey Directory and relevant help display the actual configuration.
- Focused pure, controller, settings and native checks are recorded without implying unexecuted coverage.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- List final preference keys, defaults, supported chord constraints and invalid-config recovery.
- Provide default/custom allocator results, text/focus smoke evidence and settings/directory screenshots.
- Report actual tests, remaining platform limits, milestone commits and push outcomes.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
