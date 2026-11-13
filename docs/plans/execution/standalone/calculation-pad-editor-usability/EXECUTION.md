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
| M4-A | Limited. Implementation, automated verification and review remediation complete. Windows interaction unverified. |

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

- Checkpoint `d95b9937161ba46e48467ed313a5614c6e43e235` is local. Its ordinary upstream push failed with exit 1 (`github.com:443`, `Bad access`).
- The row menu now uses the row's actual bounds and selectable text responses. Go to source targets stable physical identity, applies caret and scroll in the retained editor pass, and paints a separate timed highlight.
- Initial batch passed 6/11. Three app-level cases aborted on default stack and were rerun individually with process-local `RUST_MIN_STACK=16777216`; native Inspector promotion passed and Inspector comparison/root query revealed clipping. The native child editor test also failed to focus. Corrected both wide panes' height budgets, and the expiry test now keeps editor focus while asserting selection and separately checks later focus behavior.
- Corrected batch passed 16/17, including all existing app/host checks and both splitter tests. The remaining new fixture's 25 lines fit with the corrected height. An 80-line fixture asserted the target started offscreen and passed its isolated menu/scroll/expiry case.
- All three preparations exited 0 (265.96, 167.25, 119.35 seconds). Owned-process cleanup and output drain were verified.

## M4-A evidence and handoff

- The configured reviewer independently reviewed the frozen implementation at `d95b9937161ba46e48467ed313a5614c6e43e235`. Remediation `ed368771983757fc110f3fda678cb4273ea1eed0` preserves pending display settings at shutdown and active selections on toggles, uses the final divider pointer position, targets the current viewport for repaint, and guards highlight expiry, popup identity, and retained result navigation and Inspector caret publication.
- Final library preparation exited 0 in 189.87 seconds. Final Nextest used 31 exact case selectors with one test thread, retries disabled, reusable binary metadata, and a process-local 16 MiB test-thread stack for app fixtures. All 31 passed in 9.75 seconds wall time, with verified cleanup and output drain.
- `cargo clippy -p multi_launcher --profile fast-dev --lib --bin multi_launcher --offline --no-deps` exited 0 in 116.50 seconds. It reported 115 library and 3 binary warnings on the inherited predecessor, including the pre-existing collapsible caret block in the Pad. No warnings were identified on the introduced lines. Vendored and table-core warnings remain.
- `cargo build --profile fast-dev --bin multi_launcher --offline` at the pre-remediation source exited 0 in 316.34 seconds. The isolated executable started, but the automation surfaces did not expose its window. See [Windows smoke limitation](stalls/20261113T010514Z.md). The remediated source was compiled by the final library build and checked by Clippy for both application targets.
- Evidence receipts and exact selectors are in the chat's `calc-pad-verification` evidence directory. The repository root settings file's hash matched before and after the smoke attempt.
- The remediation commit is local. Its ordinary upstream push failed with exit 1 (`github.com:443`, `Bad access`). All previous pushes likewise failed. The final documentation commit receives one ordinary push attempt and the actual resulting tip and push state are recorded in the final chat handoff.
- Create the next Goal's fresh branch directly from this Goal's final reported tip. This execution used the single existing checkout with no auxiliary clones, worktrees, agent branches, or intermediate integrations.

### Resumed Windows smoke

- Built the remediated source at `278ea969197ef9c7efd21fd7a50601f6346b4595` with `cargo build --profile fast-dev --bin multi_launcher --offline`. Exit 0, 102.29 seconds, with verified process cleanup and output drain.
- An interactive-desktop launch exposed the isolated launcher to Computer Use. The docked Pad accepted the mixed source fixture. Results showed `r2 = 2`, `r5 = 50`, `r6 = 8`, `r7 = 0`, an invalid expression at `r8`, and the additional integer and long-expression values. The Source scrollbar was exposed before enabling wrap and absent afterward. Keyboard actions turned wrapping on and line numbers off.
- Pop Out opened a separate native Calculation Pad window, and the root accessibility tree continued to expose the launcher query. Windows capture timed out on both windows, and the popped-out accessibility tree exposed only window chrome. Native divider drags, result menus and highlight, result actions, root query input, and docking back remain unverified.
- Quit the initial instance and restarted the same isolated executable with the same profile. The settings file retained `line_numbers = false` and `word_wrap = true`. The divider ratios remained at their defaults. The restarted instance could not reopen the Pad through the accessible launcher controls, so visual restoration and changed-divider persistence remain unverified. Both owned instances exited and the repository settings hash matched its initial value.
- The receipt is `calc-pad-verification/resumed-native-smoke.json` in the chat evidence directory. Status remains limited.
- A subsequent restart reopened the Pad in its saved popped-out presentation. With the native Pad open, the launcher query accepted `calc pad smoke`. The isolated settings still held line numbers off and word wrap on. The native Pad again exposed only window chrome and capture timed out, so the editor's applied display state, divider drags and restoration, and docking back remain unverified. The test instance exited and the repository settings hash still matched. See `calc-pad-verification/reopened-popout-smoke.json`.
- A docked restart verified applied Word wrap. Only the isolated profile's host presentation was set to docked before startup. A 237-character expression evaluated to `60` and exposed no Source scrollbar on opening. Turning Word wrap off exposed a horizontal scrollbar for the same source. Word wrap was restored before exit. The isolated file also retained line numbers off, though gutter visibility was unavailable through accessibility. The instance exited and the repository settings hash matched. See `calc-pad-verification/restarted-docked-wrap.json`.
- Another docked restart verified that the Line numbers control loaded its saved off state. Its first keyboard toggle saved `true`, and its second saved `false` again. This verifies the restored control state through the app's write path. Visual gutter rendering remains unverified. The isolated instance exited and the repository settings hash matched. See `calc-pad-verification/restarted-docked-line-numbers.json`.
- A native docked keyboard check verified Undo and Redo. Appending ` + 1` changed the second result from `5` to `6`, Ctrl+Z restored the original source and `5`, and Ctrl+Y reapplied the edited source and `6`. Inspect bits buttons were exposed for the integer rows. The isolated process exited and the repository settings hash matched. See `calc-pad-verification/native-editor-undo-redo.json`.
- The alternate Windows smoke used the app's existing `qa-framebuffer-capture` feature on source tip `521a7a265d11d8fceb914d246e4352c072b22e48`. The isolated build exited 0 in 204.84 seconds. ROOT/F11 captures showed the docked gutter hidden, then enabled with physical rows 1–9 including the blank line and comment. After restarting the same executable and profile, another capture showed the checked Line numbers and Word wrap controls and the restored gutter. The images and process-bound receipts are under `calc-pad-verification/alternate-framebuffer-profile/capture` and `capture-restart`.
- Framebuffer capture observes rendered pixels. Computer Use still timed out capturing the Windows window, and its accessible result-row right click returned `coordinate input geometry is unavailable`. The capture feature targets ROOT for the docked Pad and has no Calculation Pad child target. Both divider drags and their restart persistence, Go to source highlighting, and docking back remain unverified by native interaction. The wide capture also showed tight spacing between result labels. Both isolated processes exited and the repository settings hash matched.
- Follow-up framebuffer checks verified that the Windows app loads and renders nondefault divider settings after restart. Two isolated profiles were seeded with wide `0.62` and stacked `0.64`, at launcher widths of 1150 and 590 pixels respectively. Within each profile the before/after ROOT PNGs were byte-identical. Images and process receipts are under `calc-pad-verification/framebuffer-ratio-wide` and `framebuffer-ratio-stacked`, each in `capture` and `capture-restart`. This verifies settings-to-render restoration, while native pointer drag and drag-to-disk persistence remain unverified. All four owned processes exited and the repository settings hash matched.
- A follow-up correction restored ordinary horizontal widget spacing inside both wide panes. The splitter still uses zero spacing between pane allocations. A targeted Nextest batch passed 5/5, including new geometry assertions for the result label/value and Source toggles, both divider drags, result navigation, constrained layout, and host migration. The existing native captures predate this spacing correction. See `calc-pad-verification/m4-wide-spacing-test`.
