# MkMacro OCR execution ledger

## Authority and baseline

- Original specification: `C:\Users\Jay\.codex\attachments\02cca2e0-bb2f-41c3-9e86-e868825d95ec\pasted-text-1.txt`. Read completely during planning. Its 122 numbered requirements and definitions of done remain authoritative; this ledger groups them into sequential implementation milestones.
- Repository policy: root `AGENTS.md`; the checked-out source is the architectural source of truth.
- Branch: `mkmacro-OCR`.
- Clean baseline: `565e977f` (also `origin/master` when work began).
- Testing strategy: front-load the vertical implementation; use compiler feedback at architectural boundaries; defer expensive Nextest execution until the complete slice and tests exist. Construction milestones may be `implemented_unverified`, but are not complete until M08/M09 verification succeeds.

## Cross-cutting architecture and invariants

1. The ownership chain is `SearchRegion -> ScreenCaptureBackend -> CapturedRegion -> OcrBackend -> OcrDocument -> matching -> desktop OcrMatch -> executor/editor`. OCR never enters `ScreenBackend::find_image`, and WinRT types remain inside the production adapter.
2. `Windows.Media.Ocr` is the only production recognizer. Keep `windows = 0.58`; add only `Globalization`, `Graphics_Imaging`, `Media_Ocr`, `Foundation_Collections`, and the buffer-transfer features actually required. No Tesseract, cloud call, service, watcher, screen index, or persisted screenshot.
3. Capture the region once per recognition attempt, then tile the immutable RGBA frame when either dimension exceeds `OcrEngine::MaxImageDimension`. Preserve signed virtual-desktop coordinates, checked translations, overlap, spatial deduplication, deterministic line reconstruction, and negative monitor origins.
4. Platform-neutral OCR document, normalization, matching, tiling, and result types are pure/testable. Stored/read text preserves Unicode and logical lines; search text normalizes whitespace while maintaining byte-range-to-word mappings and original recognized case for output.
5. OCR conditions are immediate single evaluations. Only `WaitUntil` polls conditions. `timeout_ms == 0` uses `MkWaitOptions::timeout_duration()` and has no deadline. OCR-bearing waits recursively enforce a 100 ms minimum; unrelated waits retain existing behavior.
6. OCR search attempts atomically refresh `last_ocr_*` and configured outputs. Misses set found false/count zero and clear selected text/coordinates with `MkValue::Null` or built-in removal. Read Text does not update search-result built-ins.
7. Authoring remains transactional. Test OCR and language discovery are lazy one-shot worker operations keyed by macro, step, condition path, and draft generation. Egui never recognizes text or enumerates capabilities every frame.
8. OCR desktop debug rendering is bounded and batched into a fixed/small number of transparent, non-activating painted overlays. The existing four-native-edge-windows-per-outline path must not be multiplied by OCR words. Normal playback never schedules debug geometry.
9. Schema 12 documents and packages remain importable and normalize to schema 13. Package canonicalization must be version-aware because current validation requires exact current-schema JSON. OCR introduces no image assets or new persistence store.
10. Exactly one implementation writer owns repository modifications at a time. Each commit is preceded by status/diff/diff-check inspection and contains only intentional files.

## Pipeline status

| ID | Milestone | Depends on | Status | Commit / verification |
| --- | --- | --- | --- | --- |
| M01 | Persisted OCR model, schema 13, built-ins, and pure matching types | baseline | complete | 9340dd36; focused and full-suite verification passed |
| M02 | OCR backend, Windows adapter, tiling, capture service, and pure tests | M01 | complete | 9340dd36; Windows 0.58 adapter type-checks; focused and full-suite verification passed |
| M03 | Executor actions, conditions, outputs, polling, cancellation, and capabilities | M02 | complete | 9340dd36; focused and full-suite verification passed |
| M04 | Validation, typed fields, static analysis, compiler/runtime metadata, and package compatibility | M03 | complete | 9340dd36; focused and full-suite verification passed |
| M05 | Catalog, transactional OCR editors, shared region routing, language jobs, and Test OCR preview | M04 | complete | 9340dd36; focused and full-suite verification passed |
| M06 | Bounded batched OCR debug overlay and authoring integration | M05 | complete | 9340dd36; focused and full-suite verification passed |
| M07 | Comprehensive test expansion, compile stabilization, and existing-test migration | M06 | complete | d40e9a7a; 45/45 OCR Nextest and 51/51 MkMacro integration tests passed |
| M08 | Targeted verification, formatting/checks, and full authoritative Nextest | M07 | complete | `cargo fmt --all -- --check`, `cargo check --all-targets`, and `git diff --check` passed; final committed-tree rerun: 3,464/3,464 passed, 7 skipped |
| M09 | Independent review, remediation, final verification, and ledger completion | M08 | complete | ed512943; independent reviewer approved the remediated diff; 54/54 OCR, 5/5 schema-12, nested monitor routing, and full 3,464-test Nextest passed |

## M01 — Persisted model and pure matching foundation

**Objective:** introduce focused OCR configuration/payload/result types and schema 13 without runtime behavior.

**Likely ownership:** `src/mkmacro/model.rs`, `src/mkmacro/variables.rs`, new `src/mkmacro/ocr.rs` or `src/mkmacro/ocr/{mod,matching}.rs`, `src/mkmacro/mod.rs`, `src/mkmacro/store.rs` for the additive document migration.

**Required behavior:** typed Auto/tag language, Contains/WholeWordPhrase/Regex, First/Nth, shared search spec, focused Find/Click/Read payloads, optional normalized outputs, `OcrTextSearch`, action variants, `last_ocr_*`, document/line/word/bounds/search-result types, Unicode-safe logical normalization, byte-range word mapping, match bounds/centers and recognized matched text. Bump schema 12 to 13 with a no-content-rewrite migration.

**Invariants:** no WinRT/egui dependency in pure types; no new `MkValue` variants; zero-length regex matches cannot invent coordinates; Nth is one-based and match count remains the total even when selection misses.

**Construction gate:** formatting of touched files, focused pure unit tests where useful, exhaustive `rg` inventory, and compile errors captured for downstream owners. Suggested commit: `feat(mkmacro): add OCR domain foundation`.

## M02 — Production backend and recognition service

**Objective:** implement one reusable capture/recognize/tile/merge service behind `OcrBackend`.

**Likely ownership:** OCR modules, `Cargo.toml`, `Cargo.lock`, narrow screen geometry helpers only if necessary.

**Required behavior:** available languages, max dimension, Auto/explicit engine resolution with clear unavailable-language diagnostics, per-thread balanced WinRT MTA initialization, checked RGBA-to-BGRA SoftwareBitmap transfer, blocking `RecognizeAsync(...).get()` inside adapter, platform-neutral conversion, bounded lazy engine cache, non-Windows unsupported backend. Capture one frame; tile with documented overlap; checked local-to-desktop translation; spatial/text deduplication; stable line reconstruction; cancellation before/after capture and each OCR call.

**Invariants:** no WinRT outside adapter; no recapture per tile; no filesystem/network; no OCR work at startup/idle; distinct identical text at different locations survives deduplication.

**Construction gate:** focused matching/tiling/adapter-helper tests, formatting, and `cargo check` if the adapter API needs compiler stabilization. Suggested commit: `feat(mkmacro): add Windows OCR recognition service`.

## M03 — Executor vertical integration

**Objective:** execute OCR through existing runtime, wait, variable, input, and diagnostic ownership.

**Likely ownership:** `src/mkmacro/executor.rs`, `src/mkmacro/runtime.rs`, production/fake backend constructors, capability/step-outcome owners.

**Required behavior:** Find Text, Click Text, Read Text, immediate Text Search condition, nested condition use, WaitUntil-based appearance/disappearance, interpolation timing, finite/indefinite polling, Continue/Fail policies, selected occurrence/count, atomic explicit outputs and built-ins, stale clearing, click center/offset/finalization/input cleanup, Read Text line preservation, concise outcomes, structured failures.

**Invariants:** one capture/recognition per condition evaluation/poll; cancellation wins before further processing/click; no implicit activation; no real input in tests; existing visual actions are unchanged.

**Construction gate:** fake backend compiles through unsupported/production/runtime wiring, `cargo check`, formatting, diff inspection. Suggested commit: `feat(mkmacro): execute OCR automation actions`.

## M04 — Static integration and compatibility

**Objective:** make every exhaustive domain owner understand OCR and preserve old files/packages.

**Likely ownership:** `validation.rs`, `authoring_fields.rs`, `authoring_analysis.rs`, `compiler.rs`, `structure.rs`, `editor_mutation.rs`, `templates.rs`, `package.rs`, store/probe paths, capability metadata.

**Required behavior:** structural language validation without live enumeration, nonempty/interpolated/static-regex/Nth/output/region validation, recursive OCR wait-minimum detection, typed template reads and variable writes, producer types/availability, condition traversal, schema-12 package version-aware canonical import and schema-13 export, no OCR assets/dependencies.

**Invariants:** no OCR during validation/analysis; built-ins remain read-only; package future schemas still reject; schema-12 model semantics remain unchanged; no wildcard silencing of exhaustive owners.

**Construction gate:** exhaustive `rg "MkAction::|MkCondition::"` review, package/store focused unit checks where useful, `cargo check`, formatting. Suggested commit: `feat(mkmacro): integrate OCR authoring metadata and compatibility`.

## M05 — Catalog and transactional authoring

**Objective:** expose complete OCR authoring and asynchronous in-editor testing through existing picker/editor lifecycles.

**Likely ownership:** `action_catalog.rs`, `condition_editor.rs`, `action_editor.rs`, new focused OCR controls/test-job/preview modules, generic SearchRegion controls, window/rectangle routing, dialog construction/injection.

**Required behavior:** five Visual catalog entries with exact defaults and discoverability; dedicated editor kinds; concise labels; full search/read/condition controls; shared Desktop/Monitor/Rectangle/Window/ClientArea authoring; explicit Add Activate Window Before; lazy installed-language discovery/refresh; asynchronous Test OCR with stable identity; in-memory image/text/geometry/match preview; Apply validation and Cancel isolation.

**Invariants:** no per-frame language/monitor capability lookup introduced; stale completions cannot affect new drafts; Test OCR does not block egui or persist captures; committed actions do not activate implicitly.

**Construction gate:** editor lifecycle and catalog unit tests as needed, formatting, `cargo check`, diff inspection. Suggested commit: `feat(mkmacro): author and test OCR actions`.

## M06 — Efficient OCR desktop overlay

**Objective:** add authoring-only desktop OCR geometry visualization without native-window explosion.

**Likely ownership:** `visual_overlay.rs`, `visual_overlay_windows.rs`, OCR preview/test result integration.

**Required behavior:** platform-neutral plan includes region, recognized lines/words, selected match and styles; `MAX_OCR_DEBUG_RECTS` cap always preserves region/selected match and reports truncation; native renderer uses a fixed/small number of full-surface transparent non-activating windows and paints many primitives.

**Invariants:** never one native window per OCR rectangle; overlay is mouse-transparent/non-activating; signed coordinates remain correct; normal runtime never invokes it.

**Construction gate:** pure planning/cap/flag tests, formatting, `cargo check`, diff inspection. Suggested commit: `feat(mkmacro): add bounded OCR debug overlays`.

## M07 — Test expansion and stabilization

**Objective:** add the full deterministic regression matrix and migrate existing exhaustive/schema/catalog expectations.

**Coverage:** pure matching and Unicode; tiling/coverage/dedup/order/negative coordinates; fake backend behavior; Find/Click/Read runtime, cancellation and timeout-zero; recursive conditions and polling ownership; variables/analysis; serialization/migration/package; editor catalog/apply/cancel/pickers/language/job staleness; overlay planning. Extend existing binaries/modules rather than adding a new top-level test binary. Real installed-language OCR smoke tests, if any, are explicit manual/environment-aware tests and never authoritative.

**Gate:** `cargo check`, compile all tests, then focused OCR filters only after the vertical slice exists. Suggested commit: `test(mkmacro): add OCR regression coverage`.

## M08 — Authoritative verification

Run focused actual targets discovered in the tree, then `cargo fmt --all --check`, `cargo check`, `git diff --check`, and finally `cargo nextest run --no-fail-fast`. Capture exact counts. Group and remediate failures before another full run. Do not declare earlier milestones complete until their acceptance coverage passes.

## M09 — Independent review and completion

Assign a reviewer that did not implement the primary slice. Review the full specification, ledger, cumulative diff, surrounding source, and tests for architecture leakage, runtime semantics, coordinates, tiling, UI lifecycle, privacy, performance, and compatibility. Resolve every substantive finding, rerun focused checks and any full suite affected by remediation, inspect final Git state, update every milestone and commit hash, and provide the exact required final-report headings.

The independent review found and the remediation commit resolved: OCR-condition editor panics; bypassable Apply validation; stale/unowned Test OCR previews and errors; overlapping test-job admission; lossy direct window-picker synchronization; missing condition feedback and path-specific monitor identification; state-blind language diagnostics; stale/ineffective Auto-engine caching; missing monitor availability validation; unreliable native overlay hit-test transparency; and insufficient schema-12 compatibility fixtures. A final focused re-review approved the resulting diff. Native interactive smoke testing was unavailable because this host exposed no native application-control surface; deterministic Windows compilation and adapter/helper tests remain authoritative here.
