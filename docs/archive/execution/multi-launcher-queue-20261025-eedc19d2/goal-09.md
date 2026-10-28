# Goal 09 execution record

Queue: `multi-launcher-queue-20261025-eedc19d2`.
Branch: `codex/large-csv-queue-20261027`.
Worktree: `C:\Users\Jay\.codex\worktrees\large-csv-queue-20261027\Multi_Launcher`.
BASE09_OVERRIDE / accepted CARRY: `ceff1f17fb09545d8b9a6010aae9eca7892a7ea7`.
M0-A: `c0168dac18f5fdaa2bc43354331f8bb3de045c25` (normal upstream push succeeded).

## Status and provenance

Goal09 remains in progress; no FINAL09 exists. Goal08 is deferred/incomplete, so the explicit queue recovery authority selects the last accepted cumulative implementation anchor. This branch inherits Goals01-03 and accepted Goal04 M1-A/M1-B, without unaccepted Goal04 editor or Goal05-08 work. Their original worktrees and evidence remain preserved.

The original Goal09/Kickstart, root AGENTS, master queue and recovery override govern this execution. The baseline is an intentionally empty branch-specific M0-A whose parent is CARRY and whose source tree is `c6ed88d8dc5d158e77e57dd26ed14bae66d14a69`. No branch or baseline was recreated.

| Checkpoint | State |
| --- | --- |
| M0-A | Complete and pushed |
| M1-A | Accepted and pushed: `106f937368b9555d7b1210e2e9b5b9f8c1fa6941` |
| M1-B | Verified; checkpoint commit/push identities recorded externally after creation |
| M2-A | Pending: complete-file filtering, sorting and streaming export |
| M2-B | Pending: lifecycle/native acceptance, actual >500MiB workflow and final lint |

## M1-A: bounded source storage and policy

The pure owner remains `crates/multi_launcher_table_core`; `src/table_utility.rs` stays a re-export. The existing lexical scanner is shared between compact parsing and the new bounded file source. File-backed sources retain the read handle, native identity, validated options/schema/counts, bounded diagnostics and a quota-owned disk sparse index. Source-bound row ordinals are u64. Cursors decode bounded records; no retained whole-source string or all-row vector represents a large file.

One aggregate pool charges admitted work, retained sources, compact documents, projections, selections and exported payloads. Captured policy views share the same usage counters. Shared immutable data retains its lease through independently cloned handles; source replacement, cancellation and stale completions release only their own resources. Temporary growth checks the selected OS temp volume before each append; partial writes remain charged, and failed deletion retains bounded retryable cleanup ownership.

Persisted `table_utility_resources` preferences use `ram_limit_mib` (default 256, range 64-1024) and `temp_limit_gib` (default 8, range 1-64). Missing fields retain defaults; invalid values explicitly refuse new admissions. At least 1 GiB free space is required on the selected temporary volume. These are aggregate tool-owned working-data limits, not a whole-process RSS promise. Settings commits update subsequent admissions; active jobs keep their captured policy and existing leases. Unrelated settings saves preserve the latest preferences.

Large parsing enforces 1024 columns, 8 MiB encoded/decoded logical-record limits including headers, and 4 MiB decoded fields incrementally. Compact Table/CSV limits/defaults remain unchanged. Profile choice and strict/lenient row-width policy remain independent, including pending typed rereads.

The existing Condvar worker owns file opening and scanning. Windows admission is read-only and denies write/delete sharing, with identity obtained from the retained handle. A failed or stale replacement preserves last-good ownership/options. Close retains accepted ownership; shutdown joins. The typed `request_file_backed_source` is the large-route boundary; legacy `request_file_read` keeps its exact compact limit/refusal semantics. M1-B supplies bounded paging; full-file outputs remain pending M2-A.

## Verification and review

Evidence directory: `C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2\g09`.

Targeted preparations used Cargo Nextest binaries metadata for `multi_launcher_table_core --lib` and `multi_launcher --lib`, profile `fast-dev`, with build jobs 2. Initial compilation ceiling 45 minutes; changed-target ceiling 25 minutes. Ordinary test runs used the prepared artifacts, one test thread, no-fail-fast, a 600-second execution deadline and a separate 5-second cleanup deadline. No full repository suite was run.

- Core: 45 current cases have passing evidence across retained scoped receipts. The 23 unchanged legacy parser/projection cases were preserved; only new or affected source/resource cases were rerun. `m1a-core-cumulative-coverage.json` maps every case to its actual receipt.
- App: 20 directly affected session/settings cases have passing evidence across retained scoped receipts, including compact exact/one-over limits, captured policy, invalid-preference refusal, transactional quota refusal, independently retained data/output handles and profile/width independence.
- A real typed Windows source beyond 16 MiB scanned 220000 rows, returned the indexed last row, retained native identity/read-only exclusion across close/reopen, preserved last-good bytes/identity on invalid replacement and released the source handle after ownership teardown. The successful large case took 33.8629931 seconds; it is not a >500MiB or GUI acceptance claim.
- Core tests cover parser rows/error-location parity, UTF8 at a reader boundary, exact/one-over record/field/header-column guards, cancellation, shared quota/overflow rollback, free-space refusal, injected partial-write/disk-full behavior, retryable deletion and bounded tracked peak accounting.
- All accepted launches have verified cleanup/output drain. First failures and bounded corrections remain recorded; assertions were preserved. Formatting and the scoped diff check pass. No compiler profile, Cargo manifest, lockfile or production dependency changed.

The configured Implementer owned source edits. Parent reviewed the submitted source and actual receipts under the explicitly authorized reviewer-capacity fallback; no configured independent Reviewer acceptance is claimed. The final review corrected profile inference without broadening scope. Final first-party Clippy remains at the original end-of-Goal boundary, after all later implementation/native work; no final Goal09 or lint pass is claimed here.

The external queue ledger records actual checkpoint SHA, exact accepted bindings, commit/push receipts and the next checkpoint. A commit cannot record its own SHA, so no recursive hash-recording commit is used.
## M1-B: projected pages and compact large selections

The dialog now exposes an explicit Open Large File route, preserving the existing compact opening API and CSV extension delimiter behavior. The accepted source remains fully validated. Scan progress uses a separate bounded, leased provisional preview, with unknown final totals and no export eligibility. A late validation error discards provisional state and preserves the previous accepted source.

The existing worker loads one projected page with at most 64 rows and 2 MiB of retained page data, with bounded display previews and exact source-column/row identities. Page bytes, overlapping snapshots, pending column descriptors and sparse row selections share the existing resource pool. Byte-limit stops remain distinct from EOF. All-minus-one selection retains one exception rather than constructing every row ID; counts remain u64 and edits refuse quota exhaustion transactionally.

Page/source/view/lifetime checks reject obsolete results. Same-page requests and selection-only edits do not refetch unchanged data; idle frames issue no new scan/page work. Failure/cancellation preserves the last good page. A successful resident parse clears superseded file-page state. The large dialog virtualizes rows and visible result columns, uses a bounded schema chooser that remains available after deselecting every column, and registers controls through existing Jump ownership. Full-file filtering, sorting and export remain explicitly unavailable until M2-A.

M1-B verification adds 8 current core cases and 20 current app cases mapped to retained scoped receipts; this is cumulative focused evidence, not a full-suite run. Tests cover cursor/row parity, huge logical row counts, page byte limits/EOF/preview truncation, provisional late failure, shared quota and cloned-handle lifetime, controlled stale completions, close/reopen, page admission refusal/no retry, idle counters, projection recovery, and directly affected small Table/CSV/export/popup behavior. The final affected app run passed 10/10 in 36.731311 seconds with cleanup/output drain verified and all 15 recorded process identities absent. Unchanged earlier passing cases were retained rather than repeated.

Initial compile/fixture failures and their bounded corrections are preserved in `g09/M1B_PARENT_REVIEW.md` and the raw receipts. The configured Implementer owned source changes; the parent independently reviewed that implementation under the user-authorized reviewer-capacity fallback. No configured independent Reviewer acceptance is claimed. No new dependency, Cargo/profile change, native acceptance, >500MiB pipeline or final lint result is claimed. M2-A and M2-B remain pending.