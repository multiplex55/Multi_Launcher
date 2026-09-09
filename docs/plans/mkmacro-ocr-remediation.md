# MkMacro OCR Remediation Ledger

This ledger tracks the numpad regression fix and the source-backed OCR compliance audit on branch `mkmacro-OCR`. It supplements, and does not replace, the historical `docs/plans/mkmacro-ocr.md` plan. The working tree was clean when this initiative began.

Status vocabulary: `pending`, `implemented_unverified`, `verified`, `blocked`. Audit vocabulary: `satisfied`, `partial`, `missing`, `incorrect`, `obsolete`.

## Remediation status

| ID | Milestone | Status | Acceptance and verification |
| --- | --- | --- | --- |
| R01 | Numpad input-routing regression | implemented_unverified | Pre-`TextEdit` physical routing and focused frame coverage are implemented; targeted numpad, top-row, and query-history tests pass. Full-suite verification remains pending. |
| R02 | OCR compliance audit | implemented_unverified | Current source was audited independently; matrix below records evidence. Recheck evidence and status after remediation. |
| R03 | OCR condition ownership cleanup | implemented_unverified | Canonical recursive `MkCondition::contains_ocr()` now serves validation, executor outcomes, and editor lifecycle routing; focused tests pass. Full-suite verification remains pending. |
| R04 | OCR runtime capability reporting | implemented_unverified | `WaitUntil`, `If`, and `WhileStart` now derive support from their condition trees through an injected OCR-capability seam; deterministic nested tests pass. Full-suite verification remains pending. |
| R05 | Other discovered OCR gaps | pending | Expose Click Text failure policy in the editor and add direct OCR authoring-field traversal coverage. |
| R06 | Test expansion/migration | implemented_unverified | Numpad routing, top-row, focus/modifier/idle-probe/repeat, and history coverage is implemented; OCR remediation coverage remains pending. |
| R07 | Targeted verification | pending | `cargo fmt --all`, `cargo check --all-targets`, `git diff --check`, source searches, and focused Nextest groups pass. |
| R08 | Full verification | pending | `cargo fmt --all --check`, `cargo check --all-targets`, `git diff --check`, and `cargo nextest run --no-fail-fast` pass. |
| R09 | Independent review/remediation | pending | A non-implementing reviewer inspects numpad and OCR risks; findings are fixed and affected/full verification is repeated as required. |

## Ordered implementation plan

1. **Numpad routing:** add a typed, isolated Win32 key-state seam and a pre-`TextEdit` focused-query event router; remove ambiguous post-editor `NumN` routing; preserve `handle_key` selection math. Test classification, conservative adjacent Key/Text removal, no idle probe calls, frame-level keypad/top-row behavior, focus isolation, modifiers, arrows, and history. Commit as `fix(gui): restore physical numpad result navigation` after targeted verification.
2. **OCR condition capabilities:** move recursive OCR detection into `MkCondition`, migrate every caller, and make condition-bearing action capability metadata inspect nested conditions through a deterministic platform seam. Test direct and nested `All`/`Any`/`Not` trees plus non-OCR trees. Commit as `refactor(mkmacro): centralize OCR condition capabilities` after targeted verification.
3. **Additional audited gaps:** add the Click Text failure-policy editor control and direct OCR authoring-field traversal regression coverage without changing schema or runtime semantics. Commit as `fix(mkmacro): remediate OCR compliance gaps` only if substantive changes remain separate from milestone 2.
4. **Stabilize and verify:** format, check all targets, inspect searches/diff, run focused Nextest filters, then the full Nextest suite late in the initiative.
5. **Review and close:** obtain independent read-only review, remediate findings, rerun affected/full verification as required, finalize this ledger, commit final intentional changes, and confirm a clean worktree.

Invariants: schema 13 remains current; schema-12 documents/packages remain compatible; no OCR engine/capture rewrite; no new dependency, global hook, thread, timer, idle key polling, idle/background/per-frame OCR, captured-image persistence, or normal-level recognized-text logging; manual OCR smoke testing is not a completion gate.

## OCR requirements matrix

| Requirement | Source owner | Implementation evidence | Test evidence | Status | Remediation |
| --- | --- | --- | --- | --- | --- |
| Windows OCR backend | `mkmacro/ocr.rs` `WindowsOcrBackend` | Windows.Media.Ocr adapter is isolated behind `OcrBackend` | Backend and OCR module tests | satisfied | Preserve. |
| Backend abstraction | `mkmacro/ocr.rs` | Platform-neutral `OcrBackend`, `OcrDocument`, words/lines | Fake-backend recognition tests | satisfied | Preserve boundary. |
| Language selection | `mkmacro/ocr.rs`, `model.rs` | Auto and explicit installed-language paths | Language/backend tests | satisfied | Preserve. |
| Available language discovery | `mkmacro/ocr.rs`, `ocr_controls.rs` | Lazy explicit enumeration and diagnostics | Editor/job tests | satisfied | Preserve lazy behavior. |
| SearchRegion reuse | `mkmacro/screen.rs`, `model.rs` | OCR search embeds shared `SearchRegion` | Region validation/capture tests | satisfied | No duplicate region type. |
| Capture path | `mkmacro/ocr.rs`, `screen.rs` | One shared `ScreenCaptureBackend` capture per attempt | Capture-count/recognition tests | satisfied | Preserve single capture. |
| Matching modes | `mkmacro/ocr.rs` | Contains, whole phrase, regex, case toggle | Matching-mode tests | satisfied | Preserve. |
| Whitespace/multiline semantics | `mkmacro/ocr.rs` | Logical normalization and reconstructed lines | Whitespace/cross-line tests | satisfied | Preserve. |
| First/Nth | `mkmacro/ocr.rs`, `model.rs` | First and 1-based Nth selection | Occurrence tests | satisfied | Preserve. |
| Match count | `mkmacro/ocr.rs`, `executor.rs` | Search results expose deterministic count | Matching/output tests | satisfied | Preserve. |
| Tiling | `mkmacro/ocr.rs` | Max-dimension tiles, overlap, translation, deduplication, reading order | Tiling/dedup/cross-tile tests | satisfied | Preserve. |
| Negative coordinates | `mkmacro/screen.rs`, `ocr.rs` | Signed desktop coordinate translation | Screen/OCR negative-coordinate tests | satisfied | Preserve. |
| Find Text | `model.rs`, `executor.rs`, `action_editor.rs` | `OcrFindText` runtime/editor path | Executor/editor tests | satisfied | Preserve. |
| Click Text | `model.rs`, `executor.rs`, `action_editor.rs` | Runtime supports button/count/offset and not-found policy | Executor tests; editor policy coverage absent | partial | Add failure-policy editor control/test. |
| Read Text | `model.rs`, `executor.rs` | One pass, Unicode/lines, empty success; leaves search built-ins alone | Read-text tests | satisfied | Preserve. |
| Wait for Text | `action_catalog.rs`, `executor.rs` | `WaitUntil(OcrTextSearch found=true)` preset uses shared waiter | Wait/runtime tests | satisfied | Preserve. |
| Wait for Text to Disappear | `action_catalog.rs`, `executor.rs` | `WaitUntil(OcrTextSearch found=false)` preset | Wait/runtime tests | satisfied | Preserve. |
| OCR conditions | `model.rs`, `executor.rs` | Immediate OCR leaf works in If/While/WaitUntil/All/Any/Not | Nested/short-circuit/capture-count tests | satisfied | Centralize metadata ownership only. |
| Interpolation | `interpolation.rs`, `executor.rs` | OCR search fields pass through existing interpolation | Executor/interpolation tests | satisfied | Preserve. |
| Output variables | `model.rs`, `executor.rs`, `authoring_analysis.rs` | Found/text/point/x/y/count typed outputs | Output and authoring-analysis tests | satisfied | Preserve. |
| `last_ocr_*` built-ins | `variables.rs`, `executor.rs` | Typed found/text/x/y writes | Built-in tests | satisfied | Preserve. |
| Stale clearing | `executor.rs` | Failed later searches clear configured and built-in stale values | Stale-output tests | satisfied | Preserve. |
| `timeout_ms = 0` | `executor.rs` | Existing waiter interprets zero as indefinite | Timeout-zero/cancellation tests | satisfied | Preserve. |
| Polling minimum | `validation.rs`, `action_catalog.rs` | OCR default 250 ms; nested OCR minimum 100 ms | Validation/catalog tests | satisfied | Migrate to canonical `contains_ocr`. |
| Cancellation | `executor.rs` | Wait loop checks run control | Wait cancellation tests | satisfied | Preserve. |
| Editor controls | `action_editor.rs`, `condition_editor.rs`, `ocr_controls.rs` | Search/language/region/wait/output/click controls present | Editor tests | partial | Add Click Text failure policy. |
| Test OCR async lifecycle | `ocr_test_job.rs`, `action_editor.rs` | Worker job, stable identity, stale-result rejection, image/text/geometry/match preview | Job/editor tests | satisfied | Preserve. |
| Preview geometry | `ocr_test_job.rs`, `image_preview.rs` | Captured image and word/line/selected-match geometry | Preview/job tests | satisfied | Preserve. |
| Desktop debug overlay | `visual_overlay.rs`, `visual_overlay_windows.rs` | Authoring-only, bounded 256 rects, non-activating transparent surfaces | Overlay-planning tests | satisfied | Preserve. |
| Validation | `validation.rs` | OCR search/language/region/output/wait validation | Validation tests | satisfied | Use canonical domain helper. |
| Authoring field traversal | `authoring_fields.rs` | OCR template/language/region/output traversal implemented | Direct OCR traversal coverage absent | partial | Add direct nested/action regression tests. |
| Authoring analysis | `authoring_analysis.rs` | OCR output types and condition paths included | OCR analysis tests | satisfied | Preserve. |
| Runtime capability reporting | `executor.rs` `has_runtime_support` | Direct actions gated; condition-bearing actions ignore OCR tree | No deterministic nested capability tests | incorrect | Add condition capability evaluator and migrate If/While/WaitUntil. |
| Schema 12 -> 13 migration | `store.rs` | Additive migration to schema 13 | Store migration tests | satisfied | No schema bump. |
| Package compatibility | `package.rs` | Schema-12 import, schema-13 round trip, future rejection, no OCR assets | Package tests | satisfied | Preserve. |
| Privacy | OCR/editor/runtime modules | No capture persistence, cloud request, or normal-level recognized-text logging found | Source audit; deterministic persistence tests where applicable | satisfied | Recheck diff/review. |
| Idle-performance contract | OCR/editor/runtime modules | OCR runs only for actions/conditions/Test; language enumeration explicit | Capture/job/runtime tests plus source audit | satisfied | Recheck diff/review. |

## Audit findings requiring changes

- Recursive OCR-condition detection is duplicated in `mkmacro/validation.rs` and `mkmacro/executor.rs`, while the editor reaches through validation. Move the semantic question to `MkCondition`.
- `executor::has_runtime_support` treats `WaitUntil`, `If`, and `WhileStart` as supported without considering nested OCR. Add deterministic condition capability traversal.
- `MkOcrClickPayload::not_found_policy` is persisted and executed but is not exposed by the Click Text editor. Add the symmetric control.
- OCR authoring-field traversal exists but lacks direct regression coverage. Add tests without changing traversal semantics.

No other substantive OCR implementation gap was found in the source audit. Correct backend, capture, matching, tiling, runtime, persistence, overlay, privacy, and performance behavior should remain architecturally intact.
