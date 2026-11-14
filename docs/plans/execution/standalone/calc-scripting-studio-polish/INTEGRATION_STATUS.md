# Calc scripting and studio polish execution

- Parent branch `feat/calculation-pad-editor-usability`, tip `7e55f5299122228e6d218118c4816f5f251cef46`. The preceding standalone Goal's archived execution handoff calls for its final tip. The local and remote-tracking branches agree, and the primary branch has integrated this exact tip. The predecessor reports limited native acceptance.
- Active branch `feat/calc-scripting-studio-polish` in the existing checkout. The starting checkout and index were clean.
- M0-A baseline `a2ced169aa3962ac2295b31494fe17dd70875cf3`, verified as a direct child of the recorded parent. `git push -u origin HEAD` succeeded and set upstream to `origin/feat/calc-scripting-studio-polish`.
- The Goal and Kickstart are available via `git show 93261f1b66c9462b7bb269f40e19cea9b074a632:docs/plans/CODEX_GOAL_CALC_SCRIPTING_STUDIO_POLISH.md` and the corresponding Kickstart path. That commit was on the initial checkout.

| Checkpoint | State |
| --- | --- |
| M0-A | Complete and pushed |
| M0-B | Complete and pushed at `8249b28fa91bc603a4067db6b7d90cc4adad150a`, 8 focused baseline tests passed |
| M1-A | Complete and pushed at `7ef7818fb2e1577d08b1bcbeb9fd33ff5530e347`, 29 focused tests passed after review fixes |
| M1-B | Complete at the domain and GUI-test level, 73 focused tests passed plus 1 exact codec rerun. UI-7 pending. |
| M2-A through M6-A | Pending |

## M1-A evidence

- Editable flowchart parsing now uses original source spans for semicolon statements and bounded arrow chains. It accepts `graph`, ordinary comments and direction tokens RL/BT. The normal editable geometry and comment-safe visual round trip are owned by M1-B.
- The selected parser, assistance, style, stale-draft, codec and renderer batches passed 28/28 initially and 29/29 after review corrections. Cases include Unicode/CRLF diagnostics, quoted and pipe labels, color style separators, the edge limit reached mid-chain, and non-flowchart preview regressions. Each batch used the 100-second watchdog with verified cleanup and output drain.
- Independent review fixes cover hex colors, header whitespace, and subgraph assistance at semicolon boundaries. A remaining Unicode subgraph-whitespace assistance case is assigned to M1-B. Touched-file rustfmt checks confirmed inherited formatting differences in `document.rs`, `model.rs`, `session.rs`, and `style.rs`.

## Affected baseline

- Calc bindings: `src/calculation_pad/document.rs` owns preparation, definition and reference resolution, and evaluation. `session.rs` attaches stable line IDs and owns cached evaluations, frozen comparison context, and dependency snapshots. Both document and comparison resolution currently rebuild duplicate-name definitions.
- Pad editor effects: `src/gui/calculation_pad_dialog.rs::complete_pad_host` checks retained viewport pass, live host, document incarnation, and revision. It applies source before resolving accepted result actions.
- Mermaid: `src/diagram/mermaid.rs` owns bounded parsing and formatting. `session.rs::apply_transaction` validates a regenerated candidate before publication. `rendering.rs` routes source-rendered families separately. Legacy codec snapshots include stale drafts which can become valid under the expanded grammar.
- QR: `src/gui/qr_dialog.rs` owns fresh reset, raster/texture cache, and current immediate viewport effects. The migration joins `HostedUtilityDialog` through the launcher command, panel lifecycle, rendering, and QA adapters, retaining the existing `qr-generator` size key.
- Nextest 0.9.135 is available. Use the `fast-dev` library binary and bounded Windows process runner. Reuse prepared Nextest binary metadata so the 100-second watchdog applies to each execution batch separately from compilation.
- Library preparation used `cargo nextest list --list-type binaries-only --message-format json -p multi_launcher --lib --cargo-profile fast-dev --offline`. Cargo exited 0. The owned runner reported descendants present at root exit and verified cleanup and output drain. The emitted binary metadata listed and ran the selected tests successfully.
- Nextest selected eight cases covering legacy binding diagnostics, cached evaluation, Mermaid grammar, assistance and stale preview, retained same-pass Pad copying, and QR fresh reset and cache reuse. All eight passed in 0.245 seconds with one test thread, retries disabled, and a 100-second batch watchdog. No app-level fixtures were constructed.
- Mermaid syntax was cross-checked against the [official flowchart reference](https://mermaid.js.org/syntax/flowchart.html).

## M1-B evidence

- Visual transactions preserve full-line comment text and order using source-derived anchors. Deleted targets leave standalone comments. The formatter checks combined source bounds before publishing a transaction. Reverse direction controls and explicit layout reach first-party scene and export geometry.
- The codec retains readable legacy stale drafts and history. The retained Refresh preview action re-admits now-supported exact source as one undoable operation. It leaves the saved file untouched until an explicit save.
- Nextest passed 59/59 in the first batch and 73/73 after review corrections, including comment order and limits, reverse arrowheads, source/image export ownership, legacy Refresh preview, and Notes/non-flowchart regressions. The sharpened legacy codec guard then passed 1/1 in isolation. Batches used a 100-second execution watchdog. The batches with an app fixture used a process-local 16 MiB test-thread stack. The touched Notes app fixture now uses a temporary profile and already disables Paste Sequence shortcuts.
- Review findings on exact-source activation, legacy guard specificity, trace traversal and header help were corrected. UI-7 remains scheduled for native verification.

| Requirement | Checkpoints | Implemented | Automated verification | Native verification |
| --- | --- | --- | --- | --- |
| MMD-01 | M1-A, M1-B | Yes | Passed | Pending |
| MMD-02 | M1-A | Yes | Passed | Pending |
| MMD-03 | M1-A | Yes | Passed | Pending |
| MMD-04 | M1-A | Yes | Passed | Pending |
| MMD-05 | M1-B | Yes | Passed | Pending |
| CP-01 | M2-A/C/D | Pending | Pending | Pending |
| CP-02 | M4-A/C | Pending | Pending | Pending |
| CP-03 | M3-B, M4-C | Pending | Pending | Pending |
| CP-04 | M3-B | Pending | Pending | Pending |
| CP-05 | M4-A/C | Pending | Pending | Pending |
| CP-06 | M3-A/C | Pending | Pending | Pending |
| CP-07 | M3-A/C | Pending | Pending | Pending |
| CP-08 | M3-B | Pending | Pending | Pending |
| CP-09 | M4-B/C | Pending | Pending | Pending |
| CP-10 | M3-C | Pending | Pending | Pending |
| CP-11 | M3-C | Pending | Pending | Pending |
| CP-12 | M3-A/C | Pending | Pending | Pending |
| CP-13 | M2-B | Pending | Pending | Pending |
| CP-14 | M2-B | Pending | Pending | Pending |
| CP-15 | M2-C/D | Pending | Pending | Pending |
| CP-16 | M2-B/C | Pending | Pending | Pending |
| QR-01 | M5-A | Pending | Pending | Pending |
