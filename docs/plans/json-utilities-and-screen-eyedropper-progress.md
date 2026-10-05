# Goal A execution record

Active plan: [JSON utilities and screen eyedropper](json-utilities-and-screen-eyedropper.md).
Implementation branch: `json-and-eye-dropper`. Initial working tree was clean.

| Checkpoint | Status | Evidence |
| --- | --- | --- |
| M1-A shared JSON transformations | complete | Shared ordered parser; strict JSON and CM compatibility tests pass. |
| M1-B JSON commands | in_progress | Typed command bus and built-in registration inspected. |
| M1-C JSON utility | pending | Existing dialog and panel lifecycle inspected. |
| M2-A shared color conversion | pending | Existing color plugin inspected. |
| M2-B frozen desktop picker | pending | [Native handoff](json-utilities-and-screen-eyedropper-native-handoff.md) reconciled capture, parking, and ROOT lifecycle. |
| M2-C launcher integration | pending | Depends on picker runtime. |
| M3-A qualification and documentation | pending | Targeted verification and native smoke pass. |
| Independent review | pending | After integration and targeted verification. |

## Decisions

- Preserve JSON object ordering locally; do not change global `serde_json` features.
- Reuse the MkMacro virtual-desktop compositor and generalize existing capture-safe parking narrowly.
- Sequential source writers; checkpoint commits owned by the orchestrator.
- Preserve existing Clipboard Modify operation IDs and color result generation.

## Verification

A full repository suite is not required by this goal.

- M1-A: targeted library Nextest run passed 8 tests covering `json_transform::` and operation ID serialization; exact Clipboard Modify JSON parity test passed separately (1 test). Formatting and diff whitespace checks passed.
