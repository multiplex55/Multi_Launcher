# Windows 11 acceptance — disposable manual session

**Status: NOT_RUN for every gate below.** The automated/fake-provider checks and the local build do not establish any complete native flow. This packet is for one later, voluntary user session; no results have been observed here. A PASS applies only to the checks actually performed. Mark unavailable prerequisites **NOT_RUN**; record a real mismatch as **FAIL**. In particular, an unsupported Explorer capability on an otherwise applicable, tab-capable Windows 11 host is a **FAIL / capability gap**, even if the application stops safely.

Record before starting: performed by: ______; date: ______; Windows 11 edition/build (`winver`): ______; monitor count/scaling: ______; code checkpoint: `08c91e819ef8f488fdc83da33f0e052230a6a34d`; executable SHA-256: ______. The reviewed executable was built from base `f04c2eccaf7219eb7acc1c04b9cdd2153c717da5` plus the accepted test-only changes; its tracked build inputs were verified equivalent to checkpoint `08c91e819ef8f488fdc83da33f0e052230a6a34d`. A later documentation commit is not embedded in the executable. Expected executable: `target/fast-dev/multi_launcher.exe`, 143,680,000 bytes, SHA-256 `E393FD9F07B58758105C429DAE9467AC41A914226A1D288631A756901F6F8BC8`.

## Safe setup

Use a Windows 11 desktop session with normal user permissions and a writable local drive; no administrator, system folders, private notes/files, credentials, or personal clipboard data are needed. Close any other Multi Launcher instance and reserve F2 for this session. Disable notifications containing private information and use only a plain, nonsensitive test window for capture. Copy/paste checks will replace the clipboard, so first preserve anything you need. These instructions create a new uniquely named folder under your own temporary directory; do not use an existing folder containing anything valuable. The copied executable keeps executable-relative note and template files isolated, while its working directory holds `settings.json` and other relative application data. No existing profile is copied or overwritten.

In PowerShell, from the repository root, run the following **only when ready for the manual session**. It checks the built file before copying it, installs the provided minimal profile before the application starts, and creates five empty Explorer folders plus a known-size Folder Size tree. Keep this PowerShell window for the paths. If the hash differs or the executable is absent, stop and report NOT_RUN; do not use a different binary as if it were this one.

```powershell
$exe = (Resolve-Path .\target\fast-dev\multi_launcher.exe).Path
$expected = 'E393FD9F07B58758105C429DAE9467AC41A914226A1D288631A756901F6F8BC8'
if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $expected) { throw 'Executable hash mismatch; stop.' }
$taskRoot = Join-Path $env:TEMP ('MultiLauncher-acceptance-' + [guid]::NewGuid().ToString('N'))
$profile = Join-Path $taskRoot 'profile'
New-Item -ItemType Directory -Path $profile | Out-Null
Copy-Item -LiteralPath $exe -Destination (Join-Path $profile 'multi_launcher.exe')
Copy-Item -LiteralPath .\docs\plans\execution\standalone\desktop-productivity-completion-recovery\fixtures\manual-settings.json -Destination (Join-Path $profile 'settings.json')
$env:ML_NOTES_DIR = Join-Path $profile 'notes'
$env:ML_NOTE_TEMPLATES_DIR = Join-Path $profile 'note-templates'
$folders = 1..5 | ForEach-Object { $p = Join-Path $taskRoot ("folder-$_"); New-Item -ItemType Directory -Path $p | Out-Null; $p }
$tree = Join-Path $taskRoot 'size-tree'
New-Item -ItemType Directory -Path (Join-Path $tree 'child') -Force | Out-Null
[IO.File]::WriteAllBytes((Join-Path $tree 'eleven.bin'), [byte[]]::new(11))
[IO.File]::WriteAllBytes((Join-Path $tree 'child\seventeen.bin'), [byte[]]::new(17))
$lockFile = Join-Path $taskRoot 'held.txt'
[IO.File]::WriteAllText($lockFile, 'owned disposable fixture')
"Task root: $taskRoot`nFive folders: $($folders -join ', ')`nSize tree: $tree`nHeld-file fixture: $lockFile"
Start-Process -FilePath (Join-Path $profile 'multi_launcher.exe') -WorkingDirectory $profile
```

The minimal profile explicitly sets Paste Sequences **Enable Next Shortcut** and **Enable Cancel Shortcut** to `None` before construction; it does not mute error notifications or alter production defaults. The application uses `settings.json` in its working directory; the two `ML_NOTE...` variables redirect note data for this child process. F2 opens the launcher. Type the quoted *search query* and select the named result; the typed internal action strings are not necessarily direct query text. If a feature is absent, check the isolated profile's Settings plugin enablement and report the actual message. Do not switch to a personal profile to make a check pass.

Recommended **20–30 minute smoke** (a suggestion, not a timing/result claim): perform N1, N5, the basic N6/N7 steps, N8, N9, N10, and one short GIF plus one short MP4 in N11/N12. The remaining steps and subchecks are optional deeper checks; an unperformed gate/subcheck remains NOT_RUN. Use the per-item fields rather than marking a whole flow PASS from a partial smoke.

## N1 — Calculation Pad host

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: host move [ ] PASS [ ] FAIL [x] NOT_RUN; focus/Jump/close [ ] PASS [ ] FAIL [x] NOT_RUN.

1. Search `calc pad`, select **Open Calculation Pad**. Enter a disposable expression such as `123 >> 12`; leave an unsaved edit. Choose **Pop Out**, hide the launcher, change the source in the detached Pad, then Undo and use its Copy control.
2. Choose **Dock**; verify the same source and undo history remain, and the copied value is the selected result. Start closing a dirty Pad and choose **Keep Editing**: it stays open with the draft. If Jump Mode is enabled, verify focus and hints belong to the visible owner, and Escape dismisses an owned popup/hints before closing the parent.
3. Expected: no second Pad/session, lost edit or duplicate action. Close or discard only this disposable document.

## N2 — Notes, Markdown, MkMacro

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: Notes [ ] PASS [ ] FAIL [x] NOT_RUN; Markdown [ ] PASS [ ] FAIL [x] NOT_RUN; MkMacro [ ] PASS [ ] FAIL [x] NOT_RUN.

1. Search `note` and choose **note: edit notes** for Quick Notes; search `note new acceptance` and choose **New note acceptance** for an individual Note. Enter synthetic text, Pop Out, hide the launcher, edit and Dock. Exercise the close prompt with Cancel; the same draft remains. Save only to the isolated notes directory.
2. Search `markdown studio`, select its opener, enter `# Test` and a short Unicode sentence. Select text, Pop Out, edit/Undo, Dock, and cancel a dirty close; the text and selection/history remain. Exercise an owned file dialog then cancel it.
3. Search `mkmacro`, open the editor, create a harmless macro that writes only a short synthetic string to an empty disposable text editor; select an item, Pop Out/Dock and run it once. Verify the selection persists and exactly one string appears. Cancel an owned prompt. Do not record credentials, launch external programs, or use destructive macro actions.

## N3 — Settings and mouse/gesture owners

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: settings [ ] PASS [ ] FAIL [x] NOT_RUN; gesture recording [ ] PASS [ ] FAIL [x] NOT_RUN.

Search `settings` → **Open settings** and `dashboard` → **Dashboard Settings**. Change a reversible visual option, Pop Out/Dock and Cancel; reopen to confirm it did not save. Change it again and Save; reopen to confirm it did. Search `mouse click settings` for Mouse Settings and `mg settings` → **Open mouse gesture settings**; make and cancel a reversible change in each. Search `mg edit` → **Edit mouse gestures**; use **Add Gesture** to create a disposable entry, draw in its **Recorder** canvas, then **Clear Recording** without **Use Recording**, including while detached. Close and reopen the editor: the empty entry may persist because edits auto-save, but no recorded stroke or tokens should have been applied. Delete the task-created entry if desired. Confirm mouse movement/clicks and the launcher hotkey still work and no input suppression remains. Restore any saved option in the disposable profile.

## N4 — Representative utility families

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: file/ZIP [ ] PASS [ ] FAIL [x] NOT_RUN; clipboard [ ] PASS [ ] FAIL [x] NOT_RUN; structured/table/time/image [ ] PASS [ ] FAIL [x] NOT_RUN.

Use only the task folder. Search `zip`, choose a small ZIP you create locally, choose an empty destination, Preview, and verify the destination stays empty until the explicit confirmation; extract one selected physical entry and verify its bytes. Search `cm` for Clipboard Modify and use synthetic clipboard text. Search `paste` → **Open Paste Sequences**, create two ordered synthetic text steps with a disposable text editor as target, press **Run** for each run and resolve any preflight, then use the on-screen **Next** while waiting for the next step (the shortcuts are disabled in this profile). Use **Start** only if the interface presents a Ready state. Complete two runs and verify the editor receives each pair once and in order. Open a structured editor or Table Utility, edit a small value and Pop Out/Dock; open Timer or Calendar and cancel an owned popup before closing. Search `imgdiff` and compare two disposable images, then Dock and confirm the pair remains. These are representative native checks, not a substitute for each surface's automated wiring evidence. Delete only outputs you created.

## N5 — Programmer Inspector and comparison

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: bit/insert [ ] PASS [ ] FAIL [x] NOT_RUN; comparison [ ] PASS [ ] FAIL [x] NOT_RUN.

In Calculation Pad enter `u64(9007199254740993)` and `123 >> 12` on separate lines. Confirm the second result is zero. On the first integer result choose **Inspect bits…**; bit 0 is least significant. Toggle a bit in the separate Inspector and verify Pad source is unchanged. Check binary/decimal/hex and **Copy**; choose a real source selection in the Pad, return to Inspector and **Insert at Pad selection**, then Undo once and confirm the exact original source. Choose **Select for comparison** on a Pad result; in Inspector use **Use selected result** for one side, type a second expression in the other, and **Compare / Refresh**. The comparison leaves Pad/history and Inspector bits unchanged. Change the source or selection: stale work stays visible; refresh explicitly before inserting against a new source. Check signed/unsigned mode, **Checked cast** vs **Reinterpret bits**, labels and copy formatting if time permits.

## N6 — Mermaid legacy

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: edit/view/export [ ] PASS [ ] FAIL [x] NOT_RUN; dirty decisions [ ] PASS [ ] FAIL [x] NOT_RUN.

Search `diagram studio` and choose **Open Diagram Studio**. In a new Mermaid document enter `flowchart LR` followed by `A --> B`, save a `.mld` inside the task folder and reopen it. Edit into invalid Mermaid briefly: current source remains visible and any last-good preview is identified as stale. Fix it, open Viewer and export the current diagram to a new path. Make another edit, attempt Open/New and Cancel the dirty decision; verify source and unsaved state remain. Discard only when intentionally moving on.

## N7 — Visual diagram, exports, draw.io

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: editable hierarchy/history [ ] PASS [ ] FAIL [x] NOT_RUN; PNG/SVG [ ] PASS [ ] FAIL [x] NOT_RUN; draw.io round trip [ ] PASS [ ] FAIL [x] NOT_RUN; independent reader [ ] PASS [ ] FAIL [x] NOT_RUN.

In Diagram Studio choose a **Visual Templates** starter (for example Architecture) and confirm the dirty transition. Add/edit shapes and text, a locally created harmless image, and a connector. Select two objects and use **Align** → **Left**; check their left edges align and Undo restores them. Use **Add layer**, verify it appears in the layer controls, then select multiple objects, **Group selection**, move the group, Undo/Redo, and use **Ungroup**. Create a container and horizontal/vertical swimlane; move/resize/rotate a parent and check members stay arranged, with independent child edits still possible. Save to a new `.mld`, reopen and check IDs/geometry/labels visually and by editing again. Open Viewer and confirm its navigation does not alter the editor.

Use **Export** → **Export SVG…** and **Export PNG…** to new task-folder paths. Try a scale and transparent/chosen background; inspect the actual saved files in local viewers and compare the current scene, frame headers and backgrounds. For draw.io use **Draw.io** → **Import diagram…**, then review → **Create .mld copy…** → **Open copy…** with the tracked [grouped fixture](fixtures/diagram-grouped-supported.drawio); preserve its original bytes. Export a reviewed separate `.drawio` copy and reimport it. If an independent draw.io-compatible local reader is installed, open that exported file there and inspect nested groups, lane labels and connectors; otherwise mark only that subcheck NOT_RUN. Prior XML parsing and app reimport are not independent rendering proof.

Supported interchange is deliberately bounded: unrotated nested parent groups/containers and horizontal/vertical lanes with supported styles and local geometry, plus rotated leaves and supported anchored connectors. Invisible group owners stay invisible. Rotated parents with descendants, relative vertex ports, unsupported routing offsets/waypoints/styles, active HTML/resources, external images, compressed/multipage documents, cross-layer ancestry and nonrepresentable stacking are refused/reported rather than silently flattened. A refusal of an out-of-profile file is expected; it is not proof of universal draw.io compatibility. Never upload private diagrams to an online service.

## N8 — Explorer tabs (Windows 11)

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: ordered five folders [ ] PASS [ ] FAIL [x] NOT_RUN; duplicates/cancel [ ] PASS [ ] FAIL [x] NOT_RUN; unrelated window [ ] PASS [ ] FAIL [x] NOT_RUN.

Requires Windows 11 Explorer with tabs and the provider capabilities reported by the application. Open one unrelated Explorer window on another empty task folder and leave it open. Search `explorertabs`, select the management opener, create a list, use **Add folder** to enter the five distinct `$folders` in order, then **Save list** and **Run saved list**. Verify exactly one newly identified Explorer window contains five tabs in that order, each showing the corresponding path. The previously open window's tabs and paths must remain unchanged. Read the application's verified prefix, uncertain and not-attempted report rather than assuming success from a window appearing.

Create a second list containing a repeated folder and verify the duplicate order, then start a separate run and use **Cancel run** or deliberately change focus during the run. It must stop safely and report any attempted-but-unverified and remaining paths honestly; do not expect rollback of a window/tab already created. If the supported Windows 11 provider is unavailable or refuses the target, record FAIL/capability gap and its sanitized reason; safe refusal is not a successful five-tab run. A non-Windows-11 machine is NOT_RUN. Close only the Explorer windows created for this test.

## N9 — Folder Size

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: totals/drill [ ] PASS [ ] FAIL [x] NOT_RUN; cancel/refresh/host [ ] PASS [ ] FAIL [x] NOT_RUN.

Search `foldersize`, open **Open Folder Size**, **Choose folder…** → `$tree`, enable **Include subfolders**, then **Scan**. The two regular files have logical sizes 11 and 17 bytes: expected aggregate is 28 bytes if the scan is complete; child is 17 bytes. Inspect sorted rows and drill/Up, then Pop Out and Dock without losing the selected result. Start Refresh, Cancel while active if feasible, and Refresh again. Cancelled/incomplete/permission results must be identified, not presented as complete totals; a small tree may finish before Cancel can be exercised, in which case that subcheck is NOT_RUN. No physical disk-space claim is implied.

## N10 — File users

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: inspect held file [ ] PASS [ ] FAIL [x] NOT_RUN; release/refresh [ ] PASS [ ] FAIL [x] NOT_RUN.

Optional user-operated helper: open a **second** PowerShell window and replace the placeholder with the printed `$lockFile` path. It only opens your disposable file and waits; press Enter in that window to release the handle. No admin, process termination, unlock action or change to another process is involved.

```powershell
$path = 'PASTE THE PRINTED TASK-ROOT held.txt PATH HERE'
$held = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
try { "Helper PID: $PID"; Read-Host 'Press Enter to release the disposable file' } finally { $held.Dispose() }
```

Search `filelock`, choose **Inspect File Users**, **Choose file…** → `$lockFile`, then **Inspect**. If Windows reports the helper, check its PID and creation identity; it must not be confused with a different process. The list is Windows-reported users, not a guarantee that every handle is found. Release the helper and **Refresh**: a successful empty result says **No processes reported**; errors, stale and incomplete metadata must remain distinct. Pop Out/Dock and use **Copy snapshot** only with this synthetic file. Do not kill or unlock anything.

## N11 — Silent GIF and MP4 recording

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: GIF [ ] PASS [ ] FAIL [x] NOT_RUN; MP4 [ ] PASS [ ] FAIL [x] NOT_RUN; non-primary/DPI [ ] PASS [ ] FAIL [x] NOT_RUN.

Search `record` → **Open Screen Recorder**. Put a plain text editor with synthetic moving content on screen. In **Source**, select **Window** for that editor or **Display / region** and use **Select region…**. Choose GIF, short duration/FPS, then **Choose output and record…** to a new task-folder path. After countdown, move only the synthetic content, hide the launcher, and use the compact **Stop** control. Wait for Saved and open the GIF locally: check movement, expected dimensions and plausible duration. Repeat with MP4; inspect/play it locally and confirm video only, with no audio track. If an encoder/capture target is unavailable, record its actual diagnostic rather than claiming a recording. Repeat on a non-primary display or different scaling only if available; otherwise mark that subcheck NOT_RUN.

## N12 — Cancellation, conflicts and cleanup

Gate: [ ] PASS [ ] FAIL [x] NOT_RUN. Subchecks: cancel/loss/repeat [ ] PASS [ ] FAIL [x] NOT_RUN; interlocks/live pin [ ] PASS [ ] FAIL [x] NOT_RUN; exit/cleanup [ ] PASS [ ] FAIL [x] NOT_RUN.

Using the same synthetic window, cancel region selection and countdown, then start and **Discard** a recording; no completed output should appear for a discarded run. Close a captured target during a separate short recording and verify any saved-early or failure status is truthful; repeat a fresh start/stop and confirm it succeeds after retirement. Search `pinboard live` → **Capture Live Window Pin** and select a crop of the synthetic window. With that live Pinboard item running, verify its visible frame updates before, during and after a recording. While the recorder owns capture, try starting another capture/selector and verify the UI refuses conflicting admission without interrupting the active recording; a still screenshot should work again after it finishes. Check normal mouse and launcher hotkey operation, then exit the application normally. Check only task-owned windows/processes; do not terminate unrelated processes. Once all task-owned windows are closed and files are no longer needed, delete the printed `$taskRoot` folder yourself.

## Feedback (redacted)

For each attempted item, send: **gate/subcheck ID**, PASS/FAIL/NOT_RUN, expected versus observed, Windows 11 build, executable hash, and minimal steps to reproduce using the synthetic fixture. Include a short sanitized error message if needed. Do not send private paths, clipboard/notes contents, credentials, full logs, or unredacted screenshots. For NOT_RUN state the missing prerequisite. For Explorer capability refusals on an otherwise applicable Windows 11 host, report FAIL and the reason, distinguishing any created window from verified tabs. All fields above remain NOT_RUN until you actually perform them.
