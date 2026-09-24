# Radial reliability and action authoring — acceptance matrix

**Starting archive:** `multi_launcher(20260924-210943).zip`  
**SHA-256:** `f21ca0ee92d1b7572f779b936bb3ce1de7e685314f685ae8711d1734952d2544`  
**Status:** planned tests; none are reported as executed by this packet.

This matrix defines evidence, not just a checklist of UI features. Case IDs below are new logical IDs; prefix native runner IDs with `RR-` to avoid confusing them with historical H0/D0/A0 cases. Preserve existing valid acceptance coverage while migrating explicitly superseded expectations. A row can expand into several parameterized tests, not a separate test binary per row.

Layers: **U** deterministic/unit or fake-backend; **I** integration of application services; **E** retained/headless egui; **N** opt-in native Windows through the production path. All layers listed for a case are required where supported by the standard Windows configuration. Extra monitor/elevation combinations are reported separately when unavailable.

## Required fixtures and observable state

Use an isolated `AppDataRoot`/temporary profile per runner instance. Keep the user's active profile untouched and enforce single-instance ownership of each data root. At minimum provide two uniquely titled notes, two notes with the same title but different supported IDs, enough custom actions to search beyond the old first-50 limit, one stable pinned target, one ephemeral target with a fake/stale-identity unit variant, two ranked query results, a no-result query, a UI-required action, and a destructive fixture action requiring confirmation.

For positive external execution, configure a real harmless fixture action whose normal command dispatch writes a nonce-tagged marker in the test directory. A bounded marker-helper mode in the acceptance runner or an existing suitable test helper is acceptable; it must be invoked as a normal external action, not an alternate production dispatch backdoor. Assert a single matching effect. The starting fixture strings named `radial_acceptance_harmless_*` by themselves are not successful execution evidence.

Read-only tests snapshot document bytes/draft generation, runtime action-dispatch count, execution history/usage, grid query/results/selection/visibility, and relevant marker/clipboard state. Execution cases explicitly specify which state may change. Avoid destructive operations against real user files, real macros, or unrelated windows. Collect detailed text only from controlled fixture data; ordinary diagnostics remain bounded and privacy-safe.

ROOT presentation oracle: validate HWND and process identity; inspect desired state/revision, ROOT command trace, monitor-intersecting bounds, minimized state, and relevant foreground/window observations. Ordinary offscreen parking can retain `WS_VISIBLE`; a single `IsWindowVisible` result is not proof of presentation. No-flash cases must observe transitions, not merely the final hidden state. Runtime dismissal must account for every owned radial surface/submenu and stale/replacement HWNDs; the Designer/native authoring preview is not a runtime radial surface.

## H — Hotkey/grid/runtime interaction (Gate H)

| ID | Setup and operation | Required oracle | Layer |
|---|---|---|---|
| H01 | ROOT hidden, radial closed; press/release exact chord below threshold. | One admitted short decision; ROOT shown/focused through its normal path, no runtime radial. | U, I, N |
| H02 | ROOT visible and focused; short tap without pointer movement. | ROOT parks/hides; no external click, no radial, no new focus/restore that brings it back. | U, I, N |
| H03 | ROOT visible but not focused; short tap. | ROOT toggles off, not merely focuses itself; next tap shows it. | I, N |
| H04 | Starting hidden then starting visible; uninterrupted burst lengths 1, 2, 5, 10, 25. | Exactly N admitted short decisions, unique gesture IDs, alternating desired states, final `initial XOR (N mod 2)`; no hold promotion. No waits/refocus/pointer between gestures. | U, I, N |
| H05 | Below threshold, exactly threshold, above threshold; delay timer and release delivery using physical timestamps. | Below is tap; at/above is hold/correct existing delayed-release behavior; no tap plus hold; early timer rearms remaining duration. | U |
| H06 | Runtime radial open, ROOT hidden; short tap with a hovered executable cell. | ROOT becomes visible; runtime radial and child surfaces dismissed; zero selection/dispatch/marker effects. | U, I, N |
| H07 | Runtime radial open, ROOT visible; short tap. | ROOT hides AND runtime radial closes; no action; next gesture remains usable. | U, I, N |
| H08 | Runtime open is preparing; short tap before ready, then deliver late preparation/Ready. | Grid toggles once; exact pending runtime presentation cancelled; late reply cannot reopen radial. | U, I; N with real service delay gate |
| H09 | ROOT hidden and visible separately; hold to open closed radial. | Radial opens; grid desired visibility unchanged. In release-to-select mode retain existing release semantics; no grid flash. | U, I, N |
| H10 | Runtime radial open, cell hovered; a new hold closes it, then release every key. | Radial closes at hold decision; release does not toggle grid or select cell; no delayed reopen. | U, I, N |
| H11 | Dirty Designer focused; repeated short taps; ROOT initially hidden/visible. | Grid toggles; same Designer identity/session and draft digest remain; no Save/Discard/close event; ordinary focus policy not artificially forced back. | E, I, N |
| H12 | Native authoring preview open with Designer; runtime radial separately open/closed; tap and hold. | Runtime behavior follows contract; Designer/authoring preview leases remain separate; draft unchanged. | U, E, N |
| H13 | Duplicate key-up/down, key auto-repeat, left/right modifier variants, release-order permutations. | One physical cycle yields at most one decision; fully released next cycle works; no sticky modifier state. | U; N exact mapped sequence/control |
| H14 | Settings reload, feature disable, shutdown, suspend/lock, or hook failure while pending/holding. | Existing lifecycle cancellation and safe release drain; no late toggle/dispatch; no stuck ownership. | U, I |
| H15 | Exclusive screenshot/Screen Draw/macro capture or emergency owns the gesture. | Existing higher-priority owner preserved; no admitted launcher toggle or unintended radial. | U, I; N safe smoke |
| H16 | Direct-trigger runtime radial open; use normal launcher tap through native and legacy routes. | Runtime radial dismissed and grid toggled through both supported routes; direct-trigger behavior itself retained. | U, I, N |
| H17 | Run F11 regression control and exact `Shift+Alt+Win+End` profile with normal mouse-gesture service enabled. | Production admission decisions correspond to actual configured chord; no fake F11 substitution; observer/injection/profile metadata agree. | U, N |
| H18 | ROOT parked/idle, pointer stationary, Designer closed; press hold or tap. | Relevant owner wakes without mouseover or periodic full repaint; radial and grid display through their proper paths. | I, N |

### Burst driver requirements

Set up foreground once before each burst. Use monotonic scheduling of physical events. A reasonable initial diagnostic schedule is approximately 25 ms down plus 25 ms released per cycle, below the configured threshold; record actual edge timestamps and adjust only documented fixture timing, not production behavior. Do not wait for ROOT visibility, insert a sentinel, or refocus an anchor between cycles. The deterministic counterpart must exercise many more lengths/orderings cheaply with a fake clock.

Observe logical per-gesture decisions and final native state after the burst. For individual presentation transitions, run a separate readable-cadence sequence; frames cannot display every state when gestures are faster than the refresh rate. Native scheduling overload that causes an actual physical hold must be reported with timestamps, not silently relabeled as a failed short tap. Artificially waiting out the hold threshold before every tap is not an acceptable test of responsiveness.

## L — Ordering, lifecycle, responsiveness

| ID | Setup and operation | Required oracle | Layer |
|---|---|---|---|
| L01 | Show request A; hide B; delay A's native restore/desktop/focus stages. | After B becomes authoritative, A cannot leave ROOT presented/focused; cancellation/reconciliation recorded at side-effect boundary, not only completion. | U fake activation backend, I; N normal stress |
| L02 | Show A, hide B, show C; complete stale work out of order. | C's intended geometry/visibility wins; no older worker steals focus or resets placement; bounded live workers. | U, I |
| L03 | Capture ROOT preservation snapshot; admit a newer hotkey toggle; restore snapshot. | Snapshot cannot overwrite newer visibility/restore revision or resurrect ROOT. | U, I |
| L04 | Slow query preview for cell A; select/edit B or close/reopen Designer; deliver A. | A reply rejected by session/draft/request identity; no UI mutation/dispatch/reopen. | U, E, I |
| L05 | Slow runtime query; dismiss/supersede session or change configuration before resolution. | Uncommitted work cancelled; no-result fallback does not reopen grid after cancellation; close-for-action-handoff remains a valid continuation. | U, I |
| L06 | Repeated dispatch token/reply, repeated confirmation completion, and replay after lease retirement. | At most one executed action; stale/duplicate outcome is explicit; bounded tombstones and pending work. | U, I |
| L07 | Repeated frames with closed/idle Designer, or unchanged open query/preset gallery. | No provider catalog build while closed; no rebuild per unchanged frame; bounded cache/task counts; normal close pending still drains. | U counters, E; N measured smoke |
| L08 | Move main window, then open note editor/mkmacro UI from radial and toggle grid. | Restore-only handoff preserves intended current geometry; configured placement applies only where existing explicit show policy requires it. | I, N |

## P — Model, persistence, compatibility (Gate P)

| ID | Setup and operation | Required oracle | Layer |
|---|---|---|---|
| P01 | Decode v1 and v2 documents with old bindings/styles. | Explicit migration to current schema; preserved IDs and semantic fields; no disk write on read. | U, I |
| P02 | Round-trip v3 pinned/contextual/query/command bindings, both query modes, args, alternate clicks. | Same semantics/defaults; no result index/HWND serialization; Auto Submit OFF only defaults absent new choice. | U |
| P03 | Open existing dynamic LauncherQuery cell and new saved query cell. | Distinct representations and behavior survive load/save; dynamic pagination remains available. | U, I |
| P04 | Empty/whitespace/oversized query and command; invalid structured args. | Actionable validation with documented limits; no execution and no partial invalid publication. | U, E |
| P05 | Future schema, malformed candidate, invalid external reload. | Unsupported/corrupt candidate rejected; last-known-good runtime/document survives. | U, I |
| P06 | Concurrent disk change or store revision mismatch during Save/Apply. | Existing conflict handling preserved; no lost update or silent overwrite. | U, I |
| P07 | Package export/import new bindings plus media and menu references. | Shared decoding/validation, exact binding semantics, correct references, no command execution on import. | U, I |
| P08 | Save failure at existing transaction hooks; asset/migration failure. | Atomicity/backups/rollback maintained; dirty draft intact with error. | U, I |
| P09 | Copy/duplicate/new IDs/undo/redo with new assignments. | Query/command fields and alternate actions preserved; IDs unique and stable as appropriate. | U, E |
| P10 | Load current user/custom skin/settings fixture then save unrelated query cell. | No reset of threshold, selected skin, advanced overrides, menu visibility, triggers, or unrelated settings. | U, I |

## Q — Search, execution, safeguards (Gate Q)

| ID | Setup and operation | Required oracle | Layer |
|---|---|---|---|
| Q01 | Compare grid search and read-only query results on same snapshot across exact/fuzzy/alias/plugin/usage settings. | Same ordered results including first result and argument payload; cache behavior intentionally accounted for. | U, I |
| Q02 | Create saved query without touching Auto Submit; invoke. | Auto Submit OFF; grid shows query for interaction; no first-result effect. | U, E, I, N |
| Q03 | Auto Submit ON and unique safe fixture first result. | Exactly one primary action/effect, correct query/history/source, no extra query-wrapper execution. | U, I, N |
| Q04 | Change ranking/results after cell save AND after menu open but before activation. | Live query resolves current first result; persisted query unchanged. | U, I |
| Q05 | Pin target/action, then change ranking and query matches. | Same pinned identity/action runs; not new first result. | U, I, N safe fixture |
| Q06 | Delete/disable pinned target; rename display title with stable ID; change supported ID separately. | Stable-ID title rename remains same target; missing identity unavailable; no query fallback or retarget. | U, I |
| Q07 | Query returns no results. | Grid opens with original query and no-result explanation; no effect. | U, I, N |
| Q08 | Query's first result unavailable, second executable. | No silent skip to second; query + reason shown, no unintended effect. | U, I |
| Q09 | Query returns ephemeral window/clipboard-style result. | Fresh runtime identity may execute; pin is unavailable; stale identity cannot target reused handle/index. | U, I |
| Q10 | Non-UI result while grid hidden and while visible. | No unnecessary ROOT Show/Focus/Restore, no presentation flash; ordinary grid query/selection/visibility preserved. | U, I, N |
| Q11 | UI-required note editor/dialog/manual query and confirmation. | Required UI actually opens/focuses through established interaction policy; preserve geometry. | U, I, N |
| Q12 | Saved query or exact command intentionally invokes launcher show/hide/toggle/query navigation. | Explicit visibility/query command is honored, not reversed by preserve-state wrapper. | U, I, N subset |
| Q13 | Destructive resolved action; Cancel, then Confirm in separate case. | Normal/radial safeguards apply; Cancel zero effects; Confirm one effect on originally selected validated identity. | U, I; N safe fixture |
| Q14 | Preview search, select row, pin, assign, mode switch, reopen. | Zero dispatched actions/history usage changes/marker effects; document changes only when authored mutation applied. | U, E, I, N |
| Q15 | Advanced exact command with arguments and parser external fallback. | Same parse/dispatch as launcher; UI clearly identifies fallback; no query-to-shell reinterpretation; invalid command errors preserved. | U, I, N safe fixture |
| Q16 | First result is query navigation, or explicit nested queryexec creates a cycle. | Navigation opens requested query once; recursion bounded with clear fallback; no implicit unbounded auto-run. | U, I |
| Q17 | Slow/erroring provider, temporary cache pending, cancellation, superseding result. | Distinguish Pending/No results/Error/Cancelled; bounded work; no late action/reopen from cancelled request. | U, I |
| Q18 | Choose result, then change ranking/availability during release/close/confirmation wait. | Dispatch frozen selected identity only; if unavailable, safe failure/fallback, never a different result. | U, I |
| Q19 | KeepOpen/after-action with non-UI, UI, ExternalInput, capture, or unresolved query. | Shared compatibility rules enforce safe handoff; unresolved is not falsely declared interaction-free. | U, E, I |
| Q20 | Existing Quick Tools, query parser aliases, favorites/macro callers, and input-source attribution. | Existing non-radial semantics preserved while sharing resolver; no double history/usage or global manual-query regression. | U, I |

## D — Shared editor, insertion, bulk edits, navigation (Gates C/D)

| ID | Setup and operation | Required oracle | Layer |
|---|---|---|---|
| D01 | Query notes with repeated Edit Note actions and duplicate titles. | Target/action/type and distinguishing ID readable without hover in both Inspector and Cell Properties. | E, N |
| D02 | Search a target beyond position 50 and a token/title match the old Inspector filter rejected. | Correct results reachable; same search across surfaces, filter before cap; no second rejecting filter. | U, E, N |
| D03 | Edit same binding through both surfaces; reopen; switch cells quickly. | Same component semantics, stable scoped IDs, no cross-cell buffer/selection contamination. | E, I |
| D04 | Pin specific result/secondary action versus save live query with mode. | Correct distinct persisted binding; summary clearly names mode/action; one authored mutation. | U, E, N |
| D05 | Use `would execute` and explicit Test on a dirty binding. | Preview zero effects; Test separately invokes normal validation/confirmation; stale draft never executes. | U, E, I |
| D06 | Add to radial from grid/list result with Designer closed. | Correct action captured semantically, ordinary Designer open lifecycle, explicit destination, no source activation. | U, E, I, N |
| D07 | Add while Designer already dirty; occupied destination; Cancel/Replace. | Existing draft preserved, replace explicit, Cancel no mutation, one history step on acceptance. | U, E, N |
| D08 | Add ephemeral/nonpersistable result. | Honest disabled pin/contextual-or-query alternative; no saved HWND/list index. | U, E |
| D09 | Query/preview outstanding; close, Keep Editing, Discard, reopen. | Existing lifecycle controls responsive, matching session receives replies, late work cannot reopen or overwrite draft. | U, E, N |
| D10 | Ctrl-click and Shift-range multi-selection; rename/reorder/delete/reload/undo. | Stable IDs and deterministic range, missing targets pruned, valid selection maintained. | U, E, N subset |
| D11 | Bulk label/style/after-action on mixed cells; Undo/Redo. | One atomic mutation/history unit; untouched fields/overrides retained; mixed states honest. | U, E, N |
| D12 | Bulk policy includes incompatible query/UI cell or missing selected target. | All-or-nothing failure with reason; no partial mutation or hidden skipped cell. | U, E |
| D13 | Search/reveal menu/cell; breadcrumb/back across shared submenu with several parents; delete prior location. | Actual navigation path, stable selection, no implicit submenu execution or lost draft; invalid history skipped safely. | U, E, N subset |
| D14 | Compact window, long labels, text-field shortcuts, popups, dynamic generated cells. | Usable controls, unique IDs, local keyboard semantics, explicit generated-source editing rule; no accidental launcher action. | E, N |

## S — Appearance and rendering (Gate S)

| ID | Setup and operation | Required oracle | Layer |
|---|---|---|---|
| S01 | Fresh profile and existing custom-skinned profile. | New modern default only for new/starter data; existing chosen skin/overrides unchanged. | U, I |
| S02 | Gallery scroll/select/preview/apply/cancel. | Same renderer/style compiler, no action execution, bounded lazy thumbnails; cancel preserves unrelated edits. | U, E, N |
| S03 | Accent/scale/spacing/opacity/label edits with advanced overrides. | Declared scope, unrelated overrides retained, masked values identified; one transaction; Advanced still usable. | U, E |
| S04 | Inherit versus Clear versus explicit value; switch preset; undo/redo. | Existing precedence semantics preserved; round-trip exact intended overrides. | U, I, E |
| S05 | Compact/Comfortable/High Contrast/Classic presets at available DPI scales. | Readable labels and distinct focus/hover states; deterministic style semantics; no claimed unmeasured formal compliance. | U, E, N smoke |
| S06 | Dense ring including 50-cell authored fixture within existing model limits. | Useful warnings/remedies; no deletion or arbitrary new cap; invalid geometry distinguished from warning. | U, E, N |
| S07 | Scale/spacing/preset changed; hover/click targets in native and embedded preview. | Drawing and hit-testing use same effective geometry; correct target action and submenu/page controls. | U, I, N |
| S08 | Missing/bad/large media, cache invalidation, preset/package round-trip. | Existing asset limits/fallbacks/package safety; bounded cache; no filesystem escape or execution on import. | U, I |
| S09 | Designer closed or gallery unchanged across many frames. | No new thumbnail/provider work per frame, no recurring idle repaint introduced. | U counters, E |

## R — Whole-application and evidence integrity (Gate R)

| ID | Setup and operation | Required oracle | Layer |
|---|---|---|---|
| R01 | Final complete Nextest normal Windows run and applicable doctests. | Final source identity, counts, exit codes; no hidden new ignores/default filters; failures resolved or explicitly block completion. | Full Rust |
| R02 | Rerun exact-chord core native suite after M4/M6 and final remediation. | All mandatory cases belong to same finalized candidate/source manifest; historical runs not substituted. | N |
| R03 | Inject test failure at input, startup, query, and UI stages. | Bounded useful failure-stage evidence, valid report, nonzero status, no false pass on missing trace. | U runner, N controlled |
| R04 | Increase/overflow report/trace capacity and omit a required case. | Report explicitly fails; cleanup/final integrity retained; no silently dropped cases marked complete. | U runner |
| R05 | Timeout/failure during key-down; child exits; desktop/integrity mismatch. | Owned keys released best-effort, owned process cleanup, clear environment status; no unrelated process/profile changes. | U, N safe cases |
| R06 | Normal startup, grid search/Enter, hide-on-focus-loss, follow-mouse/static placement, dialogs, favorites/history, direct radial, paging/submenus, packages/import, macro/capture ownership. | Existing behavior preserved except approved launcher-tap radial dismissal; current regression suite remains meaningful. | I, E, N smoke |
| R07 | Authorized consistent disposable user-profile copy, when available. | Source untouched; no second active-data owner; migration/authoring/close/reopen correct; actual copy status recorded. No live user action execution. | I, N conditional |
| R08 | Independent reviewer reads final diff/contracts/evidence; fixes retested. | No unresolved substantive correctness/compatibility/proof gap; final report accurately names coverage and limitations. | Review |

## Gate membership and minimum native release path

Gate H: H01–H18 as applicable plus L01–L03/L07/L08 and runner isolation/report safeguards. At least H01/H02/H04/H06/H07/H09/H10/H11/H17/H18 must have actual native evidence; lifecycle races also need deterministic coverage even when a natural native occurrence is hard to force.

Gate P: P01–P10. Gate Q: Q01–Q20 plus L04–L06; native minimum includes manual query, positive Auto Submit, no-result fallback, hidden-grid no-flash, visible-grid preservation, and UI-required execution. Gate C: D01–D09 plus H regression and Q core. Gate D: D10–D14. Gate S: S01–S09 plus H/core preview regression. Gate R: integrated relevant matrix and R01–R08, with R07 explicitly conditional on available authorized copy.

An unsupported environmental variant does not silently reduce the mandatory core set. A standard desktop Windows candidate must pass the mandatory native path. Physical firmware/hardware mapping is not proven by `SendInput`; report native chord injection and admission separately from actual hardware provenance. The automated suite should eliminate repetitive manual testing as the primary workflow, while honestly describing that boundary.

## Evidence record

For each case record: logical/runner ID, setup/profile hash, candidate/source/runner identity, injected event timestamps and provenance, relevant request/session/revision IDs, expected/observed state, semantic widget/native window observations, execution/marker count, elapsed time, status, failure stage, and artifact references. Keep diagnostics bounded and avoid free-form private labels/commands unless they are controlled test fixtures.

Record PASS only when all applicable observations are obtained and agree. FAIL means a behavioral/evidence assertion failed. BLOCKED/UNSUPPORTED records a specific unmet environment prerequisite; map to existing report states explicitly or add a typed status with versioned serialization. NOT RUN is not SKIPPED-as-success. Retried failures remain visible; a flaky first attempt cannot disappear from the summary. Cleanup is part of acceptance, not an optional trailing log message.

Measure release-to-intent, release-to-ROOT-command, and command-to-observed-presentation latency separately. Deterministic release classification has no threshold wait. Native responsiveness should show no hold-threshold-sized delay or unexplained regression versus a comparable source-matched baseline. Absolute latency budgets may be calibrated and recorded for the actual machine; no unmeasured universal millisecond promise is part of this packet.
