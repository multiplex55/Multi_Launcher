# Follow-on Goal — Migrate the File Search domain fixture sender

Status: `PROPOSED / NOT_EXECUTED`. Discovering role: G01 implementer. Priority: P1 test-infrastructure unblock. This pair does not authorize immediate implementation.

## Evidence and ownership

At G01 HEAD `8d183f663b212f8a707ac16a4c056eef5fe641ff`, the selected `domain` preparation failed with E0308 at `tests/domain_cases/file_search_e2e.rs:43`: `filenames` passes `&std::sync::mpsc::Sender<_>` to `search_filenames_in_directory`, which expects `&SearchEventSender` (`src/file_search/walkdir.rs:52`). See [initial compiler receipt](../../roles/implementer/verification/list.stderr.log). The domain suite includes this fixture at `tests/suites/domain.rs:28`.

`src/file_search/coordinator.rs:145` already provides `From<mpsc::Sender<SearchEvent>> for SearchEventSender`; the conversion uses the existing endpoint without an activity reporter. Current walkdir tests use that conversion. No G01 commit changed the failing fixture or production search signature. Baseline fixture blob: `3916054e7c3ab2479a512fbb455b9d35f647f162`; walkdir blob: `a6873277a42c6dc56452c97a222080eb0920c861`.

Confidence: high for an unmigrated test caller blocking the monolithic domain target. Counterevidence/proof limits: no File Search runtime test was executed by this finding; this does not establish a production search defect. The production abstraction already models the publication endpoint.

## Objective and scope

Migrate the `filenames` test helper to the current typed sender API while preserving its existing event capture and assertions. The test fixture owns this adaptation. Production search/coordinator behavior remains unchanged.

Allowed source edit: `tests/domain_cases/file_search_e2e.rs`; associated task receipts only. Read `src/file_search/coordinator.rs`, `src/file_search/walkdir.rs` and the `domain` suite declaration. No production compatibility overload, new sender framework, activity lifecycle change, dependency/config/manifest edit or unrelated fixture migration.

## M1-A — update the stale test caller

1. Revalidate the source and first compiler error on a parent-selected fresh task branch. Treat the SHA above as historical evidence, not a reset target. If already migrated, record `NO_CHANGE_REVALIDATED` with evidence.
2. Import/use the existing `SearchEventSender` conversion for the channel sender in `filenames`; keep the receiver and result extraction unchanged. Inspect current adjacent tests for the established pattern.
3. Preserve all expected filename results, root-error assertions, cancellation token and request semantics. Do not weaken or remove tests.
4. Prepare and run the focused inventory below. Inspect the resulting diff; obtain one focused review, then make one coherent checkpoint commit and one normal best-effort push through the parent.

Suggested subject: `test(file-search): [M1-A] migrate domain fixture sender`.

## Verification and done criteria

Use package `multi_launcher`, `--cargo-profile fast-dev --test domain --offline --locked`. One `cargo nextest list` preparation (20-minute cap), then one run (180-second built execution cap) of these four existing cases:

```text
test(=file_search_e2e::global_roots_resolve_and_search_only_configured_roots) | test(=file_search_e2e::multi_root_custom_searches_include_each_root) | test(=file_search_e2e::overlapping_roots_are_deduplicated) | test(=file_search_e2e::invalid_roots_reported_while_valid_roots_still_search)
```

Require a nonzero four-case inventory and passing results, unchanged assertions, `git diff --check`, and no production/config changes. Allow one localized correction for a real migration error, with at most 10 minutes of incremental preparation; do not broaden into other domain failures. No full suite, native driver, external search backend or LauncherApp fixture is required.

If another independent compile failure prevents the target, preserve the first diagnostic and defer that scope with its exact owner; do not claim the domain suite passes. Rollback is limited to this task's fixture adaptation; preserve unrelated work. Parent owns source branch, acceptance, Git and integration.

## Roles and dependencies

No G01 runtime dependency; it is independently useful for compiling the existing domain test target. Requested planner/reviewer: Astra/max; implementer: Astra/xhigh, subject to current user authorization and supported invocation. Record configured/requested/backend identity separately; do not edit model configuration. A small focused handoff and one implementation checkpoint are sufficient.

Completion means the named domain cases compile/run through the existing sender abstraction with review and truthful receipts. A passing fixture migration is not File Search feature-wide or native lifecycle acceptance.

Canonical copy promoted by parent from the G01 implementer role package; only the evidence link was rebased. This proposed follow-on is not part of accepted production work and has not been executed.
