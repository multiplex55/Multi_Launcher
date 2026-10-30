# G06 R1 contingency — reject Jump/Paste Sequence shortcut conflicts

State: **RESOLVED_IN_G06** at corrected checkpoint `7cb16df951cb2eac1642fb428dbce84559f77562`; see [named-issue confirmation](../../R1_CONFIRMATION.md). Origin: [independent review R1](../../ROLE_RECORD.md). This pair is closed historical correction guidance and must not enter a future queue. The original reproduction and handoff below describe the rejected pre-correction source.

## Goal and source anchor

Make a newly configured enabled local Jump chord conflict-free against effective Paste Sequence shortcuts at the existing final merged-settings preparation boundary. Review anchor: branch `codex/closeout-g06-jump-settings-and-focus-20261029-r01`, checkpoint `b708e38adc47ef17e5598a771fd7e194c888166e`, base `d88f00b2375d7acaeb007e85b355a5cafb4d1030`. Revalidate the current branch/checkpoint before continuing; do not blindly replay this donor state after integration.

`src/hotkey/validation.rs::jump_chord_conflict` handles exact local modifiers versus inclusive global modifiers. `src/main.rs::resolve_hotkey_route_candidate_with_macro` currently runs the base validator before resolving sequences. `validate_paste_sequence_hotkey_routes` checks other global owners but omits Jump. The existing `paste_route_test_snapshot` and explicit sequence-binding resolver input permit deterministic tests without real settings stores or native input.

## Reproduction and required outcome

With Jump enabled at `Ctrl+Shift+F8`, Paste Sequences enabled, custom Cancel `Ctrl+F8`, Next disabled, radial disabled and no macros, current final resolution accepts both bindings. Its global trigger accepts extra Shift and dispatches cancellation on the local Jump chord. An enabled saved Start shortcut can collide by the same route. This conclusion follows the real route construction and dispatch code; it is not a claim of native observation.

The corrected final resolver must refuse this conflicting enabled Jump candidate before settings persistence. It must also check resolved saved Start routes. Disabled Jump, disabled Paste Sequences, absent optional shortcuts and routes requiring modifiers absent from the local chord must not produce a false conflict.

## Ownership and bounded implementation

One source owner performs one coherent correction using the existing shared validation helper and main-owned final resolver. Relevant files are `src/hotkey/validation.rs`, `src/main.rs` and their direct tests; `src/settings_editor/state.rs` and direct mapping tests may need a small reuse of the same decision for available Next/Cancel draft feedback. Choose minimal helper visibility/signature changes required by the library/binary boundary. No second shortcut registry, listener, hook, polling path or dispatch-time workaround.

Preserve existing optional-default route omission and unrelated conflict rules. Compare the effective applicable routes, not disabled or omitted declarations. Keep Jump out of `legacy_triggers`, shared native related bindings and global registrations. Preserve raw-draft retention, typed conversion, transaction ownership/rollback, acknowledgement-only live publication, release fences, schema/default fallback, and pending-editor drafts.

## Verification and completion

1. Add a direct `resolve_hotkey_route_candidate_with_macro` regression for the custom control collision and saved Start collision; exercise exact/local versus subset/global modifier behavior.
2. Include a compact set of disabled Jump/plugin/optional-shortcut and nonconflicting-required-modifier controls. Assert final resolution rejects the conflicting candidate instead of merely testing the helper. Preserve existing default-omission behavior with the nearest existing control.
3. List the exact selected tests before one focused Nextest run. Parent supplies the preparation/run budgets and approves any necessary binary-target preparation; do not silently broaden verification. Existing 25-test receipts remain historical evidence, not proof of this new path. No native test is required to prove this pure admission boundary.
4. Inspect the correction diff, source blobs and cleanup. Record any failure honestly. One named R1 confirmation follows; no broad second review or repeated implementation loop.

Done requires source-supported final-resolver rejection, passing targeted proof, unchanged registration and transaction contracts, and reviewer confirmation. Whole G06/native M3 acceptance remains separate. If the allowed correction cannot close R1, preserve work and this pair, report the exact remaining counterexample and stop source acceptance.

See [Kickstart](KICKSTART.md) for the restart sequence.
