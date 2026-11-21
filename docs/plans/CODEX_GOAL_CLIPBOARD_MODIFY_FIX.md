# Codex Goal — Fix Clipboard Modify Clipboard Access

**Target:** Multi_Launcher, Codex Desktop, the existing local repository checkout.  
**Companion:** [CODEX_KICKSTART_CLIPBOARD_MODIFY_FIX.md](CODEX_KICKSTART_CLIPBOARD_MODIFY_FIX.md).  
**Planning review date:** November 21, 2026 (America/New_York).  
**Task scope:** Repair Clipboard Modify; finish with small commits, normal pushes, and a pull request to `master`.

## 0. Explicit task overrides and authority

These are the user's specific instructions for this task. They override conflicting generic task templates, earlier Goal/Kickstart documents, historical acceptance procedures, and default branch conventions:

1. **Stay on the branch that is already checked out when execution starts. Do not create a new branch. Do not switch to another branch.** Continue from that branch's actual current HEAD, including the committed Goal/Kickstart already present. A planning SHA below is a reference, not a checkout/reset target.
2. **Use the single existing checkout.** Do not create Git worktrees, auxiliary clones, secondary checkouts, per-agent branches, or milestone branches. Do not invoke a Codex workflow that creates a worktree for this task.
3. **Use small coherent commits and frequent normal pushes to that same branch.** No empty branch-bootstrap commit is required. No reset, automatic stash, history rewriting, rebase, squash, force push, or destructive cleanup.
4. **Fix only Clipboard Modify and directly necessary supporting code.** Inspect Clipboard Snippets and existing clipboard helpers as references. Preserve their working behavior.
5. **Use targeted verification. Do not run the full test suite.** No repository-wide regression campaign, broad Clippy cleanup, or repeated release builds.
6. **Continue implementation if native Windows/UI verification is unavailable.** Complete the code, available focused checks, review, commits, pushes, and PR. The user will perform final manual verification. Record exactly what remains unverified; do not claim a native pass.
7. **At the end, open or update a PR from the existing branch to `master`, and check for conflicts.** You are authorized to merge freshly fetched `master` **into the current task branch**, resolve conflicts there, commit the resolution, and push. This final integration is the explicit exception to generic no-merge guidance. Do not merge the PR or push directly to `master`.
8. **Execute this with Codex Desktop.** Read applicable `AGENTS.md` and use available Codex capabilities. Do not invoke OpenCode, its commands, or its execution harness. Historical OpenCode reports are source evidence only.
9. Work autonomously through ordinary implementation and integration problems. Do not stop after a checkpoint to ask permission. If a real blocker remains, distinguish an unresolved feature defect from unavailable verification or delivery tooling.

The existing repository and this task are sufficient inputs. Do not require an uploaded source snapshot, an attachment, a questionnaire, or a newly generated plan. Locate the companion files where they already exist. Do not change durable repository instructions or Codex configuration just to apply these task-specific overrides.

## A. Desired outcome

Ordinary text copied in Windows can be loaded into Clipboard Modify, reloaded, transformed in the existing preview, applied to the system clipboard, copied through Copy Result, and restored through Undo. Immediate Clipboard Modify commands use the same repaired boundary where applicable. The reported persistent error,

`Permanent("The clipboard could not be accessed.")`

must cease for normal readable clipboard text because the underlying access path is corrected, not because the error is hidden or failed reads are converted to empty text.

The GUI remains responsive. A failed or obsolete read cannot erase a draft, replace a newer source, invalidate a successful preview unnecessarily, overwrite clipboard contents, or destroy a valid Undo record. Existing protection for privately copied items remains intact.

Completion can include an explicitly documented final Windows verification handoff to the user when Codex cannot perform it. A known failing native result must be investigated and fixed; it cannot be relabeled as “verification unavailable.”

## B. Verified baseline and investigation targets

### B1. Planning snapshot; recheck at execution

The GitHub review used repository [multiplex55/Multi_Launcher](https://github.com/multiplex55/Multi_Launcher).

| Reference | Observed value | Meaning |
| --- | --- | --- |
| Recovery branch | `fix/nextest-stack-overflows-and-clipboard` | Branch reviewed; not an instruction to switch branches |
| Reviewed tip | `39efd5587082f15c081b3be26d886644a41c9c19` | Latest commit records a native acceptance stall |
| Last implementation commit | `004950bddbb83be30b106b03d9f895337918e671` | Reset/source authority and asynchronous Undo fencing |
| Reviewed `master` | `209708b9f2fb37b4259f2d96d957e5ecb9eeb765` | Must be fetched again for final integration |
| Comparison at review | 22 commits ahead, 52 behind; diverged | A PR already includes inherited work; preserve it |
| Comparison merge base | `edb15c5c96f51d194bc6c8bf6106c2bff9a870fc` | Historical context only |

[Reviewed branch history](https://github.com/multiplex55/Multi_Launcher/commits/fix/nextest-stack-overflows-and-clipboard/) and [immutable comparison](https://github.com/multiplex55/Multi_Launcher/compare/209708b9f2fb37b4259f2d96d957e5ecb9eeb765...39efd5587082f15c081b3be26d886644a41c9c19).

Record the execution-time `start_head` separately. Do not reset to these SHAs or discard newer local commits. Assess this task's new changes against `start_head`; assess the cumulative PR separately against the fetched `master`. Do not erase inherited commits to make the PR appear limited to this task.

### B2. Strong regression boundary

Commit [`979b112582416a0595e5f4567c4f799f3645938b`](https://github.com/multiplex55/Multi_Launcher/commit/979b112582416a0595e5f4567c4f799f3645938b), “feat(passwords): [M8-A2] publish and admit protected clipboard items,” replaced Clipboard Modify's direct `arboard::Clipboard::get_text()` with the protected WinRT reader. Its writer remained `arboard::set_text()`. The parent, `d51a629621001f7b490585cbc7ce8a143f56fcde`, contains the earlier reader.

This is a verified implementation transition and a strong investigation target, **not a proven first-bad commit**. The user's estimate of roughly thirty recent commits is a starting window. This transition is older than thirty all-path commits because unrelated work intervenes. Use path-specific history and `git show`; do not switch branches or launch a broad historical bisect/build campaign.

### B3. Recent recovery work to retain

| Commit | Relevant improvement |
| --- | --- |
| `b829d22`, `56950a4`, `a936750` | Error classification, retry behavior, preservation of source/drafts, avoiding GUI lock waits |
| `308535f`, `0008e46`, `ed6d5e0` | Bounded WinRT worker, diagnostics, callback completion, original read deadline, worker cleanup/admission |
| `e94053c` | Asynchronous initial Load/Reload; current-request and session/source revision checks |
| `3c63a17` | Guarded Apply/Copy Result/Undo off the GUI thread |
| `debf3fd`, `004950b` | Source authority, root repaint, mutation/reset/visibility fencing |
| `39efd55` | Documentation of unrun native acceptance; no further production fix |

The [native stall report](https://github.com/multiplex55/Multi_Launcher/blob/39efd5587082f15c081b3be26d886644a41c9c19/docs/plans/execution/standalone/resolve-clipboard-modify/stalls/20261121T163310Z.md) records 220 passing Clipboard Modify tests and an earlier 32-test reader check. It also explicitly says native acceptance was not started and no failing native HRESULT was observed. These are historical reported results, not fresh verification for this task.

The report's demand for a specially isolated disposable desktop is **not an active gate for this task**. Missing native access must not produce another documentation-only stop while source-level repair remains possible.

### B4. Actual production paths

| Area | Verified path / ownership | Instruction |
| --- | --- | --- |
| Modify reader | `src/clipboard_modify/clipboard.rs::ArboardClipboardBackend::read_text` → `map_protected_read(read_clipboard_for_modify())` | Investigate and repair this boundary first |
| Modify writer | Same backend's `write_text` → `arboard::Clipboard::new().set_text(...)` | Retain unless evidence implicates it |
| Platform reader | `src/platform/protected_clipboard.rs::read_clipboard_for_modify` → Windows `read_for_modify` | Current dedicated MTA WinRT worker; 750 ms caller deadline, at most two workers |
| Private publishing | Same platform module's `copy_secret` | Retain existing private marker and history/roaming policy |
| Service / transactions | `src/clipboard_modify/clipboard.rs::ClipboardService` | Read policy, guarded Apply, conflict confirmation, Undo retention |
| Dialog | `src/gui/clipboard_modify_dialog.rs` | Asynchronous source/mutation state and stale-result fencing |
| Entry and completion paths | `src/clipboard_modify/coordinator.rs`, `runtime.rs`, `src/commands/handlers/clipboard_modify.rs`, relevant portions of `src/gui/actions.rs`, `command_host.rs`, `render.rs` | Trace affected calls; edit only where the fix requires it |
| Ordinary snippet copy | `copy_snippet_text` / `submit_snippet_prompt` → `actions::clipboard::set_text` → arboard | A useful reference for writing, not proof that clipboard reads succeed |
| Snippet clipboard capture | `capture_snippet_context_with_launcher_origin` → `actions::clipboard::get_text().ok()` → `read_clipboard_briefly` | Uses protected WinRT too; failed reads can become absent context |
| Existing native examples | `src/image_compare/clipboard.rs::load_windows_clipboard`; `src/gui/note_clipboard.rs::windows_clipboard_formats` | Reuse API/lifetime/error-handling idioms; do not redesign these features |

See the [current Modify backend](https://github.com/multiplex55/Multi_Launcher/blob/39efd5587082f15c081b3be26d886644a41c9c19/src/clipboard_modify/clipboard.rs), [platform boundary](https://github.com/multiplex55/Multi_Launcher/blob/39efd5587082f15c081b3be26d886644a41c9c19/src/platform/protected_clipboard.rs), [shared clipboard actions](https://github.com/multiplex55/Multi_Launcher/blob/39efd5587082f15c081b3be26d886644a41c9c19/src/actions/clipboard.rs), and [snippet host methods](https://github.com/multiplex55/Multi_Launcher/blob/39efd5587082f15c081b3be26d886644a41c9c19/src/gui/command_host.rs).

### B5. Facts versus hypotheses

The current platform error `Unavailable` displays “The clipboard could not be accessed.” Several HRESULTs and abnormal worker/completion outcomes map to it, and the Modify adapter maps it to `Permanent`. This text does not establish `E_ACCESSDENIED` or a missing Windows permission.

WinRT context, provider behavior, format inspection, and completion remain possible failure points. **Do not assert that MTA alone is invalid.** Microsoft's [Clipboard class reference](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.datatransfer.clipboard?view=winrt-26100) contains a foreground/UI-thread remark and also declares `ThreadingModel.Both`. That does not prove this unpackaged Rust application's exact failure.

Use existing `multi_launcher::clipboard` diagnostic events when available: stage, outcome, elapsed time, apartment, and HRESULT. Current WinRT Modify HRESULT values are logged numerically; render a hexadecimal code during analysis. If logs are unavailable, proceed with a source-grounded implementation and focused tests. Do not make the user supply logs before you do useful work.

## C. Approved functional requirements and invariants

1. **Ordinary source reads:** Initial Load and Reload succeed for normal Windows text, including empty text, non-ASCII characters, CRLF, and supplementary Unicode characters. Preserve the text exactly before any requested transform.
2. **Failures:** Busy, timeout, nontext, private-item, and actual API failures remain distinguishable. A transient failure is recoverable on the next explicit attempt. Do not cache one error as permanent dialog state.
3. **Private items:** The marker `application/x-multi-launcher-private-text-v1` must be recognized before text retrieval/exposure. A present marker rejects the read; a failed or incomplete protection check fails closed. Do not expose private text to source, preview, Undo, logs, or fallback reads. An unrelated custom format alone is not proof that an item is private.
4. **State preservation:** Failed reads preserve accepted source, user edits, reset authority, and usable preview state. A delayed result cannot supersede a newer Load/Reload, user edit, reset, dialog session, or clipboard mutation.
5. **Mutations:** Apply, Copy Result, immediate commands, reviewed overwrite, and Undo continue through the existing service. Failed preflight/read/write must not silently write empty or stale content, overwrite an unreviewed external change, or consume valid Undo.
6. **Responsiveness:** OS clipboard operations and retry waits stay off the egui frame path. Preserve bounded admission, late-result rejection, and eventual worker cleanup.
7. **Recovery:** After contention or failure ends, a later ordinary text item can be loaded and modified without restarting the app.
8. **Compatibility:** Preserve command aliases, transform semantics, settings/data formats, keyboard behavior, and unrelated plugin behavior. No migration or new user setting is required by default.

Preserve the existing conflict-preflight semantics. A same-open marker/text read is an atomic classification/read snapshot; it does **not** make the separate arboard write an atomic compare-and-swap against every other Windows application. Do not claim a stronger guarantee or expand this task into a new clipboard transaction framework.

## D. Scope and implementation strategy

### D1. Preferred scope

The smallest complete fix should normally live in `src/platform/protected_clipboard.rs`, the Modify adapter/service, and their directly relevant tests. Make a small private platform helper if it improves ownership. Touch the dialog/command plumbing only if the repaired reader reveals a concrete integration defect.

Keep `read_clipboard()`, `read_clipboard_briefly()`, the protected publisher, Snippets, Clipboard History, Notes, and image comparison behavior unchanged unless an unavoidable shared-boundary correction requires a narrowly justified change. Their existing native helpers are references, not new workstreams.

Do not revert the entire protected-clipboard commit, remove privacy checks, replace the transform engine, add a clipboard framework, alter startup/hotkeys, repair unrelated Nextest failures, change Codex/OpenCode configuration, or upgrade dependencies. `Cargo.toml` already has `windows = "0.58"`, arboard, and the needed Win32 DataExchange/Memory/Ole features.

### D2. Decision sequence

1. Trace the current failing read and compare it with the pre-transition implementation and real snippet operations.
2. If an identifiable small defect in the existing WinRT path explains the failure and can be corrected cleanly, fix it with meaningful coverage.
3. Otherwise, **prefer a narrowly scoped native Windows ordinary-text reader for Clipboard Modify**, retaining the current protection contract and asynchronous service boundary. This is an approved engineering option; it does not require a new user decision or proof from a full bisect.
4. A native reader should replace Modify's normal failing read path deliberately. Do not build a chain of speculative retries, focus changes, apartment switches, and unprotected fallbacks.
5. Stop architectural exploration once there is a credible, testable repair. A diagnostic-only change or a larger timeout alone is not completion.

### D3. Native reader contract, if selected

Use the repository's existing Windows idioms from [image clipboard input](https://github.com/multiplex55/Multi_Launcher/blob/39efd5587082f15c081b3be26d886644a41c9c19/src/image_compare/clipboard.rs) and [format enumeration](https://github.com/multiplex55/Multi_Launcher/blob/39efd5587082f15c081b3be26d886644a41c9c19/src/gui/note_clipboard.rs). Inspect the bindings for the pinned `windows` version rather than assuming raw C signatures map directly to Rust.

- Open the clipboard once in the worker and pair successful open with `CloseClipboard` using RAII. A null owner is supported for read-only access. Do not empty the clipboard as part of reading. [OpenClipboard](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard).
- Resolve the existing private format name. Treat failed registration/classification as failure. Inspect marker presence and retrieve ordinary text within the **same open scope**. Do not check via WinRT, close, and blindly reopen through arboard.
- Prefer a format inspection with explicit present/absent/error semantics. With `EnumClipboardFormats`, distinguish normal end of enumeration from an API error using the documented last-error contract, as the existing Note helper does. Do not return ordinary text before establishing marker absence. [Enumeration contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumclipboardformats).
- **Do not turn “private format absent” into the same access error being fixed.** A false/failed availability probe is not automatically access denial; the binding may expose a boolean API as a `Result`. Do not blindly propagate it with `?`. [Availability semantics](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-isclipboardformatavailable).
- Establish that the native reader detects the marker created by the **existing WinRT `copy_secret` publisher**. A test that only creates a native marker with the expected name is insufficient to demonstrate interoperability. Add a focused Windows test using a harmless fixture through the actual publisher/test seam. Do not weaken `SecretResult` ownership or expose a new production API merely to seed tests. [Registered format contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerclipboardformatw).
- Retrieve `CF_UNICODETEXT` only after classification permits it. The clipboard owns the returned handle. Copy data while open; do not free the handle or retain a borrowed pointer after unlock/close. Use `GlobalSize` and checked byte-to-UTF-16 bounds, with a terminator found inside the allocation. Distinguish valid empty text from absent text format. No unbounded NUL scan, trimming, newline normalization, or silent partial result. Handle malformed data explicitly. [GetClipboardData](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboarddata), [GlobalSize](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-globalsize), [GlobalLock](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-globallock).
- Pair successful locks with unlocks on every path. A zero `GlobalUnlock` result can represent the successful final unlock; use that API's contract rather than treating every zero as an error. Capture native errors immediately at the failing call. [GlobalUnlock](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-globalunlock).
- Preserve the bounded caller deadline and worker limit. Native delayed rendering can block inside `GetClipboardData`; a timed-out caller does not cancel that call or instantly release its resources. Keep the worker permit until cleanup completes, reject late results, and never create unlimited replacement workers. Do not promise synchronous native cancellation. [Microsoft delayed-rendering discussion](https://devblogs.microsoft.com/oldnewthing/20220609-00/?p=106731).
- Keep error diagnostics useful and payload-free: backend, operation/stage, elapsed time, and native/HRESULT code where applicable. Do not log clipboard contents, source/preview text, or secrets.
- Remove only Modify-specific WinRT machinery made obsolete by the repair. Retain code still used by protected publishing or other callers. Do not maintain two competing default readers without a concrete reason.

If native execution is unavailable, still implement the protection checks and the focused interoperability test. Mark that test/native behavior unrun for final user verification; never compensate by making an uncertain classification succeed.

## E. Milestones and checkpoint boundaries

Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan. The boundaries below are for **the same existing branch**. Combine a tiny conditional subsection with its related commit when separate commits would be artificial.

### M0-A — Establish current-branch baseline and bounded diagnosis

**Objective / owner:** The parent Codex agent establishes repository state and the reader failure boundary. This is read-only preparation, not branch creation.

**Tasks:**

- Read applicable `AGENTS.md`, this entire Goal, and the companion Kickstart. Inspect available `.codex` role configuration without changing it.
- Record repository root, `task_branch`, `start_head`, upstream, working-tree/staged changes, and both document paths. Preserve pre-existing work and distinguish it from task changes.
- Review current relevant source and approximately thirty recent commits, extending only path-specific history far enough to inspect `979b112` and its parent. Use `git show`, not checkout/bisect.
- Identify the actual candidate executable when native testing is possible; avoid mistaking an older running launcher for the current candidate.
- Read existing safe diagnostic lines or reproduce once with synthetic text if feasible. Note exact stage/code if observed. If unavailable, write “not observed” and proceed.
- Select the smallest credible repair using Section D. Clearly distinguish observed facts, hypotheses, and chosen engineering direction.

**Verification / acceptance:** Current branch and start HEAD are captured; relevant call paths and protection/state invariants are understood; no branch/worktree was created; no user work was staged/discarded. A dirty checkout alone is not a blocker for independent edits.

**Commit boundary:** None required merely to record a baseline. Record execution notes in the existing Goal's final section or existing task record; do not create an empty M0 commit or a new planning bureaucracy.

If HEAD is detached, an unrelated merge is already unresolved, or the current branch is `master` itself, report the precise incompatibility with the current-branch PR contract. Do not silently create/switch branches to resolve it.

### M1-A — Repair the ordinary-text reader and protection contract

**Objective / ownership:** Fix the production OS read boundary used by Modify, including a meaningful regression test.

**Likely files:** `src/platform/protected_clipboard.rs`, `src/clipboard_modify/clipboard.rs`, and directly related tests; a small private helper only if warranted.

**Tasks:**

- Implement the selected repair, retaining typed ordinary/private/nontext/error outcomes.
- Make ordinary text without the private marker a successful read.
- Preserve private detection before text access, explicit classification failure, empty text, Unicode fidelity, deadline/admission handling, and resource cleanup.
- Keep the existing arboard writer unless evidence requires a related correction.
- Add or migrate behavior tests that exercise the new production boundary, not solely an obsolete WinRT callback fixture.
- If replacing the reader, add the production-publisher interoperability check described in D3; remove only truly obsolete Modify-specific paths.
- Ensure diagnostics identify the actual backend/API and don't expose data.

**Edge cases / constraints:** Missing marker is normal; incomplete classification is failure. Distinguish no text format from empty text. A timed-out worker may still own resources; no unbounded replacement threads. Preserve non-Windows compilation paths.

**Verification / acceptance:** The smallest relevant reader/service tests pass where executable, the changed Windows target compiles when available, and a review confirms there is no unprotected fallback. Record platform limitations explicitly.

**Suggested commit:** `fix(clipboard-modify): [M1-A] restore protected ordinary text reads`. Inspect/stage only intended paths, commit, and attempt one normal push.

### M1-B — Confirm all Modify operations use the repaired boundary

**Objective / ownership:** Retain correct service/dialog behavior through the repaired reader; correct only concrete integration gaps.

**Likely files:** Existing service, dialog, coordinator/runtime, command handler, and root completion polling where actually needed.

**Tasks:**

- Trace initial Load, Reload, Apply, Copy Result, immediate pipeline commands, overwrite confirmation, and Undo to the intended reader.
- Retain session/source/mutation revision checks, source authority, reset behavior, root repaint, and delayed Undo visibility protection from the recent commits.
- Ensure failure updates useful error state without clearing drafts/preview or consuming Undo.
- Ensure a later successful read clears stale access-error presentation and restores normal operation.
- Reuse existing tests for these invariants. Add a test only for an uncovered behavior introduced or exposed by this repair.
- Do not refactor working Snippets. If the writer/shared action changes, perform one directly affected snippet-copy check.

**Verification / acceptance:** No operation bypasses protection; no worker result overwrites newer state; no synchronous OS wait returns to the GUI; failed mutations preserve existing clipboard/Undo behavior.

**Suggested commit when needed:** `fix(clipboard-modify): [M1-B] preserve guarded operations after reader repair`. If inspection proves no additional production change is necessary, record the evidence and include only meaningful test changes with the related checkpoint. Do not invent a second implementation.

### M2-A — Focused verification and one independent review

**Objective / ownership:** Establish sufficient confidence in this fix and resolve concrete findings.

**Tasks:**

- Execute Section F's applicable checks, using one build/test/native process at a time.
- Run the small native sequence if supported. If not, complete available checks and prepare the exact user handoff without stopping implementation/delivery.
- Obtain one focused read-only review of the task's diff and directly affected code when an agent is available; otherwise perform an explicit self-review.
- Resolve actual Clipboard Modify defects and rerun only affected checks.
- Update the existing task notes with root-cause evidence or clearly labeled uncertainty, chosen repair, commands/results, and native verification state.

**Acceptance:** Relevant findings are resolved; available targeted checks pass; no known reproducible feature defect is ignored; unavailable checks are accurately listed. The full suite has not been run merely for this task.

**Commit boundary:** Commit meaningful remaining tests/corrections/evidence, for example `test(clipboard-modify): [M2-A] cover access and recovery behavior`. Push normally. Do not create an empty verification commit.

### M3-A — Integrate master on the same branch and deliver the PR

**Objective / ownership:** The parent agent produces a reviewable PR and verifies its conflict status.

Follow Section G. Preserve prior work, resolve normal conflicts autonomously, perform only affected validation, commit/push integration changes, and open/update the PR. This stage does not authorize merging the PR.

**Acceptance:** The PR has base `master` and head `task_branch`; its head matches the pushed final commit; conflict status is checked against a recorded current master SHA; any remaining delivery limitation is explicitly reported. Native verification may be assigned to the user without blocking the PR.

## F. Targeted verification and final Windows handoff

### F1. Verification budget

Choose the narrowest meaningful checks. No unfiltered `cargo nextest run`, no `cargo test --workspace`, no full GUI suite, and no unrelated cleanup.

Use the project's existing Nextest Windows stack wrapper; do not revive runner/stack repairs. Discover current test names before running filters. Example scoped commands, to adapt to the current code and host:

~~~powershell
cargo nextest list -p multi_launcher --lib -E 'test(clipboard_modify) | test(protected_clipboard)'
cargo nextest run -p multi_launcher --lib -E 'test(clipboard_modify) | test(protected_clipboard)'
~~~

Run smaller exact filters during iteration; use the combined scope once when it adds final coverage. A successful run matching zero tests is not verification. If Nextest is unavailable, use equivalent narrow `cargo test` module/name filters. Do not make installation or runner modernization a new project.

Use targeted compilation only where not already established by the chosen tests/build, such as:

~~~powershell
cargo check -p multi_launcher --lib --bin multi_launcher
~~~

For a native app smoke check, use one existing supported development build/profile for `multi_launcher`. Avoid repeated release builds. Linux-only checks do not compile/test the Windows `cfg` implementation. Report the actual host/target.

Keep individual test execution bounded, aiming for roughly 100 seconds for a scoped run once compilation is complete. Compilation is separate; do not repeatedly kill and restart a useful first build to meet that target. A timeout is diagnostic evidence, not an automatic product failure. Fix relevant hangs; do not expand to the full suite.

Format touched Rust files, inspect `git diff --check`, and review the task diff. Run a scoped first-party Clippy check only if useful for the change; do not make unrelated baseline warnings or a repository-wide lint cleanup a gate.

### F2. Meaningful coverage

Reuse current tests and extend only gaps introduced by the fix:

| Behavior | Evidence needed |
| --- | --- |
| Ordinary text, no private marker | Production reader boundary succeeds; regression catches accidental absence-as-error mapping |
| Private marker with valid text | Returns protected-item result without retrieving/exposing text |
| Failed protection inspection | Fails closed; no fallback, no write, no loss of prior state |
| Empty vs no text | Empty text succeeds; nontext has the intended typed outcome |
| Unicode / allocation handling | Preserve CRLF and non-ASCII/supplementary characters; native buffers stay within bounds; malformed data fails safely |
| Contention / timeout / late result | Bounded caller behavior; existing draft/Undo survive; later normal read can recover |
| Load/Reload and mutation fencing | Existing focused tests continue to prove that newer user/session/mutation state wins |
| Guarded Apply and Undo | Successful apply records undo; failed preflight/write preserves it; conflict behavior remains intact |

If selecting native reads, a small Windows interoperability test through the actual protected publisher is directly in scope. Keep tests that touch the real system clipboard opt-in or serialized using existing repository conventions; do not let parallel test workers contend for it. Do not duplicate every existing service/dialog test.

### F3. Native smoke sequence

This task authorizes a focused test with harmless synthetic data in the connected Windows session when the available tools permit it. Use the existing test-profile/app-data mechanism where supported. Separate app data does not isolate the Windows clipboard; do not claim it does. Do not require a new VM/disposable desktop or an elaborate candidate harness.

1. Confirm the running candidate corresponds to the current task build. Avoid testing an older instance reached through the global hotkey.
2. Deliberately seed `ml modify alpha` as ordinary clipboard text.
3. Open Clipboard Modify. Confirm the source loads exactly and the persistent access error is absent.
4. Seed `ml modify beta`, click Reload, and confirm the new source.
5. Choose the existing uppercase operation. Confirm preview `ML MODIFY BETA`; Apply; verify that exact system clipboard result.
6. Invoke the existing Undo operation, including `cm undo` if that is the configured alias. Confirm restoration of `ml modify beta`.
7. Exercise Copy Result and one immediate transform through the existing command syntax if those paths were affected. Reuse tests for unchanged paths.
8. Confirm a controlled failed read preserves the draft/Undo and a later ordinary read succeeds; use a deterministic test seam rather than manufacturing intrusive OS contention if simpler.

Use only fixture values for evidence. Do not read or archive unrelated clipboard contents or expose them in screenshots/logs. Use the existing app-test defaults for Paste Sequences hotkeys (`next_hotkey = None`, `cancel_hotkey = None`) when constructing a test app; preserve production defaults.

If GUI/native tools, Windows compilation, or runtime access are missing, record the exact limitation and continue to PR delivery. Give the user steps 1–6 plus any unrun protected-marker check, the candidate build/commit identity, and the expected outputs. Do not require intermediate manual testing from the user.

## G. Commits, conflict resolution, and PR contract

### G1. Checkpoints and push behavior

- The parent/orchestrator owns Git mutations. Before each commit, verify the current branch still equals `task_branch`, inspect staged paths, and stage only this task's intended changes. Do not use blanket staging that captures unrelated user work.
- Commit coherent increments using checkpoint subjects. Do not squash/amend prior checkpoints.
- After each checkpoint, attempt one normal push to the configured matching upstream. If no upstream exists and `origin` is confirmed as the intended repository, `git push -u origin HEAD` is allowed; this publishes the existing branch, not a newly created local branch.
- On a network/auth/permission failure, preserve local commits and continue safe local work. Record actual push state. Do not loop, bypass controls, or claim remote visibility without success.
- For a non-fast-forward rejection, inspect remote movement. Never force push. Reconcile only clearly understood same-branch changes through a safe, history-preserving merge on the current branch; if concurrent work cannot safely be reconciled, preserve everything and report the delivery blocker.

### G2. Final integration sequence

1. Fetch the intended remote safely and record the current `origin/master` SHA as `integration_base_sha`. Verify `task_branch` and inspect local changes before any merge.
2. If that master commit is already an ancestor of HEAD, no merge is needed. Otherwise merge that recorded commit into the existing task branch, using a reviewable non-rewriting merge. Do not switch to master, rebase, cherry-pick onto a new branch, or create a worktree.
3. Resolve conflicts by reading the surrounding source and preserving the intended behavior on both sides. No blanket “ours”/“theirs.” Necessary conflict resolution is authorized even if it touches an inherited file outside the new fix; do not use it for unrelated redesign.
4. Preserve pre-existing user edits. If they overlap integration and cannot be preserved safely, report that precise blocker; do not auto-stash or discard them.
5. Confirm `git ls-files -u` is empty, inspect conflict resolutions/staged changes and `git diff --check`, and run only the compilation/tests affected by the merge. Commit the merge/resolution and push normally.
6. Open a PR to `master`, or update an existing open PR with the exact same head/base. Use the GitHub connector or authenticated `gh` if available. With `gh`, put a multiline body in a temporary file and pass `--body-file`.
7. Verify the PR URL, head branch/SHA, base branch, and mergeability after the final push. A clean local index alone is not proof of remote mergeability. If GitHub reports an unknown/calculating status, make a bounded refresh. If master moved and creates conflicts, resolve against the updated base on the same branch and repeat only affected checks.
8. Do not bypass repository checks or branch protection, alter CI, or merge the PR. Existing remote CI may run normally; report its state without launching a local full-suite campaign. If remote mergeability stays unknown or delivery tools fail, report “not confirmed,” not “conflict-free.”

The PR body must explain the user-visible failure, actual cause or remaining uncertainty, repair, protected-item/state guarantees, focused tests actually run, native checks performed or assigned to the user, and the integration-base SHA. Disclose inherited branch changes separately from this task's changes; do not pretend the cumulative PR is exclusively new Clipboard Modify work.

## H. Definition of done and blocker handling

### Successful implementation delivery

- The real Modify read boundary is repaired and wired through the relevant operations.
- No known reproducible Clipboard Modify defect introduced or observed during this task remains ignored.
- Privacy, source/preview preservation, guarded mutation, Undo, and responsiveness invariants are retained.
- Appropriate available scoped verification and review are complete; unrun native checks are explicitly assigned to the user.
- Meaningful changes are committed on the original `task_branch`; normal pushes were attempted and their result is known.
- A PR to `master` is opened/updated when credentials/network allow, with actual conflict status verified or explicitly limited.
- No new branch/worktree, unrelated workstream, full-suite campaign, or PR merge was introduced.

### Do not stop for these reasons

A first compile/test failure, an ordinary merge conflict, missing logs, unavailable native UI automation, a historical disposable-session requirement, an unrelated pre-existing warning, or a failed push is not by itself a reason to abandon the code fix. Resolve relevant defects and continue everything that remains possible.

If a viable fix cannot be completed, report the specific failing boundary, evidence, attempted remedies, why remaining approaches cannot satisfy the contract, and the smallest next requirement. “Cannot verify here” does not mean “unfixable.” Follow applicable concise stalled-report conventions only for a genuine blocker; do not reopen old queues or create repeated stall documentation instead of implementing.

## I. Final Codex response

Start with one clear, evidence-matched status:

- **Done — Clipboard Modify fixed; native Windows verification passed.**
- **Done — Clipboard Modify implementation complete; ready for your final Windows verification.**
- **Blocked — [specific unresolved feature or integration/delivery blocker].**

Then provide:

1. Root-cause evidence and what changed; separate unproven hypotheses.
2. Relevant files and significant design decisions.
3. Exact targeted verification commands/results; what was not run and why.
4. `task_branch`, `start_head`, checkpoint commits, `final_tip_sha`, and actual push state.
5. PR URL, recorded master SHA, PR head SHA, and verified mergeability/conflict status. State clearly that the PR was not merged.
6. Any inherited PR scope or genuine limitations.
7. The short final manual sequence from F3 if native verification is pending.

Do not describe only passing mocks as proof that the original system clipboard error is gone. Do not make the user reconstruct the candidate version or expected outcomes. Stop once the scoped fix and delivery are complete.

## J. Execution notes — fill during Codex execution

Use this section or an already established task record; do not create a competing queue ledger.

| Field | Execution value |
| --- | --- |
| Repository / Goal path | `multiplex55/Multi_Launcher`, `docs/plans/CODEX_GOAL_CLIPBOARD_MODIFY_FIX.md` |
| Task branch / start HEAD | `fix/nextest-stack-overflows-and-clipboard` / `5cff552e32ab014f0aaf6104adadb0f0bb702afc`, tracking the same branch on `origin` |
| Pre-existing user changes | Clean index and working tree at start |
| Observed failure stage/code | A standalone MTA WinRT diagnostic accepted the ordinary text format, then its `IAsyncOperation::get` returned `0x8001010E` (`RPC_E_WRONG_THREAD`). The launcher uses a different completion wrapper, so this establishes a WinRT failure on the same read path without establishing its exact production call site. The publisher returned `0x800401F0` at `SetContentWithOptions` in MTA and succeeded on an STA thread. |
| Selected repair and evidence | Commit `979b1125` moved Modify's ordinary reads from arboard to WinRT. Modify now enumerates formats and reads `CF_UNICODETEXT` under one Win32 clipboard-open scope. The private marker takes precedence over text, and a format inspection failure aborts the read. |
| M1-A / M1-B checkpoints | `6f783805bad7e1cd848dce4a1bca796a1246e1fd`. Load, Reload, Apply, Copy Result, immediate transforms, reviewed overwrite, and Undo still use the existing service. Their guards required no separate code change. |
| Targeted tests / review | Windows Nextest compiled the library and ran 229 selected tests in 19.379 seconds with `-p multi_launcher --lib -E 'test(clipboard_modify) \| test(protected_clipboard)'`. Independent source review found no correctness defects. The opt-in test was then adjusted to run the real publisher from a caller with an STA, matching the successful diagnostic. |
| Native Windows verification | The live Nextest probe read the Unicode/CRLF and empty fixtures. A staged STA diagnostic published and flushed the existing WinRT package and verified that its registered marker was visible to Win32. The corrected publisher interoperability test and GUI smoke are pending. |
| Integration base / final tip | Fetched `origin/master` at `209708b9f2fb37b4259f2d96d957e5ecb9eeb765`. Final tip and verification are recorded in the PR. |
| Push state / PR URL / mergeability | M1-A pushed to the existing upstream. The final push and mergeability are checked at PR delivery. |
| Final status / remaining checks | Final scoped verification and integration follow this checkpoint. |

Record actual commit SHAs after the commit exists. Do not create a self-referential extra commit just to write that commit's own SHA into this file; the final response and PR can carry the final tip.

