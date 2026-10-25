# Post-queue manual verification

**Acceptance sign-off: BLOCKED_PRODUCT_DEFECT.** Known selection-shield Escape leakage is unresolved. Use this sheet for the single deliberate hands-on review; a manual pass cannot establish that an unfixed source defect was repaired.

Prepared 2026-10-24 for branch `qa/post-queue-native-acceptance`. Last Rust source checkpoint: `f53bb83906548e1dd12ea2cee85949f5b14014fc`; final gate/commit metadata is in [results](POST_QUEUE_NATIVE_ACCEPTANCE_RESULTS.md) and [execution ledger](post_queue_native_acceptance_execution.md). Native evidence host: Windows 10 Home 22H2 / build 19045.6466, one 2560×1440 display at (0, 0), 96 DPI. Multi-monitor, negative-origin and higher-DPI acceptance remains unproved.

Run the bound M4 manual runtime artifact below, whose source was committed as `2b000ee140af56911267e1f32f8676053f6289b7` (compiled from HEAD `87c69b85` plus diff `cb31ce08`, fully recorded in results). Independent review confirmed later retained dev/test changes preserve normal runtime behavior. **This is not a final-HEAD build or an M6 native pass.** Use disposable profiles only; do not run master or your regular profile. The separate DFF smoke uses fake effects.

## Review priorities

Run the first five workflows in order, one disposable instance at a time. Record normal and small-window outcomes; repeat Jump ON/OFF where specified. Record actual DPI and monitor positions. If no second/negative-origin display or OS live source is available, mark that subcase NOT_TESTED rather than inferring a pass.

| Check | Area | Automated disposition | Priority / why human review remains | Evidence |
|---|---|---|---|---|
| 1 | Shield keyboard/input and pin click-through | M1 unresolved PRODUCT_DEFECT; mouse/probe subphases PASS_NATIVE. M2 HARNESS_DEFECT, native NOT_RUN | P0: known Escape leakage blocks sign-off; routing/recovery unproved. A manual pass cannot erase an unfixed source defect | `m1/input-primary/native/`, `m1/probe-primary/native/`, `m2/attempted-harness.patch` |
| 2 | Exact file/folder repair | M3 partial PASS_NATIVE, remaining HARNESS_DEFECT | P0: picker Cancel and durable exact-reference repair unproved | `m3/native-phase-review.json` |
| 3 | Notes Replace/history/persistence | M4 HARNESS_DEFECT, native workflow unverified | P1: live unsaved text, Undo/Redo, Save/Discard and Jump OFF unproved | `m4/native-phase-review.json` |
| 4 | DFF drain/restart/actions/retirement | M5 Start/Cancel/footer PASS_NATIVE; later HARNESS_DEFECT | P1: current generation, guarded actions and explicit join unproved | `m5/native-phase-review.json` |
| 5 | Settings slider/numeric and Cancel | M6 HARNESS_DEFECT, native NOT_RUN; attempted source restored | P1: exact current fields and live Cancel persistence unproved in both modes | `m6/attempt-archive.json`, `m6/group1-run/`, `m6/exact-test-causal-binding/` |
| 1-live | Real OS live source | Historical generated-live compositor proof only | P1: generated frames do not establish OS WGC acquisition | Prior G3 evidence; current OS-source NOT_RUN |
| 6 | Per-monitor brightness | Real inventory/Set unproved | P2 OPTIONAL: hardware capability unknown; Set requires consent and restoration | M8 final disposition in results |

Evidence paths above are under `target/multi-goal-queue/post-queue-acceptance/`; raw ignored receipts are local and are not included in Git pushes. The tracked results and execution ledger retain the classifications and hashes.

## Disposable setup

Use PowerShell 7. These are the retained M4 Cargo application/fixture artifacts, not a claim that they were built from the final source commit. Their normal-runtime source chain was independently reviewed. Never copy ordinary settings, notes or action stores into the fixtures.

```powershell
$repo = 'G:\Repos\rust\Multi_Launcher'
$appExe = Join-Path $repo 'target\fast-dev\multi_launcher.exe'
$fixtureExe = Join-Path $repo 'target\fast-dev\jump_mode_fixture.exe'
$smokeExe = Join-Path $repo 'target\fast-dev\duplicate_finder_smoke.exe'
$expected = @{
    $appExe = '23F51EDD6422D581495F5097D12C1745E8B17333726E2920A290EC78AC90CB92'
    $fixtureExe = '8331AA1A2FC19EAEA5F4C10DC3277DB218AC3A1F78FFCB9FB21482C8DB17DACA'
    $smokeExe = '10B96DE416C658618C372E32B9D97E8CC0ECC311A4CE56E6EFD1903C4C0EFFA0'
}
foreach ($path in $expected.Keys) {
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -cne $expected[$path]) {
        throw 'Artifact hash differs: stop and obtain the bound artifact, do not run a different build.'
    }
}
$reviewRoot = Join-Path $env:TEMP ('MultiLauncher-review-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $reviewRoot -ErrorAction Stop | Out-Null

function New-ReviewProfile {
    param([string]$Name, [string]$Scenario, [string]$Outcome = '')
    $profile = Join-Path $reviewRoot $Name
    if (Test-Path -LiteralPath $profile) { throw 'Use an absent profile directory' }
    $fixtureArgs = @('generate', $profile, $Scenario)
    if ($Outcome) { $fixtureArgs += $Outcome }
    Push-Location $reviewRoot
    try {
        & $fixtureExe @fixtureArgs | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Fixture generation failed' }
        $fixtureArgs[0] = 'verify'
        & $fixtureExe @fixtureArgs | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Fixture verification failed' }
    } finally { Pop-Location }
    return $profile
}

function Start-ReviewLauncher {
    param([string]$Profile)
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $appExe
    $start.WorkingDirectory = $Profile
    $start.UseShellExecute = $false
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Normal
    foreach ($key in @($start.Environment.Keys)) {
        if ($key.StartsWith('ML_QA_') -or $key.StartsWith('ML_TRACK_C_')) {
            [void]$start.Environment.Remove($key)
        }
    }
    $start.Environment['ML_NOTES_DIR'] = Join-Path $Profile 'notes'
    $start.Environment['ML_NOTE_TEMPLATES_DIR'] = Join-Path $Profile 'templates'
    $start.Environment['ML_SKIP_CLIPBOARD_SYNC'] = '1'
    return [Diagnostics.Process]::Start($start)
}

$repairCancel = New-ReviewProfile 'repair-cancel' 'target-repair' 'file-cancel'
$repairFile = New-ReviewProfile 'repair-file' 'target-repair' 'file-confirm'
$repairFolder = New-ReviewProfile 'repair-folder' 'target-repair' 'folder-confirm'
$pinProfile = New-ReviewProfile 'pin-input' 'note-replace-disabled'
$notesOn = New-ReviewProfile 'notes-on' 'note-replace-enabled'
$notesOff = New-ReviewProfile 'notes-off' 'note-replace-disabled'
$settingsOn = New-ReviewProfile 'settings-on' 'phase2-settings'
$settingsOff = New-ReviewProfile 'settings-off-ui-setup' 'phase2-settings'
# This is generated ON; change it through normal UI as described below before recording baseline.

# Example only: choose the workflow's profile and run one launcher at a time.
# $app = Start-ReviewLauncher $notesOn
```

Generator output goes to the console so each assigned profile variable contains only its directory. Generated profiles disable Paste Sequences' Next and Cancel shortcuts before constructing the application. Close other Launcher instances after saving any open work; run one disposable instance at a time. Plugin enablement for pinboard/DFF/brightness is a deliberate change to disposable data only. Keep fixtures until failure evidence is saved, then remove only directories you created for this review.

## First workflow: selection shield and pin input

**Known defect, not merely missing evidence:** the current rectangle fixture forwards Escape messages to its harmless receiver while cancelling the overlay. A manual pass cannot establish that this known defect was fixed; that requires a corrected implementation and new proof.

Automated proof: strict geometry rejection, TOPMOST recovery once, correct drag/no mouse or edit-selection leakage, second-attempt cancellation, balanced input and owned HWND/process cleanup. Keyboard isolation failed; ordinary post-cancel button delivery was not reached. See [results](POST_QUEUE_NATIVE_ACCEPTANCE_RESULTS.md) and `target/multi-goal-queue/post-queue-acceptance/m1/native-phase-review.json`.

- [ ] Start only `$pinProfile` with `Start-ReviewLauncher`. Enable **pinboard** through **File → Apps → Edit Plugins → Plugin Settings** in that disposable profile; do not change your ordinary profile.
- [ ] Prepare an empty harmless editor/click target, with no valuable text or destructive action, beneath the selection area. Close unrelated macros that could intercept input; record their original state for restoration.
- [ ] In the disposable launcher, enter `pinboard capture` and choose **Capture Screenshot Pin**. This is the shared shield owner. The `ss` region route uses Windows snipping and does not prove this owner.
- [ ] Drag over the harmless target and release. Expect the rectangle capture to finish, without an underlying selection/click or stuck capture state.
- [ ] Start another capture and press Escape. Expect overlay closure and restored ordinary input. Record any unexpected response in the underlying harmless target; cancellation alone does not establish keyboard isolation.
- [ ] Try one ordinary click on the harmless target after cancellation. Record whether input is restored. This phase lacks automated proof because the fixture stopped at the keyboard failure.
- [ ] If a second or negative-origin monitor is available, repeat only this short sequence there and record DPI/monitor arrangement. Do not infer support from a single-monitor run.
- [ ] On failure, retain one reproducible sequence, branch/commit, Windows build and a screenshot restricted to harmless test content. The M1 automated runtime logs are under `target/multi-goal-queue/post-queue-acceptance/m1/input-primary/native/`.
- [ ] Close test pins/overlays and the disposable editor, discard disposable data, and restore paused automation. Confirm no shield or held input remains.

**Pin routing remains manual:** M2 stopped before native launch because its child-viewport guard regression failed after the permitted correction; the unsuccessful source attempt was restored. This does not establish a product routing failure or a pass.

- [ ] Place a static test pin over the harmless editor/click target. Set **Opacity** to 50%; record **Always on Top** and confirm the harmless background remains visible through the image.
- [ ] Enable the pin's **Click-Through** checkbox. Click the same harmless target through the image. Expect exactly one underlying click/edit response and no pin action; opacity must remain 50% and topmost unchanged.
- [ ] In the launcher enter `pinboard list`. Find that exact pin and click **Recover** on its row. Click the same image point again. Expect pin-owned input and no underlying click; no Reset or opacity change.
- [ ] Use **Reset opacity to 100%** only as a separate explicit action. Expect full opacity. Close the pin and verify ordinary input remains usable. Repeat live-source input only with a harmless disposable source; generated-live automation does not establish OS WGC capture. Keep OS live-source evidence separate from historical generated-live composition.

- [ ] **Real OS live source:** open a harmless disposable source window, query `pinboard live`, and select only that source through the normal capture flow. Change its harmless visible content and confirm the pin updates from the actual OS source. At 50% opacity confirm background composition, then close the pin/source and check cleanup. Record actual source, monitor/DPI, update behavior and any capture error. Generated-live fixture frames cannot satisfy this check. Mark NOT_TESTED if a safe source is unavailable.
## Second workflow: repair exact saved file and folder targets

M3 now proves exact Action Sheet → Repair entry and owned **folder picker Escape** with fresh same-token ROOT return, empty draft, unchanged live stores and zero target activation. File picker Cancel and both confirmations remain unverified. Browse sits inside a vertical form scroller; reveal it by scrolling the form or enlarging the resizable dialog rather than clicking an offscreen coordinate. Native file attempts stopped at this point. Native folder confirmation was deliberately not entered because its path-entry control was unproved.

- [ ] Use three separately generated disposable `target-repair` profiles with explicit `file-cancel`, `file-confirm`, and `folder-confirm` outcomes. Use the three generated repair profiles above; do not use regular action stores.
- [ ] Search `app Jump QA Repairable App`, select that exact saved record, press Ctrl+Enter, then choose **Repair target…**. Confirm its original label/description/arguments before proceeding. Never choose Execute.
- [ ] For file Cancel, scroll the form until **Choose file…** is visible, open its picker, then Cancel. Expect the same empty replacement draft, unchanged original record and all five live store hashes. Close the repair dialog with Cancel.
- [ ] In the separate file-confirm profile, choose generated `target-repair/replacements/Selected document Ω.txt`, then **Confirm repair**. Expect only that exact saved target and supported favorites/pins/usage identities to rekey; original label, description, arguments, order and counts survive; the unrelated action and folder remain unchanged; no target executes.
- [ ] In the separate folder-confirm profile, search `f Jump QA Repairable Folder Alias`, Ctrl+Enter → **Repair target…**, choose the generated `target-repair/replacements/Selected folder Ω`, then **Confirm repair**. Expect its label/alias/order/references/counts preserved and file/unrelated records untouched.
- [ ] Compare live Cancel bytes before closing the launcher. After normal exit, compare exact usage identities/counts separately: current serialization can reorder its array while retaining counts7/5/11. Do not confuse this normal shutdown order change with a repair rekey or a byte-identical result.
- [ ] On failure, record which exact row/profile/picker stage failed, whether Browse was visible, expected versus actual target/reference identities and safe hashes; capture only generated content. Close owned pickers/dialogs normally and retain disposable fixtures until evidence is saved.



## Third workflow: Notes Replace and the existing editor history

M4 is terminal HARNESS_DEFECT with unverified native ownership. Four bounded sessions never reached note opening/replacement; two normal exits and two forced retirements left all notes unchanged. Both mode attempt caps are closed. See `target/multi-goal-queue/post-queue-acceptance/m4/native-phase-review.json`. Use separately generated `note-replace-enabled` and `note-replace-disabled` profiles and run one launcher at a time; do not reuse a modified note profile for the second mode. Use the bound artifacts and independent profiles above.

- [ ] In each profile, query `note open Jump QA Replace Note` and open the exact generated note. Record hashes of `notes/jump-qa-note-001.md` and unrelated `jump-qa-note-002.md` before editing.
- [ ] With Jump ON, complete the observed hint and confirm it only focuses/selects the result; use a separate fresh Enter to open it. With Jump OFF, use ordinary mouse/keyboard, with no hint requirement. Confirm there is only one opening action and no extra query text.
- [ ] Click the actual editor, append a harmless Unicode marker, and confirm the note is dirty while both disk files remain unchanged. Open **Find**, expand **Replace**, search the generated literal `literal Ω $1\path`, and enter a replacement containing literal Unicode, `$1` and backslash. Expect two source matches, including the source-only Markdown destination.
- [ ] Use **Replace** once. Expect exactly one selected occurrence to change and one original match to remain; disk remains unchanged. Close Find, click the editor, Undo once, then Redo once. Expect the exact prior/changed live source respectively, including the unsaved marker.
- [ ] Reopen Find and use **Replace All** on the remaining original match. Search the replacement string and expect two matches. Check **Split**, then **Render**, and use **Reveal in Edit** for the source-only match: it should reveal the matching source without changing text or dirty status.
- [ ] Close Find and deliberately click the editor before testing Undo/Redo for Replace All. One Undo restores the exact pre-All source; one Redo restores the exact post-All source. A shortcut while a Find field owns focus does not establish editor history behavior.
- [ ] Click **Save**. Expect exact UTF-8 source bytes, including CRLF, Unicode and literal special characters, and a clean note. The unrelated second note stays byte-identical.
- [ ] Append another unsaved marker, then use the normal Escape/close flow and **Discard Changes**. Expect saved bytes to survive, the new marker to disappear and no phantom launcher action. Reopen the note to confirm.
- [ ] Repeat the bounded workflow in the other Jump profile and at a comfortably usable small window size. On failure record the exact history step, focused field, expected versus actual hashes and safe screenshot. Close the disposable launcher normally; retain fixtures until results are saved.

## Fourth workflow: Duplicate File Finder cancellation and actions

M5 proves normal/constrained footer geometry, guarded real Start, the held first-read fixture gate, and Cancel acknowledgement with generation advancement, empty results and Start disabled while draining. The `fixture-release`/`release_gate` observation-key mismatch stopped automation before Release. Drain completion, restart, current groups, actions and explicit worker-joined acknowledgement remain required manual checks. The dedicated smoke fixture uses the production dialog with fake Open/Reveal/Copy effects; the ordinary application uses real adapters. Keep those evidence types separate. Receipts: `target/multi-goal-queue/post-queue-acceptance/m5/native-phase-review.json`.

Create the tree explicitly; this recipe does not depend on ignored automation scripts. Run it once inside the fresh `$reviewRoot` prepared above. Its four duplicate members form two groups; the equal-length unequal pair must never group. The repeated `same` directory names must not confuse alias identity.

```powershell
$dffProfile = New-ReviewProfile 'dff-profile' 'note-replace-disabled'
$dffBase = Join-Path $reviewRoot 'dff-data'
if (Test-Path -LiteralPath $dffBase) { throw 'Use a fresh DFF tree' }
$dffTree = Join-Path $dffBase 'tree'
$dffNested = Join-Path $dffTree 'nested'
foreach ($part in @('A\same', 'nested\B\same')) {
    [void][IO.Directory]::CreateDirectory((Join-Path $dffTree $part))
}
$utf8 = [Text.UTF8Encoding]::new($false)
$original = Join-Path $dffTree 'A\same\source-Ω.bin'
$copy = Join-Path $dffTree 'nested\B\same\different-name.dat'
[IO.File]::WriteAllBytes($original, $utf8.GetBytes('duplicate bytes!'))
[IO.File]::WriteAllBytes($copy, $utf8.GetBytes('duplicate bytes!'))
[IO.File]::WriteAllBytes((Join-Path $dffTree 'same-size-other.bin'), $utf8.GetBytes('alternate bytes!'))
[IO.File]::WriteAllBytes((Join-Path $dffTree 'empty-one.bin'), [byte[]]@())
[IO.File]::WriteAllBytes((Join-Path $dffNested 'empty-two.bin'), [byte[]]@())
[IO.File]::WriteAllBytes((Join-Path $dffTree 'only-once.txt'), $utf8.GetBytes('one unique payload that has no twin.'))
$hardlinkAvailable = $false
try {
    New-Item -ItemType HardLink -Path (Join-Path $dffTree 'A\same\hardlink.bin') -Target $original -ErrorAction Stop | Out-Null
    $hardlinkAvailable = $true
} catch { Write-Host 'Hardlinks unavailable: record this case as NOT_TESTED.' }
$dffBefore = @(Get-ChildItem -LiteralPath $dffTree -File -Recurse | ForEach-Object {
    [pscustomobject]@{Path=$_.FullName; Length=$_.Length; SHA256=(Get-FileHash -LiteralPath $_.FullName).Hash}
})
$dffBefore | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $dffBase 'before.json') -Encoding utf8
```

Use `$dffTree` and `$dffNested` as the two roots. Expected: one group containing the two independent 16-byte copies (a supported hardlink is another alias of the same physical source, not another independent duplicate), and one group containing the two empty files. `same-size-other.bin` and `only-once.txt` stay outside groups. After closure, re-list the tree and compare its exact path set, lengths and hashes against `before.json`.

The dedicated fake-effects smoke executable is a separate bound artifact from the launcher. Choose a fresh output directory; the executable permits an existing directory and can overwrite receipt filenames. Run the following from the same PowerShell session:

```powershell
$dffOut = Join-Path $reviewRoot ('dff-receipts-' + [guid]::NewGuid().ToString('N'))
if (Test-Path -LiteralPath $dffOut) { throw 'Use an absent output directory' }
Push-Location $reviewRoot
try {
    & $smokeExe --fixture-identity ('manual-dff-' + [guid]::NewGuid().ToString('N')) `
        --root $dffTree --root $dffNested --out $dffOut
} finally { Pop-Location }
```

Expect title **Duplicate Finder M5 Fixture**. Retain `$dffOut\observation.json` and `$dffOut\retirement.json`. A normal retirement receipt must match the fixture identity/process and report `worker_joined: true`; forced termination does not prove this.

The visible **Release fixture read gate** control is intended for the manual Cancel/drain/restart check; the automated driver key mismatch does not justify bypassing it. Keep normal-app results and fake-effect smoke results in separate log entries.

- [ ] Generate a fresh disposable duplicate tree and use only its recorded roots, including the overlapping nested root. Record its full path set and file hashes. Do not scan personal folders.
- [ ] In a separate disposable profile enable **duplicate_finder** through **File → Apps → Edit Plugins → Plugin Settings**, then query `dff` and select **Open Duplicate Finder**. Use **Add Root** for the exact generated roots; there is no `dff <path>` grammar.
- [ ] At normal and constrained window size confirm **Start Scan**, **Cancel Scan** and close controls stay reachable without covering result rows. Start a scan and check exact groups against the fixture, including empty files, Unicode paths, unequal same-size files and hardlink deduplication where supported.
- [ ] For a repeatable cancellation test use the owned smoke fixture's held read gate: **Start Scan → Cancel Scan** must acknowledge cancellation, publish no stale groups, and keep Start unavailable while the old worker drains. **Release fixture read gate**, then use **Start Scan** again; there is no separate Restart button. The new generation must produce only current verified groups.
- [ ] Select an exact current validated alias and exercise **Open**, **Reveal** and **Copy Paths** through the fixture's recording adapter. Confirm one intended effect per action and no real Explorer/clipboard operation. In the normal app inspect controls without using actions unless their external effect is deliberately desired.
- [ ] Preserve stale-alias refusal as a required check; do not alter a live native fixture tree while a scan owns it. Use the final retained stale-source regression receipt or a separately closed disposable test as described in the final handoff.
- [ ] Close normally and confirm worker retirement before process exit. Compare original tree paths and hashes; no Delete/Move/Recycle operation should exist. On failure retain generation/stage/alias identity, retirement receipt and safe logs. Close the fixture and discard only owned disposable data after evidence is saved.

## Fifth workflow: Settings exact numeric control and Cancel

M6 is terminal HARNESS_DEFECT: the current-control guard regression failed after its one causal recheck, before any native build/input. The eight-file attempt was archived and restored. Use supported disposable profiles and the normal Settings UI for ON/OFF setup; the attempted `phase2-settings-jump-disabled` variant is not retained. Use the supported UI setup below.

- [ ] For the separate OFF profile, query `settings` → **Open settings**, expand **Launcher Window & Appearance**, clear **Experimental Jump Mode**, then **Save**. Wait until **Saving settings…** disappears with no error. Reopen, verify the checkbox remains clear, and **Cancel**. Only now record settings.json hash and committed Fuzzy weight baseline. This setup Save is separate from the tested Cancel workflow; do not rerun the ON fixture verifier or call this an OFF-generated fixture.
- [ ] Open the disposable profile and query `settings fuzzy weight`. Select **Fuzzy match weight** (canonical `search.fuzzy_weight`) for the actual **Fuzzy weight** slider. With Jump ON, a hint focuses/selects and later fresh activation navigates; with Jump OFF, ordinary mouse/keyboard works without hints.
- [ ] Confirm navigation focuses the actual slider without changing its value. Send one deliberate arrow edit and observe draft change while committed settings stay at baseline.
- [ ] Select **Fuzzy match weight value**, the separate numeric destination `search.fuzzy_weight/value`. Confirm the actual numeric editor owns focus; deliberately edit its value and check the second draft change. An expired highlight is not proof that the current control stopped working.
- [ ] Click **Cancel**. Expect the Settings editor to close, committed value and live settings-file bytes to remain at baseline, and the original Jump ON/OFF gate to survive. Record post-exit serialization separately from live Cancel proof.
- [ ] Repeat in the other Jump profile, and confirm controls remain reachable at a small usable window size. If a built-in plugin is disabled, discovery must not silently enable it. On failure capture canonical destination, focused control, draft/committed values, safe hashes and exact step. Close normally and preserve only disposable evidence.
## Optional hardware check — requires explicit user consent

Real brightness Set has not been performed by Codex. Hardware capability remains unknown unless the final report records an actual read-only inventory. Skip this check without treating the skip as a product failure.

- [ ] Consent to changing one intended physical display, and enable only **brightness** in the separate disposable profile if needed. Query `bright` and select **bright: edit brightness**. Do not use `bright <number>` for this per-device check.
- [ ] Opening the dialog performs its initial inventory. Identify the exact monitor; duplicate names may include Display/Monitor ordinals. Record its displayed **Current** baseline. Do not infer raw native min/max from the percentage.
- [ ] Only after confirming the target and baseline, adjust that row's **Level** by a small amount and press its explicit **Set**. Confirm automatic readback and actual display response.
- [ ] Restore the baseline with **Level → Set** and confirm readback. Stop on stale identity, unknown outcome or cleanup failure; refresh inventory before considering another Set. Close with **Cancel**.

## Optional confidence checks

Previously accepted native effects do not need an exhaustive repeat. If useful, spend about two minutes on a generated-image `imgdiff` wipe/difference, delayed screenshot cancellation on harmless content, CSV/table explicit output and unchanged source, or fake macro/Paste utility data. Generated-live pin composition is not proof of a real OS WGC source. Real OS live capture remains an explicit task in the first workflow; generated-live evidence does not close it.

## Review result log

Copy one block per workflow; report a failure without deleting the fixture or attempting irreversible cleanup.

```text
Check ID / workflow:
Result: PASS / FAIL / BLOCKED / NOT_TESTED
Date and time:
Windows edition/build:
Monitor arrangement, DPI/scaling, and which display:
Branch and exact source commit:
Executable path and SHA256:
Fixture scenario / generated profile identity:
Jump ON or OFF; normal or small window:
Exact chronological reproduction / first failing step:
Expected visible result and disk/hash result:
Actual result; focus/owner/generation when relevant:
Safe screenshot/log/receipt path:
Cleanup: normal close / worker retirement / input restored / remaining process:
Private data excluded: yes / no (do not attach private material)
Optional brightness consent and baseline restored, if applicable:
```

Keep one result log per workflow/mode. Preserve failed disposable fixtures until evidence is saved. Close only the processes, windows and directories created for this review. Final cross-owner and lint outcomes are in the results report.

Final gate metadata (2026-10-24): M9 cross-owner groups6/6 and5/5 passed; both final all-target Clippy configurations passed with warnings denied on source f53bb839. Current full suite remains6965pass/3fail/1timeout/48skipped. All owned processes closed; native acceptance remains BLOCKED_PRODUCT_DEFECT. The enclosing final metadata commit/push is identified in the handoff; these gate results do not change the M4 executable build identity or erase any manual gap.