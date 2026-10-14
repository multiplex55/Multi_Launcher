# Authoritative performance acceptance — 2026-10-14

The user's “Revise Performance Acceptance and Complete Both Tracks” directive supersedes conflicting requirements in the original plan, kickstart, ledger, handoffs and review checklists. Continue this project and complete both migration and substantial visual modernization. Exact original-egui performance parity is not an acceptance condition.

## Priorities and blockers

1. Correctness, data integrity and existing functionality.
2. Stable, responsive operation without severe usability regressions.
3. Complete Track A and Track B.
4. Reasonable, low-complexity performance hygiene.
5. Further optimization/comprehensive benchmarking as a separate future task.

Documented regressions are acceptable. Judge absolute cost, cold/warm/repeated frequency, realistic catalogs and practical native impact; do not impose a universal percentage cutoff. Higher medians, isolated tails, overlapping distributions or inability to prove exact equivalence are not automatic blockers. Synthetic CPU geometry is not application-launch or native hotkey latency.

Blocking findings must identify an observed serious defect: crash/deadlock/data loss, broken hotkeys/input/focus or duplicate execution, lifecycle defects, repeated visible freezes/severely degraded ordinary interaction, sustained genuinely idle work, unbounded resources, or broken virtualization/geometry affecting function. Slower historical benchmarks alone are insufficient. Missing environmental coverage is a documented qualification gap, neither a failure nor a pass; continue safe implementation with explicit manual QA follow-up.

## Current source and acceptance decision

Audit at directive receipt: branch `egui-modernization`, HEAD `6cf92f6b398a035b54018ff8682c56402b9d8043`. Tracked working tree clean; unrelated `Workspaces/` and an unexecuted cycle-diagnostic proposal were untracked. No agent owned active source writes and no cargo/rustc/launcher verification process was running. Keep `.gitignore` restored exactly; keep checkpoints local, without push.

Integrated application source remains `6527ddc15b6aa0c83fb2cbe41f5ceef9e4104669`, following measurement/cache source `79653eca51c80c8777b1b7e9ddaf6fd9a826e823`. Keep integrated work. ThemeSettings/ClipboardModify/radial closed-presentation guards and the strict radial fixture are provisional only because actual live compilation/affected contract execution remain pending; validate them rather than port another isolated candidate. Earlier compatible dependency/lifecycle/cache/layout evidence is reusable where source matches.

| Measurement | Previous disposition | Current disposition |
|---|---|---|
| Source796 release Full ROOT, original→migrated10k cold p50 127.5695→166.2805ms (+38.7110ms, +30.345%); warm0.2003→0.3171ms (+0.1168ms, +58.313%) | Failed original no-regression policy | `accepted_with_documented_regression`, subject to live correctness/functional closeout |
| Other retained release ROOT/list measurements | Historical failures/HOLD under the old policy remain unchanged | Known performance debt, not a Track B prerequisite |
| Isolated compact/row-storage/ASCII/shaping candidates | Experimental correctness and performance findings, including adverse samples | Deferred; not integrated and not required for modernization |

Source796 measurements are the latest applicable integrated geometry reference, not an exact benchmark of the later GUI-guard source. No performance improvement or native latency is inferred. All original numbers, adverse samples, raw logs, profile/source identities and previous dispositions remain historical evidence.

## Operative checkpoint sequence

- **M2-A:** choose current live source, establish compilation and targeted affected correctness. Close the former parity blocker as `accepted_with_documented_regression`; keep genuine correctness failures separate. Stop new dependency-level optimization experiments solely to recover original speed.
- **M2-B:** bounded available Windows functional checks for hotkey show/hide, typing/navigation/Enter/Escape, list/grid/scroll, representative notes/plugins, hidden-root child/prompt lifecycle, placement/focus and true idle. Reuse infrastructure. Record inaccessible observations and manual follow-up; do not endlessly retry unchanged capture/elevation/profiling failures.
- **M2-C:** retain explicit working Glow (`Cargo.toml`, `src/main.rs`) unless a demonstrated compatibility/serious usability defect warrants change. Comparative renderer study is deferred; do not enable/build Wgpu variants for parity.
- **M2-D:** adequate compilation/correctness, no known critical functional defect, documented accepted debt/QA gaps, review against this policy. Broad automated regression at this meaningful stabilization gate; not after every small edit. Preserve an identifiable migration-only source checkpoint, then immediately start M3-A without approval.
- **M3–M5:** deliver centralized semantic themes, System/Dark/Light/Custom, accents/presets, transactional preview/Cancel, typography/cached icons, root/list/grid, shared components, settings/plugins/secondary surfaces. No reduction to a few colors.
- **M6:** relevant functional/automated checks, visual review and bounded responsiveness/true-idle smoke. Comprehensive three-way statistical benchmarking is future work, not a completion prerequisite.

Theme application follows effective changes, reuses resources/caches, preserves widget identity/virtualization and palette-only geometry, avoids render-path I/O/native queries and continuous idle styling repaint. Bounded additional rendering work is allowed. Expensive/invasive effects stay disabled/deferred. Preserve strict behavior/layout/Unicode/cursor/output contracts; never hide functional loss in snapshot changes.

## Orchestration and dependency maintenance

Planner, implementer and reviewer have been explicitly notified that inherited no-regression gates are superseded. Review coherent milestones and commit meaningful changes; do not create separate approval cycles for receipts, parsers, listings or documentation. One current status/next-action section in `status.md` is authoritative; older handoffs/ledger entries are historical. Reuse compiled targets where appropriate; no routine cargo clean or immutable-artifact rebuild.

No repository kickstart file was found. The external attachment kickstart remains preserved, with its conflicting performance/renderer instructions superseded by this directive and addendum. This is an amendment to the existing plan, not a replacement migration initiative.

Do not broaden vendored forks solely to remove accepted regressions. Existing justified patches remain with purpose, correctness coverage and removal considerations recorded in the performance backlog and each `vendor/*/PATCH.md`. No mandatory upstreaming/rewrite before theming.

See `POST_MODERNIZATION_PERFORMANCE_BACKLOG.md` for deferred work. Final reporting separates migration, theme coverage, actual tests, accepted debt, missing native/manual checks, deferred benchmarking and any remaining release-blocking defects. No exact-parity/no-regression/complete-native claim without evidence.
