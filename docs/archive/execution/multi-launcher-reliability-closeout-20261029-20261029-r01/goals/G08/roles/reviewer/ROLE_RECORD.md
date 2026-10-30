# G08 M1 independent review

2026-10-30. **CHANGES_REQUESTED — R1/P2.** Review of `826e418b9a812128801cbbe742410889212d87bd..8d035538ba67f66565074a71b741bb0da2ff9d96` on `codex/closeout-g08-mkmacro-run-history-20261029-r01`; independent bootstrap `02d0ac64`. This is not source acceptance or whole-G08 completion.

Requested reviewer model/effort: Astra/max. Configured `.codex/agents/reviewer.toml`: `gpt-6.1-sol`/`high`, unchanged. Actual serving backend/model/effort: UNVERIFIED. Reused independent reviewer context from earlier goals; only G08's original complete Goal/Kickstart, current contract/handoff/recipe decision, five-file source diff, directly affected owners/tests and verification receipts are evidence for this disposition. Parent owns Git, source corrections, authoritative ledgers and integration. Reviewer authored only this record and its R1 continuation pair; no Cargo, native interaction, build, source edit, Git mutation or child dispatch.

## R1 — P2: the shared toolbar bypasses History's diagnostic boundary

`MkMacroDialog::show_contents_with_table_viewport` always calls `toolbar::show_with_jump_mode` (`mod.rs:7201`) before drawing the History body. The toolbar renders `dialog.command_error` verbatim (`toolbar.rs:524–526`); its `report` helper stores arbitrary `error.to_string()` (`:116–119`). `widgets::semantic_status` formats the supplied details into the displayed label (`src/gui/theme/widgets.rs:63–89`). Consequently, retain `command_error = Some("PRIVATE_DIAGNOSTIC_SENTINEL")` and select History: the real History surface displays that excluded diagnostic.

The same adjacent status block obtains arbitrary `runtime::recording_status()` (`toolbar.rs:527–529`; runtime producers use `error.to_string()` at `runtime.rs:2146,2736`) and formats the current draft's unrestricted macro name for a runtime snapshot (`toolbar.rs:530–555`). Those paths bypass History's allowlisted, bounded, sanitized projection. Treat these as one presentation-boundary defect, not separate findings or permission to redesign runtime diagnostics.

The [contract](../planner/HISTORY_CONTRACT.md) explicitly excludes arbitrary diagnostic strings and existing `command_error` from the History view. The new `g08_history_render_safe_labels_fallback_expiry_and_payload_exclusion` sets a diagnostic sentinel and asserts its absence, but its `frame` helper (`history.rs:260–313`) renders only `history::tabs` and `show_with`. It omits the production toolbar and therefore does not prove that privacy requirement at the actual composed surface. The 14/14 result is real; this particular coverage claim is incomplete.

Required bounded correction: enforce History's safe presentation at the existing shared toolbar boundary. Preserve Editor diagnostics, stored diagnostic values and Run/Stop/Pause/Resume/recording/debug controls. On History, suppress arbitrary diagnostic payloads or use fixed safe status labels, and avoid the unrestricted current-draft-name display. Do not clear diagnostic state, clone it into history, weaken the projection contract or disable the whole toolbar. Add a deterministic real-toolbar-plus-History composition regression that rejects sentinel leakage while preserving Editor detail and state/control behavior. Full details and the budget stop rule are in the [R1 Goal](follow-on/history-toolbar-diagnostic-boundary/GOAL.md) and [Kickstart](follow-on/history-toolbar-diagnostic-boundary/KICKSTART.md).

## Bounded source and evidence assessment

The fixed non-Clone frame contains at most 20 headers, one off-page selected header and 64 detail rows. Its actual storage/lease/runtime-owner charge is reserved from free headroom in the existing budget without display-driven eviction. The render callback runs after history and registry locks release. RAII returns the view charge to its original runtime owner, including unwind and registry replacement; clear removes completed charges only and applies after rendering with a generation check. Existing canonical terminal publication, active accounting and execution admission remain authoritative. The tests exercise those ownership boundaries, including publication/replacement during a callback, rather than merely counting rows.

Persistent UI state is view choice, finite filter, page positions, expiry state and typed run key; it does not retain summaries, names or runtime owners. Selection/paging/expiry/close and hidden acquisition behavior are implemented through the bounded gateway. Real response IDs use existing Jump registration. No recipe, history persistence, executor migration, manifest or native contract is added. Apart from R1, no further substantive defect was established within the assigned diff; this does not accept the source before correction and required proof.

Read [VERIFICATION](../../VERIFICATION.md), exact selected inventory, original E0382 fixture diagnostic, first 13/14 result, final output/receipt and cleanup. Final 14/14 PASS (3.635 seconds wall, 0.754 seconds test execution, exit 0) applies to the recorded source. Initial preparation was 213.145 seconds; aggregate correction preparation was 567.882/600 seconds. Only 32.118 seconds remain; this review does not renew that allowance. Native/full-suite/domain/app verification was NOT_RUN and is not newly required for this ordinary UI boundary.

Read-only `git rev-parse 8d035538:<path>` independently matches every final-run source/manifest blob:

| Path | Blob |
| --- | --- |
| `Cargo.toml` | `994a55fea039ca79614fbb019ac1b05cfab7381b` |
| `Cargo.lock` | `648e143ad2ca902e04ae9b7d4cf57899a2db8380` |
| `src/mkmacro/runtime/summary.rs` | `0ede3520e02c3e453ab00b28a12d0f73a6c5199f` |
| `src/mkmacro/runtime.rs` | `fa011b29318e8cfd673a35f8a3d0d985f0aed36f` |
| `src/gui/mkmacro_dialog/history.rs` | `07b280ff936bcc3ef80ff0bfe193d42ab7889941` |
| `src/gui/mkmacro_dialog/mod.rs` | `b9d87841395834e8e38c61fb17a31e5b80cd458c` |
| `src/gui/mkmacro_dialog/toolbar.rs` | `5f242aac8dba6b1d73a35e4dc67861254c032909` |

Source-only cumulative `git diff --check` passes. All 38 relative Markdown links in the G08 packet, including this reviewer pair, resolve locally. The full cumulative documentation check reports one extra EOF blank line in the parent-owned canonical recipe Kickstart; that cosmetic issue is not an additional acceptance finding. Parent's orchestrator record was the sole pre-existing working-tree change at final inspection; it was preserved.

## Disposition and continuation

Do not import this source as accepted M1. Parent may dispatch the single R1 correction under the remaining verification allowance; acceptance requires corrected-source proof, not inspection of a fix alone. If the remaining allowance cannot establish that proof, preserve the prototype and exact evidence and defer source acceptance truthfully. Any later restart needs explicit fresh verification authority and a current source packet.

The R1 pair is the only new unresolved finding. If corrected and verified in G08, mark it `RESOLVED_IN_G08` instead of placing it on a future queue. The existing [recipe continuation](../../follow-on/explicit-invocation-recipe-policy/GOAL.md) remains conditional and deduplicated; no recipe execution is authorized by history display. Distinct additional recommendations: **NO_ACTION**.

## Additive R1 closure status — 2026-10-30

Preserved donor `bb8c572026016d3042efce29e112f8199de41742` contains the bounded proposed correction, but the remaining preparation attempt timed out and the new regression was not executed. The [R1 closure review](R1_CLOSURE_REVIEW.md) accepts documentation/deferral only. All G08 source remains UNACCEPTED; R1 is **OPEN_VERIFICATION**, not resolved. The [complete remaining-G08 package](../implementer/follow-on/g08-history-verification-restart/GOAL.md) requires a current source packet, fresh explicit finite verification authority, qualifying proof and named confirmation before parent acceptance. The earlier 32.118-second allowance is no longer available for this attempt. Original findings/evidence above remain historical and are not rewritten as a source acceptance.
