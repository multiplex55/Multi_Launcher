# Codex Goal — Post-Queue Native Acceptance & Stabilization

**Companion:** `CODEX_KICKSTART_POST_QUEUE_NATIVE_ACCEPTANCE.md`  
**Mandatory final user-facing artifact:** `docs/plans/POST_QUEUE_MANUAL_VERIFICATION.md`  
**Nature of work:** Bounded acceptance and defect remediation, **not another feature implementation goal**.  
**Reference final G3 SHA:** `7e0f74f117975024b25e7c7cbadef32f29937965`  
**Preferred branch:** `qa/post-queue-native-acceptance`  
**Expected source:** The cumulative `feat/productivity-tools` tip at the reference SHA; it already contains Goals 1, 2 and 3.

## A. Mission and non-negotiable result

Verify the *specific* Windows-native and parallel-suite behaviors that were not accepted during G1–G3. Where genuine defects are **observed and causally established**, implement the smallest safe Rust/egui/Win32 or test-fixture correction and verify that correction against **a freshly built binary**. Preserve all existing features, saved data, privacy rules, active owners, hotkeys, and speed. At the end, deliver a truthful evidence report and a **practical Markdown checklist for the user's single hands-on review**.

The mission is complete as an *execution effort* when every listed area has a final classified disposition and all permitted test budgets, code review, lint, documentation, Git, and handoff gates are closed. That is **not** the same as claiming every scenario passed. If mandatory native evidence remains unavailable, the terminal label must be `COMPLETE_WITH_NATIVE_GAPS` or `BLOCKED_NATIVE_ACCEPTANCE`, with exact remaining manual actions. Do not silently mark unproven scenarios passed.

**Already delivered; do not reimplement:** Launcher autofocus/Notes Jump ownership; searchable Settings, guarded saved-target repair, clipboard-line operations, Paste Sequence source pickers, numbered screenshot markers, delayed capture, `imgdiff`, per-pin opacity and Reset 100, note literal replacement, `table`/`csv`, and the read-only `dff`. MkMacro typed forms and explicit clipboard steps, Hotkey Directory, and per-monitor brightness belong to existing G2. No workspaces, new utility, command grammar, persistent setting, or broad architectural rewrite.

### A1. Source/evidence authority

1. At kickoff read `AGENTS.md`, applicable nested guidance, `.codex/config.toml` and configured agent runners, this Goal, its Kickstart, and the *current* code on the verified base branch. Code at the real base is the source of truth for ownership and exact API names.
2. Read—not modify—the G2/G3 final execution records and handoff receipts, prioritizing `docs/plans/capture_productivity_expansion_execution.md`, `docs/plans/productivity_tools_execution.md`, `docs/plans/CODEX_MULTI_GOAL_QUEUE_LEDGER.md`, `target/multi-goal-queue/g3/m12/final-handoff.json`, and native evidence referenced therein if present. Evidence paths may be on local ignored `target/` and not pushed; missing evidence must be reported, not invented.
3. Respect the terminal history: **G1 complete**; **G2 complete with limitations** (final full Nextest 6,793 pass, 1 XML fail, 1 query-oracle timeout, 48 skipped; both separately passed); **G3 all 11 feature areas implemented/pushed, final focused tests/default & dev+QA all-targets Clippy passed, but native acceptance blocked**. These facts are baseline, not permission to assert current tests are green.
4. Actual prior G3 native gaps: Settings numeric/Cancel/Jump-off; exact repair file/folder pickers; pin click-through input; Note Replace/Undo/Save/Discard/Jump-off; DFF Cancel/drain/restart/actions/retirement. Inherited G2: shield input/isolation, OS live-source variants and real physical-monitor hardware evidence. G3 verified native `imgdiff` wipe/difference/retirement, delayed capture Window and Region Cancel, and static/generated-live opacity composition/export/Reset; do not redo entire successful programs.

## B0. Mandatory safe Git bootstrap — before ANY app/build/test modifications

**B0-A — Verify predecessor and isolate work**

- Inspect current branch, `HEAD`, working-tree/index/untracked status, upstream, remote, worktrees, and any already running cargo/Codex/native fixture. Prefer `git fetch origin` if authorized. Never pull/merge/rebase/master-reset to get a convenient base.
- Verify the tip of `feat/productivity-tools` (local and/or intended `origin/feat/productivity-tools`) is exactly `7e0f74f117975024b25e7c7cbadef32f29937965`, or independently verify a direct, user-intended newer descendant and record a precise discrepancy **before** proceeding. The reference SHA is the required safe starting point for this contract. Do not silently discard a later edit, retreat to `master`, use an old `jump` planning SHA, or rewrite G1–G3 history.
- Preserve **all** pre-existing tracked edits, index entries, generated outputs and unrelated untracked files. Previous handoff explicitly lists five unrelated/unknown untracked files, including `clipboard_modifiers.json`. **Do not read, delete, stage, overwrite, or assume ownership of that file.** If a clean switch is unsafe, create an isolated Git worktree at the verified SHA; no auto-stash/reset/clean.
- Create `qa/post-queue-native-acceptance` from that exact cumulative SHA, or a unique suffixed name if occupied; one new branch for the whole Goal. Record `base_sha`, `branch`, original checkout, worktree, remote, and whether the upstream SHA matches. Do not modify master/main or existing Goal branches.
- Create a legitimate `[B0-A]` baseline commit containing only `docs/plans/post_queue_native_acceptance_execution.md` (or an explicitly index-safe empty baseline). Record resulting `baseline_sha` in the **next** ledger update. Attempt one normal non-force upstream push (`git push -u origin HEAD`) when authorized. If denied, locally commit and continue. Never bypass approvals or force-push.
- **Acceptance:** Correct branch/base/baseline; no unrelated staged changes; safe worktree; best-effort initial push logged; only then release M1.

**B0-B — Freeze the baseline and establish native safety rules**

- Capture the exact compiler/tool facts (Windows edition/build, rustc, nextest, Cargo feature names, QA runner syntax) and verify available source/binary/fixture manifests without mutating user data.
- Inspect existing narrow native runners **before** invoking them: `dev/run_owned_process.ps1`, `dev/jump_mode/run.ps1`, `passive_overlay_smoke`, `pin_opacity_smoke`, `image_compare_smoke`, and the existing DFF/NoteReplace/TargetRepair scenario paths actually present. Do **not** build a new universal driver. If a named runner is absent, use the present closest owner or record `NOT_RUN_HARNESS_MISSING`; do not fabricate flags/APIs.
- Create a small manifest per owned scenario: `base/source SHA or diff hash`, `built executable SHA`, fixture content hashes, target PID + birth time + HWND + owner/nonce, allowed region/expected result, guard conditions, test command, timeout, and exact evidence directory. Bind all native receipts to the current compiled source. An old binary can support historic findings but **never** verification of a subsequently corrected implementation.
- Define the statuses in Section D before tests begin; keep status and elapsed/deadline evidence even when a phase stops before input.

## C. Non-negotiable safety, privacy and compatibility invariants

1. **Owned native interactions only.** Use synthetic temporary profiles, own generated Win32 windows/controls, own temp files, and known test canvases. No screenshots of private desktop content, broad `EnumWindows` input, UI automation in unrelated applications, or OS clipboard contents. Safe sampling checks all overlapping visible HWNDs, process/root ownership, target PID+birth, monitor/desktop coverage and title/lifetime *immediately before* sampling.
2. **Input injection fails closed.** Before mouse/key input, prove exact current foreground, root/topmost owner, HWND/PID/birth/lifetime, current rendered and clipped `egui::Response` (not stale geometry), hit-test target, absence of foreign occlusion, and idle/held modifier conditions. If unavailable, send **zero input**, report the exact failing precondition and leave both owned windows shut down. Never relax production/native ownership to get a PASS.
3. **No real brightness Set.** Real monitor enumeration is read-only and optional. Never issue SetMonitorBrightness, global `brightness:set`, change the user's displays or infer support on hardware not present. A real brightness-change check belongs on the user's manual list with explicit consent and rollback instructions.
4. **No real clipboard writes/reads of private content.** Use injected fake clipboard or synthetic, opt-in ephemeral isolated source. Never update or remove `clipboard_modifiers.json`. No personal notes/bookmarks/repositories; use temp Note storage, temp action/folder stores, fake clipboard, and disposable DFF trees.
5. **No destructive DFF behavior.** DFF remains read-only: never delete, move, quarantine, rename, create hardlinks, or auto-clean scanned user files. `Open`/`Reveal`/`Copy` must validate exact current aliases and may be exercised through owned/fake action adapters; no arbitrary external process launch.
6. **State owners remain authoritative.** Preserve Settings open drafts and no-save navigation; Notes live unsaved source and the **same** egui undoer; target-repair journaling/usage/reference safety; Pinboard binding ownership, compositor/native receipts and original RGBA; DFF identity + byte verification + bounded worker; controlled input and Jump focus-only hints. No new polling/service/hook, alternate registry, second worker, global focus workaround, raw handle cache, vendor patch, or schema migration.
7. **No feature regressions.** Retain G1 query autofocus, Note/Jump/Escape boundaries, all previous launcher commands including `diff` versus `imgdiff`, `table` versus `csv` defaults, Paste Sequence Ctrl+V policy, macro execution and privacy, Hotkey Directory, radial/editor behavior, mouse effects, and screenshot prompt/delay/cancel flows.
8. **UI responsiveness.** Do not add per-frame scanning, blocking waits, unbounded image or clipboard allocations, forced focus, repaint loops, hidden background CPU work or synchronous long worker joins on an egui frame. Keep default fast-dev profile unchanged and ensure shutdown/Drop does not leak workers.
9. **Evidence truthful, not cosmetic.** A returned Win32 success, fabricated state, fake-only unit test, root framebuffer, style bit, post-run stale `Response`, offline predicate correction or screenshot without validated ownership is NOT native end-to-end proof. Distinguish real effects from synthetic fake/regression support.

## D. Evidence taxonomy, attempt budgets and checkpoint gates

### D1. Required final scenario statuses

| Status | Meaning | Handoff treatment |
|---|---|---|
| `PASS_NATIVE` | Current-source owned executable demonstrated **all listed** native effects; guards and shutdown proved | Mark individual requirements passed, attach receipt/artefact |
| `PASS_AUTOMATED_ONLY` | Focused deterministic tests pass; native path unattempted or incomplete | Native still unverified; add concrete manual procedure |
| `PRODUCT_DEFECT` | Reproduced failure in product source/ownership; not a fixture/guard artifact | Narrow fix and one corrected-native proof; if not fixed, block sign-off |
| `HARNESS_DEFECT` | Demonstrated driver/readiness/manifest/sampling error with no product conclusion | One bounded correction, then new build/verified fixture or explicit gap |
| `ENVIRONMENT_BLOCKED` | Exact prerequisite (e.g. foreground or unsupported DDC host) genuinely unavailable | Stop before input/data; record observed environment and manual step |
| `INCOMPLETE_BUDGET` | Deadline/exhausted attempt cap; cause or later phases unverified | Stop, preserve evidence and manual step; never call PASS |
| `NOT_RUN` | Excluded, absent hardware, missing authorized fixture, or no safely created profile | Explain why; manual step or separate authorization |

`COMPLETE_WITH_NATIVE_GAPS` means completed the **bounded engineering effort**, not that the original G3 `BLOCKED_NATIVE_ACCEPTANCE` magically turned green.

### D2. Fixed attempt envelope (hard ceilings, not targets)

- **At most one primary current-source native run per independently checkable scenario**, plus **one recheck** only after a concrete product defect or a newly demonstrated, narrowly fixed harness precondition. No indefinite native retry campaign. Separate scenarios can have separate sessions; don't rerun an already-proven effect just to reach an independent later phase. Record any aborted prelaunch run separately, not as a test PASS.
- Scenario wall-clock: normally **90 seconds native + up to 10 seconds owned cleanup**; use up to **180 seconds + 10 cleanup** only when the existing scenario requires bounded two-app/startup or multiple fixed editor phases, with justification recorded *before* launch. Each scenario's combined evidence/build campaign stays within **20 minutes after current binary build**; an initial build of **at most 1,800 seconds** is separately bounded.
- At most **two focused nonzero-listed Nextest groups per checkpoint** plus one exact failed-test recheck after causal correction. `--cargo-profile fast-dev`, `queue-focus`, zero retries, standard stack, 60s/test, 10m group where applicable. If a 60s test times out, don't blindly extend the timeout or set `RUST_MIN_STACK` globally.
- At most **two substantive remediation rounds per scenario**; distinguish a newly exposed causal boundary from the same unchanged unexplained error. Within each round, inspect/review first, adjust one owner, compile, run direct test, then recheck the native scenario once if within the scenario budget. If the budget cannot prove full acceptance, stop and hand off.
- No full unfiltered regression during early milestones. **One final, bounded full Nextest run is permitted** after focused corrections, with **at most one corrected rerun** only if a reproducible new regression has a minimal verified fix. Preserve original full-run findings; zero retries/no excludes/no wholesale ignored tests. If a full run is infeasible under bounded resources, state `NOT_RUN` and retain focused proofs. Do not run a third full suite.
- **Clippy is the last code-verification gate**, after production source, tests, formatting, feature documentation and coherent checkpoint commits. Run the relevant default all-targets and `dev-smoke-binaries,qa-framebuffer-capture` all-targets configurations with `-D warnings`; no blanket `allow` annotations to conceal errors. Fix only first-party diagnostics introduced or affected; commit/push corrections before final clean gate. Do not edit source, tests or formatting after the clean final gates without rerunning the affected gate.

### D3. A native-checkpoint acceptance receipt must include

`scenario_id`, `requirement_id`, branch/HEAD, source/diff + binary SHA, fixture/nonces, tool/command and environment, exact owner/HWND/process/birth/current window layers, pre-input and pre-pixel guards, start/finish wall time, completed phases/expected vs observed results, test results, byte/file preservation when relevant, normal shutdown/verified worker retirement/forced cleanup **separately**, status and unverified phases. Record paths relative to repository or ignored `target/multi-goal-queue/post-queue-acceptance/`; never print personal clipboard content or file contents into summary logs.

## E. Ordered acceptance milestones

The planner must inspect actual current owners before assigning source changes. Each subsection below sets the **required proof**, not an instruction to write code regardless of observed behavior.

### M1 — Shield input isolation and Windows overlay recovery (P0)

**Carryover:** G2 rectangle-selection topmost and strict coverage deterministic verification passed; `passive_overlay_smoke --rectangle-input-isolation` could not obtain the owned receiver foreground **before injecting input**. Therefore real click/keyboard leakage, demotion and recovery remained unverified.

- Reuse the existing strict per-monitor layered shield/probe, input-isolation test receiver and `passive_overlay_smoke` *only after inspecting its current args*. Prepare an owned dummy edit/button beneath the selection shield, include a negative-origin monitor fixture if available (simulation otherwise), and verify exact target foreground/hit result before any injection.
- Prove actual selection prevents the synthetic receiver from observing click/keyboard; Escape and pointer release clean shields/capture state and allow a second attempt; a TOPMOST-only demotion is reasserted at most once by existing owner; unsafe/foreign/missing coverage fails closed without input. Check app/root not left hidden or stuck and no borrowed HWND remains registered.
- If foreground ownership cannot be established, **do not send input**. Record the precondition, no-input proof and `ENVIRONMENT_BLOCKED`; do not patch focus rules solely to force the test. Fake/deterministic strict-rejection guards remain supplementary.
- Focused scopes: inspect current prefixes `rectangle_shield_topmost_recovery_`, `rectangle_guard_native_failure_waits_for_release_and_never_confirms`, `pinboard_repeated_capture_`; use no more than two directly affected selections.
- **Exit:** Owned native isolation/recovery PASS, proven product defect fixed/rechecked, or precisely classified missing native evidence. No screenshot of personal desktop.

### M2 — Pinboard click-through, style ownership, recovery (P0)

**Carryover:** G3 M8-B observed six static/generated-live compositor phases (100/100 → 50/100 → 50/50 → updated live → hide/restore → Reset), full original-RGBA Copy/Save results and successful Reset native input. **Click-through input routing was not exercised** before its last 90s deadline. Generated live frames do not prove OS WGC acquisition.

- Reuse current `pin_opacity_smoke` and actual `PinWindowPresenter`, native child controls and `NativeWindowBinding`. Organize this narrow check to reach the **click-through input** phase *early*, after an owned static-pin baseline, rather than spending the budget redoing six already accepted compositor phases. If all phases share a mandatory setup, cache/reuse validated receipts within the same session, never old executable state.
- On a generated owned canvas/target, validate the *visual* owner and the *input* hit owner separately. Toggle click-through through the actual eligible child UI; verify clicks hit the owned underlying target, no click delivered to the pin, and focus/layering/opacity unchanged except requested property. Switch it off and verify the pin consumes its own input again. Verify Reset 100 is not spuriously triggered by click-through or Escape. Recheck after one hide/restore if affordable.
- For live pin, reuse generated-production-frame evidence for style/visibility and original export, but **do not** claim OS-source WGC capture without an authorized independent external source; classify separately.
- If actual foreground is unavailable, do not synthesize input or call direct SendMessage routing proof. Record `ENVIRONMENT_BLOCKED` or `INCOMPLETE_BUDGET`, exact phase and manual check. Repeated opacity pixel work is not a reason for new libraries or rendering changes.
- Selected tests may reuse exact `pin_controls_use_exact_child_commands_and_reject_invalid_owners`, `pin_opacity_ui_toolbar_and_pin_list_share_validated_state`, `native_hide_ack_and_bounded_worker_preserve_close_and_finish_lifetimes`, and existing live texture/pause tests where actually affected.
- **Exit:** Click-through and recovery proof or explicit native gap; static/live compositing accepted evidence remains intact.

### M3 — Saved-target repair: real file and folder pickers (P0)

**Carryover:** G3 M2-A/M2-B passed 20 + 16 focused cases and current record/action-sheet navigation; native run stopped before actual Repair click because a fresh-observation guard could not prove ownership. The old script read in-place observation JSON and conflated null, stale pass, missing sheet and blocked control; later QA showed a valid current Repair response but a specific cause was not logged.

- Reuse the **existing** exact-record repair transaction, main action sheet, repair dialog, `rfd` picker adapter, `dev/jump_mode/run.ps1` TargetRepair scenario. Fix **only** the owner-scoped observation/driver acquisition if proven; do not weaken provenance, remove the guard or invent a new repair API.
- Prepare a temporary isolated custom application target and a temporary folder entry containing Unicode/spaces plus unrelated saved actions/favorites/pins/usage. Capture all pre-operation source byte hashes and exact target/source references. Show the normal launcher via query, select the exact current saved entry, open Action Sheet, click visible Repair using actual current `Response`, and prove native picker ownership before click.
- Exercise **three distinct outcomes** (separate owned sessions if needed): (a) **File picker Cancel**—zero writes/no changed draft, (b) **File picker confirm**—replacement exists, labels/args/order/usage/ref and unrelated bytes preserved; no execution, (c) **Folder picker confirm**—target directory, alias and references correct. Use temp fixture and check after app restart when practical.
- Require direct evidence of durable transaction acknowledgement or guarded rollback/recovery conflict on failure, not merely a changed text field. A file path that points to a directory must remain valid only under the existing record's documented constraints. No repairing through index/label guess or modifying unrelated user action stores.
- If exact rfd UI cannot safely be driven, classify that picker phase; **never** click a native chooser without validated top-level owner/foreground/control/hit-target and the fixture path. Stop after the fixed cap, leaving manual acceptance steps.
- Focused selectors: existing `target_repair_ui_`, `target_repair_transaction_`, `target_repair_plan_`, and current Missing-plugin guards directly affected by any repair.
- **Exit:** Each of Cancel/file/folder has its own PASS or explicit gap; source integrity verified and temp stores closed.

### M4 — Notes Find/Replace, native undo and persistence (P0)

**Carryover:** G3 M9-A/M9-B passed 10 + 10 tests. A genuine egui undoer handoff issue was identified and fixed. A native session demonstrated exact note opening and live unsaved Unicode marker, but an erroneous closure-scope hash callback prevented reaching Replace/Undo/Save/Discard; Jump-off leg never ran. Correcting the *offline* callback did not retroactively prove the native workflow.

- Reuse `dev/jump_mode/run.ps1` NoteReplace scenario and current `qa-framebuffer-capture` observation/fixture only. Use **two** clean, separately generated temporary-note profiles, Jump ON and Jump OFF, with independently verified loaded Markdown. Preserve literal Unicode, CRLF, `$1`, `\\` and text that is hidden only in the appropriate preview source map (e.g. Markdown link destinations as actually implemented).
- Native sequence per profile: launch the saved note through real query/selection; make an **unsaved edit** and check disk unchanged; Find literal; Replace Current and Replace All against actual current real `Response`/eligible controls; verify expected source and dirty status; select actual note editor and do **one Undo then Redo** through its existing egui history; cross Edit/Split/Preview source-only reveal where possible; **Save** and confirm exact expected disk bytes; make a new unsaved change, **Discard**, confirm saved bytes persist and no phantom query input/Jump action occurred. Handle normal titlebar/Escape using actual owner; never guess fixed screen coordinates.
- Test one sequence with Jump hints focus-only and later fresh activation; verify ordinary mode with Jump disabled does not require any hint registry. If source editor focus is held by Find query or replacement field, a Ctrl+Z must not silently mutate the note; deliberately transfer focus through actual Response before testing note undo.
- Retain exact `NotePanel` dirty/save policy and the **same** `egui::TextEditUndoer`; no second history stack, memory-copy-per-frame, direct-save bypass or source normalization. If the product defect is real, fix the current owner and rerun **the affected native phase** with an updated binary once.
- Focused selectors: current `note_replace_`, `note_replace_panel_`, `note_find_shortcut_and_escape_leave_dirty_note_open_until_the_next_escape`, and G1 launcher/Notes focus owner guards.
- **Exit:** Individual native proof of Replace/Undo/Save/Discard/Jumpoff, or a populated manual checklist with missing phases and all temp-note hashes.

### M5 — Duplicate File Finder (`dff`): native Cancel/drain/restart/actions (P1)

**Carryover:** G3 M11-A/B/C completed the read-only discovery, streamed SHA-256 + byte equality, hardlink identity, validated aliases and UI with **17 + 14 + 11 focused passing tests**. A native session reached Start/read, exposed **footer layout drift**, and stopped before proving Cancel/restart/results/actions/retirement. The footer source was fixed and its deterministic regression passed **after that native run**. Therefore previous screenshots cannot prove the corrected binary.

- Build the new current-source executable and record its SHA. Reuse current DFF native scenario, known synthetic tree and source files; add only a narrow observation if a specific missing current `Response` makes evidence impossible. **Never** treat the earlier binary as acceptance after the footer fix.
- Seed disposable file groups: two distinct identical files, unequal same-size files, an empty file pair, two distinct aliases/hardlinks where Windows supports them, nested duplicate directory names, and one set with Unicode paths. Keep original file hashes before and after. Do not scan personal directories.
- Open ordinary `dff`, exercise **real Start**, wait for actual groups and footer at normal and constrained window size, start a second scan and press **Cancel** while a controllable worker remains active, verify Cancel acknowledgment, drain, no stale results, safe **Restart**, and exact new generation's group membership. Verify worker retirement on close and absence of unowned native processes.
- Test **Open/Reveal/Copy** actions against an exact current validated alias, preferably through owned/fake action adapters when launching File Explorer or touching the clipboard is inappropriate. Verify stale row after file rename/deletion/change is refused. Confirm no Delete/Move/Recycle controls or data mutations and no duplicate double-count of hardlinks.
- If Windows/GUI prevents legitimate Cancel or action interaction, mark exact phase `ENVIRONMENT_BLOCKED`/`HARNESS_DEFECT`/`INCOMPLETE_BUDGET` as supported; do not infer an actual Cancel from a fake unit test.
- Focused selectors: `dff_dialog_`, `dff_lifecycle_`, `dff_content_`, plus current footer-specific regression; never run the entire DFF matrix without a direct need.
- **Exit:** Fresh-binary footer + Start/Cancel/drain/restart/results/actions/retirement classified separately; source hashes unchanged.

### M6 — Settings Search: exact numeric controls, Cancel and Jump-off (P1)

**Carryover:** G3 M1-A/B implemented `settings <query>` with valid current `Response`, actual highlighted slider and one native Right edit (draft 1.0→1.05, committed 1.0), but numeric field, Cancel/unchanged disk and Jump disabled remained incompletely accepted. Original native script awaited a transient visual outline *after* it naturally expired.

- Reuse the current Settings Search native scenario with a **fresh synthetic profile** and a real supported control (e.g. actual `search.fuzzy_weight` and its numeric value editor), discovered by canonical ID rather than display label inference. New workflow should not require an expired outline when proving a still-current focused Response.
- Jump ON: enter settings search through the current typed command, select the exact search result, focus slider through a fresh activation, then deliberately edit its numeric partner; compare **draft** versus committed configuration bytes and actual numeric value. **Cancel** must restore original committed state and leave on-disk bytes unchanged. Repeat with an explicit Save in a disposable second profile if necessary.
- Jump OFF: invoke the same search, use ordinary mouse/keyboard to focus and edit the same current control and test Cancel/Save. Include disabled or unavailable built-in settings search (no auto-enable and no plugin side effects) if it is a small safe extension, not a new catalog milestone.
- Native ownership, exact `Response`, frame, clip and first-key focus filter must still be current; no forced root focus, hidden root editing or general `Jump` permission widening. Use real requested control rather than passive highlighted outline alone.
- Focused selectors: `settings_search_cross_owner_`, `settings_search_entry_`, `settings_search_retained_paint_survives_only_current_root_frame_completion`, and Settings/Notes root-focus guards as affected.
- **Exit:** Jump ON/OFF numeric edit + Cancel persistence classified independently; no actual preferences saved into a user's profile.

### M7 — Parallel Nextest reliability: XML race and query oracle (P1)

**Carryover:** G2 final full: 6,793 passed / 1 XML immediate-`Pending` race failed / 1 Track C query-oracle test timed out at 60s / 48 skipped; both passed serially in 0.022s and 39.258s. That does *not* establish baseline attribution or grouped success.

- Locate **actual test names** from current maintained source and retained G2 receipts. Specifically the XML `m7d_query_statuses_preserve_zero_scalar_and_failure_distinctions` and Track C `track_c2g_oracle_query_sequences_match_usable_cpu_frames` as present. List exact nonzero selectors before running. Use normal stack and zero retry.
- XML: inspect worker/snapshot publication ordering. The test assumed `Pending` must be observed even when the worker may legitimately publish `Nodes` before the first assertion; validate whether the product guarantees a transient Pending state or only eventual valid output. If product is safe and assertion races, correct the **fixture synchronization** to gate a worker and prove transition; do not weaken end-state, source revision, or stale-cancel safety. If product fails a genuine state guarantee, fix that owner instead.
- Query oracle: measure bounded per-phase/selected worker timing and progress under safe synthetic data, with a normal selected-alone run and **one bounded mixed-contention run** before judging. Distinguish CPU-load sensitivity, detached worker, deadlock, serialized expensive work or timeout sensitivity. No global `RUST_MIN_STACK`, long test sleeps, artificially raised 60s cap, `--test-threads=1` full-suite substitute or blanket code refactor. A justified smaller fixture/job proof is preferable to disabling assertions.
- Use a **single final unfiltered full Nextest** after causal focused fixes if feasible, with the exact safe queue-focus zero-retry bounds, same default feature set and normal parallelism; at most one corrected run after a proven new regression and focused passing fix. If the full suite still fails/timeouts, preserve exact counts, list the failing tests and classify `INCOMPLETE_BUDGET` or product defect; do not call the full suite green.
- **Exit:** Clearly distinguish `PASS_FOCUSED` from `PASS_FULL`; zero unbounded tests; precise remaining reliability/manual/CI follow-up.

### M8 — Real brightness capability: read-only evidence (P2; no Set)

- Use current G2 `BrightnessService` / named per-monitor inventory and its fixture/backend tests. If a safe, existing opt-in native inventory path and supported user-session monitor are available, perform **one bounded read-only enumeration only**, capturing monitor number/opaque instance/known support/actual native min-current-max or safe unavailable reason. Redact serials/user paths. **Do not change brightness**, install drivers, force wake displays, test multi-monitor through unrelated hardware, or use global broadcast command.
- If hardware discovery unavailable or support ambiguous, report `NOT_RUN`/`ENVIRONMENT_BLOCKED`, not unsupported on every monitor. Verify that UI does not fabricate 50% or duplicate physical targets under name collision, and stale Set remains refused through focused fake tests.
- Include **optional consent-required manual Set** in the final checklist, with starting value, small adjustment, readback and recovery; manual step can be skipped without declaring product failure.
- Focused selectors: existing `brightness_inventory_`, `brightness_dialog_`, `brightness_service_` **only if touched**; no new background DDC poller/worker architecture.
- **Exit:** One truthful read-only hardware inventory (or gap); hardware Set remains user-authorized manual evidence.

### M9 — Final independent review, evidence closure, manual handoff, lint and Git

- Reconcile each M1–M8 phase with *actual* current-source binary, source hashes, test commands and outcomes; classify every unperformed phase. Independently review all changed production owners for false success, stale-ID/handle reuse, source/draft mutation, input focus, closure/cancellation races, worker retirement, unintended writes and hidden CPU work. Do **not** restart G1–G3 subsystem reviews or refactor merely for style.
- Run **two small cross-owner selected Nextest groups at most** after actual fixes: one native focus/Notes/repair/pin boundary group and one DFF/screenshot/utility lifecycle group. Do not change third-party vendored crates, other features or full-suite baseline tests beyond causally justified fixes.
- Update README/Quick Help only where a **user-visible behavior was actually corrected**; otherwise explain test outcomes in the execution report, not a feature marketing change.
- Produce `docs/plans/POST_QUEUE_MANUAL_VERIFICATION.md` based on the companion manual-checklist starter, with **checkable reproducible steps** prioritizing every still-unproven requirement. Include even `PASS_NATIVE` areas as an optional 2-minute sanity pass only where useful; do not describe completed automation as a reason the user must redo every exhaustive test.
- Create `docs/plans/POST_QUEUE_NATIVE_ACCEPTANCE_RESULTS.md` summarizing scenario-by-scenario pass/defect/harness/environment/not-run; input/no-input, compilation/current binary, worker/process cleanup; full Nextest outcome, default+dev QA Clippy, blocked gates, limitations, branch/base/final SHAs and user review priorities. Keep a compact timestamped execution ledger `docs/plans/post_queue_native_acceptance_execution.md`; do not copy megabytes of ignored raw logs into Git.
- Final formatting and source diff/static review; **only then** the final default and dev+QA all-target Clippy gates. Commit/push any warning fixes first and repeat affected gate. After the last clean gate, only update the final evidence ledger/results/manual checklist with the recorded lint outcome; do not change Rust, tests, dependencies, app/help documentation or formatter output without rerunning affected verification. Do not claim final PASS before recorded results. Preserve all unrelated untracked items and no staged user files.
- Push final task commit normally if allowed; verify branch/HEAD/upstream match, previous branches untouched, master unchanged, no owned process alive, and no private config/source touched. If push denied, record local commit and do not block review. Do not open a PR or merge without explicit user authorization.

## F. Required final manual-verification artifact — non-optional

**Exact repo path:** `docs/plans/POST_QUEUE_MANUAL_VERIFICATION.md`. Use starter document `POST_QUEUE_MANUAL_VERIFICATION_TEMPLATE.md` supplied alongside this Goal, or create this file from the structure below if the starter is missing.

The artifact must **not** be a generic bullet list of features. It must be an actionable end-user test sheet generated at the end **from the actual scenario evidence**:

- Top metadata: branch, source commit, native test environment, date, summary of proven vs unproven scenarios, and the version/build path the user should run. Do not suggest running master.
- A **priority table**: `Check ID`, area, automated status, manual priority, why it needs a human, and exact current evidence (receipt path or `NOT_RUN`).
- Per-check **Markdown checkboxes** (use `[ ]`) including prerequisites/disposable setup, literal steps using only actual discovered UI/commands, **expected visible outcome**, persistence/safety/undo/cancel checks, and what proof to collect if it fails. Explicitly distinguish Jump ON/OFF and static/live only where needed.
- End with a **compact execution log template**: date, Windows build, monitor DPI/config, Launcher commit, result (`PASS`/`FAIL`/`BLOCKED`/`NOT_TESTED`), screenshot/log path, reproduction, whether private data was excluded. A FAIL is a test observation, not an invitation for irreversible cleanup.
- Mark hardware-changing brightness Set as **OPTIONAL / REQUIRES USER CONSENT**, with baseline and restoration; do not imply Codex performed it.
- Keep the five G3 legacy native gaps and inherited shield/hardware gaps visible unless *each* has acquired new actual acceptance evidence. If any remain unproven, the checklist must contain an explicit manual task for them.
- Include short smoke checks for already proven delayed screenshot, imgdiff, CSV and macro/Paste utility only as optional user confidence checks, not as unresolved blockers.
- Group by end-user workflow, not code module, and make the top 5 checks executable within a single deliberate review session; do not request user intervention mid-execution.

**Critical:** Generate/revise this artifact even if some native scenarios cannot be attempted or if the Goal ends in `COMPLETE_WITH_NATIVE_GAPS`. Missing acceptance is the *reason* for the manual checklist, not a reason to omit it. The final Codex reply must link/name this Markdown file and summarize the top manual checks, as well as link/name the execution/results records. The checklist is a mandatory deliverable and acceptance criterion for *this bounded acceptance Goal*, independently of whether all native tests pass.

## G. Final completion criteria and stopping policy

1. New task branch from exact cumulative G3 final commit (or explicit verified successor with documented authorization), own baseline, normal checkpoint commits and attempted pushes.
2. M1–M8 attempt/guard budgets respected. Each required native scenario has independently supported per-phase status and a safe stopped/closed outcome. No unsafe click/injection/pixel sampling or personal data/hardware change.
3. Actual product defects: minimal owner correction, targeted tests, current binary native recheck if within budget; failing or unverified native phases remain **explicit gaps**, not accepted on unit tests alone.
4. Two full-suite results at most, only when permitted; report any XML/query flake honestly. No inflated success counts or false baseline attribution.
5. No performance regression, unrelated feature or workspace additions, schema migrations, vendor edits, forced push, merge/PR/master modification or discarded user files.
6. Independent final source review, bounded cross-owner selected tests, formatting, final Clippy status and exact process ownership cleanup recorded.
7. **Both** required final documents exist, `docs/plans/POST_QUEUE_MANUAL_VERIFICATION.md` and `docs/plans/POST_QUEUE_NATIVE_ACCEPTANCE_RESULTS.md`, along with compact execution ledger; updated to the **final** evidence, not placeholder statuses.
8. Final status is one of `COMPLETE_NATIVE_ACCEPTED`, `COMPLETE_WITH_NATIVE_GAPS`, `BLOCKED_PRODUCT_DEFECT`, or `BLOCKED_BOOTSTRAP`; choose from evidence and do not withhold a partial useful manual handoff if a gate blocks.

**Stop promptly when any budget is exhausted.** Do not ask for intermediate reviews, silently start another Goal, run broad feature development, promise future background work, or treat a failed safety prerequisite as permission to bypass it. The user will perform exactly **one** hands-on review after this task's report/checklist are delivered.
