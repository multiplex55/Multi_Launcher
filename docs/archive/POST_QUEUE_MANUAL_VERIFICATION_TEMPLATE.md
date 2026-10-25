# Multi Launcher — Post-Queue Manual Verification Checklist

> **Starter checklist. NOT YET a completed Codex handoff.** Codex must generate the final repository version at `docs/plans/POST_QUEUE_MANUAL_VERIFICATION.md`, replacing every `PENDING_FINAL_EVIDENCE` with the actual outcome, current binary/commit, proof reference, and exact remaining user action. **Do not mark a check PASS without observing it.**

## 1. Build and review metadata (Codex fills at handoff)

| Field | Value |
|---|---|
| Branch | `qa/post-queue-native-acceptance` (or recorded unique suffix) |
| Original cumulative G3 base | `7e0f74f117975024b25e7c7cbadef32f29937965` |
| Final task branch commit | `TO_FILL_FROM_GIT` |
| Verified executable path and binary SHA-256 | `TO_FILL_FROM_BUILD_RECEIPT` |
| Windows build tested by Codex | `TO_FILL_FROM_HOST` |
| Native fixture, compile and cleanup receipt directory | `TO_FILL_FROM_REPORT` |
| Last full Nextest / Clippy outcome | `TO_FILL_FROM_REPORT` |
| Terminal Goal status | `PENDING_FINAL_EVIDENCE` |
| Date of your review | `____________________` |
| Your Windows version, DPI scale and monitor arrangement | `____________________` |

**Status legend (fill once per check):** `PASS` = you observed every expected effect; `FAIL` = reproducible unexpected effect; `BLOCKED` = cannot safely access the control/environment; `NOT_TESTED` = skipped. `AUTOMATED_PASS_OPTIONAL` means Codex proved the native path and user confirmation is optional. `REQUIRES_MANUAL` means the native action lacked complete proof. Avoid interpreting a mock or focused test pass as native PASS.

### Safety before testing

- [ ] Run the **new acceptance branch's built application**, not `master` or the older G3 branch. Verify the branch/commit displayed above against Codex's final report.
- [ ] Back up your regular settings or use a **disposable launcher profile** for editing saved actions, snippets and hotkeys. Prefer the disposable profile throughout this checklist.
- [ ] Use a dedicated temporary folder with harmless test files and a temporary note, not personal or valuable data. Keep a small before/after inventory and avoid allowing background automation to touch your normal data.
- [ ] Close or pause unrelated macros, mouse gestures and Paste Sequences if they could intercept test keys/clicks; restore normal settings afterward.
- [ ] For any click-through / shield test, have a harmless text editor or blank test window beneath the overlay. **Do not use the desktop, another person's window, a password field or a destructive application as an input target.**
- [ ] When documenting failures, record the app commit and one reproducible sequence. Avoid sending private clipboard content, desktop screenshots, tokens or unrelated documents.

## 2. At-a-glance priorities — Codex must update after its automated run

| Manual ID | Priority | Area | Automated status | Human verification needed because… | Result |
|---|---|---|---|---|---|
| `INPUT-01` | **P0** | Shield selection input isolation | `PENDING_FINAL_EVIDENCE` | G2 original native foreground gate prevented input injection | [ ] |
| `PIN-01` | **P0** | Pin click-through and recovery | `PENDING_FINAL_EVIDENCE` | G3 compositor/Reset proved, input routing not proved | [ ] |
| `REPAIR-01` | **P0** | File picker Cancel and file repair | `PENDING_FINAL_EVIDENCE` | Earlier native test ended before picker | [ ] |
| `REPAIR-02` | **P0** | Folder repair / durable references | `PENDING_FINAL_EVIDENCE` | Native confirmation/persistence unfinished | [ ] |
| `NOTE-01` | **P0** | Find/Replace/Undo/Save/Discard | `PENDING_FINAL_EVIDENCE` | Previous native run demonstrated only note opening/unsaved source | [ ] |
| `DFF-01` | **P1** | Cancel, drain and restart | `PENDING_FINAL_EVIDENCE` | Previous native DFF run stopped early | [ ] |
| `DFF-02` | **P1** | Verified results and Open/Reveal/Copy | `PENDING_FINAL_EVIDENCE` | Results/action flow not fully native-verified | [ ] |
| `SET-01` | **P1** | Settings numeric edit/Cancel with Jump OFF | `PENDING_FINAL_EVIDENCE` | Numeric/Cancel/Jump-off earlier native gap | [ ] |
| `SET-02` | **P1** | Settings numeric edit/Cancel with Jump ON | `PENDING_FINAL_EVIDENCE` | Current focused slider showed partial progress only | [ ] |
| `BRI-01` | **P2** | Brightness read-only support display | `PENDING_FINAL_EVIDENCE` | Real physical hardware capability not demonstrated | [ ] |
| `BRI-02` | **Optional; consent required** | Real monitor Set/readback/restore | `NOT_RUN_INTENTIONALLY` | Codex must not change display hardware | [ ] |

**Codex instructions:** At final handoff replace table statuses, put **still-unproven P0/P1** first, add exact evidence file paths or `NONE`; if automation proves a scenario completely, relocate it to Section 4 as `AUTOMATED_PASS_OPTIONAL` instead of asking the user to redo everything.

## 3. Detailed hands-on test procedures

### INPUT-01 — Rectangle-selection shield and cleanup

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Harmless empty text editor/dummy click target on a second layer. Use launcher screenshot/selection control documented by the installed application, not an unknown keybind.

- [ ] Open the launcher's capture/region-selection overlay over the harmless target, with the target positioned where you can safely drag.
- [ ] Drag an ordinary selection over the target. Look for shield coverage throughout the operation. The underlying editor should **not** receive text, clicks, selection drag or unexpected focus.
- [ ] Release and verify the capture/overlay closes normally and input is restored to the expected application.
- [ ] Start a second selection, press **Escape**, and verify no orphaned shield, mouse capture, stuck hover, or blocked input persists.
- [ ] Repeat once after another normal launcher open/close. Where applicable, check a second monitor, negative-origin monitor and different display scaling; skip if unavailable.

**Expected:** Underlying harmless window sees no input while shield is active; shield is visibly topmost and terminates/recovers cleanly; no extra action runs. **FAIL evidence:** first repeatable leak, skipped overlay region, inability to cancel or focus still captured.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes / screenshot of *owned* test window:** `____________________`

### PIN-01 — Static pin opacity, click-through and recovery

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Create a pin from a synthetic or otherwise non-sensitive image; put a harmless editable test window underneath it. The previous implementation supports separate opacity in child toolbar and Pin List, Reset to 100%, and click-through.

- [ ] Open a static pin and position it over the harmless test window. Change opacity to **50%**, then Reset to **100%**. Confirm visual appearance and displayed applied status agree and don't alter the original image.
- [ ] Change opacity to 50% again, enable **Click-Through**, and click only within the safe test window's area beneath the pin. The underlying window should receive the intended click; the pin should not steal it.
- [ ] Disable Click-Through through the available Pinboard control (or recover it using the documented escape/recovery path) and click in the pin. The pin's own controls should once again receive input, while underlying text remains unchanged.
- [ ] Hide and restore the pin. Verify opacity, position, Always On Top, and click-through state return as configured; no unrelated pin changes.
- [ ] If you use live pins, repeat a short opacity adjustment and Reset with a safe generated/live source, checking frames continue updating and no foreign window receives the click.

**Expected:** Accurate opacity, separately managed pin input routing, successful recovery, unchanged image exports. **FAIL evidence:** wrong window hit, pin unresponsive after recovery, opacity incorrectly reset, stale child HWND controls or unexpected focus.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### REPAIR-01 — Saved application/file target: native Cancel then confirmation

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** In a **disposable profile**, create a saved launcher action pointing to a temporary executable-like or harmless text file. Give it a distinct label, description, argument string and ordering. Create a harmless replacement file with a Unicode/space-containing name. Record original action metadata and file hashes. Do not execute the test action.

- [ ] Open the saved entry's actual **Action Sheet/Edit → Repair target** operation (use the label shown in the build). Verify the old path and selected exact entry are correct.
- [ ] Open the native file picker and press **Cancel**. Confirm no saved target changed, no app was executed and the original metadata remains intact.
- [ ] Reopen Repair, select the new temporary file and **Confirm**. Verify only the intended target path changed; the label, arguments, description, ordering, favorites/pins, and unrelated saved actions remain correct.
- [ ] Close/reopen the launcher from the disposable profile. Confirm the repaired target remains correct without a duplicate entry or lost usage metadata.

**Expected:** Cancel is side-effect-free; confirmation updates only the exact saved entry and its safe references; no automatic execution. **FAIL evidence:** wrong target revised, references lost, picker opens behind launcher, Cancel persists a change, saved JSON modified unexpectedly.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### REPAIR-02 — Saved folder entry and ambiguous-target refusal

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Two distinct temporary folders and a disposable saved Folder entry with a custom alias, plus an unrelated item.

- [ ] Open the exact folder entry's Repair operation; browse and confirm the replacement **directory**.
- [ ] Verify the alias and unrelated entries remain unchanged; reopen application to check persistence.
- [ ] Where the UI supports it, try an intentionally conflicting replacement or a path with unknown/denied access. Verify it refuses rather than silently changing a different record or merging metadata.
- [ ] Check any visible association/favorite/reference points to the intended repaired folder, not the original path or a neighbor row.

**Expected:** Exactly one folder is repaired, alias/references retained, conflict refusal leaves old data intact. **FAIL evidence:** silent merge/overwrite, different row edited, no durable change, unsafe rollback.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### NOTE-01 — Unsaved literal replacement, Undo/Redo, Save and Discard

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Create **one disposable note** with several repeated literal targets, Unicode text, CRLF/newlines and a Markdown link destination. Record original file contents. Use **Jump OFF first**, then repeat the core behavior with Jump ON; don't edit a personal note.

- [ ] Open the note from ordinary launcher query. Type a distinctive **unsaved** marker and verify the file on disk remains unchanged until normal Save.
- [ ] Open Find, enter a literal search target, use **Replace Current**, then **Replace All** with a simple replacement that contains literal `$` or `\\`. Verify only intended live-note text changes; inserted replacement text is not recursively searched in the same action.
- [ ] Focus the actual note editor (not Find query or replacement field). Press **Undo once** and verify one coherent replacement action reverses. Press **Redo** and verify the intended replacement returns. Verify two sequential replacement actions undo in order, and a fresh edit after Undo discards abandoned Redo.
- [ ] Switch **Edit ↔ Preview** (and Split if available). Verify the live note content is retained, and source-only matches navigate/reveal without modifying the saved note.
- [ ] Use the normal **Save** action. Compare the on-disk content with the expected edited content, including literal punctuation and CRLF policy.
- [ ] Make another unsaved change, close the note, choose **Discard**, and reopen it. Only the saved version should remain.
- [ ] Repeat basic Find/Replace/focus/Cancel with Experimental Jump ON: hint completion should focus only, while a later deliberate action performs Replace. Closing the note should not execute the launcher query behind it.

**Expected:** Live unsaved content is authoritative; no premature write; Replace creates coherent Undo/Redo boundaries; Save and Discard are exact. **FAIL evidence:** Ctrl+Z changes Find query unexpectedly, redo vanishes across view switch, source text mangled, Save/Discard writes wrong bytes, ghost action behind note.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### DFF-01 — Duplicate scan Cancel, drain and Restart

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Disposable directory with two different files with identical bytes, one equal-size-but-different-content file, two zero-byte files, a subfolder and a few Unicode names. Optionally include a hardlink pair if you know how to create one safely; no need to do so for the basic test.

- [ ] Open `dff` via normal Launcher search. Add/select the disposable directory. Verify no scan runs merely by browsing/searching the command.
- [ ] Click **Start**. With an active sufficiently large *disposable* tree, press **Cancel** while scanning and verify UI acknowledges cancellation.
- [ ] Confirm the worker drains, the session is not stuck Busy, and no outdated terminal results overwrite the cancelled generation.
- [ ] Click **Start** again and let it complete. Verify only distinct files with matching bytes form duplicate groups; equal-size-but-different bytes should not match; Windows hardlink aliases should not inflate logical duplicates.
- [ ] Close and reopen DFF, confirming there is no stuck worker, high idle CPU, or spinner that never stops.

**Expected:** Cancel truly interrupts/invalidates publication and permits a fresh run; all displayed statuses are truthful. **FAIL evidence:** Cancel ignored, Start disabled forever, stale completion reappears, window becomes unusable or unrelated files modified.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### DFF-02 — Results action integrity, footer and stale alias refusal

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Use the disposable DFF results from DFF-01. The earlier native attempt exposed **footer drift**, which was fixed in code afterward but not reverified against that corrected executable.

- [ ] Resize DFF to a normal and a relatively small usable window. Verify Start/Cancel/status/footer, result counts, and action buttons are reachable and correctly aligned while scrolling.
- [ ] Select an exact verified duplicate result; use **Open** and/or **Reveal** only for a harmless fixture path. Verify that the intended file/folder is targeted. Use **Copy Path** if supported and inspect the result in a harmless destination, avoiding passwords or personal clipboard text.
- [ ] Change or rename a fixture file **after the results appear** and try the same result action. The stale alias should be refused or require a refresh, not silently target an unrelated file.
- [ ] Inspect that DFF offers no unexpected destructive Delete/Move/Recycle operation and that the scanned files' bytes and names are unchanged unless *you* intentionally modified a fixture.

**Expected:** Correct current result identity, footer layout, stale refusal, read-only scan. **FAIL evidence:** action targets neighboring row, old path accepted unsafely, footer inaccessible, file silently modified.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### SET-01 — Settings search with Experimental Jump **OFF**

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Disposable settings with a known numeric value (e.g., Search **Fuzzy match weight** as actually named in the current UI); record the current setting or settings-file hash.

- [ ] Disable Experimental Jump Mode. Invoke the registered `settings` search command and search for the numeric control; use the returned result to navigate to the actual editable control.
- [ ] Click the real slider and the separate numeric value editor. Use an arrow key / number edit. Verify the **draft value** changes intentionally and the launcher search field does not steal focus.
- [ ] Press **Cancel** without Save. Reopen Settings and verify the committed value and on-disk bytes did not change.
- [ ] Repeat on a second disposable profile and **Save**; check that only the intended setting is committed and other settings remain intact.
- [ ] Search for a disabled plug-in's setting if safely available: result should show a truthful prerequisite, not auto-enable or silently save.

**Expected:** Works independently of Jump; exactly the current numeric control receives keyboard input; Cancel makes zero persisted changes. **FAIL evidence:** no focus, numeric editor unresponsive, ghost launcher activation, Cancel persists a draft.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### SET-02 — Settings search with Experimental Jump **ON**

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Same disposable control/profile family as SET-01, Jump ON.

- [ ] Search for the exact numeric control, activate Shift+Space Jump hints if that is your configured leader, type the target hint and verify **hint completion focuses only**.
- [ ] Deliberately activate and edit the slider, then numeric field; test first ArrowRight/ArrowUp input after focus, with a freshly visible current Response.
- [ ] Press Escape once while Jump is active; only the Jump overlay should close. Use normal Settings Cancel to discard unsaved values.
- [ ] Reopen and ensure committed settings remain unchanged; previously active note or search query must not receive the numeric text.

**Expected:** One-shot focus, no automatic checkbox/Save activation, native first-arrow working, correct layered Escape priority. **FAIL evidence:** hint triggers action by itself, wrong control focused, no numeric edit, Cancel writes disk.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### BRI-01 — Real monitor detection, read-only

**Automated evidence:** `PENDING_FINAL_EVIDENCE`  
**Setup:** Do this on your normal machine only if you are comfortable opening the brightness dialog. **Do not press Set**.

- [ ] Open the Brightness dialog from the launcher. Confirm that each detected physical display is separately identified where possible.
- [ ] Check that *supported* monitor rows display actual known values and unsupported/unknown rows display an error/unavailable status instead of fabricated 50%.
- [ ] Use **Refresh** once. Verify selection doesn't jump to another same-named monitor, UI stays responsive, and no automatic hardware change happens.
- [ ] Close and reopen the dialog; verify no stuck worker or perpetual refresh.

**Expected:** Honest hardware support and current-known data, no automatic Set or misleading target reassignment. **FAIL evidence:** wrong monitor is selected, unsupported is reported as known, unrequested brightness change, hang.  
**Result:** `PASS / FAIL / BLOCKED / NOT_TESTED`  
**Notes:** `____________________`

### BRI-02 — OPTIONAL real hardware brightness Set/readback/restore

**Status:** `OPTIONAL — REQUIRES USER CONSENT; Codex must NOT perform this`. This is **not required** to call the machine's software acceptance work complete.

- [ ] I explicitly choose to test actual hardware brightness and have verified it is safe for my display.
- [ ] Record current known brightness and any external monitor physical controls as a fallback.
- [ ] On **one known supported physical display**, change the requested value slightly via the explicit per-device Set control (never global broadcast), verify the correct monitor visibly changes and readback corresponds.
- [ ] Restore the original value manually and verify readback. If Set outcome is Unknown/timed out, stop and Refresh rather than repeating uncontrolled writes.

**Expected:** Only selected monitor changes, original setting recoverable. **Result:** `PASS / FAIL / BLOCKED / NOT_TESTED (OPTIONAL)`  
**Notes:** `____________________`

## 4. Optional quick regression checks — usually already covered automatically

> Codex should reduce this section to the few best sanity checks for actual code it changed, and cite any proven native receipts. These are **not** substitutes for unresolved P0 acceptance.

- [ ] **Launcher autofocus:** Hide launcher; press global launcher hotkey; query input is immediately focused when eligible. Repeat with Notes open, Jump ON/OFF and no accidental underlying command execution.
- [ ] **Delayed screenshot:** Start a short timed capture of a harmless generated window, open a menu/tooltip during countdown, and verify the capture contains the menu/tooltip but not countdown overlay. Cancel a second attempt, no output/history.
- [ ] **`imgdiff`:** Load two disposable images of different size; endpoints show image A/image B; Difference shows changed pixels, including alpha changes. Close and reopen without a retained-worker/presentation leak.
- [ ] **`csv` vs `table`:** Open disposable CSV with quoted commas/newlines and a ragged-row example; verify `csv` strict/header-on/lexical versus `table` lenient/Auto behavior, JSON/TSV outputs and source not overwritten.
- [ ] **Numbered screenshot markers:** Add labels 1, 2, 3, undo one, redo, export PNG; numbers and crops are legible.
- [ ] **Paste Sequence pickers:** Search a snippet and history entry, inspect the permitted preview, Insert, Cancel, and verify hidden snippet contents remain concealed. Start and cancel using the current sequence semantics.
- [ ] **MkMacro typed parameters:** Create a disposable test macro with File/Folder/Choice, Browse without executing, Run explicitly and confirm exact parameter values; Cancel has no effects.
- [ ] **Window/UI scaling:** At normal and narrow dialog widths, verify no clipped footer, missing button or permanently disabled first-frame control in Settings, DFF, Note editor and Pin List.

## 5. How to record a problem (copy this block for each failed test)

```text
Check ID:
Date/time and timezone:
Multi Launcher branch / exact commit:
Executable path / SHA-256 (if available):
Windows version/build:
Monitors (count, scaling/DPI, arrangement):
Experimental Jump Mode: ON / OFF
What I clicked/typed (reproduction steps):
Expected behavior:
Actual behavior:
PASS / FAIL / BLOCKED / NOT_TESTED:
Can reproduce? always / intermittently / once
Owned test fixture path (not personal data):
Screenshot or log path (synthetic windows only):
Any settings or files unexpectedly changed? (yes/no, what):
Cleanup/restoration completed? (yes/no):
```

**Final handoff rule for Codex:** The first section should be rewritten with actual evidence and only remaining human tasks; every currently unverified phase in Section 2 must appear somewhere even if Codex could not start its native scenario. Do not mark `NOT_TESTED` as `PASS`, and do not make the user rerun every already-proven scenario. Keep this checklist under source control and cite its exact path in the final Codex response.
