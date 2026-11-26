# GPUI UI parity program index

## Current state

Read and follow the required [Execution Strategy](EXECUTION_STRATEGY.md) before planning, implementing, reviewing, verifying, or resuming migration work.

Execution is on `feat/gpui-ui-parity`. The G00-A baseline checkpoint is `3e67f7396f127d6dcb7bec432e9bb1684e9e6ae3`, pushed to `origin/feat/gpui-ui-parity`. The planning reference and execution parent are recorded separately in [Source baseline](SOURCE_BASELINE.md).

The current checkout integrates the Windows virtual-desktop fix from master at `00620851f`. G14 JSON and XML have retained hosts and saved pins with focused test evidence. Matched Windows replay remains open. The next implementation is the G14-C Regex Tester workspace. See [Checkpoints](CHECKPOINTS.md) and [Progress](PROGRESS.md) for evidence and continuation.

## Contracts and work

- [Master Goal](../../CODEX_GOAL_GPUI_UI_PARITY.md) and [complete user requirements](REQUIREMENTS.md).
- [Required Execution Strategy](EXECUTION_STRATEGY.md) and [Kickstart](../../CODEX_KICKSTART_GPUI_UI_PARITY.md).
- [Goal register](GOAL_REGISTER.md): all 27 initial goal packets and dependency order.
- [Source baseline](SOURCE_BASELINE.md) and [provisional discovery inventory](GUI_INVENTORY.md).
- [Frontend selection](FRONTEND_SELECTION.md), [core contracts](CORE_CONTRACTS.md), [design system](DESIGN_SYSTEM.md), and [component mapping](COMPONENT_MAPPING.md).
- [Validation strategy](VALIDATION_STRATEGY.md), [performance](PERFORMANCE.md), and [documentation protocol](DOCUMENTATION_PROTOCOL.md).

## Execution records

- [Progress](PROGRESS.md), [parity matrix](PARITY_MATRIX.md), [validation log](VALIDATION_LOG.md).
- [Known issues](KNOWN_ISSUES.md), [decisions](DECISIONS.md), [blockers](BLOCKERS.md), [checkpoints](CHECKPOINTS.md).
- [Final acceptance](FINAL_ACCEPTANCE.md).

## Record templates

- [GUI specification](templates/GUI_SPEC.md).
- [Checkpoint implementation packet](templates/CHECKPOINT_PACKET.md).
- [Evidence](templates/EVIDENCE_RECORD.md).
- [Review](templates/REVIEW_RECORD.md).
- [Continuation](templates/CONTINUATION_RECORD.md).

Create substantive gui/, evidence/ and reviews/ files during execution. The source discovery seeds are not a completed runtime inventory.
