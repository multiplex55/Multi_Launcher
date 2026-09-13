# Native radial menu implementation ledger

Status: approved, M0-M2 complete

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
| M3 actions/context/submenus/handoffs | pending | focused integrated/regression batch |
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

## Known evidence gaps

- No `Preferences.json`, Radify sounds, or Radify `Settings.json` is supplied;
  source-defined defaults and generated shape are evidence, and fixtures must be
  labelled synthetic.
- Native cross-process click-through, nonactivation, mixed-DPI behavior, real chord
  suppression, visual quality, and resource/performance measurements are unverified
  until their stated live gates run.
- Radify stock icon/emoji rights and all RM4 redistribution rights are insufficient
  for bundling; references remain user-provided compatibility inputs.
