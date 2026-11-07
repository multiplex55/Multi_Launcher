# Productivity Tools Refinement execution

Contract: `docs/plans/CODEX_GOAL_PRODUCTIVITY_TOOLS_REFINEMENT.md` and companion Kickstart.
One existing checkout; sequential source writers; no worktrees or auxiliary branches.

## Ancestry / M0-A

- Preparation: 2026-11-07; original clean branch `master`, HEAD/upstream `5824ae576ca20549f392177f0aee80df3264b3e0`.
- Parent branch: `test/productivity-native-acceptance-closeout`.
- Exact parent tip: `1d7180aa2d231b6569596624719961d6ca7c9a6f`, local and origin branch equal; Git object verified.
- Predecessor owner: recovery orchestrator; EXECUTION.md R06/M6-C records IMPLEMENTATION_COMPLETE_USER_VALIDATION_PENDING, with no queued implementation. Its native evidence is not adopted as this task's acceptance.
- Task branch: `feat/productivity-tools-refinement`, created directly from the exact parent; initial HEAD verified equal.
- Current master contains later documentation archival and the new contract. The contract files alone are intentionally carried into this baseline; historical archival is not replayed.
- No pre-existing staged, unstaged or untracked work. Initial sandbox Git lock denial was resolved by authorized escalation; no files discarded.
- Ignored predecessor receipt `target/desktop-productivity-completion-recovery/r06-m6-c-final-receipt.json` independently confirms exact final/upstream tip, clean tree, terminal agents and no queued implementation. Source/Cargo/build inputs compare unchanged between predecessor and preparation-time master (`git diff --quiet ... -- src Cargo.toml Cargo.lock build.rs`, exit 0).
- Baseline SHA: `fbf1f2d15cdf0db4e0415adcabfe0f996c6a9dd5`; its parent verified as the exact parent tip above.
- Initial push: `git push -u origin HEAD` succeeded; upstream `origin/feat/productivity-tools-refinement`.

## Ordered checkpoint plan

The detailed objective, ownership, scope, invariants, tests and done criteria in section F are the execution packets. Each row is one commit boundary after focused verification; native checks remain pending until M8-A. No broad suite is planned.

| Checkpoint | Deliverable | State |
| --- | --- | --- |
| M0-A | Verified direct-parent branch, own baseline, push receipt | complete |
| M1-A | Numeric dispatch, contextual aliases and infix remainder | complete |
| M1-B | Typed bounded vectors and result/reference consumers | pending |
| M1-C | Executable searchable catalogue and independent reference host | pending |
| M2-A | Checked working modes, grid groups, named bit spans | pending |
| M3-A | Bounded Deflate extraction | pending |
| M3-B | New-load and bulk physical member selection | pending |
| M3-C | Safe multi-source ZIP creation domain | pending |
| M3-D | ZIP creation UI and accepted chooser effects | pending |
| M4-A | Visual navigation, fonts, marquee and viewer sizing | pending |
| M4-B | Structural clipboard and keyboard ownership | pending |
| M4-C | Owned persistent inline labels | pending |
| M4-D | Persistent editable connector bends | pending |
| M4-E | Four additional editable templates | pending |
| M5-A | Timeout-capable UIA activation | pending |
| M5-B | Saved folder snapshot picker | pending |
| M6-A | Cold-cache recorder enumeration | pending |
| M6-B | Frozen-region preview and active indicators | pending |
| M7-A | Command-only help | pending |
| M7-B | Complete Recycle command boundary | pending |
| M8-A | Consolidated native flows, independent review, scoped lint, handoff | pending |

Use configured planner/implementer/reviewer roles, one explicitly bounded packet at a time. Parent owns Git, ledger and push attempts. One Cargo job at a time, fast-dev/targeted Nextest; disable Paste Sequence test hotkeys before application construction. Native/environmental retries follow section H. Final disposition requires actual native evidence and final SHA; pending checks are never reported passed.

## M1-A investigation

Planner confirmed quantity `parse_prefix` consumes decimal zero plus bit-unit evidence before scalar lowering for binary-first input. Fix locally rather than altering shared unit alias boundaries. Alias normalization must know declarations (including comparison evaluation context), preserve source and programmer routes, and recognize infix `mod` only as an operator. Implementer owns the three calculation modules and focused tests; no later packet source changes run concurrently.

### M1-A result

Implemented in document.rs and quantity.rs: declaration-aware aliases, one token pass producing scalar/quantity text and operator-only programmer text, prefixed numeric lowering before quantity evidence, contextual infix remainder. Parent review corrected RHS variable ambiguity (`7 mod mod + 1`); typed `%`/`mod` parity included. Source/CRLF/Unicode and bound-name behavior covered. No operators.rs change needed.

Initial `cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E 'test(calculation_pad::document::tests::) | test(calculation_pad::operators::tests::) | test(calculation_pad::quantity::tests::)'`: 38/39 passed, execution 0.421s after 3m27s compilation. Sole failure was a new quantity expectation with seven instead of existing six significant digits; actual `3.14159 m` was correct. Final affected rerun: 8/8 passed, 7605 skipped, execution 0.100s after 1m37s compilation. Initial inventory compilation took 7m41s. Full selection was not redundantly repeated. Exact final selector follows in checkpoint receipts. No application fixtures/native tests run.

Diff inspected by parent; git diff --check passed. Accidental cargo-fmt changes outside scope were proved identical to formatting HEAD and restored after automatic review initially rejected insufficiently demonstrated restoration safety; second evidence-backed restoration succeeded. Only intended two source files and this ledger remain changed. No user work discarded. Checkpoint commit/push pending parent action; native acceptance pending M8.

Final Nextest run ID: `3a287696-4626-493d-8a3e-f38748508010`. Command prefix: `cargo nextest run -p multi_launcher --cargo-profile fast-dev --lib --no-fail-fast -E`; exact selector:

```text
test(calculation_pad::document::tests::binary_first_and_contextual_scalar_aliases_preserve_source_and_bindings) | test(calculation_pad::document::tests::infix_mod_matches_) | test(calculation_pad::document::tests::pi_aliases_are_resolved_in_quantities_and_comparison_contexts) | test(calculation_pad::document::tests::supports_remainder_and_contextual_euclidean_mod_without_reserving_mod) | test(calculation_pad::document::tests::remainder_rejects_zero_and_non_finite_operands) | test(calculation_pad::document::tests::typed_references_can_adopt_large_exact_literals_without_changing_scalar_errors) | test(calculation_pad::document::tests::programmer_integer_operators_are_exact_and_select_explicit_modes)
```

## M1-B prepared handoff (dependency: accepted M1-A)

Add a private validated 1–128 finite-component vector type and a bounded structured expression owner under calculation_pad. Reuse the common scalar preparation/evaluation seam for components; do not recurse through whole document evaluation or substitute result strings. Ordinary scalar/quantity/programmer expressions retain their routes. Prepare vector candidates for explicit syntax and vector-valued references, preserving source spans and collecting dependencies; early scalar-error paths must not reject valid vector candidates. Reject quantity/Integer/ExactBase operands without coercion. Check intermediate arithmetic finiteness, dimension, zero division, nesting/node/work bounds; dot/cross are explicit products. Format each component through existing format_number.

Migrate exhaustive consumers in document/programmer/comparison; vector ordering is explicitly unsupported. Session persistence is source/line identities, requiring no schema change. Preserve Integer and LegacyExactBase Inspector actions while excluding vectors. Focus tests on required arithmetic/symbolic examples, invalid dimensions/types/work, dependency edits, one undo/redo/save/reopen fixture, rounded/exact formatted copy roundtrip, comparisons and Inspector exclusion. Necessary consumers include session and GUI copy tests. Run one scoped fast-dev Nextest selection with directly affected compatibility cases; native remains pending.

## M1-C prepared handoff (dependency: accepted M1-B)

Single catalogue owner remains examples.rs; add search metadata and expected-result display from its executable expectations. Search matches group/title/source/keywords, bypasses collapsed groups while active; pinned search/clear/host actions above one scrolling body including detailed references. Independent companion state belongs to CalculationPadDialog: own UtilityHostState, native viewport/tool geometry identity, query/scroll and help lifetime epoch. No second Pad session. Help effects carry host/pass/owner lifetime and apply only after retained ROOT and originating pass; process Pad retirement first. Close/reopen must advance a help epoch even when docked (retire_to_docked alone does not). Retire help only on actual Pad owner retirement, not canceled dirty-close decisions. Preserve search/scroll across host migration and reject stale close/copy. Reuse central CopyText dispatcher. Audit render.rs hidden-ROOT scheduling and ordinary query-focus availability when Pad is native but help docked, not merely layer registration. Register reference-specific UtilityToolKey in settings/utility_dialog_sizes.rs. Scoped catalogue/search/host tests plus narrow render input test; native remains M8.

## M2-A prepared handoff

InspectorDraft already owns model/target mode/generation; checked_cast is atomic and reinterpret_bits explicit. On width/signedness selection immediately attempt checked_cast; style active working mode distinctly from failed attempted target, and keep reinterpret targeting that failure. Remove redundant two-step checked-cast UI and migrate the existing render interaction around that action. Add draft-local Bit grouping (default 4; powers through width) separate from persisted output-string grouping. Preserve all bit indices and stable IDs with wrapped groups/fragments in narrow layouts. Collect actual cell rectangles and paint distinguishable overlapping/cross-group span fragments with inclusive labels; retain out-of-width annotations as invalid, without valid-span paint. Preserve accepted InspectorActions publication and Insert guards. Focus model/dialog tests and directly affected render interaction only; native remains M8.

## M3-A prepared handoff

Catalogue/planner already accept Deflate; writer validate_plan_layout selected-file guard rejects it. EntryLayout must carry compression and declared output size. Keep physical validation for every entry (including omitted records), metadata/descriptors/ZIP64/spans/overlap/source identity before destination mutation. Raw inflate in fixed buffers must feed the same plaintext stage-write/CRC/actual-quota/progress path as Stored, with cancellation, exact compressed consumption/stream end/no-progress checks, declared output bounds and CRC before publication. Keep stage abort/flush/no-clobber and partial-run reporting. Stored equal-size validation remains Stored-specific; directory payload validation must remain explicit.

Dependency rationale from local ZIP 9.1.2 inspection: raw decompressor is private; public ZipArchive duplicates metadata parsing, collapses names and allocates before application limits, and public streaming Drop reads payloads. A minimal direct flate2 dependency using already locked Rust backend is justified for raw Decompress with explicit bounded accounting. No pathname extraction or parser replacement. Replace deliberate Deflate rejection test with actual content/CRC success while retaining separate malformed-layout cases; test multi-chunk/mixed/empty/descriptor/ZIP64, truncation/corruption/size/CRC/quota/cancel cleanup and earlier-publication preservation at the new boundary. Scoped writer selection plus a session preview boundary case if necessary; native pending M8.

## M3-C prepared handoff

Creation-specific draft/frozen-manifest/validation/job/session types belong under archive; do not overload ExtractionPlan. Reuse SafeRelativePath/ValidatedRelativePath, windows_component_cmp/windows_relative_cmp, NativeArchiveDestination held-parent/probe, create_stage, OwnedStage write_chunk/flush_final/publish/abort, ScanCancellation/ArchiveIoControl and joined worker patterns. Narrowly factor ordinary-file held/no-follow admission from NativeArchiveSource (ancestor/reparse/type/volume/identity/length checks), without archive-specific catalog state. Existing bounded metadata scan seam may help; do not expose another subsystem wholesale.

Pinned ZIP exposes ZipWriter::new_stream<W: Write>: use a narrow bounded cancellable Write facade over stage, not Seek. Keep writer lifetime/finalization separate from publication because Drop attempts metadata completion. Explicit finish, release writer, cancellation check, flush_final, one no-clobber publication. Output/input byte limits are creation-specific. Draft rows have stable IDs/revisions and editable relative destinations; files default basename, folders own name, empties preserved. Bound recursive entry/depth/path/work/metadata collection during scan. Freeze identity/size/change evidence, revalidate held sources before/after reads, reject changes/reparse/device/unsafe aliases. Detect Windows-equivalent names, file/directory and file-as-ancestor conflicts with source-row feedback, no renaming. Reject pre-existing output and output/stage inside selected source tree before stage creation, revalidate ancestry/publication. Abort owned stages only; report cleanup failures; join retirement before next admission. Test multi-root/Unicode/empty folder, conflict editing, unsafe inputs/limits, competitor/recursion/alias safety, changed sources, streaming/finalization cancellation, and created-ZIP catalogue/extraction byte roundtrip with unchanged sources. Depends M3-A; focused tests only.
