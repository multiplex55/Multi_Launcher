# Source, testing, native proof and bounded execution policy

**Planning-time verified master:** `56223fdc123396f4c41c48e0542506d912c6c634` from merged #1456/#1457 on 2026-10-29. It is **not** a future source-reset instruction. Before each goal, verify the exact current accepted task base and whether owner implementations already changed. Reuse matching focused test evidence for *identical source and relevant manifests*, not for a materially changed owner.

## Evidence tiers

| Tier | May claim | Does not establish |
|---|---|---|
| `SOURCE_OBSERVED` | Cited source branch/path/symbol proves a narrow code property | The Windows UI actually behaved that way, an OS race or failure rate |
| `DETERMINISTIC_TEST_PASSED` | A nonzero selected test exercised the stated algorithm/state boundary | Global app correctness, native mouse/clipboard/hotkey, all compiled targets |
| `NATIVE_PROOF_PASSED` | A supported tool executed the exact owned native scenario and captured expected output/effect | Other Windows versions, long-running stability, all race interleavings |
| `STATIC_CHECK_PASSED` | Files/links/JSON/schema or source-text contract reconciled | Rust build/behavior, provider behavior, real file-system safety |
| `HISTORICAL_RECEIPT` | A named commit and saved report claimed a past test result | Current code/commit passed it unless exact inputs/outputs still match |
| `NOT_RUN`, `INCONCLUSIVE`, `DEFERRED` | An honest absence of proof and a precise future protocol | Acceptance; do not convert to PASS after timeout or unsupported tool |

## Default bounded checks per production checkpoint

- Read-only baseline/owner inspection, `git diff --check`, changed-path manifest, targeted existing test source and call-site checks. No Cargo during planning or pure-document stages.
- Exactly one initial selected build/list (suggested maximum 20 minutes if a real Cargo build is necessary); confirm a **nonzero** test inventory. If a selected executable/library target is already prepared, do not compile a second ceremonial `cargo build`.
- At most one focused selected Nextest run; default test-execution budget **180 seconds after preparation** (specific goal may choose 300 seconds for bounded file/ZIP resource cases). Run 1–2 localized correction reruns only for a named failing test. Allow one further compile/Clippy preparation of at most 10 minutes if the change actually requires it.
- One narrowly relevant first-party Clippy pass at final source checkpoint **only if needed** by the accepted implementation; prefer changed package/target with `--no-deps`, no full workspace lint unless a cross-crate change invalidates that. Do not touch `vendor/` warnings to satisfy a project lint gate.
- Controlled native run: **only for an explicitly required changed Windows/API/UI contract** and only with a supported actual input/output interface. Suggested per native scenario 180 seconds including cleanup and artifact collection; use an owned temp profile, reversible test data and minimal screenshots/effect receipts. Unsupported native driver -> `NATIVE_PROOF_DEFERRED`, not generic PASS.
- Never run full suite, global OS automation sweep, benchmark suite, clean rebuild, package installs, or extensive Clippy campaign merely because a milestone completed. If meaningful proof genuinely requires broader testing, record the owner/why and use a bounded review decision rather than silently expanding.

## Required artifacts for every test attempt

1. Exact invocation, tool version/target/manifest where relevant, source/head SHA and purpose.
2. Start/end timestamps, elapsed and timeout cap, process/child ownership when a timeout wrapper is used, exit code, selected test names/count, first meaningful failure and output-drain/fixture cleanup status.
3. `PASS`/`FAIL`/`NOT_RUN`/`DEFERRED` for each behavior (including separate native behavior); warnings classified by touched owner vs preexisting source, without suppressing facts.
4. Corrective changes linked to failing case and only affected retests. Never weaken an assertion or mark tests ignored solely to pass.

## Native and privacy guardrails

- All destructive probes operate on isolated, task-owned temporary files and directories. **No user folders, real clipboard contents, unsanitized macro history, private passwords, camera/screen capture or unbounded filesystem mutation.**
- Application fixtures which can capture the global paste sequence must disable Paste Sequence next/cancel hotkeys before constructing `LauncherApp`, preserving production defaults.
- Screenshot privacy tests use synthetic images/text and treat Solid redaction as the only supported masking promise; blur/pixelate are not secrets-erasure guarantees.
- If an owner needs OS-level native proof, the test runner must actually support the specific input (e.g. secondary mouse drag) and observable effect (saved/copied pixel output, worker join/handle receipt). Internal state injection is a unit test, not a native test.
- A safety failure makes that checkpoint nonintegrable. Stop only that workstream; publish exact blocker and follow-on. Maintain existing fail-closed behavior.

## Program-level testing

The parent combines accepted changes and performs **one** narrow diff/source/persistence interaction review, plus directly overlapping focused tests if accepted branches touch shared code. Historical test receipts can be reused when code+manifest identities are unchanged. No all-tests requirement by default. A PR is not the same as a GitHub status check; report actual host check-run/review state.
