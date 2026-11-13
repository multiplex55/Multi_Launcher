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
| M2-A | Pending |
| M2-B | Pending |
| M3-A | Pending |
| M4-A | Pending |

## M1-A evidence

- Checkpoint `1e3043f20458851166bd56bcec5771606b896188` is local. Its ordinary upstream push failed with exit 1 (`github.com:443`, `Bad access`).
- Cargo Nextest binary preparation used `list --list-type binaries-only --message-format json -p multi_launcher --lib --cargo-profile fast-dev --offline` with two build jobs and a 900-second bound. Initial and corrected preparations exited 0. The owned runner reported surviving descendants at root exit and verified cleanup and output drain for both. Reused the resulting binary metadata for execution.
- Initial batch ran four exact cases. Full-line comments and contextual `mod` passed. The first nested `min` call and its executable example failed with `InvalidExpression`.
- Reused the existing contextual `mod` call lowering for balanced binary functions. Exmex receives equivalent parenthesized infix operands. Wrong arity and unfinished calls retain their error behavior.
- Corrected batch ran five cases in one test thread, with retries disabled, in 1.01 seconds wall time. Nested min/max, full-line comments, scalar `min` versus minutes, contextual `mod`, and the executable examples all passed.

## M1-B evidence

- Prepared the same library target with the M1-B change. Cargo exited 0 in 171.81 seconds, with verified descendant cleanup and output drain.
- The mixed rendered-results test and both existing Copy all/explicit copy tests passed in 1.10 seconds wall time. The new fixture keeps physical result IDs, zero, errors, dependencies, CRLF, and source-containing copy text.
