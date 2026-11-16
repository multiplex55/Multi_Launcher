# Calculation Pad dynamic vectors and analysis dialogs

| Git identity | Value |
| --- | --- |
| parent_branch | `feat/calc-pad-and-file-search` |
| parent_tip_sha | `df62fd792a7a20d5a1c55fb542eddc73dd37bad1` |
| task branch | `feat/calc-pad-dynamic-vectors` |
| baseline_sha | `b745cb397d9f2d1445081c603e4ccdb604d19d28` |
| upstream | `origin/feat/calc-pad-dynamic-vectors` |

M0-A passed. The only later commit on the predecessor branch archived documents and added this task's instructions. The delivered predecessor tip remains the parent. The baseline was an intentional empty commit, its direct parent matched the delivered tip, and the initial upstream push exited 0. The existing checkout was clean at bootstrap.

| Checkpoint | Status | Requirements |
| --- | --- | --- |
| M0-A | Accepted and pushed | Branch ancestry and baseline |
| M0-B | Live owners and focused test names confirmed | Preparation |
| M1-A | Accepted and pushed at `ab6fa52d4` | V1, V3 |
| M1-B | Accepted and pushed at `0911e8000` | V2, V3 |
| M2-A | Accepted and pushed at `71075bbd6` | V4, V5 |
| M2-B | Accepted and pushed at `6f1d2f89d` | V5, V6 |
| M3-A | Accepted and pushed at `05f5cfbe1` | C1, C2 |
| M3-B | Accepted | C1, C2 |
| M4-A | Next | U1 |
| M4-B | Pending | U2, U3 |
| M5-A | Pending | V6, C1, C2, U1–U3 |
| M5-B | Pending | Integrated verification and delivery |

Automated work uses targeted library Nextest selectors and the `fast-dev` profile. Build and cached execution receipts go under `target/calc-pad-dynamic-vectors-evidence`.

## M1-A

Empty vectors and 1,024 finite scalar components are supported. Source keeps the ordinary 2 KiB expression limit, with 32 KiB for vector-literal expressions and separately bounded prepared templates. Vector parse fuel is separate from shared runtime metering.

The first library compile exited 101 due to three new test lifetimes. After correction, `cargo nextest list --lib --cargo-profile fast-dev --list-type binaries-only` compiled in 209.30 seconds owned elapsed. Cargo exited 0. The wrapper flagged descendants at root exit, then verified all recorded PIDs absent and output drained. Reusing the resulting binary metadata, five document/vector cases and one vector aggregate/cancellation case passed in their 100-second cached deadlines. The temporary archived diagram fixture was removed and was absent before staging.

## M1-B

The twelve collection operations use the shared script catalog and pure vector value semantics. Vector snapshots share an immutable allocation. Transformations that return a new vector copy the result once, after bounds and budget checks. The ordinary evaluation limit remains 50,000 work units. Vector work has a separate ceiling of 4,194,304 units for bounded traversal, copying, source bytes and other vector costs. Sensitivity charges one aggregate unit per 32 vector units, rounded up per evaluation.

Library compilation exited 0 in 283.78 seconds, with cleanup and output drain verified. The runner again reported descendants at root exit and verified retirement. The focused cached Nextest batch passed 17/17 in 3.12 owned seconds. The required two-vector script took 40.90 ms in fast-dev with 37,940 ordinary units and 1,068,086 vector units. Immutable append writes 1,049,600 components across the two chains. Snapshot publication shares vector storage. All three required scripts, local work exhaustion, cancellation and sensitivity aggregate exhaustion passed.

## M2-A

Both evaluator routes preserve the immediate valid binding after an evaluated collection call or argument fails outside a running loop. The failed physical row and its diagnostic remain visible, while named dependencies use the retained origin. Nested/formula failure provenance stays private to evaluation. Loop rollback and poisoning and ordinary errors retain their existing behavior.

The scoped library build exited 0 in 268.32 owned seconds with PIDs retired and output drained. The 11 listed Nextest cases passed in 1.42 owned seconds. Evidence covers both evaluator routes, dependency origins, compound assignments, loop transactions, Manual observation, rename/Undo/save and reopen, and worker retirement.

## M2-B

Vector rows format the first eight values and a count when longer. Canonical Copy uses full, bit-preserving scalar tokens and remains separate from explicitly formatted copying. The existing shared catalog drives highlighting, completion and nested hints.

The scoped library build exited 0 in 161.64 owned seconds with output drained and cleanup verified. Six listed Nextest cases passed in 1.44 owned seconds, including exact 1,024-component round-trip, subnormal and finite extrema, signed zero, clipboard routing, and editor discovery.

## M3-A

Vector charts accept 0–1,024 points and bar labels, use zero-based automatic X, and omit the scene and Copy action for matched empty data. Sensitivity remains at 1,000 samples. One-point line markers, exact length checks, distinct all-hidden errors and renderer limits remain enforced. Grouped bars use subpixel widths and a half-group inset at the plot ends.

Library build exited 0 in 160.00 owned seconds. Ten listed Nextest cases passed in 1.84 owned seconds, with cleanup and drain verified. In Windows x64 fast-dev with Rust 1.99.1, all eight 1,024-point series produced 8,192 hits per chart. The 640 × 400 scene and raster took 57.23 ms for line (16,394 primitives), 337.83 ms for bar (8,210 primitives), and 67.07 ms for scatter (8,210 primitives). Existing render budgets were sufficient.

## M3-B

Charts select final named bindings or physical result rows using typed references. Selectors use the current values' lengths without copying their components. Named documents now write v4. v3 chart selections keep their numeric physical IDs, v1–v3 records remain readable, and recovery stays v2. Rename history follows named selections and preserves independent edits and visibility.

The library build exited 0 in 299.42 owned seconds with cleanup and drain verified. Nextest passed all 18 chart and compatibility cases in the cached run. The substring selector also matched one unrelated diagram smoke case, which passed. Total owned execution was 5.73 seconds.
