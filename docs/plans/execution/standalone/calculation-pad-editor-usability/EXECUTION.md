# Calculation Pad editor usability

- Task branch: `feat/calculation-pad-editor-usability` in the existing checkout.
- Parent branch: `feat/utility-usability-and-insights`.
- Parent tip: `7528fa3c355ca9542d4aee789e609a15d2412e8e`, confirmed by the completed Goal's final chat handoff, tracked status ledger, and local and remote-tracking refs. The later Nextest cleanup was a one-off task.
- M0-A: `e64f1be076120cdf60f1dce11777f8d2b73915a6`. Its sole parent matches the recorded predecessor. The checkout and index were clean before branch creation and after baseline.
- Initial push: `git push -u origin HEAD` failed with exit 1. The connection to github.com:443 reported `Bad access`. Baseline is local.
- The Goal and Kickstart live on `master` at `1a9ad5499975f9d9b244fe7f8fa25cc66deb58ec`. Read them with `git show` from the task branch.

| Checkpoint | State |
| --- | --- |
| M0-A | Complete |
| M1-A | Complete, 5 targeted cases passed after the parser correction |
| M1-B | Complete, 3 targeted cases passed |
| M2-A | Complete, 7 targeted cases passed after the editor height correction |
| M2-B | Complete, 6 targeted cases passed |
| M3-A | Complete, targeted interaction and host cases passed after correction |
| M4-A | Pending |

## M1-A evidence

- Checkpoint `1e3043f20458851166bd56bcec5771606b896188` is local. Its ordinary upstream push failed with exit 1 (`github.com:443`, `Bad access`).
- Cargo Nextest binary preparation used `list --list-type binaries-only --message-format json -p multi_launcher --lib --cargo-profile fast-dev --offline` with two build jobs and a 900-second bound. Initial and corrected preparations exited 0. The owned runner reported surviving descendants at root exit and verified cleanup and output drain for both. Reused the resulting binary metadata for execution.
- Initial batch ran four exact cases. Full-line comments and contextual `mod` passed. The first nested `min` call and its executable example failed with `InvalidExpression`.
- Reused the existing contextual `mod` call lowering for balanced binary functions. Exmex receives equivalent parenthesized infix operands. Wrong arity and unfinished calls retain their error behavior.
- Corrected batch ran five cases in one test thread, with retries disabled, in 1.01 seconds wall time. Nested min/max, full-line comments, scalar `min` versus minutes, contextual `mod`, and the executable examples all passed.

## M1-B evidence

- Checkpoint `9a53eff4a4e1f77cd476f88179f4a4420409a28a` is local. Its ordinary upstream push failed with exit 1 (`github.com:443`, `Bad access`).
- Prepared the same library target with the M1-B change. Cargo exited 0 in 171.81 seconds, with verified descendant cleanup and output drain.
- The mixed rendered-results test and both existing Copy all/explicit copy tests passed in 1.10 seconds wall time. The new fixture keeps physical result IDs, zero, errors, dependencies, CRLF, and source-containing copy text.

## M2-A evidence

- Checkpoint `03edd2de4fec64a3830d0e65846c05624353ead9` is local. Its ordinary upstream push failed with exit 1 (`github.com:443`, `Bad access`).
- Fixed gutter positions come from the TextEdit galley. The existing editor scrolls horizontally by default and resets its horizontal offset when wrapping. A dedicated settings save owns the display fields and preserves open drafts and pending newer choices.
- Initial batch passed 6/7. The dock-to-popout test failed again in isolation after scrolling a pending-decision editor. Restored vertical shrinking for short documents while keeping horizontal overflow. The corrected batch passed 7/7 in 2.35 seconds, including migration, constrained geometry, editor undo/redo, accepted preferences, tolerant settings serialization, and settings save ownership. Three additional retained navigation, stale-host, and native-shaped popout cases passed before the height correction.
- Initial and corrected library preparation exited 0 in 297.48 and 79.69 seconds. The owned runner verified descendant cleanup and output drain.

## M2-B evidence

- Checkpoint `22897b39a4361d674e8c1f538de97ea3864b7aaf` is local. Its ordinary upstream push failed with exit 1 (`github.com:443`, `Bad access`).
- Dragging updates a retained host preview. Release persists one ratio through the existing display owner. Wide and stacked preferences default to 0.50 and 0.48 and remain independently validated.
- Library preparation exited 0 in 278.22 seconds with verified descendant cleanup and output drain. Six exact cases passed in 1.48 seconds wall time. Tests exercised both orientations, actual divider movement, release save, stale and discarded passes, constrained layouts, host migration, editor geometry, and settings compatibility.

## M3-A evidence

- The row menu now uses the row's actual bounds and selectable text responses. Go to source targets stable physical identity, applies caret and scroll in the retained editor pass, and paints a separate timed highlight.
- Initial batch passed 6/11. Three app-level cases aborted on default stack and were rerun individually with process-local `RUST_MIN_STACK=16777216`; native Inspector promotion passed and Inspector comparison/root query revealed clipping. The native child editor test also failed to focus. Corrected both wide panes' height budgets, and the expiry test now keeps editor focus while asserting selection and separately checks later focus behavior.
- Corrected batch passed 16/17, including all existing app/host checks and both splitter tests. The remaining new fixture's 25 lines fit with the corrected height. An 80-line fixture asserted the target started offscreen and passed its isolated menu/scroll/expiry case.
- All three preparations exited 0 (265.96, 167.25, 119.35 seconds). Owned-process cleanup and output drain were verified.
