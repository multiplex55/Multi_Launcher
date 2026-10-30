# G08 implementer — combined M1 history checkpoint

2026-10-30. DEFERRED_WITH_DESIGN; all G08 source is unaccepted. The 14/14 result below is historical prototype evidence, superseded for current status by the R1 stop appended below. Requested model: Astra/xhigh. Configured `.codex/agents/implementer.toml`: Luna/max, unchanged. Actual serving backend: UNVERIFIED. Parent owns Git and integration.

Read the complete original Goal/Kickstart, planner HISTORY_CONTRACT, M1_HISTORY_HANDOFF and RECIPES_DECISION. Baseline HEAD `826e418b9a812128801cbbe742410889212d87bd`, independent bootstrap `02d0ac64`; authoritative accepted integration `ca2f6acb` remains separate. Existing SummaryHistory, runtime gateway and MkMacro dialog are the owners.

Implementation uses a fixed inline, non-Clone projection (20 headers, optional off-page selected header, 64 detail rows, 256-byte display names), one free-headroom reservation in the existing history budget, and a scoped lease returning its exact charge to its original owner. No allocation is performed for projection storage. Both registry and history locks end before rendering. Completed-only clear is deferred until after lease release and checked against runtime generation. UI state contains keys/filter/pages and view choice; test-only widget rectangles contain no summary metadata.

Actual History controls use the runtime gateway, bounded safe formatting and existing Jump registrations. Editor data and execution controls remain owned by their existing paths. No recipe, invocation, persistence, native API, app fixture or LauncherApp change.

Tests: seven new `g08_history_` cases plus seven directly affected existing controls. Final verification is 14/14 PASS, exit 0 in 3.635 seconds. Initial compiler E0382 and the first 13/14 run are preserved in the verification record; both corrections were fixture-only and retained all assertions. Aggregate correction preparation was 567.882/600 seconds, with no renewal. Exact commands/blobs/inventory/times are linked in [VERIFICATION](../../VERIFICATION.md). Native/manual/full-suite/domain verification NOT_RUN.

Distinct new follow-on finding: NO_ACTION. The planner's [recipe continuation](../planner/follow-on/explicit-invocation-recipe-policy/GOAL.md) remains conditional; this checkpoint does not implement it. Parent handles review/commit/push and final dispositions.

Final ownership review: persistent production HistoryState contains only view/filter/key/page/expiry state; no summary/name/row/runtime Arc cache. The callback receives fixed storage and returns no projection. Clear is applied after the lease returns and checks captured generation. Canonical publish/admission and original borrowed APIs are intact. Normal/unwind and old-owner release, concurrent clear/publication/replacement, active accounting, name/detail limits, paused/busy semantics and real controls are tested. Existing toolbar/layout controls pass. All sessions drained; no task-owned process remains; ignored raw evidence/cache is retained. No Git mutation, next milestone, acceptance or native claim.

## R1 bounded correction and stop

Read the complete reviewer record and history-toolbar-diagnostic-boundary Goal/Kickstart at parent HEAD `4be660385c8a0a91302f4f6ed101bb1c0992c59a`. Requested Astra/xhigh, configured Luna/max unchanged, actual backend UNVERIFIED. Sole writer; no Git mutation/agent dispatch/global-ledger/reviewer-file edit.

Corrected the first responsible owner, the shared toolbar status block: History gets fixed diagnostic labels and state/count progress; Editor retains details and current draft name. Added an inert external-status provider to the complete toolbar path and a toolbar-plus-History regression with all three excluded sentinels, Editor restoration and actual AccessKit control enablement. No summary/runtime/invocation/store/schema/control migration.

Only one affected five-test preparation was attempted with the remaining 32-second cap. It timed out; taskkill of owned PID 32752/tree returned OS Access denied. Source stayed frozen; the process drained naturally at 101.880 seconds with late exit 0. No R1 tests were run; this is not qualifying proof or acceptance. The original 14/14 results and failures remain historical. Detailed deadlines, actual drainage, exact blobs/commands, late inventory and cleanup are in [VERIFICATION](../../VERIFICATION.md). No new Cargo or alternate termination workaround followed.

All source is UNACCEPTED. The [remaining-G08 restart Goal](follow-on/g08-history-verification-restart/GOAL.md) and [Kickstart](follow-on/g08-history-verification-restart/KICKSTART.md) require a fresh source packet and new explicit finite budget. They reference the reviewer R1 pair rather than duplicating the defect and retain the existing optional recipe gate. Distinct new finding: NO_ACTION. Parent handles checkpoint, docs-only reviewer disposition and any later acceptance/integration.
