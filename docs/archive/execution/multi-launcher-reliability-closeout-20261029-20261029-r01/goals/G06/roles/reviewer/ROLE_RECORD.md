# G06 independent source review — M1 editor/transaction checkpoint

Disposition: **CHANGES_REQUESTED — one bounded P2 finding, R1**. Reviewed checkpoint `b708e38adc47ef17e5598a771fd7e194c888166e` against `d88f00b2375d7acaeb007e85b355a5cafb4d1030` on `codex/closeout-g06-jump-settings-and-focus-20261029-r01`. This is source review of M1-A/M1-B and their focused M2 controls; it is not whole-G06 or native M3 acceptance.

## Provenance and scope

- Role: independent reviewer, continuing the existing reviewer context. Requested model/effort: Astra/max. Repository configuration remains `gpt-6.1-sol`/`high`; actual serving model, reasoning effort and backend are **UNVERIFIED**. No pin changes were made.
- Read the complete active G06 Goal/Kickstart, the [planner handoff](../planner/M1_EDITOR_TRANSACTION_HANDOFF.md), source/risk/verification records and implementer receipts. Inspected the actual nine-path source/test diff and directly affected conversion, widget, transaction, final route-resolution and dispatch callers.
- Read-only source/Git inspection only. No Cargo, tests, builds, native input, application launch, Git mutation or child dispatch. Before writing this record the working tree was clean. Reviewer writes are confined to this record and the linked contingency pair.

## Finding R1 — P2: validate Jump against the resolved Paste Sequence shortcuts

The new Jump check in `src/hotkey/validation.rs:158-184` only sees process, radial and macro reservations. Its caller `src/main.rs::resolve_hotkey_route_candidate_with_macro` runs that check before resolving Paste Sequence routes at lines 1015-1063. The subsequent `validate_paste_sequence_hotkey_routes` at line 2459 never compares a sequence route with Jump.

A concrete accepted candidate has Jump enabled with `Ctrl+Shift+F8`, the Paste Sequences plugin enabled, a custom Cancel shortcut `Ctrl+F8`, Next disabled, radial disabled and no macro bindings. The editor's `jump_preferences` only compares launcher/quit/help, the base validator has no Paste Sequence reservation, and the final sequence validator finds no conflict. Both bindings remain accepted. The real `HotkeyTrigger` listener accepts extra modifiers, so pressing the local Jump chord also dispatches Paste Sequence Cancel; `handle_paste_sequence_hotkey_using` cancels active playback without a Jump guard. A saved Start shortcut has the same omitted validation path. This is a source-supported conflicting-route admission defect; no native execution is claimed.

The active Goal requires conflict rejection, and handoff requirement 3 requires checking active bindings at final merged-candidate preparation. Existing tests cover only the base validator and a directly injected preparation closure; the 25 passing library tests do not exercise this full resolver gap.

**Bounded correction:** use the existing shared exact-local-versus-required-global-modifier comparison at the final resolver boundary against its effective Paste Sequence routes, including enabled custom Next/Cancel and saved Start routes. Retain current optional-default omission, disabled-plugin/disabled-route behavior and unrelated reservation rules. Reject conflicting enabled Jump configuration before persistence; keep Jump out of global registration. Supply inline feedback for applicable Next/Cancel bindings already present in the editor if that can reuse the same decision. Do not introduce a shortcut registry or change playback/focus/hook ownership to compensate.

Add direct final-resolver regression coverage for the supplied conflicting custom control and saved Start cases, with disabled Jump/plugin/shortcut and a genuinely distinct modifier requirement as controls. Reuse the existing snapshot/binding test seam; no native playback is necessary. Run only the parent-approved focused correction set and confirm this named issue once.

## Other reviewed behavior and evidence

- Raw invalid drafts remain visible; typed save construction is fallible and all mapping callers migrated. Reset and hint samples are draft-only, use existing parsers/allocator, and the new controls register real stable Settings responses.
- All three Jump fields have general-save ownership, stale-baseline checks, conditional rollback and acknowledged overlay. Live publication remains after the existing acknowledgement; later raw edits survive a pending save. Plugin-selection ownership does not write Jump preferences.
- Existing controller, release fence, foreground/focus and query behavior source is unchanged. The direct tests cover acknowledged publication, preparation/apply failures, conditional rollback, stale preparation, real control focus and the selected unchanged focus/Enter controls.
- Inspected [final selected inventory](../implementer/verification/m1/selected-tests.json), [final run receipt](../implementer/verification/m1/run-02-fixture.receipt.json) and [output](../implementer/verification/m1/run-02-fixture.stderr.log): 25/25 passed, nine new tests and sixteen controls, no ignored/retried tests. Initial 24/25 failure and the fixture-client lifetime correction are preserved in [verification](../../VERIFICATION.md); no production correction was hidden. Initial preparation 207.519 seconds, incremental total 87.603 seconds and final run 6.726 seconds are attributed to the implementer receipts, not reviewer execution.
- Independently compared all 17 recorded source/manifest blob IDs to the reviewed checkpoint: all match. `git diff --check` passed. The [cleanup receipt](../implementer/verification/m1/cleanup.json) reports drained output and no remaining owned/compiler/runner processes.

## Acceptance and follow-on disposition

The cumulative source checkpoint remains unaccepted until R1 is corrected and its direct boundary proof passes. No other substantive finding is established in this bounded review. Native M3 remains **NOT_RUN** and requires its separate provenance/isolation assessment after source acceptance.

The complete [R1 Goal](follow-on/jump-paste-sequence-conflicts/GOAL.md) and [R1 Kickstart](follow-on/jump-paste-sequence-conflicts/KICKSTART.md) are a contingency for this active G06 correction. If corrected and confirmed here, mark them **RESOLVED_IN_G06**, not a new future queue item. Distinct unrelated recommendations: **NO_ACTION**.
