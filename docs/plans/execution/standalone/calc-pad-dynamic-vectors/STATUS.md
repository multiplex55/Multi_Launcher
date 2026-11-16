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
| M2-A | Accepted | V4, V5 |
| M2-B | Next | V5, V6 |
| M3-A | Pending | C1, C2 |
| M3-B | Pending | C1, C2 |
| M4-A | Pending | U1 |
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
