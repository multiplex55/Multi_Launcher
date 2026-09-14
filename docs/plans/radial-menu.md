# Native radial menu implementation ledger

Status: approved, M0-M3 complete

Canonical requirements: `docs/multi_launcher_radial_codex_plan.md`
Historical navigation notes: `docs/multi_launcher_radial_source_notes.md`

This is the persistent execution ledger. Historical notes are evidence only where
revalidated below. Reference scripts are read as text/data and never executed.

## Immutable Git baseline record

| Field | Recorded value |
|---|---|
| feature branch | `radial-menu-1` (attached) |
| baseline kind | `first-feature-branch-commit` |
| baseline commit | `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` |
| baseline subject | `radial refs` |
| baseline timestamp | `2026-09-13 10:26:41 -0400` |
| branch point commit | `f7c5f61ed2faa2288f5c19ddeaea66de6f760a2b` |
| master ref | `refs/heads/master` |
| master tip at resolution | `f7c5f61ed2faa2288f5c19ddeaea66de6f760a2b` |
| task-start commit | `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` |
| task-start changes | none; staged, unstaged, and untracked state clean |
| shallow repository | `false` |

Selection evidence: the first-parent reverse range from the recorded master tip to
task-start HEAD returned exactly the pinned baseline. Its sole parent is the recorded
branch point. The branch reflog records creation at the branch point and a fast-forward
to this commit. The baseline is an ancestor of task-start HEAD. This full SHA is pinned
and must be reused; later feature work must not recalculate or move it.

At task start, baseline, task-start HEAD, and current source were identical. The
branch-point comparison contains only the briefs and repository-local references.
The baseline is comparison evidence and must never be checked out over this worktree.

## Repository-local reference manifest

All inputs were tracked and unchanged at task start. ZIP inspection was read-only;
nothing was executed or extracted into the repository.

| Path | Bytes | SHA-256 | Role |
|---|---:|---|---|
| `docs/references/Radial menu v4.zip` | 4,009,952 | `442A8A16128C64012C85AC520A6B4AF49027A70BC39B9F362DA40628E8FCEFDA` | RM4 4.48 source, real settings/menu definitions, five skins, licenses |
| `docs/references/RadifyClass-RadifySkinEditor-main.zip` | 15,213,205 | `16F67A5100ADA57915966017EDA56626F77A7668D6CC583C99B82432716B920C` | Radify 1.2.0 source/editor, README, ten skins |
| `docs/references/screenshots/reference1.png` | 451,287 | `65E533D509465D5C6DBCBB01CAF7CDB72069639BF0D26198B23DE1736FF9FAC2` | RM4 ornate multi-ring collage |
| `docs/references/screenshots/reference2.png` | 55,584 | `17CDA73BE314BEACE70330714EA650B5FD676E67AA4A04563CD563FCE9C6AD09` | compact Carbon concentric menu |
| `docs/references/screenshots/reference3.png` | 126,625 | `67D047A99EE4AC6AD9E9EAC0DD8C6B1BE353A4241B023854AAA8CF343813D286` | editor UI inspiration; visible v1.0.0 title is not archive-version evidence |
| `docs/references/screenshots/reference4.png` | 83,189 | `1E408AF0323862D2BBAF0E8EC67EB19A4A385A805CC9F2F3B77EE7BD4BECDCB6` | dense emoji rings |
| `docs/references/screenshots/reference5.png` | 97,168 | `ED7A3E0FA3EF9A1A8C2FF500E6ADB19EB98C3B2E04507D6EEC7227F73859E171` | RM4 light/plain menu |
| `docs/references/screenshots/reference6.png` | 1,386,949 | `0A09CCCE0B607D87AF563EA2869522BB94F7EA37622D664A567FEFA582679808` | Carbon/wood variants; wood fixture absent from supplied archive |
| `docs/references/screenshots/reference7.png` | 63,948 | `B19CD5B865D4FC38A902634C909F815470DCFABD106195C8E4CE25A76CDC2A41` | Carbon symbol-output use case |

RM4 has 135 entries, 5,275,818 expanded bytes, and no detected traversal, rooted
path, ADS-like name, case-fold collision, or symlink. Radify has 296 entries,
20,051,046 expanded bytes, and the same clean static checks. Static screenshots prove
appearance only, never focus, timing, hit testing, or cross-process click-through.

Selected archive-member evidence:

- Radify `README.md` SHA-256 `FE494779A0D47C5BE82878EE6E8E43AC2BE11B6BA25F8FFEE856A129F49513B4`.
- Radify `Radify.ahk` SHA-256 `3C8C00A8A0CE1EE040C6D1D9722E5DB3AA9DDB9D8BDF1EBE8C1C18FA8E7B9B58`.
- Radify `Radify Skin Editor.ahk` SHA-256 `E5DD31C9A3338E287D8C99F35FB35CE1B5F01E510BDAC6519BA3C4103C671F99`.
- Radify `Radify Menus.ahk` SHA-256 `9ED3DD689764792B57E5A348FF1D3AC581341227A841A9506B661E800A713553`.
- RM4 `Internal/Codes/RM2module.ahk` SHA-256 `63FE3530D0347E8EFC9CC4094EA5D391435E976B9BDA3DDBD1E10A885163F222`.
- RM4 `Menu definitions/General settings.txt` SHA-256 `1F14564054A3DA8E0F7EE45B63CD007B3C3DB2D27F1F4569DEA0A1A36A16C185`.
- RM4 `Skins/Orb/Skin definition.txt` SHA-256 `9698626ECD4B7263B495EEC16BE4C5FDABF7E095D51310C906D7CB5CAAF1C56B`.

Radify is MIT, but stock-image/icon sublicensing is not proven by the root license.
RM4 4.48 has a restrictive non-commercial/no-redistribution license and mixed asset
rights. Both packages remain reference/import fixtures; built-in runtime assets will
be original native/vector primitives unless separate rights are established.

## Revalidated source map

Because baseline and current source are identical, every confirmed current finding
is also confirmed at the pinned baseline.

| Area | Revalidated evidence and decision |
|---|---|
| action domain | `ActionSurface::RadialMenu` exists in `src/universal_actions/model.rs`; presentation stays separate from physical `ActivationSource`. |
| stable targets | `PersistedUniversalActionRef` and `PersistableActionTargetRef` exclude transient HWND/index identities; production reverse resolution is missing and will be added. |
| execution | `LauncherApp::execute_universal_action` owns availability/confirmation; confirmed execution ignores surface and primary actions can mutate launcher state. Add a narrow radial-origin policy, not a second executor or whole-app snapshot restore. |
| launcher trigger | `HotkeyTrigger` is a 20 ms polling boolean with no release/timestamp/provenance. Shared tap/hold needs one dedicated lifecycle service and pure reducer. Disabled mode retains the exact legacy path. |
| Screen Draw | `take_screen_draw_trigger_actions` runs before visibility. Emergency/recovery remains immediate and consumes co-fire before radial timing. |
| gestures/input | `GestureSuppressionGuard` is reusable and independent of gesture enablement. Existing gesture hooks are not reusable unchanged; they reject all injected input and lack complete key-cycle ownership. |
| native lifecycle | Screen Draw supplies message-wakeup/readiness/idempotent-cleanup patterns; its passive transparent overlay and launcher parking are not an interactive radial host. MkMacro supplies `MA_NOACTIVATE` input-shield precedent. |
| visibility | Short taps route to the existing visibility toggle. Holds never set that trigger. `PreserveCurrentGeometry` remains the root restoration policy. |
| persistence | `AppDataRoot`, atomic persistence, catalog/recovery, watchers, and revision patterns are reused. Add radial definition and asset stores plus retained-last-valid publication. |
| GUI/editors | Editors integrate through `Panel` and existing settings/dialog routing, use stable entity IDs, immutable drafts, revision-aware Save/Apply/Cancel, and the shared scene renderer. |
| dependencies/tests | Pinned/current versions are egui/eframe 0.27, windows 0.58, image 0.24. Tests use unit modules, grouped `domain`/`plugin_queries`, and top-level integrations. |

## Architecture decisions

1. `radial::model`, `validation`, and `store` own durable definitions, stable IDs,
   migrations, limits, revision transactions, and immutable published snapshots.
2. `hotkey::launcher_invocation` owns the configured launcher chord only while
   shared mode is enabled. A pure timestamped reducer owns tap/hold/dismiss/drain
   semantics. It performs no sleeping or IO.
3. `radial::session` owns one active tree, navigation stack, captured config/context
   revision, arming, pointer ownership, generations, paging, and dispatch tokens.
4. `radial::geometry` creates one immutable layout snapshot shared by renderer and
   hit testing, with explicit desktop-physical/logical coordinates.
5. `radial::native` owns one event-driven Win32 host thread, readiness and cleanup
   acknowledgements, layered composition, nonactivation, and explicit input regions.
   It uses no second eframe application and no window per cell.
6. `radial::bindings` resolves stable references on open and revalidates before
   GUI-owned execution. It never retargets missing identities.
7. `radial::render`, `skin`, and `assets` own effective style, bounded caching,
   fallback rendering, and a shared runtime/editor scene.
8. egui menu/skin editors own revisioned drafts and bounded undo/redo only. Preview
   never dispatches; explicit Test routes through the normal safety executor.
9. Screen Draw emergency/recovery, quit, and exclusive input owners classify first.
   Crop, Screen Draw, recording, and text injection dispatch only after radial cleanup
   and claimed invocation-key release.

## Compatibility inventory

Classification terms: `native`, `translated`, `incompatible`, `not-applicable`.
Every accepted field must affect runtime/preview, have editor/import coverage, and be
validated; serialized-but-unused fields are not accepted.

Radify menu/default fields:

| Classification | Fields |
|---|---|
| native | `skin`, `itemGlowImage`, `menuOuterRimImage`, `menuBackgroundImage`, `itemBackgroundImage`, `centerBackgroundImage`, `centerImage`, `submenuIndicatorImage`, `itemSize`, `radiusScale`, `centerSize`, `centerImageScale`, `itemImageScale`, `itemImageYRatio`, `submenuIndicatorSize`, `submenuIndicatorYRatio`, `outerRingMargin`, `outerRimWidth`, `itemBackgroundImageOnCenter`, `itemBackgroundImageOnItems`, `menuClick`, `menuRightClick`, `centerClick`, `centerRightClick`, `mirrorClickToRightClick`, `closeOnItemClick`, `closeOnItemRightClick`, `enableItemText`, `enableGlow`, `autoTooltip`, `enableTooltip`, `alwaysOnTop`, `activateOnShow`, `fillCenterHitZone`, `fillItemsHitZone`, `textColor`, `textFont`, `textSize`, `textFontOptions`, `textShadowColor`, `textShadowOffset`, `textBoxScale`, `textYRatio`, `soundOnShow`, `soundOnClose`, `soundOnSelect`, `soundOnSubShow`, `soundOnSubClose` |
| translated | `textRendering`, `smoothingMode`, `interpolationMode` map to named native quality modes |
| incompatible | `autoCenterMouse` is diagnosed/disabled because cursor warping is prohibited; `closeMenuBlock` cannot disable Esc/emergency recovery; raw `guiOptions` AHK flags are not portable |

Radify public item fields `Image`, `Tooltip`, `Text`, `Click`, `RightClick`,
`CtrlClick`, `ShiftClick`, `AltClick`, five `Hotkey*`, five `Hotstring*`,
`Submenu`, and `SubmenuOptions` map to typed native content/input/style scopes.
Local shortcuts are native; global variants require explicit opt-in/conflict ownership.
Submenus inherit skin-defined style only, not arbitrary parent behavior. Explicit
false, zero, and clear remain distinct from inheritance.

RM4 evidence includes 56 real skin attributes across five `Skin definition.txt`
fixtures. Image/text/glow/shadow/rim/center/size/opacity attributes are native or
translated. Narrow `fac+N`, `fac-N`, `add+N`, and `add-N` size expressions are
translated as data. GDI+ quality integers become named native modes. Arbitrary color
matrices/hatch modes are diagnosed unless a bounded equivalent is implemented.
Process handles/pointers, AHK functions/actions/variables, raw GUI flags, updater,
process-priority, cursor warp, power scripts, and arbitrary executable definitions
are incompatible or not applicable and are never evaluated. Known `Close`,
`CloseMenu`, and `Drag` controls translate to native intents.

Supported user-selected image data targets PNG/JPEG/BMP and, after codec review,
ICO/GIF/TIFF with explicit static/animated semantics. Raw `hIcon`, `hBitmap`, and
`pBitmap` are incompatible. EXE/DLL/CPL icon resources may be parsed only as data,
never by executing or initializing imported code.

## Milestones

| Milestone | State | Acceptance/verification boundary |
|---|---|---|
| M0 source audit, parity, ledger | complete | ledger reviewed against the approved brief; `git diff --check` passed; no baseline suite |
| M1 model/store/reducer/session/geometry | complete | reviewer remediation passed a source-identical focused core/unit Nextest batch |
| M2 native input and host | complete | recovery ownership provenance remediation passed the source-identical focused gate; interactive native probe remains explicitly unverified |
| M3 actions/context/submenus/handoffs | complete | decision-review remediation passed the combined focused M3 Nextest gate; native interactive evidence remains unverified |
| M4 skins/assets/import/export/recovery | pending | focused store/skin/import/resource batch |
| M5 complete editors/settings/commands/starters | pending | focused editor/command/serialization batch |
| M6 hardening/performance/full verification/review | pending | format/check/diff, full Nextest, native evidence, serialized baseline/candidate measurements, independent review |

Each milestone is implemented and committed sequentially by one writer. Tests are
written with each coherent batch and run only at gates. One Cargo/build/test job may
use this checkout/target at a time.

## Verification and job records

Read-only tooling inspection: `cargo-nextest 0.9.135`; `cargo nextest run --help`
confirmed expression filters, target selection, captured output, status levels, and
no-capture serialization. No build or test job has yet run. Before every gate, record
command, cwd, source revision/diff identity, profile/environment, start time,
session/PID, log path, and eventual true exit code. Reattach to quiet jobs using the
required 60/120/180/up-to-300-second observation cadence.

M0 verification: baseline/ref/source audits completed read-only; the planned
seven-milestone boundaries were independently reviewed; `git diff --check` exited 0.

M1 focused gate job (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD adf98d4771d673adad5c24d149fa860b4db5910d` plus the M1 paths reported by `git status --short`; radial source SHA-256 values captured at launch (`geometry A8281D81...`, `invocation 640C051A...`, `model 23E89C61...`, `session 463B64A0...`, `store F5839BD2...`, `validation 9AC6ED1D...`) |
| profile/environment | default Nextest profile; existing incremental target; no toolchain/dependency/profile changes |
| requested start | `2026-09-13T10:59:50.7340247-04:00` |
| preflight | no `cargo`, `cargo-nextest`, or `rustc` process was active |
| session/PID | exec session `41032`; observed `cargo` PID `1992`, `cargo-nextest` PID `41516`, child Cargo PID `47532` |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m1-nextest.log` |
| exit/result | exit `0`; compile completed in `25m 11s`; Nextest run `52357e71-27da-4df5-8ea8-ef0e8c393edd`; 38 passed, 0 failed, 4,008 filtered/skipped across 70 discovered binaries; test execution `0.425s` |

The compiler reported one unused tuple binding in `radial::invocation`; after the
successful gate, that binding alone was removed without changing control flow. Its
post-cleanup file SHA-256 is
`E6B3B2129ACE9F45EBAAF8126B5515790FCD23EED33AED3B7451ED23F018FEB7`.
The milestone's exactly-one focused Nextest constraint precluded a duplicate rebuild.
`git diff --check` passed after this mechanical cleanup, and no Cargo/Nextest/rustc
process remained active. No full suite or native probe was run at M1.

M1 reviewer-remediation focused gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD adf98d4771d673adad5c24d149fa860b4db5910d` plus the M1 worktree; remediation source SHA-256: geometry `35DE11EC...`, invocation `354C9433...`, session `614D0416...`, store `65ADB2A4...`, validation `606F54F8...`; unchanged model `23E89C61...`, module root `180BAD7B...` |
| profile/environment | default Nextest profile; existing incremental target; no toolchain/dependency/profile changes |
| requested start | `2026-09-13T11:45:26.5196456-04:00` |
| preflight | no `cargo`, `cargo-nextest`, or `rustc` process was active |
| session/PID | exec session `68264` |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m1-remediation-nextest.log` |
| exit/result | exit `101`; compile stopped on ambiguous float literals in the new one-cell-wedge test before tests executed |

The first remediation attempt exposed a compile-only fixture defect: the angle array
needed an explicit `f32` element before calling `sin`/`cos`. The fixture was corrected
without changing production behavior. A successful replacement gate is required below.

M1 reviewer-remediation replacement gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | same recorded remediation snapshot except corrected geometry fixture SHA-256 `C8D270DD6B67EF76DF1F6ABE4CFB5FAC4D4B5C461D0BFC156D39F90F1AD8BAF3` |
| profile/environment | default Nextest profile; existing incremental target |
| requested start | `2026-09-13T11:50:11.1217531-04:00` |
| preflight | failed job fully exited `101`; no `cargo`, `cargo-nextest`, or `rustc` process remained active |
| session/PID | exec session `98655` |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m1-remediation-retry-nextest.log` |
| exit/result | exit `0`; compile completed in `19m 49s`; Nextest run `afe26368-e578-4ddf-a645-f7b798eade1b`; 46 passed, 0 failed, 4,008 filtered/skipped across 70 discovered binaries; test execution `0.526s` |

The successful replacement used the corrected, recorded source without further source
changes. It covered all nine reviewer remediations: safe release-selection cancellation,
exclusive-release draining, generation-safe arming resets, sticky drag cancellation,
one-cell/wrap wedge ownership, bounded memoized DAG validation, frame-scoped dynamic
snapshots, parsed canonical hotkey conflicts, and validated store initialization with
explicit revision overflow. `git diff --check` passed afterward and no Cargo tree
remained active. The failed compile attempt and successful replacement are both retained
above rather than being collapsed into a false first-pass success.

M2 focused gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD 13f3af10` plus M2 worktree; tracked-diff identity `71342cd0f6c72c2197ae8f12a42027cba5749cee`; new source SHA-256 values captured at launch (`launcher_invocation 6EF7CE5C...`, `controller 6AB963B3...`, `native A5EE27F0...`, `render 8079D447...`) |
| profile/environment | default Nextest profile; existing incremental target; no dependency/profile changes |
| requested start | `2026-09-13T12:25:22.3559190-04:00` |
| preflight | process-command-line inventory was access denied; fallback process-name inventory confirmed no `cargo`, `cargo-nextest`, or `rustc` process active |
| session/PID | exec session `6292` |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m2-nextest.log` |
| exit/result | exit `1` (`cargo` exit `101`); compilation stopped before tests on one test float literal, an ambiguous `.into()`, and the missing `Graphics::Gdi::SetWindowRgn` import |

The failed process fully exited. Those three compile-only defects were corrected and
`cargo fmt` completed. No behavior or dependency changed. The replacement gate below
is the required M2 gate.

M2 focused replacement gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | same M2 worktree with only the recorded compile corrections and formatting; source SHA-256: launcher invocation `6EF7CE5C...`, controller `6AB963B3...`, native `8138E423...`, render `73B575B0...` |
| profile/environment | default Nextest profile; existing incremental target |
| requested start | `2026-09-13T12:29:17.1157974-04:00` |
| preflight | failed job fully exited; no `cargo`, `cargo-nextest`, or `rustc` process active; `git diff --check` exited 0 |
| session/PID | exec session `64765`; observed `cargo` PIDs `1112`/`61528`, `cargo-nextest` PID `12244`, active compiler children during the build |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m2-nextest-retry.log` |
| exit/result | exit `0`; compile completed in `18m 34s`; Nextest run `cdd3b56c-9855-4179-8ad6-136db836681b`; 53 passed, 0 failed, 4,014 skipped; test execution `1.338s` |

After the successful gate, inspection found and corrected one adjacent pure-adapter
issue: a repeated modifier down could leave a count elevated after its physical up.
Modifier state now tracks each left/right key as an idempotent boolean, with a focused
regression test. Startup was also connected to the retained `RadialStore` snapshot
instead of always constructing a starter fixture, the Win32 paint path now rasterizes
the retained vector scene rather than presenting an unpainted host, and two unused
fallback assignments were removed. These post-gate changes are formatted and `git diff --check` passes, but
the single-gate constraint means they remain pending source-identical validation;
therefore M2 remains `in_progress` for the next review/remediation gate.

M2 independent-review remediation gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/launcher_invocation/) | test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD 13f3af10` plus the M2 remediation worktree; pre-ledger tracked-diff identity `7b62e5b95d71880961957119b11c1d75bf572487`; source SHA-256: launcher invocation `BF41B3FC...`, controller `550A1A14...`, native `A37A38B2...`, render `73B575B0...` |
| profile/environment | default Nextest profile; existing incremental target; Windows UI HiDPI API feature enabled; no new dependency |
| requested start | `2026-09-13T13:31:00-04:00` |
| preflight | `git diff --check` exited 0; no `cargo`, `cargo-nextest`, or `rustc` process was active |
| session/PID | exec session `53066`; `cargo-nextest` PID `51128`, Cargo PIDs `61836`/`23740`, compiler children observed during build |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m2-review-remediation-nextest.log` |
| exit/result | exit `100`; compile completed in `10m 23s`; Nextest run `e65984fe-9a95-4071-af31-7388a310ad08`; 20 passed, 1 failed, 45 not run after fail-fast, 4,010 skipped. A stale `Ready` test exposed that the coordinator took the pending session before checking its identity. |

The remediation addresses timer ownership, deterministic high-priority cancellation
and release draining, centralized application injection provenance, physical chord
ordering and AltGr semantics, joined legacy/native lifecycle teardown, lazy host
startup, correlated controller readiness/feedback, monitor work-area and DPI geometry,
capture-loss/drag/Escape handling, native state and annular paint ownership, the
opt-in live probe contract, and stable direct-trigger routing. The live probe has not
been run, so native desktop behavior remains explicitly unverified.

The failed process fully exited. The coordinator was corrected to inspect correlation
before taking pending ownership; no other source changed. A replacement gate is
required below.

M2 independent-review remediation replacement gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/launcher_invocation/) | test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | failed-gate source plus the single stale-readiness ownership correction; controller SHA-256 `6757042C...`; all other source hashes unchanged from the failed gate |
| profile/environment | default Nextest profile; existing incremental target |
| requested start | `2026-09-13T13:43:00-04:00` |
| preflight | failed gate fully exited; no `cargo`, `cargo-nextest`, or `rustc` process active |
| session/PID | exec session `1850`; process IDs were not separately captured for the replacement |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m2-review-remediation-retry-nextest.log` |
| exit/result | exit `0`; compile completed in `14m 15s`; Nextest run `d9b3eafc-d19d-4ba4-afdb-171f002a93ad`; 66 passed, 0 failed, 4,010 skipped; test execution `1.822s` |

Post-gate `cargo fmt --all -- --check` and `git diff --check` both exited 0.
No native live probe was run, so nonactivation, cross-process click-through, capture,
and mixed-DPI behavior remain unverified native acceptance items rather than claimed
passes.

M2 post-remediation lifecycle replacement gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/launcher_invocation/) | test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD 13f3af10` plus post-review worktree; pre-ledger tracked-diff identity `9f6b14ae6fd4f64c76398da45f8444770b3aac84`; source SHA-256: launcher invocation `AFB0FBEC...`, controller `0444C158...`, native `F0A0BCD2...`, render `50E38B68...` |
| profile/environment | default Nextest profile; existing incremental target; no new dependency |
| requested start | `2026-09-13T14:27:00-04:00` |
| preflight | formatted source parsed successfully; `git diff --check` exited 0; no `cargo`, `cargo-nextest`, or `rustc` process active at `2026-09-13T14:26:21-04:00` |
| session/PID | exec session `40784`; process IDs were not separately captured |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m2-post-review-nextest.log` |
| exit/result | exit `101`; compilation stopped before tests because `UpdateWindow` was imported from the wrong Windows binding namespace |

This gate covers lifecycle-published exclusive owners, queued-open rejection,
direct-only routing when shared tap/hold is disabled, same-menu close/different-menu
replacement, balanced primary-first and Escape key cycles, transactional reload
draining, invalidated wake-failure command generations, bounded deferred joins,
physical/logical mixed-DPI conversion, staged surface failure, and presentation-before-
readiness behavior. The interactive native probe remains unrun.

The failed process fully exited. Registry inspection confirmed that windows 0.58
exports `InvalidateRect` and `UpdateWindow` from `Win32::Graphics::Gdi`, with BOOL
return semantics. Only that presentation boundary was corrected, including explicit
invalidation before the synchronous update. A replacement gate is required below.

M2 post-remediation lifecycle replacement retry (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/launcher_invocation/) | test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | failed-gate source plus the single GDI presentation-boundary correction; native SHA-256 `1DA4308A...`; other recorded source hashes unchanged |
| profile/environment | default Nextest profile; existing incremental target |
| requested start | `2026-09-13T14:31:00-04:00` |
| preflight | failed gate exited `101`; formatted correction parsed; no `cargo`, `cargo-nextest`, or `rustc` process active at `2026-09-13T14:29:43-04:00` |
| session/PID | exec session `40784`; `cargo-nextest` PID `38524`, Cargo PID `61852` |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m2-post-review-retry-nextest.log` |
| exit/result | exit `0`; compile completed in `15m 32s`; Nextest run `56949202-de08-4dcf-bd85-daea43fd1f5a`; 77 passed, 0 failed, 4,010 skipped; test execution `0.826s` |

Post-gate `cargo fmt --all -- --check` and `git diff --check` both exited 0.
No interactive native probe was run, so real cross-process delivery, nonactivation,
click-through, capture, and mixed-DPI behavior remain explicitly unverified.

M2 final lifecycle remediation gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/launcher_invocation/) | test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD 13f3af1095405af7034d9f9b1487d327e7df689e` plus final lifecycle remediation worktree; pre-ledger tracked-diff identity `ac9fc9102279df6bd29a51a75bba52b02ee9a57c`; source SHA-256: launcher invocation `5D4E7C73...`, main `63E0882E...`, unchanged native `1DA4308A...` |
| profile/environment | default Nextest profile; existing incremental target; no new dependency |
| requested start | `2026-09-13T15:08:00-04:00` |
| preflight | formatted source and `git diff --check` passed; no `cargo`, `cargo-nextest`, or `rustc` process active at `2026-09-13T15:07:29-04:00` |
| session/PID | exec session `37935`; `cargo-nextest` PID `38572`, Cargo PIDs `39636`/`60708`, compiler children observed during the build |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m2-final-lifecycle-nextest.log` |
| exit/result | exit `0`; compile completed in `15m 36s`; Nextest run `a024d972-eb75-4fa3-bcf1-d8a7070dc108`; 81 passed, 0 failed, 4,010 skipped; test execution `1.831s` |

This gate covers acknowledged route handoff across shared/legacy/disabled transitions,
held-release draining and fresh-cycle ownership, startup-timeout cancellation and
restart, one-shot Screen Draw recovery cycles with provenance, and exclusive rejection
of queued legacy/radial/direct launcher visibility intents. The interactive native
probe remains unrun.

Post-gate `cargo fmt --all -- --check` and `git diff --check` both exited 0.
The launcher-invocation and main source hashes exactly matched the recorded launch
snapshot, and no Cargo/Nextest/rustc process remained active. No interactive native
probe was run, so the previously recorded native evidence gap remains.

M2 recovery-provenance remediation gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/launcher_invocation/) | test(/radial/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD 13f3af1095405af7034d9f9b1487d327e7df689e` plus recovery-provenance remediation worktree; pre-ledger tracked-diff identity `4541de0a34f019f1973d1e2c49456612954982fe`; launcher invocation SHA-256 `34362806...`; unchanged main SHA-256 `63E0882E...` |
| profile/environment | default Nextest profile; existing incremental target; no dependency changes |
| requested start | `2026-09-13T15:30:00-04:00` |
| preflight | formatted source and `git diff --check` passed; no `cargo`, `cargo-nextest`, or `rustc` process active at `2026-09-13T15:29:28-04:00` |
| session/PID | exec session `96523`; `cargo-nextest` PID `29012`, Cargo PIDs `59048`/`49536`, compiler children observed during the build |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m2-recovery-provenance-nextest.log` |
| exit/result | exit `0`; compile completed in `17m 24s`; Nextest run `5ad1ef48-f79a-451f-8572-a2f7403d29a9`; 81 passed, 0 failed, 4,010 skipped; test execution `0.836s` |

This gate specifically adds recovery ownership provenance: only matching-provenance
repeat/release events are consumed or clear the latch, while accepted interleaved
physical/external events cannot mutate one another's owned cycle. The interactive
native probe remains unrun.

Post-gate `cargo fmt --all -- --check` and `git diff --check` both exited 0.
The launcher-invocation and main hashes exactly matched the recorded launch snapshot,
and no Cargo/Nextest/rustc process remained active.

M3 focused integration gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/) | test(/launcher_invocation/) | test(/universal_actions/) | test(/universal_action_executor/) | test(/gui::actions/) | test(/gui::watch/) | test(/gui::search/) | test(/command_host/) | test(/active_window/) | test(/window_catalog/) | test(/window_activation/) | test(/dashboard::data_cache/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD 99ba6a29adc0d551926f2aea64390b4f44e90e5f` plus M3 worktree; tracked source-diff identity `31a0cc2a4c8556c287f50e9e2e76bf8e24617ae3`; SHA-256 bindings `6DB775E5...`, context `BFBA50A4...`, dynamic `790CB014...`, handoff `7571EE13...`, controller `8407F328...`, launcher invocation `EBB76930...`, GUI radial adapter `93777B51...`, executor `7B5EF300...`, persisted resolver `04BBC3F1...` |
| profile/environment | default Nextest profile; existing incremental target; no new dependency |
| requested start | `2026-09-13T16:40:46.7397029-04:00` |
| preflight | `rustfmt --check` parsed the complete touched source set; formatting applied; `git diff --check` exited 0; no `cargo`, `cargo-nextest`, or `rustc` process active |
| session/PID | exec session `18302`; cargo-nextest PID `16936`, Cargo PIDs `54236`/`56764`, compiler children observed |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m3-nextest.log` |
| exit/result | exit `101`; compilation stopped before tests on one omitted match arm in the test-only GUI event receiver |

The failed job fully exited. The only correction added the two new radial watch-event
variants to the existing test receiver's ignored-event branch; production behavior was
unchanged. Replacement source-diff identity is
`be49881c9388e0e9940fd47578302bb80e021545`; corrected `gui/mod.rs` SHA-256 is
`2EDA6399D609E7097EF7D580F9FCD3CE586C3402E9D94B6D84CDE31301E23947`.
At `2026-09-13T16:44:36.4661818-04:00`, formatting and `git diff --check`
passed and no Cargo/Nextest/rustc process remained active. The permitted source-identical
replacement gate uses the same expression and log path with `-retry` suffix.

M3 focused integration replacement gate (result):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/) | test(/launcher_invocation/) | test(/universal_actions/) | test(/universal_action_executor/) | test(/gui::actions/) | test(/gui::watch/) | test(/gui::search/) | test(/command_host/) | test(/active_window/) | test(/window_catalog/) | test(/window_activation/) | test(/dashboard::data_cache/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD 99ba6a29adc0d551926f2aea64390b4f44e90e5f` plus M3 worktree; tracked source-diff identity `be49881c9388e0e9940fd47578302bb80e021545`; corrected `gui/mod.rs` SHA-256 `2EDA6399D609E7097EF7D580F9FCD3CE586C3402E9D94B6D84CDE31301E23947`; all other recorded source hashes unchanged |
| requested start | `2026-09-13T16:44:55-04:00` |
| preflight | formatting and `git diff --check` passed; no `cargo`, `cargo-nextest`, or `rustc` process active |
| session/PID | exec session `88739`; cargo-nextest PID `54288`, Cargo PIDs `55592`/`58240`, compiler children observed |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m3-nextest-retry.log` |
| exit/result | exit `0`; compile completed in `16m 58s`; Nextest run `fdf80343-2369-432d-a167-48d480a758bb`; 238 passed, 0 failed, 3,869 skipped; test execution `8.309s` |

The replacement verified the source-identical root correction and the complete focused
M3 integration set. Interactive native behavior remains explicitly unverified.
Post-gate `cargo fmt --all -- --check` and `git diff --check` both exited 0, and no
Cargo/Nextest/rustc process remained active before the formatting check.

M3 independent-review remediation gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/) | test(/launcher_invocation/) | test(/universal_actions/) | test(/universal_action_executor/) | test(/gui::actions/) | test(/gui::watch/) | test(/gui::search/) | test(/command_host/) | test(/active_window/) | test(/window_catalog/) | test(/window_activation/) | test(/dashboard::data_cache/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | `HEAD 99ba6a29adc0d551926f2aea64390b4f44e90e5f` plus M3 remediation worktree; pre-ledger tracked source-diff identity `6b3692bf3deda728a76102590eeb940cbf1e703f`; SHA-256 bindings `FA022046...`, controller `2114E9BE...`, dynamic `E845EB1E...`, handoff `648CBF3A...`, launcher invocation `B61F722B...`, GUI radial adapter `A34D7763...`, executor `8A2FC98B...`, persisted resolver `737023C4...` |
| profile/environment | default Nextest profile; existing incremental target; no dependency/profile changes |
| requested start | `2026-09-13T18:04:36.5362426-04:00` |
| preflight | direct rustfmt parsed/formatted the complete touched source set; `git diff --check` exited 0; no `cargo`, `cargo-nextest`, or `rustc` process active |
| session/PID | exec cell `996`, unified session `32312`; cargo-nextest PID `14388`, Cargo PIDs `52760`/`52624`, compiler children observed |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m3-review-remediation-nextest.log` |
| exit/result | exit `101`; compilation stopped before tests on test-module paths, one inferred scheduler option, a Win32 type path, an immutable menu borrow, and a moved dynamic query |

This gate covers the twelve independent-review remediations, including selectable
frozen dynamic frames with exact catalog revalidation, effective after-action policy,
direct-key release ownership, exact-once GUI dispatch leases, composite navigation,
submenu-only dwell, typed execution policy and async completion preservation, context
routing, exact custom identities, and scheduled action-handoff timeout. Interactive
native behavior remains explicitly unverified.

The failed process fully exited. Only those compiler-root issues were corrected:
test paths were made crate-qualified, the scheduler deadline received its concrete
`u64` type, `WPARAM` was qualified, the selected menu is cloned before mutation, and
the frozen source query is cloned into its fingerprint. The permitted replacement
below uses the same focused expression.

M3 independent-review remediation replacement gate (launch record):

| Field | Value |
|---|---|
| command | same focused Nextest expression recorded above |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | failed-gate source plus only the five recorded compiler-root corrections; controller SHA-256 `FE6BF997...`, dynamic `BFE4B0CC...`, native `40818599...`; all other source hashes unchanged |
| profile/environment | default Nextest profile; existing incremental target; no dependency/profile changes |
| requested start | `2026-09-13T18:08:04.0959360-04:00` |
| preflight | failed Cargo tree fully exited; direct rustfmt and `git diff --check` passed; no `cargo`, `cargo-nextest`, or `rustc` process active |
| session/PID | exec cell `1003`, unified session `47122`; cargo-nextest PID `29872`, Cargo PIDs `30272`/`46704`, compiler children observed |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m3-review-remediation-nextest-retry.log` |
| exit/result | exit `100`; compile completed in `15m 53s`; Nextest run `cecef543-c03e-420f-9738-2f0565e5a658`; 149 passed, 1 failed, 103 not run after fail-fast, 3,869 skipped |

The replacement exposed a real composite-host defect rather than a fixture issue:
the unioned extent still modeled one circular input region, so a cascaded parent cell
could be treated as exterior. The root correction adds explicit disjoint input regions
to `LayoutSnapshot`, unions them for cascades, uses the same regions for pure ownership,
and builds the native Win32 region from their union. This preserves protective parent
gaps and true click-through between separated wheels.

M3 independent-review remediation second replacement (launch record):

| Field | Value |
|---|---|
| command | same focused Nextest expression recorded above |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | prior replacement source plus only the composite input-region root correction; geometry SHA-256 `6E4473A9...`, render `BF206EA8...`, controller `F3AEA0B5...`, native `770B0BE3...`; all other source hashes unchanged |
| profile/environment | default Nextest profile; existing incremental target; no dependency/profile changes |
| requested start | `2026-09-13T18:28:13.9901425-04:00` |
| preflight | failed Nextest process fully exited; direct rustfmt and `git diff --check` passed; no `cargo`, `cargo-nextest`, or `rustc` process active |
| session/PID | exec cell `1031`, unified session `7086`; cargo-nextest PID `50896`, Cargo PIDs `52904`/`59940`, compiler children observed |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m3-review-remediation-nextest-retry2.log` |
| exit/result | exit `101`; compilation stopped before tests because windows 0.58 returns typed `GDI_REGION_TYPE` from `CombineRgn` |

The failed job fully exited. The sole correction compares the typed region result to
`GDI_REGION_TYPE(0)`; no control flow or other source changed.

M3 independent-review remediation third replacement (launch record):

| Field | Value |
|---|---|
| command | same focused Nextest expression recorded above |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | second-replacement source plus only the typed Windows API comparison; native SHA-256 `7FCEA5CA...`; all other source hashes unchanged |
| profile/environment | default Nextest profile; existing incremental target; no dependency/profile changes |
| requested start | `2026-09-13T18:30:30.8076600-04:00` |
| preflight | failed Cargo tree fully exited; direct rustfmt and `git diff --check` passed; no `cargo`, `cargo-nextest`, or `rustc` process active |
| session/PID | exec cell `1037`, unified session `12017`; cargo-nextest PID `56172`, Cargo PIDs `11012`/`61836`, compiler children observed |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m3-review-remediation-nextest-retry3.log` |
| exit/result | exit `0`; compile completed in `17m 14s`; Nextest run `96ec3d1c-c692-4c35-a272-76866532474b`; 253 passed, 0 failed, 3,869 skipped; test execution `8.283s` |

The successful replacement used source identical to the recorded hashes. Direct
`rustfmt --check` over the complete touched Rust set and `git diff --check` both
exited 0 afterward, and no Cargo/Nextest/rustc process remained active. The focused
GUI tests created an untracked default `clipboard_modifiers.json` fixture at the
repository root; it was removed after inspection without modifying source. No native
interactive probe was run, so native cross-process behavior remains unverified.

M3 closure remediation gate (result):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/) | test(/launcher_invocation/) | test(/universal_actions/) | test(/universal_action_executor/) | test(/gui::actions/) | test(/gui::watch/) | test(/gui::search/) | test(/command_host/) | test(/active_window/) | test(/window_catalog/) | test(/window_activation/) | test(/dashboard::data_cache/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| initial source | tracked source-diff identity `943e6b887c833b2a2a62f37bae9e0321658306f1` |
| initial attempt | requested `2026-09-13T19:17:21.9663770-04:00`, exec session `51525`; exit `101` before tests because the new projected-cell role helper omitted the existing `CellContent::Spacer` arm |
| first replacement | requested `2026-09-13T19:20:29.2584589-04:00`, exec session `5481`, source identity `381b4ecf47b33c37d02e89f7757664ec1d4432ba`; compiled in `16m 54s`, Nextest run `b23c7032-0d5c-4137-9658-a423c4d46781`; 102 passed and 1 new fixture test failed before fail-fast because it attempted a fresh chord while its prior radial lifecycle was intentionally active |
| successful replacement | requested `2026-09-13T19:39:34.5761841-04:00`, exec session `23006`, source identity `1081befce4cbf7ce00fece9dd22dde3f84c2b5fc`; compiled in `18m 03s`, Nextest run `4cc17c37-b860-4533-a703-8accadb609f4`; 258 passed, 0 failed, 3,869 skipped; test execution `7.452s` |

Both failed trees fully exited before their root-only correction and replacement.
The only source root correction was the exhaustive spacer role; the second change
corrected the new test lifecycle to dismiss its first active session before asserting
a second admitted invocation. No native interactive probe was run.

M3 final closure review gate (result):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/) | test(/launcher_invocation/) | test(/universal_actions/) | test(/universal_action_executor/) | test(/gui::actions/) | test(/gui::watch/) | test(/gui::search/) | test(/command_host/) | test(/active_window/) | test(/window_catalog/) | test(/window_activation/) | test(/dashboard::data_cache/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| initial attempt | requested `2026-09-13T20:16:10.1055848-04:00`, exec session `53778`, source identity `44335b0c25bec3332e6a820a3a1c2f8a4bfe24fa`; exit `101` before tests because windows 0.58 locates pointer tracking types/functions under `UI::Input::KeyboardAndMouse` and `WM_MOUSELEAVE` under `UI::Controls` |
| first replacement | requested `2026-09-13T20:21:05.2520322-04:00`, exec session `71932`, source identity `3c23ac71a2f5c0676800081d16fe2b3e9ea06d20`; compiled in `17m 32s`, Nextest run `1cdaecfb-4ad3-4160-a615-bb52406cf4bb`; 210 passed and one new validation fixture failed before fail-fast because it used malformed `screen-draw:start` instead of the canonical `screen_draw:start` action |
| successful root-only replacement | requested `2026-09-13T20:40:16.7302723-04:00`, exec session `82190`, source identity `bc639a0636567f5d514f63f543490460aff08ba5`; compiled in `16m 58s`, Nextest run `fd913367-16e9-4711-a3ca-726833e381d2`; 262 passed, 0 failed, 3,869 skipped; test execution `7.189s` |

Both failed trees fully exited before the narrowly scoped correction and next run.
No native interactive probe was run; the test-created `clipboard_modifiers.json`
fixture was inspected and removed after the successful gate.

M3 decision-review remediation replacement gate (launch record):

| Field | Value |
|---|---|
| command | `cargo nextest run -E 'test(/radial/) | test(/launcher_invocation/) | test(/universal_actions/) | test(/universal_action_executor/) | test(/gui::actions/) | test(/gui::watch/) | test(/gui::search/) | test(/command_host/) | test(/active_window/) | test(/window_catalog/) | test(/window_activation/) | test(/dashboard::data_cache/)'` |
| cwd | `G:\Repos\rust\Multi_Launcher` |
| source | tracked source-diff identity `e775422b704acdc3db91ef055e3601b31534211a`; SHA-256 bindings `BB356254...`, model `AEF436A0...`, validation `F440CC91...`, handoff `1560C8AA...`, controller `C2F08CD4...`, GUI radial adapter `C7A6F17C...` |
| requested start | `2026-09-13T21:19:09.7431864-04:00` |
| preflight | `cargo fmt --all -- --check` and `git diff --check` exited 0; no Cargo/Nextest/rustc process active |
| log | `C:\Users\Jay\AppData\Local\Temp\multi-launcher-m3-decision-review-nextest.log` |
| initial session/PIDs | exec session `6079`; cargo-nextest PID `47344`, Cargo PIDs `11160`/`50248`, compiler children observed |
| initial result | exit `101` before tests: the new multi-ring assertion needed an explicit `sum::<usize>()` because multiple dependencies implement `Sum<usize>` |
| replacement source | root-only test annotation; tracked source-diff identity `a5fa7be8b65513932ef7884a198db0e1f0329be5`; bindings SHA-256 `0E3DC17C...`; all other source hashes unchanged |
| replacement requested start | `2026-09-13T21:23:48.2898699-04:00`; failed tree fully exited, formatting/diff checks pass, no Cargo tree active |
| first replacement session/PIDs | exec session `85744`; cargo-nextest PID `60372`, Cargo PIDs `13988`/`52444`, compiler children observed |
| first replacement result | exit `100`; compile `16m 27s`; 136 passed, 1 new regression fixture failed, 133 not run after fail-fast. The outer ring capacity was exactly 20 for exactly 20 fixture entries, so correctly had no Next control. |
| second replacement source | root-only fixture correction uses 25 entries per ring to exercise paging on both; tracked source-diff identity `12c7bf21f89f4852fd2db3c630a26eab6832a09e`; bindings SHA-256 `3299125C...`; production source hashes unchanged |
| second replacement requested start | `2026-09-13T21:42:17.2455136-04:00`; failed tree fully exited, formatting/diff checks pass, no Cargo tree active |
| second replacement session/PIDs | exec session `98218`; cargo-nextest PID `58164`, Cargo PIDs `50500`/`32692`, compiler children observed |
| result | exit `0`; compile `14m 22s`; Nextest run `5dca8537-fdff-45a0-9284-f2a540315da6`; 270 passed, 0 failed, 3,869 skipped; test execution `8.418s`; interactive native evidence remains unverified |

## Known evidence gaps

- No `Preferences.json`, Radify sounds, or Radify `Settings.json` is supplied;
  source-defined defaults and generated shape are evidence, and fixtures must be
  labelled synthetic.
- Native cross-process click-through, nonactivation, mixed-DPI behavior, real chord
  suppression, visual quality, and resource/performance measurements are unverified
  until their stated live gates run.
- Radify stock icon/emoji rights and all RM4 redistribution rights are insufficient
  for bundling; references remain user-provided compatibility inputs.
