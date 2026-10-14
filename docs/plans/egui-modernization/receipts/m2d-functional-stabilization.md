# M2-D functional migration stabilization

Authority: PERFORMANCE_ACCEPTANCE_ADDENDUM.md, user directive2026-10-14. Local branch egui-modernization; no publication. Performance parity is not a gate. Source796's documented release ROOT regressions remain accepted debt; none of these checks is a native latency benchmark.

## Implemented corrections

- Oversized same-frame regex paste retains the full Unicode buffer and one valid full-range LayoutSection before the max_rows0 early return. Strict elided/no-row/no-vertex and three-editor paste contracts remain.
- Diff persists body dimensions as before. Its GUI converts body/default/minimum sizes to egui0.36 outer dimensions using actual Frame margins, the prepared WidgetText title galley and Heading control height. Attainable inner minimums account for work-area constraints. Strict reopening, manual resizing and pathological-content clipping tests remain; reopening also covers a larger font override and extra line spacing.
- The shared radial action result column respects captured entry width on Properties and Inspector; only Inspector reserves its parent scrollbar gutter. Complete text, pin/test controls and bounds assertions remain.
- Headless fixtures explicitly dispose FullOutputs after their last consumer. Production texture-delta enforcement remains; no global clearing wrapper or vendor change was introduced.
- Fixtures follow released AccessKit Label values, exact Tree Search identity, current Window and ScrollArea IDs, real root visibility, measured pane-specific paint/input gaps, first-frame counters and intentional trace deduplication. Disabled submenu clicks are followed by reopening and remeasuring before the positive action. Unicode literals were restored, not assertions weakened. The test PI constant uses the identical standard-library f64 value.

## Retained execution evidence

Raw commands, source bindings, start/result JSON and stdout/stderr remain under target/egui-modernization/m2d/. The reused build target is m2a/live-policy-closeout-003/build-target-fast-dev. Locked/offline Windows fast-dev Nextest runs did not activate ignored benchmarks. Native exits were captured before parsing.

| Run | Actual result | Meaning |
|---|---|---|
| stabilization-001 | 6463 run:6242 passed,221 failed,45 skipped; format0,Clippy101 | Meaningful broad migration gate. All failures preserved, including denied test-only PI lint and mixed texture/behavior observations |
| corrections-001 | 242 run:195 passed,47 failed; native100 | Original failed cases plus affected regex/angle coverage. Oversized paste/Unicode corrected; remaining assertions exposed |
| corrections-002 | native101 before tests | Three task-owned disposal type errors; retained and corrected at their actual output types |
| corrections-003 | 173 run:162 passed,11 failed; native100 | Remaining failures plus affected diff/action-editor/snippet/regex modules |
| corrections-004 | 26 run:24 passed,2 failed; native100 | Remaining cases and Diff module. Diagnostic isolated fractional title metric versus actual galley allocation; retained helper output still needed disposal |
| corrections-005 | 17/17 passed; Nextest0,format0,all-target Clippy0 | Default and larger-font restoration plus final helper cleanup. Native durations101.408/14.022/134.471s |
| corrections-006 | 17/17 passed; Nextest0,format0,final lib Clippy0 | Final shared WidgetText path, including extra line-spacing restoration. Native durations65.860/12.056/64.167s; actual source bindings and result JSON retained |

Failure inventories preserve every panic rather than only the final panic during unwinding. Later scoped passing evidence closes the earlier failed cases; the original broad run has not been relabeled passing, and no redundant whole-suite rerun was required. Clippy's existing warnings were not mass-refactored.

Final coherent source review accepted without unresolved substantive findings. Reviewer independently matched the final source and raw-log hashes; no source drift.

## Qualification and continuation

No known critical functional defect remains from these scoped checks. Native physical hotkeys, current desktop focus/placement/DPI and true-idle observations remain unexecuted qualification gaps, not invented passes; see WINDOWS_FUNCTIONAL_QUALIFICATION.md. Earlier source/headless/integration/native framebuffer evidence retains only its documented scope. No unconditional release-readiness or exact-parity claim is made.

Glow remains explicitly selected. Comparative renderer benchmarking and isolated dependency optimization candidates remain deferred in POST_MODERNIZATION_PERFORMANCE_BACKLOG.md. Preserve this migration-only checkpoint before theme source changes, then immediately begin M3-A and complete the runtime/preview/typography/root/dialog rollout.
