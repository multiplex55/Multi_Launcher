# Codex Kickstart — Desktop Productivity Completion & Acceptance Recovery

**Companion:** `docs/plans/CODEX_GOAL_DESKTOP_PRODUCTIVITY_COMPLETION_RECOVERY.md`  
**Goal ID:** `desktop-productivity-completion-recovery`  
**Exact predecessor:** `1852f264dae69056f79bc0c3b593c70331fa620f` on `feat/silent-screen-recorder`  
**First category:** R01 — Shared Host Core Recovery  
**First branch:** New child `fix/productivity-host-core-recovery` (or fresh unique suffix if taken)  
**Execution mode:** Autonomous Codex; one checkout, sequential chained branches, checkpoint commits and pushes, independent review, focused tests, final user verification permitted.

> **AUTHORIZED DEFERRED MANUAL VERIFICATION:** The user explicitly agrees to do final Windows 11 or GUI checks that Codex cannot perform on its host. **Implement the actual code first**, with focused automated/owner-level tests, build checks and local native probes wherever possible. Do not pause, block code work, request intermediate screenshots or ask the user to run tests because the Codex host is Windows 10 or its interactive observer is unavailable. At the very end, provide **one consolidated manual N1–N12 checklist**, exact setup/commands, expected outcomes and requested feedback. Use `IMPLEMENTED_USER_VALIDATION_PENDING` for tests the user has not yet run. Never infer those tests passed, and never conflate an unsafe/unimplemented native action with mere unavailability of testing.

## 1. Mandatory first actions — before editing product source

Read fully, in order:

1. `AGENTS.md` and any local applicable instructions.
2. This Kickstart and `docs/plans/CODEX_GOAL_DESKTOP_PRODUCTIVITY_COMPLETION_RECOVERY.md`.
3. The original `CODEX_GOAL_DESKTOP_PRODUCTIVITY_EXPANSION.md` (original C.1–C.7, E.4 IDs, C.2 host inventory, H.4 N1–N12) and its Kickstart.
4. `docs/plans/execution/standalone/desktop-productivity-expansion/{EXECUTION.md,INTEGRATION_STATUS.md}`.
5. Original final stop `docs/plans/execution/standalone/desktop-productivity-expansion/stalls/20261106T135856Z.md` and the five older stopped-increment reports listed in the companion Goal.
6. The final Git and receipt metadata, if available, under `target/desktop-productivity-expansion/`; treat ignored artifacts as *optional evidence*, not tracked source.

Confirm predecessor identity and existing checkout safety first. PowerShell examples (commands illustrate checks; use repo conventions):

```powershell
$Base = '1852f264dae69056f79bc0c3b593c70331fa620f'
git status -sb
git branch --show-current
git rev-parse HEAD
git remote -v
git branch -vv
git cat-file -t $Base
git show --no-patch --format='%H %P %s' $Base
# Optional read-only awareness; does not change the required parent.
git fetch origin master
```

- Verify `$Base` is the **actual final pushed M17-B tip** and the repository/remote is correct. No alternative base is authorized just because `master` advanced. If the local commit is missing, safely fetch the known branch/ref and verify the SHA; never guess a replacement.
- Inspect index, untracked and modified files, any existing Git operation, and current user work. **Do not auto-reset, auto-stash, rebase, merge, cherry-pick, force-push or clean.** Do not stage unrelated paths. One checkout only; no Git worktrees or auxiliary clones.
- If safe and branch name is free, start directly from the SHA:

```powershell
git switch -c fix/productivity-host-core-recovery $Base
git rev-parse HEAD
# This must equal $Base before the baseline commit.
```

- Create `docs/plans/execution/standalone/desktop-productivity-completion-recovery/EXECUTION.md` and `INTEGRATION_STATUS.md` with actual source snapshot, original 50-ID/38-milestone/host/N1–N12 matrices, verified parent, environment and existing tests; then commit only this initial R01/M0-A ledger baseline.
- Verify the **M0-A commit's parent** equals `$Base`, record the resulting baseline SHA and best-effort `git push -u origin HEAD`. Push failure is **not a reason to stop local implementation**; record it and keep clean local checkpoint history.
- This authorization starts a **new goal and a fresh corrective budget** for the named original failures. The prior `PARTIAL_BLOCKED_FROZEN` status does not forbid working on those areas in this new user-approved goal.

## 2. One sequence — do not spawn independent branches

Carry out the following **back-to-back**, with a fresh branch and M0-A at the beginning of each recovery category. **Each branch must start from the previous category's exact final reporting tip.** All agents for a category share the sole checkout and source writer; read-only planning/review may overlap safely.

| Order | Category | Implementation checkpoints | Suggested branch |
| --- | --- | --- | --- |
| 1 | **R01 Host Core** | M1-A repair two Diagram click paths; M1-B host geometry/focus controls | `fix/productivity-host-core-recovery` |
| 2 | **R02 Host Migrations** | M2-A Docs/MkMacro; M2-B Settings/Mouse; M2-C File/Clipboard/Developer/Time; M2-D references and new utility hosts | `feat/productivity-host-migrations` |
| 3 | **R03 Calculation** | M3-A Inspector bit model; M3-B independent dockable clickable grid, Copy/Insert; M3-C comparisons and reference | `feat/productivity-programmer-inspector` |
| 4 | **R04 Diagram** | M4-A containers/swimlanes; M4-B five starters; M4-C visual PNG/SVG controls; M4-D supported grouped draw.io/interchange | `feat/productivity-diagram-completion` |
| 5 | **R05 Utilities/Explorer** | M5-A safe Win11 Explorer tab backend; M5-B list editor/launcher/progress/cancel; M5-C utility/recorder integration checks | `feat/productivity-explorer-utility-completion` |
| 6 | **R06 Acceptance** | M6-A cumulative focused verification; M6-B final user N1–N12 manual test packet; M6-C truthful final receipt | `test/productivity-native-acceptance-closeout` |

For categories R02–R06:

1. Finish or explicitly disposition the preceding category's current source, review, tests and report; commit and attempt one normal push.
2. Read its **actual final `git rev-parse HEAD`**, not a previously mentioned source-checkpoint SHA. Verify it is committed and the checkout can be switched without discarding user work.
3. Create the new branch directly from that exact SHA; commit an M0-A ledger update; verify direct parent; push.
4. Keep all category implementation/verification/checkpoint commits on that same branch; no per-milestone branch or mid-chain merge.
5. Do not alter the default branch or automatically open/merge the final PR unless specifically authorized.

## 3. Recover the exact four failed UI increments; don't mask failures

The previous goal exhausted retries. **A new budget is expressly authorized here.** Before implementation, read the relevant stop report, recover exact failing test name/selector, compare preserved patches (if any) with current accepted code, verify patches' checksums and stop if they do not match. *Missing local ignored patch? Rebuild safely from accepted source; continue.*

| Original blocker | Previous observed failure | What must be proved now |
| --- | --- | --- |
| **C01/M1-B.2** | 52/54: docked Diagram Pop Out click and Diagram Viewer open click did not transition | Real enabled/visible UI click changes the correct retained host session; ordinary Dock return preserves state |
| **C03/M9-B.2** | 15/16: grouped drag fixture pointer falls outside retained canvas mapping before modifier | Genuine user pointer/selection on visible canvas; group membership, descendant transforms and round-trip work |
| **C03/M10-A.1** | 14/15: `visual_authoring_template_flowchart` remains disabled in menu | Actual accepted menu click creates one of five visual starters; preserve dirty/undo/cancel |
| **C03/M10-B.2** | 15/16: ×2 export scale control lies outside clipped UI before click | Actual reachable scale/background control, valid PNG/SVG publication with stage/cancel/late-collision safety |

Do not merely broaden the tests' clipping predicate or make fixtures call internal methods instead of the user click they are meant to verify. Fix a production UI fault if present; fix a bad fixture only when production behavior is independently verified in the available harness. Run the exact named previous failures first and add directly related layout/revision tests. Initial test run plus **up to three focused correction/retest cycles for each documented independent root cause**; record attempts precisely. A material new root cause needs an evidence-grounded, reviewed budget decision, not a renamed checkpoint or infinite loop.

## 4. Existing code is the asset; keep accepted behavior intact

- **R01/R02:** one authoritative session/worker per logical tool; preserve dirty history, selections, generations, cancellation, owned prompts/file choosers, accepted-pass semantics, hotkey/Jump Mode focus, monitor/DPI geometry, hidden launcher behavior. Migrate *each originally named dialog*, including calendar child editors, new Folder Size/File Lock setup and recorder setup; exclude specialized overlays from ordinary Dock/Pop Out.
- **R03:** reuse exact programmer engine, required 8/16/32/64 signed/unsigned modes, `u64` default programmer mode, width-safe shifts; interactive bit grid is a **separate** dialog. Bit toggles edit inspector working copy only. Add explicit copy/format/insert, staleness/undo and side-by-side comparison, without changing scalar `^`, `/`, `min`, `mod`, unit parsing, variable or row references.
- **R04:** reuse accepted Mermaid/Visual dual-authority and rich vector/raster encoder; complete containers, group transforms and swimlanes, all five starters, actual PNG/SVG controls and safe publication, supported uncompressed draw.io grouped subset. Preserve old documents, stale export and Save/Discard/Cancel safety. Do not retry already-corrected late Save As publication or scalar min parsing unless regression tests fail.
- **R05:** use bounded Explorer identity/operation/store; **implement supported Win11 logic even on a Win10 development host** with deterministic fake-provider and command-path tests. Exact same-window tab proof is user-final pending on Win11. Do not falsely report successful tabs from an Unsupported fallback, unrelated Explorer windows or blind foreground keys. Add a real saved-list GUI, typed query command, launch/progress/cancel and review utility UI/recorder cooperation.
- **R06:** verify all original selected IDs, original milestone matrix, named host wiring and N1–N12; repair touched regressions; deliver user test packet and exact Git receipt.

## 5. Test workflow — efficient, strict and honest

For every coherent checkpoint:

1. Narrow source/behavior review (including real ownership and acceptance paths); implement with one writer and no unrelated changes.
2. Scoped `cargo fmt`/diff check of task-touched Rust, and independently review source.
3. `cargo nextest list` with **an exact nonzero match count** for new/affected tests. Use `--locked`, appropriate `--cargo-profile fast-dev`, `--offline` only when already available; inspect current Cargo config rather than changing it.
4. `cargo nextest run --no-fail-fast` on the same relevant exact subset; if failure, re-run only failed names after a targeted correction, with bounded budgets. Distinguish a list-compile error from a runtime assertion. Keep original logs.
5. Production `cargo check/build --locked -p multi_launcher --lib --bin multi_launcher --profile fast-dev` when test-only configuration hides native picker/hotkey code; run app/native fixture where supported. Make real artifacts (GIF/MP4/draw.io/PNG) where needed with disposable data.
6. Commit accepted increment, append exact commands/results and scope diff to recovery ledger, then attempt one normal push. Preserve failure evidence; don't paper over unaccepted code.

The previous final 444 distinct tests passing is the **starting regression baseline**, not a waiver for new tests. Avoid full-suite reruns unless focused failures warrant expanding scope; one final cumulative affected-case run is required. Run a final relevant first-party Clippy (`cargo clippy --locked -p multi_launcher --lib --bin multi_launcher --profile fast-dev --no-deps`), and classify unrelated pre-existing warnings instead of hiding them. Do not edit Nextest TOML or vendored crates to force a green summary. Never claim a native test passed from a fake backend.

## 6. Windows 11 and interactive observer rule — most important

The original host is Windows 10 Home 22H2 build 19045. It cannot demonstrate Explorer tabs. The previous observer timed out, and unchanged reprobes are not helpful. **These facts do not prohibit Codex from coding Explorer Tabs, Programmer Inspector, Dock / Pop Out, Diagram controls or recorder integration.**

On this host, Codex should:

- Design and compile the intended behavior; prove the pure state machine, bounded operations, failure/partial paths, exact source/target identity, user-command and GUI event routing with deterministic tests.
- Probe native interactions only when this host actually supports them, using isolated disposable fixtures. Don't waste time repeatedly reinitializing an unchanged screenshot service or waiting for Windows 11 to appear.
- Use **`IMPLEMENTED_USER_VALIDATION_PENDING`** when code is implemented and tested but a positive Windows 11/GUI native check is unavailable. Still commit, push and advance. Continue independent work even when one capability test cannot run.
- Use **`IMPLEMENTATION_BLOCKED`** if the underlying action is absent, unsafe or does not compile/behave correctly. Missing Windows 11 evidence may explain a deferred *test*, not excuse an omitted *feature*. Any unresolved implementation blocker is documented as an honest limitation at final handoff.
- Defer all requests of the user to **one final verification packet**; the user has explicitly said this is acceptable. Do **not** demand an early manual run or pause the work until the user responds.

For Explorer on Windows 11: prefer supported UIA/Shell observation, exact new-HWND identity, selected tab→owning Shell view→expected folder identity and order, with a pre-existing unrelated Explorer window left untouched. If supported Windows 11 observation is absent on the Codex host, implement a guarded operation and visible capability/reporting path; never blindly send navigation keys to an unverified foreground window. Do not claim success solely from fake-provider checks. On the user's supported machine, the final N8 checklist will provide the positive test.

## 7. Native acceptance packet — prepare at the end, not during development

At the end, create and link:

`docs/plans/execution/standalone/desktop-productivity-completion-recovery/MANUAL_WINDOWS_11_ACCEPTANCE.md`

Use this starter matrix, then expand each into short exact instructions, expected results, safe isolated fixtures and PASS/FAIL/NOT_RUN fields. Record Windows version/build, current commit/build identity and whether test was performed by Codex or user.

| Gate | Final user-visible scenario |
| --- | --- |
| **N1** | Calculation Pad: edit/undo/copy while detached, hide root, Dock, Cancel Close; source/focus unchanged |
| **N2** | Quick Notes, note editor, Markdown and MkMacro: detach/return and owned prompts; no duplicate execution |
| **N3** | Main/Dashboard and Mouse/Gesture settings: edit, save/cancel, detach and input ownership |
| **N4** | Representative file/clipboard/developer/time/image/ZIP hosts; second Paste Sequence run; no state reset on migration |
| **N5** | Exact integer → separate Programmer Inspector → click bits → Copy formatted result → explicit Insert/Undo → side-by-side comparison |
| **N6** | Legacy Mermaid document, invalid/stale preview, native viewer/current export, source remains intact |
| **N7** | Rich Diagram: shapes, asset, connectors, container/layer, undo, reopen, rich PNG/SVG, supported grouped draw.io round-trip |
| **N8** | Windows 11: five ordered disposable folders appear as tabs in *one new identified* Explorer window; other Explorer window untouched; cancel safely |
| **N9** | Folder Size Analyzer: known tree, sorted progressive totals, partial/cancel, refresh, detach/Dock |
| **N10** | File Lock: disposable helper holds/releases file; inspect/refresh/copy/reveal, honest No processes reported |
| **N11** | Actual launcher GIF and silent MP4: record controlled moving content, hide root, Stop; independently inspect media, no audio |
| **N12** | Cancel selection/countdown/recording, target loss, repeat, exit; normal mouse/hotkeys/Pinboard/still capture unaffected |

Provide **one compact recommended 20–30 minute smoke subset** followed by optional deeper checks; time is a usability suggestion, not an assertion about runtime. State exactly how to send failures: gate ID, expected vs observed, build/OS, redacted screenshot/log and reproduction. The user should not be asked to paste full private logs or perform developer commands unless the user elects to do so. If non-primary monitor, external draw.io application or screenshot observer is unavailable even on the user's host, mark those subchecks NOT_RUN, not FAIL.

Do not wait for this manual check to produce the implementation report or push the final branch. No user result is presumed.

## 8. Required documents and reporting cadence

Track only one authoritative recovery ledger:

```
docs/plans/execution/standalone/desktop-productivity-completion-recovery/
  EXECUTION.md
  INTEGRATION_STATUS.md
  MANUAL_WINDOWS_11_ACCEPTANCE.md
  stalls/
```

Each checkpoint records: recovery milestone, original ID(s)/Cxx milestone(s), source scope, test inventory/pass/fail, native proof vs user pending, reviewer disposition, relevant SHA/push and exact remaining obstacle. Reuse prior stop reports by reference, don't duplicate huge logs. Store large task-owned logs/patches under ignored `target/desktop-productivity-completion-recovery/` and put stable hashes in tracked reports. If a patch is unavailable, say so and reconstruct from code. `INTEGRATION_STATUS.md` is only the short summary; `EXECUTION.md` is canonical.

Keep the user informed through ordinary Codex checkpoint reports, but **do not interrupt with manual test requests**. The one end-of-goal ask should be the consolidated checklist, not many separate “please check this UI” interruptions.

## 9. Final handoff — definition of a successful recovery execution

Before final response:

- Verify all six recovery branches descend from exact predecessor, each M0-A is direct and all category tips are chained. No worktrees, auxiliary clones, intermediate merges or history rewrites.
- Review the original 38-milestone/50-ID matrix and all host rows. Don't drop missing original work from the report.
- Summarize implemented source, exact test commands and counts, local native proofs, known implementation gaps, and `USER_VALIDATION_PENDING` gates. Validate no accepted feature regression. Report scoped Clippy and any warnings accurately.
- Commit final docs first, then read actual final SHA, inspect checkout and push status, write the SHA in a separate ignored/external receipt, and provide it in the final answer. **Do not modify committed report just to include its own SHA.**
- Link the **single user acceptance checklist**. If everything implementable is complete and only external native checks remain, report **`IMPLEMENTATION_COMPLETE_USER_VALIDATION_PENDING`**. If real code/interaction failures remain, report `PARTIAL_IMPLEMENTATION_BLOCKED`. Only report fully verified after actual N1–N12 positive checks.
- Never auto-merge to master or claim a PR/merge exists unless it actually does. If creating a final PR, use the final cumulative tip and wait for user merge approval.

**Start now with the exact original M17-B tip, fresh R01/M0-A, and the two host click-path failures. Do the independent implementation autonomously. The user will perform any necessary final Windows 11/native verification at the end.**
