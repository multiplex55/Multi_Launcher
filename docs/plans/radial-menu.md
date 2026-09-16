# Native radial menu implementation ledger

Status: approved; M0-M5 complete

> **Superseded repair track:** The approved R0-R5 runtime/Designer repair changes
> shared tap/hold behavior, SameCenter defaults/placement, tooltip diagnostics, and
> editor presentation. Its live status and evidence are maintained separately in
> `docs/plans/radial-repair-and-designer.md`. Historical M0-M6 records below remain
> immutable evidence and do not establish repair acceptance.

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

Package checksums, content-addressed managed assets, atomic radial recovery grouping,
and live generation identities are native runtime/persistence metadata rather than
legacy fields. They are never inferred from or written into Radify/RM4 sources; import
preview remains side-effect-free and recovery always operates on the validated native
document plus managed-asset group.

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
| M4 skins/assets/import/export/recovery | complete | closure remediation and source-identical combined focused gate passed; native interactive evidence remains explicitly unverified |
| M5 complete editors/settings/commands/starters | complete | contextual bindings carry captured identity across every surface; exact replacement gate and warning-free check passed |
| M6 hardening/performance/full verification/review | in_progress | source, focused/full gates, release build, serialized benchmarks, and independent review passed; automated live Win32 host evidence passed, while physical chord, real cross-process clicks, Narrator, high-contrast, mixed-DPI, and destructive-target handoff remain manual release checks |

Each milestone is implemented and committed sequentially by one writer. Tests are
written with each coherent batch and run only at gates. One Cargo/build/test job may
use this checkout/target at a time.

### M6.1 invocation and lifecycle hardening (source complete; gate pending)

`LifecycleCancellation` now preserves reload, disable, host/hook failure, shutdown,
suspend, session-lock, input-desktop loss, replacement, and priority-preemption
identity across the pure invocation reducer, the retained hook service, main-loop
notices, and native controller close reasons. Reload/disable retain a claimed key
until its matching release before route acknowledgement. Shutdown and operating-system
transitions that invalidate input continuity abandon stale key/modifier ownership
without synthesizing releases or replaying a launcher tap. The native hook thread owns
a hidden session/power notification window and an `EVENT_SYSTEM_DESKTOPSWITCH` hook;
all registrations are released with the hook lifecycle. A terminal hook/timer failure
restores the legacy launcher route fail-closed. Gesture suppression is acquired before
surface creation and its guard is released on every failed or successful session path.

Automated invocation rows 1–16 are covered by the existing reducer/adapter/main/session
tests plus the new boundary, lifecycle-reason, modifier-side/AltGr, provenance,
dismissal, post-action-release, native-message, correlated-controller-close, and
suppression-count matrices. These tests use synthetic timestamps/events and injected
factories only; no `SendInput`, desktop mutation, or timing sleeps were added. Source
format and diff checks pass. Cargo/Nextest and native acceptance have not yet run for
this substep.

### M6.2 drag, geometry, and native input ownership (source complete; gate pending)

Center/background/cell `Drag` controls now enter the `SessionReducer` as a typed
`CellRole::Drag`. The reducer owns the squared-distance threshold, emits one
generation-correlated `BeginNativeDrag` intent only after that threshold, and clears
pending drag state on capture loss, outside interaction, relayout, release, and close.
The controller validates the current layout generation before forwarding
`BeginSystemDrag`; the native window procedure no longer starts a system move from a
raw button-down.

The Windows host now owns two coordinated surfaces. A full layered visual HWND is
always nonactivating and click-through and is never clipped to input geometry. A
separate transparent input-proxy HWND receives an OS-owned `SetWindowRgn` assembled
deterministically from the immutable `LayoutSnapshot`: circles use elliptic regions,
wedges use bounded winding polygons, protective wheel gaps remain owned, optional
center holes are subtracted, and true exterior is absent. Region construction is
completed before publication; ownership transfers explicitly to the system only on a
successful `SetWindowRgn`. Create, present, relayout, animation, and system-drag moves
keep both surfaces aligned while preserving the visual overflow and the drag offset.

Synthetic coverage adds exact drag-threshold/stale-generation/exact-once/capture-loss
cases, controller command correlation, generated ring/count/one-cell/spacer/rotation/
overlap cases, four-corner fractional-DPI work-area clamping, deterministic wedge
polygon conversion, protected-gap/center-hole/exterior region membership, and
separate visual/input surface assertions in the opt-in live probe. Existing reducer,
geometry, render, cascade, dwell, release, paging, context-freeze, and navigation tests
continue to cover rows 17–30. Default tests use no `SendInput` or sleeps. Direct source
format and diff checks pass; Cargo/Nextest and interactive cross-process acceptance
have not yet run for this substep.

### M6.3 auditable native construction and teardown (source complete; gate pending)

Native construction now gives temporary window, region, memory-DC, bitmap, and GDI
selection resources explicit RAII ownership. Window and region ownership transfer only
after successful publication to their Win32 owner; every earlier return restores or
destroys the resources already acquired. Lifecycle window, session notification,
desktop notification, and keyboard-hook registrations likewise have independent
guards, so partially completed registration and unwind paths clean up in reverse
dependency order. Surface teardown cancels animation ownership and capture before
retiring the input and visual HWNDs, whose `GWLP_USERDATA` retirement continues to
make subsequently delivered messages inert.

Both retained native services now expose an explicit `Running` to `Stopping` to
`Stopped` lifecycle. Their stopped acknowledgement is published only after the worker
has left its resource-owning loop, repeated close/stop is idempotent, and public sends
are rejected once stopping begins. Timed-out worker handles are submitted to one
process-wide bounded join owner with backpressure instead of spawning one deferred
join thread per failure.

Synthetic construction coverage enumerates class registration, both HWNDs, region
create/combine/apply, DC, DIB, select/restore, layered update, capture, timer, hook,
lifecycle window, session/desktop notification, and suppression stages. Every injected
stage failure withholds Ready and returns all counters to zero; 100 synthetic
construction cycles and 100 real fake-surface host cycles exercise zero-resource and
zero-worker teardown, repeated close/stop, suppression release, and harmless late
commands without desktop input or sleeps. Direct format and diff checks pass;
Cargo/Nextest and live Win32 fault injection remain deferred to the milestone gate.

### M6.4 demand-driven runtime resources (source complete; gate pending)

`main` now has one `RadialRuntimeResources` lifecycle boundary whose demand is the
union of runtime enablement and explicit authoring-session leases. The retained store,
published document, and control/authoring mailboxes remain lightweight while disabled.
Controller asset/font caches and the radial watcher are constructed on the first
runtime/editor demand transition and retired on the final release; repeated enable or
disable signals are transition-idempotent. Native surfaces remain lazy per accepted
open, native authoring preview remains separately leased, invocation hooks exist only
for enabled runtime routes, audio exists only for an active cue/audition, and deadline
schedulers are dropped at disable/shutdown rather than remaining after their first use.
Opening `radial edit` acquires an authoring lease without changing trigger enablement;
clean close, forced close, successful Save, failure shutdown, and process exit release
or clear the lease and cancel preview ownership.

The watcher callback no longer sleeps. It records a bounded dirty bitset and wakes the
main owner once until drained. The watch set is the app-data parent nonrecursively
(events are accepted only for `radial.json` or the asset-root entry) plus the existing
`radial_assets` tree recursively; creation of that tree refreshes the narrow watch set.
Document identity continues to suppress self-save echoes. Authoring sound preparation
now reuses the demand-owned `AssetService` cache rather than constructing an unrelated
service for each audition.

Synthetic instrumentation covers disabled startup with no acquisition transition,
editor-only acquire/release without enabling triggers, and two repeated enable/disable
cycles with one acquire/release per edge. Watcher tests cover coalescing and exact watch
paths. Radial-plugin prefix rejection has a thread-local inventory probe proving
ordinary/non-prefix queries do not read the published radial inventory. Direct format
and diff checks pass; Cargo/Nextest and live worker/handle measurements remain deferred
to the milestone gate.

### M6.5 action, security, editor, and accessibility closure (source complete; gate pending)

Requirements 39–64 are now tied to direct module evidence rather than a single broad
integration assertion. Stable persisted references are resolved by semantic identity;
an added table drives clipboard-entry, indexed-list, browser-runtime, and HWND churn
through the frozen-binding revalidation boundary and proves that the old object cannot
retarget to the replacement. Those runtime identities have no serializable binding
form. Availability rejection occurs before confirmation or execution, and a cancelled
confirmation consumes its pending value so a delayed/repeated response cannot run it.
Existing controller/handoff tests retain the frozen surface/source/history/context,
cleanup/key-release, stale-generation, dynamic revisit, pagination, and exact-once
coverage.

Every one of the 71 serialized full-scope style fields is enumerated in an exhaustive
typed audit and assigned to its first concrete runtime consumer: geometry, scene,
compositor, audio, input shaping, or native-window policy. Adding a field without
classifying it now requires an explicit source change. Existing geometry/render/
compositor/audio/native effect tests remain the behavioral evidence for those groups;
preview and runtime continue to share the same immutable frame builder and straight-
alpha compositor. Asset/import/package/store tests cover corrupt and oversized media,
alpha, missing font/icon fallback, named real-versus-synthetic legacy evidence,
archive/path/collision/link limits, dependency/ID remapping, atomic staging/publication,
and preservation of malformed/newer or revision-conflicted state.

The editor now publishes AccessKit widget names/roles for otherwise blank toggle,
scalar, color, offset, quality, font, text, and media controls. Color wells include an
RGBA value in the name; stable entity/field IDs remain independent of labels and list
positions. Selection changes schedule focus restoration to the stable menu/ring/cell
widget on the next frame, and deleting a selected menu chooses a surviving stable menu
instead of retaining a dangling selection. Textual availability/error/selection cues
remain present in addition to semantic color. Standard focused egui controls retain
Space/Enter navigation, with non-text-edit Ctrl+Z, Ctrl+Shift+Z, and Ctrl+S affordances.
The manual smoke checklist now has explicit Narrator/AccessKit, keyboard-only, 200%
scale, high-contrast, target-churn, input-shape, lifecycle, and native cleanup rows.

Direct tests cover the added identity matrix, unavailable/cancel exact-once boundary,
style-consumer completeness/uniqueness, deletion focus fallback, and editor semantic
contract. No live accessibility or native evidence is claimed. Direct rustfmt/diff
checks pass for this substep; Cargo/Nextest remains deferred to the M6 gate.

### M6.6 bounded performance and resource observability (source complete; gate pending)

The compositor now enforces one configurable aggregate raster budget across reusable
static layers and completed frames. The production default is a hard 128 MiB ceiling;
admission uses exact RGBA byte counts, deterministic touched-order/key eviction, and
does not evict useful entries merely to attempt admission of a frame larger than the
whole budget. An over-budget frame can still be presented from its caller-owned return
value but is not retained. Animated scenes replace the prior timestamp variant for the
same generation/DPI, so animation cannot fill the former twelve-frame cache with full
bitmaps. Generation invalidation decrements exact byte ownership for both cache kinds.

`CompositorCache` owns its optional counters and nanosecond timings. They are activated
only when the existing `MULTI_LAUNCHER_PERF` switch is enabled (or explicitly injected
by a test), are emitted only as composition work occurs, and create no timer or polling
loop. Snapshots include hit/miss/eviction/rejected-admission counts, completed/static
resource counts and bytes, the latest generation, and aggregate/maximum composition
time. The controller similarly exposes an on-demand census of its native host,
pending/active session, preparation bridge, deadline scheduler, asset, and font
ownership only through the existing debug-diagnostics switch; there is no global
mutable resource registry.

One Criterion target, `radial_runtime`, covers pure layout, hit ownership, scene build,
selection projection, static rasterization, warmed composition, and first/last-page
dynamic projection for 8, 32, 128, and 512 cells. It constructs only in-memory
fixtures. Unit coverage exercises deterministic byte eviction, oversized admission,
static/completed aggregate accounting, single animated timestamp retention, exact
generation invalidation, opt-in metrics, lazy resource census, warm repeated
composition, and 100 synthetic open/close-style generation cycles returning retained
bytes/resources to zero. Cargo and the benchmark have not been built or run in this
substep.

Proposed acceptance thresholds, pending same-host observation and noise calibration:

- compositor retained raster bytes never exceed 128 MiB in production or the injected
  budget in tests;
- one full-frame animation variant per generation/DPI and at most twelve completed
  frames overall;
- warm hover performs zero filesystem reads and zero provider inventory calls by the
  immutable scene/compositor capability boundary;
- 100 synthetic generation cycles return completed/static cache resources and bytes to
  zero after invalidation;
- candidate medians for comparable pure radial workloads should not regress more than
  15% from the first measured stable candidate series without investigation; no
  baseline timing claim is possible for the new radial-only benchmark because the
  pinned baseline contains no radial implementation.

The M6 gate should run measurements serially, never from the user's live data folders.
Create an isolated detached worktree at pinned baseline
`0d0acaf471a52f49a7ebdff61879416eefa9fc9b`, set `CARGO_TARGET_DIR` and
`LOCALAPPDATA` to dedicated temporary directories, build its release binary, and
capture startup/idle/search baselines with `MULTI_LAUNCHER_PERF=1`. Stop that process
and Cargo job before repeating the identical profile/environment against the candidate
checkout. For the candidate-only radial microbench, run three serialized
`cargo bench --bench radial_runtime` samples and retain Criterion output outside the
application-data sandbox. Record exact revisions, Rust/Cargo versions, power state,
display/DPI, and cold/warm classification. Retain raw samples for named
baseline/candidate comparisons; for same-target stability repetitions, retain a full
aggregate table plus the command and exit record unless a named saved-baseline run is
requested. Do not compare the new radial target to an invented baseline or historical
numbers.

M6 focused candidate gate attempt 1 used inventory UUID
`5ed514d4-c5a7-4c8a-ba93-8069f9e9f91e` at HEAD
`5e4f20c829a365afe9b7e50af9d35451b6fcf2c5` with the exact requested filter. It
exited `101` after `128.456s` before producing a selection: compilation found missing
local `ShowWindow`/show-command imports in native relayout, an ambiguous test-only
`sum`, and the embedded preview's missing arm for the new typed native-drag intent.
No tests ran and no count is claimed. The root-only corrections preserve runtime
behavior; the identical inventory expression is required for the replacement.

Replacement inventory UUID `1c32de87-d2c7-4927-89af-9384ae5bed14` exited `0`
after `663.048s` (Cargo compile/link `10m52s`) and printed a nonempty filtered
inventory. It also identified two `unused_must_use` warnings for `ShowWindow`; the
calls already intentionally ignore the Win32 visibility-status return and now bind it
explicitly before the warning-free gate. The exact selection count is recorded from a
post-compile count-only inventory query before test execution.

The source-current count query UUID `c472b77b-4232-4519-859e-d9dd2baacee1`
exited `0` after `1125.498s` (Cargo `18m35s`) with 2,129 selected tests. Focused
candidate attempt 1 used wrapper UUID `00ed9318-b770-4ff1-a5e0-84df2f7786eb`
and Nextest run `cb738061-b103-4647-baf1-8b449c7698e1`; it exited `100` after
`47.679s`: 2,129 run, 2,126 passed, 3 failed, and 2,289 skipped. A three-test
uncaptured diagnostic (UUID `21ba3d86-8f1c-4648-b7cc-999c4e06f868`, Nextest
`2553d951-057b-43db-ae58-8b20edeaff0c`) confirmed the failures after a
`19m10s` timestamp-triggered rebuild: color semantics used premultiplied bytes,
menu deletion chose a positional rather than stable default-menu survivor, and a
duplicate sticky release emitted a second acknowledgement. Root corrections use
unmultiplied sRGBA, the surviving default menu ID, and the reducer's typed
`trigger_still_down` guard. The full identical filter must pass on replacement.

The first root-correction diagnostic used wrapper UUID
`935da1fd-8cff-4583-832d-2a0db0c588e0` and Nextest run
`c55ebf64-02ee-4f79-858a-321705d89154`; after a `20m17s` rebuild it ran three
tests in `2.798s`, with the menu-survivor and duplicate-release tests passing.
The color semantic test still failed because even an unmultiplied conversion rounds
channels after egui's low-alpha premultiplication. The final root correction retains
the exact serialized RGBA while untouched and adopts converted widget bytes only after
an actual change. The next replacement is the full identical focused filter.

The full identical replacement used wrapper UUID
`b968e06f-d02e-4a1b-8e96-0c63d080ca16`. After a `20m45s` rebuild it ran all
2,129 selected tests; its Nextest run UUID scrolled out of the retained output,
and the wrapper exited `100` after `1297.742s`: 2,128 passed, one failed, and
2,289 were skipped. The only failure was the pre-existing wall-clock assertion in
`sound::play_sound_returns_quickly_and_no_panic`: the no-op invalid-name call
has no I/O or audio-service acquisition, but the test process was descheduled
long enough to exceed its `<100ms` assertion on this slow-machine gate. No
production or test source is changed for that environmental result; the exact
full filter is repeated warm and must pass before the candidate gate advances.

The exact warm replacement passed under wrapper UUID
`57ea352f-f2a0-4807-a60c-040b71ee2d1a` and Nextest run
`aa23f957-f96f-4146-b410-89bca522986e`. Cargo compiled in `19m47s`; the test
run completed in `31.175s` with all 2,129 selected tests passing and 2,289
skipped. The wrapper exited `0` after `1230.578s`. This is the focused/static
candidate test result; M6 remains `in_progress` pending the remaining static,
benchmark-compile, full-suite, native, performance, and independent-review gates.
The subsequent warning-free `cargo check` used wrapper UUID
`d810d4c0-ac22-46d4-8770-3e6de729a8d8` and exited `0` after `76.487s`
(`1m16s` Cargo time), with no compiler warnings.
The bench compile-only gate `cargo check --bench radial_runtime` used wrapper
UUID `2bba2a1d-f5a2-4e0e-88d3-3d5191bae519` and exited `0` after `37.754s`
(`37.67s` Cargo time), without executing a benchmark or emitting warnings.
`cargo fmt --all -- --check` (gate UUID
`f65e7117-a404-465e-9776-3b49195bf57a`) exited `0` after `6.541s`, and
`git diff --check` (gate UUID `a210fef3-ef74-4999-bab9-fef95ab12850`)
exited `0` after `0.401s`; the latter printed only Git's expected LF/CRLF
working-tree advisories. The post-gate Rust/Cargo/bench diff identity is
`3e48daff91ab5e4fa7322c1b748284dbe9a58c04` (`git hash-object --stdin`).
No Cargo, rustc, or Nextest process remained. The focused run created a root
`clipboard_modifiers.json`, which was removed as a test artifact. The ignored
`toast.log` predates this gate and was preserved. No non-target temporary or
backup artifact remained. Stale-ownership inspection confirmed that the only
invocation-local `thread::spawn` is a joined test helper, production timed-out
joins use the one bounded `thread_reaper`, `WM_NCLBUTTONDOWN` is reachable only
from the generation-correlated `begin_system_drag` command, and recursive
watching is limited to the optional `radial_assets` tree.

### M6 independent-review remediation

The six review findings are under source remediation while M6 remains
`in_progress`. Runtime and authoring preview now share the real visual resource
preparation boundary (effective layout/style, lazy asset and font services,
prepared glyphs/tooltips, special center/background cells, and prepared scene),
and embedded frames travel through the typed main-owned authoring endpoint with
generation/token correlation. Both previews reduce page and drag intent without
executing dispatch. Native-preview failure/stopped events retire the active
identity and host before a later start. Faulted worker joins use pre-reserved
bounded capacity and a completion-notified supervisor that blocks without
polling and joins only finished handles. Input ownership follows reverse/topmost layout order, with
background special ownership explicitly placed below concrete cells. Blank
editor cells publish real AccessKit selectable semantics with label, tooltip,
icon-role, then stable ID fallback. Menu and skin package import require exact
canonical asset dependency closure and discard already-owned skin asset bytes.

The first remediation-specific Nextest attempt used wrapper UUID
`241c4e56-62ac-47e6-a777-adb387fdb288` and Nextest run
`4c7a17ae-8bd8-4f86-af80-742f69b1390f`. After a `23m53s` build it ran 12
tests in `0.273s`: 10 passed and two fixture assertions failed. The embedded
drag fixture had not explicitly assigned the center Drag control, and the
runtime/preview parity fixture incorrectly rejected the identical expected
`LabelTruncated` diagnostics. Those test fixtures now state those expectations
directly; no production behavior changed for these two failures. The exact full
M6 focused expression was then used for every full replacement.

The first full remediation replacement used wrapper UUID
`06c6d6e7-f7f4-4758-ab4e-d1b57076438b` and Nextest run
`47ae6146-4fec-4a80-9661-6d66de7b2d3e`. It compiled in `21m06s` and exited
`100` after `1340.090s`: 2,139 tests ran, 2,137 passed, two failed, and 2,289
were skipped. The failures exposed two integration-fixture assumptions from the
new topmost/preview paths: the controller test still treated cell index zero as
ordinary after the background cell moved below it, and embedded page controls
were incorrectly simulated with the action-only reducer event. The focused
diagnostic Nextest run `5c8fd47b-4de9-4c50-a9b1-adf89e7a3450` compiled in
`20m55s` and reproduced exactly those two assertions. The correction locates the
ordinary cell by stable non-special identity and routes preview controls through
the production pointer-down/pointer-up reducer sequence.

The identical full replacement then passed under wrapper UUID
`9b7d4b3c-fb1d-44d2-910d-07412a6679e2` and Nextest run
`e46f0025-5488-42e9-a4b2-a1192c09320c`: Cargo compiled in `21m08s`, all
2,139 selected tests passed in `29.496s`, 2,289 were skipped, and wrapper wall
time was `1337.750s`. Static self-review subsequently strengthened the two
package-closure fixtures so their unreferenced archive entries use valid
content-addressed `assets/<sha>.png` names; rejection can therefore only be
attributed to dependency closure rather than malformed path shape. The final
source-current identical gate passed under wrapper UUID
`e2838ee3-e05d-4c2b-a803-375624b6c65e` and Nextest run
`e41d289b-93d6-437b-bd84-950f4d8a8fb9`: Cargo compiled in `21m13s`, all
2,139 selected tests passed in `27.736s`, 2,289 were skipped, and wrapper wall
time was `1338.952s`.

The final warning-free `cargo check` used wrapper UUID
`b79bdd0b-1331-4487-93ac-f08adf31ac26` and exited `0` in `35.606s`
(`35.44s` Cargo time). The compile-only benchmark gate
`cargo check --bench radial_runtime` used wrapper UUID
`3afbd611-28bc-43fe-969c-c261bf23827b` and exited `0` in `18.390s`
(`18.24s` Cargo time); no benchmark was executed. M6 remains `in_progress`
pending independent review plus the full/native/performance evidence gates.
Final `cargo fmt --all -- --check` and `git diff --check` exited `0`; the
latter emitted only the expected LF/CRLF working-tree advisories. The final
Rust/Cargo/bench diff identity is `2d2e0572a9b7352eb108ee13ce965e77734fc6a0`
(`git hash-object --stdin`). No Cargo, rustc, or Nextest process, Git index
lock, generated clipboard fixture, or non-target temporary/backup artifact
remained. The ignored pre-existing `toast.log` was preserved. No native or
performance execution evidence is claimed by this remediation gate.

The final closure pass resolves six further review blockers while retaining M6 as
`in_progress`. `PreviewFramePreparer` now invokes the same production
`project_menu_frame_with_style` dynamic projection, page-count, effective-style, and
control-placement path used at runtime. Authoring requests carry the immutable page,
frozen synthetic dynamic entries, selected-skin override, and a validated managed-asset
overlay. Embedded and native previews consequently render disjoint real projected
pages and unsaved image/GIF/font/sound references without writing the persisted asset
root; hash, kind, per-item, and aggregate budget failures become visible preview
diagnostics. The embedded frame token includes page and selected skin, and native page
navigation rebuilds and presents the changed projected scene. Explicit unreferenced
skin selection is applied before preparation.

The timed-out-join owner is now event-driven: a completion guard notifies one shared
condition variable, the single bounded supervisor joins only `is_finished` handles,
and a hung worker retains one bounded permit without polling, blocking callers, or
allowing unbounded worker creation. Synchronous native-preview Open, Present, and
BeginSystemDrag send failures all close the active identity, take and shut down the
host, and permit a fresh later host. Editor resource notices use typed Info, Warning,
and Error severity so successful export/portable choices cannot publish error
semantics; the same severity is present in visible text and the rendered AccessKit
tree.

The first exact closure gate used wrapper UUID
`3f2507f6-5b35-4bfb-bb7c-cfd2852e793d` and Nextest run
`c4af002f-6298-44f6-bc98-bf1dcac3f1c3`. It ran all 2,146 selected tests in
`26.402s`: 2,145 passed and one new native-preview fixture failed validation because
it had replaced a root submenu link and thereby left the child menu unreachable. An
isolated diagnostic run (`fc108f4f-71e3-43e8-8a66-c2b959e653e0`) reproduced that
single failure. The root-only correction exercises the existing Applications dynamic
submenu instead; the production document remains valid and the embedded, shared
preparer, and native paging tests all use that real greater-than-capacity projection.

The exact full replacement passed under wrapper UUID
`abe95ace-9046-4a79-afa6-d7ee6cee460b` and Nextest run
`a8412cf9-e4e2-4f09-8824-f5dcfbffe561`: Cargo compiled in `22m32s`, all
2,146 selected tests passed in `25.754s`, and 2,289 were skipped. Warning-free
`cargo check` passed under UUID `c4b16dec-ea68-4958-b91e-7522b0fef89d` in
`38.58s`. Compile-only `cargo check --bench radial_runtime` passed under UUID
`ea1a59b9-3020-44fd-bc7d-579a2308a124` in `7.28s`; no benchmark executed.
No native interactive or serialized performance evidence is claimed. M6 remains
`in_progress` pending final independent review, complete-suite verification, and the
documented native/performance gates.

Final self-review removed the last inert `Thread::unpark` submission call and stale
“polling supervisor” comment; completion guards and the shared condition variable are
now the supervisor's only wakeup path. The source-current exact replacement passed
under wrapper UUID `72c6126a-463c-474d-9e3b-d4c86eda2cab` and Nextest run
`c020d10f-4b3a-49c8-8255-da101f785a69`: Cargo compiled in `19m48s`, all
2,146 selected tests passed in `28.068s`, and 2,289 were skipped. The source-current
warning-free `cargo check` passed under UUID
`b39a6bd2-d83c-4f19-9a20-cc4f9bc9e782` in `34.64s`; compile-only benchmark check
passed under UUID `f972260d-1aea-4d33-a948-c81ac6dd9f23` in `21.91s` without
executing the benchmark. The final Rust/Cargo/bench diff identity is
`257a948bbd4764bffb3a55f38cd3f5eb52d3fe39` (`git hash-object --stdin`).
Formatting and diff checks passed; no Cargo/Nextest/rustc process, Git index lock,
generated clipboard fixture, or non-target temporary/backup artifact remained.

The amended transport closure adds three final safeguards. Embedded preparation now
turns overlay-construction and disconnected-service failures into typed visible Error
notices. Its failure fingerprint includes draft generation, selection, page, selected
skin, preset, and content-addressed overlay metadata, so a failed overlay is not
rehashed or resent every frame; changed inputs or the explicit Retry control rearm it.
A synchronous send failure clears only its exactly correlated pending request and
cannot strand the editor in AwaitingRequest. Faulted-join supervisor creation is now a
fallible injected boundary with no panic and no registry lock held across thread
creation. Failure safely detaches the bounded handles while their worker completion
guards retain and eventually release permits; callers receive or log the typed
degradation. Completion state and counters transition under the same registry lock,
preventing a detach/notification race. Native submenu and page presentation emits a
typed lease-correlated diagnostic update, including an empty list when warnings clear;
the authoring session ignores stale lease or generation updates.

The four new focused regressions passed under Nextest run
`6bd27d72-0bfc-45bb-aaf6-e010a32a738e` after a `25m35s` link. The first exact M6
replacement used wrapper UUID `fec0033d-af6b-4525-8b7d-ad18da8e78e8` and Nextest
run `a4b3fd59-b45c-4143-b7d6-e3591a5c308b`; 2,148 of 2,149 tests passed and one
existing native paging fixture retained the obsolete expectation that no notice was
emitted. Diagnostic run `21c08e6c-15bb-4bd0-afbe-9c0c03d946b3` confirmed that
the emitted value was the intended diagnostic update. The fixture now requires every
notice to be diagnostic-only and correlated to the active lease.

The exact source-current replacement passed under wrapper UUID
`258ca9cc-fbf4-43eb-8aa6-9d4bacce8b07` and Nextest run
`f251cd02-d87b-4b4d-b6a6-348cd376c5fd`: Cargo compiled in `22m01s`, all
2,149 selected tests passed in `22.585s`, and 2,290 were skipped. Warning-free
`cargo check` passed under UUID `05e4ae31-af71-454d-8ff1-5c1acf1e598a` in
`53.19s`; compile-only `cargo check --bench radial_runtime` passed under UUID
`9a564813-b8da-4d99-8d25-b0483b89d030` in `21.88s` without executing the
benchmark. Final Rust/Cargo/bench diff identity is
`0ad39d8e51e7c416c8ffd26f8da02e79a03e69c8` (`git hash-object --stdin`).
Formatting/diff, process, index-lock, and generated-artifact scans passed. M6 remains
`in_progress`; no native interactive, complete-suite, or
serialized performance evidence is claimed by this focused remediation gate.

The lost-before-handoff reaper closure latches completion independently of whether a
timed-out join has been armed. Registration inserts the handle under the registry lock
before its running-unarmed-to-armed compare/exchange; an already-completed notifier
increments the supervisor counter after insertion, while an armed notifier increments
and wakes directly. The completion guard therefore proves user work returned even in
the short OS thread epilogue where `JoinHandle::is_finished` is still false, allowing
the event-driven supervisor to join without polling. Deterministic channel/barrier
fixtures cover notifier drop before reap, during registration, and after arming; the
hung-handle and supervisor-spawn-failure capacity guarantees remain covered.

The source-current exact M6 expression passed under wrapper UUID
`14bb453c-d580-4e48-816e-7c3086fdf903` and Nextest run
`684b9e84-b31b-4cc8-ba87-6c6f125b9611`: Cargo compiled in `23m38s`, all
2,149 selected tests passed in `26.111s`, 2,293 were skipped, and wrapper wall time was
`24m49.673s`. Because that mandated expression does not select tests named solely for
`thread_reaper`, a direct source-current `test(/thread_reaper/)` run passed all five
tests under wrapper UUID `906e5168-d30d-4ba2-b3ef-f9c6fab4f836` and Nextest run
`95cdfba5-40d1-4b3e-89ee-53596d7192cd` (`20m35s` compile, `0.640s` test time).
Warning-free `cargo check` passed under UUID
`f9d04154-1a2a-4704-bce9-4571eb729faa` in `38.98s`; compile-only
`cargo check --bench radial_runtime` passed under UUID
`e47d488b-1227-4189-883d-69a8c7dfd849` in `29.35s` without executing the benchmark.
Formatting/diff, process, index-lock, temporary-artifact, and generated-fixture scans
passed. The tracked Rust/Cargo/bench diff identity remains
`0ad39d8e51e7c416c8ffd26f8da02e79a03e69c8`; the untracked new reaper source blob is
`4bfd373f319b2445e7cc223f4710f28f1996e8e8`. M6 remains `in_progress`; no native
interactive, complete-suite, or serialized performance evidence is claimed.

Final Rust/release verification ran from clean commit
`da0215fb08d8f4dff198147dbfe0b7daf667a247` with one Cargo tree at a time and no
source correction. `cargo fmt --all -- --check` passed under UUID
`4fa1eeb0-42dc-475f-9486-9b783ba7bad9` in `5.507s` (log
output was empty and no separate log file was retained). Warning-free
`cargo check` passed under UUID `6575782e-31a9-4dcf-8134-7259e384d90e` in
`9.935s` (`9.82s` Cargo time; log
`multi-launcher-final-check-6575782e-31a9-4dcf-8134-7259e384d90e.log`).
`cargo clippy --all-targets` passed under UUID
`029af54c-4b91-4292-b27a-65ea5a4c7c95` in `1m25.846s`; no new warning-denial policy
was added, and the lib-test target reported the repository's existing 253-warning
baseline (log `multi-launcher-final-clippy-029af54c-4b91-4292-b27a-65ea5a4c7c95.log`).
`cargo build --release` passed under UUID
`6498b1d7-e62b-4271-a732-108f840db935` in `4m43.801s` (`4m43s` Cargo time; log
`multi-launcher-final-release-6498b1d7-e62b-4271-a732-108f840db935.log`).
`git diff --check` passed under UUID `31933192-4bf3-43fd-8721-4e5f82204fc6` in
`0.048s`; output was empty and no separate log file was retained.

The complete requested `cargo nextest run --no-fail-fast --status-level slow
--final-status-level fail --success-output never --failure-output final` passed under
wrapper UUID `2ef74c86-3c15-403d-9413-6f67665d763d` and Nextest run
`974be829-9b53-448b-8360-0a66c3ad939d`: Cargo compiled in `21m10s`, all 4,434
executed tests passed in `80.014s`, 8 were skipped, and wrapper wall time was
`23m06.791s` (log
`multi-launcher-final-nextest-2ef74c86-3c15-403d-9413-6f67665d763d.log`). A source
scan found no Rust documentation code fences under `src`, so there are no doctest
examples requiring a separate doctest invocation. The sole generated
`clipboard_modifiers.json` fixture was removed after the suite. M6 remains
`in_progress` pending the documented native interactive and serialized performance
evidence; this gate makes no such claims.

### M6 serialized performance evidence

Measurements ran serially, baseline first, on Windows `10.0.19045`, Intel64 Family 6
Model 158 Stepping 9 with 8 logical processors, `rustc 1.97.1`/`cargo 1.97.1`.
The active Windows power scheme was `Balanced`
(`381b4222-f694-41f0-9685-ff5bb260df2e`); no `Win32_Battery` device was present.
Baseline commit `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` used isolated worktree
`C:\Users\Jay\AppData\Local\Temp\multi-launcher-perf-baseline-0d0acaf` and target
`target/perf-baseline-0d0acaf`; candidate commit
`da0215fb08d8f4dff198147dbfe0b7daf667a247` used
`target/perf-candidate-da0215fb`. The search benchmark sources are semantically
identical (`git diff --no-index` empty; hashes differ only by line endings). Both
commands used locked commit-specific dependencies and Criterion 0.5.1 arguments
`--warm-up-time 2 --measurement-time 5 --sample-size 20 --noplot --verbose`.
Criterion's own CLI parser and `cargo bench --help` were inspected before launch.

The first baseline launch UUID `f8ff1e92-22af-44b0-9b8b-f16c320b2a47` exited `101`
in `1.453s` before compilation because sandbox path canonicalization denied the
cross-worktree target. The identical approved replacement passed under UUID
`fb2c8534-175b-4832-aa36-0e92766b97cf` in `9m25.913s` (`6m54s` compilation). The
candidate search passed under UUID `9860824b-70db-4db2-bf59-dc642d7f1a54` in
`9m59.548s` (`7m23s` compilation). Outliers below are
low-severe/low-mild/high-mild/high-severe. Times and 95% median confidence intervals
are microseconds; MAD is the median absolute deviation point estimate. The regression
rule is candidate median greater than baseline median plus the larger of 10% or three
times the larger measured MAD.

| Search benchmark | Baseline median [CI], MAD, outliers | Candidate median [CI], MAD, outliers | Delta / allowance | Result |
|---|---:|---:|---:|---|
| completion/build_index/10000 | 3572.4457 [3535.7826, 3591.2333], 54.3566, 1/1/2/1 | 3531.0525 [3483.6040, 3628.2125], 94.6303, 0/0/0/0 | -1.16% / 10.00% | pass |
| completion/build_index/500 | 267.9977 [264.0675, 288.1817], 16.0066, 0/0/0/1 | 267.9587 [265.4254, 273.0942], 8.7171, 0/0/0/0 | -0.01% / 17.92% | pass |
| completion/suggestion_lookup/10000 | 1.1465 [1.1318, 1.1940], 0.0439, 0/0/0/0 | 1.7287 [1.6655, 1.7457], 0.0815, 0/0/0/1 | +50.77% / 21.32% | single-run flag; resolved below |
| search_command_cache/lookup_real/250 | 101.2238 [99.4348, 101.7043], 1.6692, 0/0/0/0 | 105.3063 [104.2850, 106.2100], 1.9206, 0/0/1/0 | +4.03% / 10.00% | pass |
| dynamic/browser_tabs_cached_filter_1000 | 192.6195 [186.9365, 202.8195], 14.2717, 0/0/0/0 | 201.8710 [196.9367, 205.9047], 8.4736, 0/0/1/1 | +4.80% / 22.23% | pass |
| dynamic/browser_tabs_clear_command | 1.1859 [1.1312, 1.2896], 0.1147, 0/0/2/0 | 1.1406 [1.1067, 1.1760], 0.0631, 0/0/2/0 | -3.82% / 29.02% | pass |
| dynamic/layout | 32.3671 [32.2899, 32.7828], 0.3604, 0/0/1/0 | 25.8439 [25.7333, 25.9562], 0.1905, 0/0/2/1 | -20.15% / 10.00% | pass |
| dynamic/missing | 242.7150 [240.8758, 244.7434], 3.3633, 0/0/1/0 | 256.0118 [254.5119, 257.8783], 2.8354, 0/1/0/1 | +5.48% / 10.00% | pass |
| dynamic/mouse_gestures | 4.1426 [4.1147, 4.1953], 0.0863, 0/0/1/1 | 4.0338 [3.9685, 4.0949], 0.0982, 0/0/1/0 | -2.63% / 10.00% | pass |
| dynamic/network | 4.9729 [4.8522, 5.0392], 0.1282, 0/0/1/0 | 4.8138 [4.7515, 4.8971], 0.1235, 0/0/0/1 | -3.20% / 10.00% | pass |
| dynamic/processes | 41.7818 [40.9329, 42.5690], 1.2775, 0/0/0/0 | 44.3376 [43.0716, 45.6595], 2.7687, 0/0/0/0 | +6.12% / 19.88% | pass |
| dynamic/shell | 2.4359 [2.3948, 2.4648], 0.0591, 0/0/1/1 | 2.4614 [2.4354, 2.5202], 0.0742, 0/0/1/1 | +1.05% / 10.00% | pass |
| dynamic/sysinfo | 3.0764 [3.0612, 3.1257], 0.0587, 0/1/0/1 | 3.2050 [3.0734, 3.6662], 0.4244, 0/0/1/0 | +4.18% / 41.38% | pass |
| dynamic/volume | 3.7787 [3.7509, 3.8411], 0.0904, 0/0/0/1 | 3.8024 [3.7599, 3.8752], 0.1079, 0/0/0/0 | +0.63% / 10.00% | pass |
| static/representative_500/broad_real | 389.4530 [379.7959, 393.4381], 12.2363, 0/0/0/0 | 420.1641 [408.6172, 431.4789], 19.1218, 0/0/1/0 | +7.89% / 14.73% | pass |
| static/representative_500/high_specificity_real | 152.3244 [149.1761, 157.2398], 6.8194, 0/0/0/1 | 153.1639 [152.0050, 154.7538], 2.3573, 0/2/1/1 | +0.55% / 13.43% | pass |
| static/representative_500/no_match_real | 81.3640 [80.9036, 81.9441], 0.8601, 0/0/0/0 | 83.7283 [82.5335, 84.3275], 1.7714, 0/0/0/0 | +2.91% / 10.00% | pass |
| static/stress_10k/cached_repeat | 0.0080 [0.0078, 0.0083], 0.0006, 0/0/0/0 | 0.0094 [0.0092, 0.0098], 0.0010, 0/0/1/0 | +17.84% / 37.44% | pass |
| static/stress_10k/real_query_cycle | 2871.8593 [2838.9446, 2927.8474], 81.1723, 0/0/0/0 | 3065.7640 [3059.8878, 3108.6277], 41.8557, 0/1/0/0 | +6.75% / 10.00% | pass |

The first of three serialized candidate-only `radial_runtime` commands used the same
Criterion settings and passed under UUID
`1a84cbc1-4082-42df-abf8-d7c1832e8a23` in `7m46.706s` (`3m33s` compilation).
Two further identical invocations in exec sessions `94794` and `29032` both exited
`0`; the former rebuilt in `3m29s`, while the latter reused the artifact in `0.97s`.
All three retained the same absolute threshold classification below. The table records
the first run's median [95% CI], MAD, and outliers; units are explicit. Criterion's
ordinary unnamed repetition updates its `base`/`new` state, so the two stability
repetitions are retained as command/exit records rather than falsely claiming three
separately named raw-sample archives.

| Cells | Operation | Median [CI] | MAD | Outliers |
|---:|---|---:|---:|---:|
| 8 | hit | 14.9833 ns [14.7492, 15.4037] | 0.6232 ns | 0/1/1/1 |
| 8 | layout | 2.1244 us [2.0814, 2.1706] | 0.0689 us | 0/0/0/0 |
| 8 | page_0 | 9.4083 us [9.2206, 9.7683] | 0.4781 us | 0/0/1/0 |
| 8 | scene | 3.3846 us [3.2642, 3.4505] | 0.1504 us | 0/0/1/0 |
| 8 | selection | 3.5407 us [3.5044, 3.6219] | 0.1723 us | 0/0/1/1 |
| 8 | static composition | 4.0420 ms [4.0342, 4.0802] | 0.0475 ms | 0/0/0/1 |
| 8 | warmed composition | 154.5350 ns [150.6391, 166.6779] | 17.1134 ns | 0/0/1/0 |
| 32 | hit | 33.3683 ns [33.1901, 33.6395] | 0.4189 ns | 0/1/0/0 |
| 32 | layout | 8.2037 us [7.9477, 8.3785] | 0.3794 us | 0/0/1/0 |
| 32 | page_0 | 12.9854 us [12.7497, 13.2872] | 0.4741 us | 0/0/0/0 |
| 32 | page_3 | 13.7199 us [13.3352, 13.8321] | 0.5054 us | 0/0/0/1 |
| 32 | scene | 13.6614 us [13.5537, 13.7948] | 0.2306 us | 0/0/0/1 |
| 32 | selection | 12.6346 us [12.4129, 12.8758] | 0.3660 us | 0/0/1/0 |
| 32 | static composition | 5.9257 ms [5.8970, 5.9421] | 0.0419 ms | 0/0/0/0 |
| 32 | warmed composition | 512.4612 ns [479.8900, 546.0287] | 61.1184 ns | 0/0/1/1 |
| 128 | hit | 116.4828 ns [115.6312, 117.4723] | 1.8050 ns | 0/0/0/0 |
| 128 | layout | 34.6328 us [34.0239, 34.9956] | 0.9082 us | 0/2/1/0 |
| 128 | page_0 | 27.3128 us [26.7964, 28.3991] | 1.5375 us | 0/0/2/0 |
| 128 | page_15 | 29.1500 us [28.6049, 29.7769] | 1.0456 us | 0/0/0/0 |
| 128 | scene | 55.0993 us [54.5061, 56.1110] | 1.2344 us | 0/0/0/0 |
| 128 | selection | 55.3491 us [54.7919, 55.9575] | 1.2931 us | 0/0/1/0 |
| 128 | static composition | 13.9014 ms [13.8482, 13.9589] | 0.0888 ms | 0/0/2/1 |
| 128 | warmed composition | 1.7683 us [1.7186, 1.8211] | 0.0948 us | 0/0/3/0 |
| 512 | hit | 605.4071 ns [603.4310, 611.7436] | 6.5028 ns | 0/0/0/0 |
| 512 | layout | 126.4905 us [124.8738, 127.5472] | 2.2916 us | 0/0/0/0 |
| 512 | page_0 | 84.8036 us [83.2737, 86.7069] | 2.8117 us | 0/0/0/0 |
| 512 | page_63 | 87.2937 us [84.6417, 90.5522] | 5.4625 us | 0/0/1/0 |
| 512 | scene | 196.8995 us [192.9771, 198.1744] | 3.8530 us | 0/0/0/0 |
| 512 | selection | 203.0363 us [198.7579, 205.5240] | 4.9274 us | 0/0/0/0 |
| 512 | static composition | 81.2490 ms [80.8097, 81.4411] | 0.6427 ms | 0/0/0/1 |
| 512 | warmed composition | 7.2667 us [6.9444, 7.5399] | 0.6033 us | 0/0/1/0 |

All hit medians satisfy the provisional 0.5 ms threshold. The `static_composition`
case creates a fresh cache and performs its first composition, so its applicable cold
asset/first-frame threshold is 150 ms: 8, 32, 128, and 512 cells all pass at 4.0420,
5.9257, 13.9014, and 81.2490 ms. It is not correctly compared with the steady-frame
33.3 ms dense threshold. `warmed_composition` and selection-scene construction all
pass the 16.7 ms small/medium and 33.3 ms dense thresholds; the slowest of those is the
512-cell selection case at 0.2030 ms. This benchmark is a pure per-operation harness,
so it is not startup, idle, native-present, or end-to-end frame evidence. Every
composition used the configured 128 MiB aggregate cache budget and completed, but the
harness does not export a byte watermark; it therefore provides no measured
cache-headroom claim beyond functional admission under that cap.

Across the repetitions, the only greater-than-15% point change from the first series
that was not an improvement was the third run's 8-cell scene median (about `3.96 us`
versus `3.38 us`). An exact-filter investigation used a 3-second warmup, 10-second
measurement, and 100 samples and exited `0` in `14.755s`; its median was
`3.2681-3.3172 us` and slope interval `3.3148-3.3823 us`, reproducing the first series
rather than the excursion. The absolute result remains four orders of magnitude below
the applicable frame budget, so no source change is justified.

Verbose logs and Criterion raw estimates are retained. Baseline search log SHA-256 is
`9FC543D4B880D613313FAE29D0B5A3C242F336C41372283ACE5FEC24A1D04363`, candidate
search log `3F4C47F786285AA179737FDEDE60C4044FC2972B1FB0A2196433254F809C9A9A`, and
candidate radial log `F9192A096C684671CE5F54EB85004CDFDE801765B1C7E8F1FEAB0E21C52DAB98` under
`%TEMP%`; raw estimates remain under the two isolated target `criterion` directories
(19 baseline search and 50 candidate search/radial estimate sets). The single-run
lookup flag was investigated as recorded below. At this stage, live native and
application-process evidence had not yet run; it is recorded later. The benchmark does
not export a separate runtime byte watermark, so none is claimed. The cache-budget
acceptance is instead exact and deterministic: the full passing suite includes
`aggregate_byte_budget_evicts_deterministically_and_invalidation_is_exact`,
`completed_and_static_layers_share_one_budget`, and the 100-cycle assertion that
retained bytes never exceed the injected budget and return to zero after invalidation.

#### Lookup flag investigation and threshold correction

The exact Criterion filter was verified with
`cargo bench --locked --bench search -- 'completion/suggestion_lookup/10000' --list`;
it listed only that benchmark. Four runs then alternated candidate, baseline,
candidate, baseline using the already-built isolated target directories, identical
toolchain/environment, and the longer settings `--warm-up-time 3 --measurement-time
10 --sample-size 100 --noplot --verbose`. Each used a distinct saved-baseline label.

| Order | Commit | UUID | Wall / compile | Median [95% CI] | MAD | Outliers |
|---:|---|---|---:|---:|---:|---:|
| 1 | candidate | `1f202927-24f5-46a9-b7f1-34d090616417` | 3m54.127s / 3m36s | 1.190109 us [1.180247, 1.206290] | 0.055503 us | 0/0/2/0 |
| 2 | baseline | `772bf357-53f2-4d33-8cae-f19070f552a6` | 18.455s / 0.98s | 1.176390 us [1.168417, 1.196739] | 0.060994 us | 0/0/1/0 |
| 3 | candidate | `a1b1455c-6a66-4a7a-9f64-bee2972fe0cb` | 18.443s / 0.94s | 1.183810 us [1.164572, 1.204599] | 0.073696 us | 0/0/1/1 |
| 4 | baseline | `4278e68a-54f2-4aa0-a59f-1afb95c40b9d` | 3m20.734s / 3m02s | 1.177560 us [1.161417, 1.189794] | 0.066561 us | 0/0/4/3 |

Outliers are low-severe/low-mild/high-mild/high-severe. The individual distributions
overlap materially. Pooling the two 100-sample raw distributions per commit gives a
baseline median/MAD of 1.177158/0.042283 us and candidate median/MAD of
1.188431/0.042584 us. The candidate delta is +0.958%; the specified allowance is
10.852%, so repeated evidence does **not** classify this as a regression. The original
20-sample candidate median of 1.7287 us was an order/noise excursion, not reproduced
by either longer candidate run, and the material latency remains approximately 1.2 us.

Call-path inspection confirms the benchmark constructs the 10,000-entry FST once and
measures only `completion::suggestions`: lowercase the fixed query, create an FST range
stream, collect at most five matching strings. `benches/search.rs`,
`src/completion.rs`, and `src/actions/mod.rs` have empty baseline-to-candidate source
diffs; both locks resolve `fst 0.4.7` with the same checksum. Candidate manifest
differences are radial/Windows features, additional image codecs, SHA-256 support, and
the radial benchmark target; none changes the measured lookup implementation or its
FST dependency. Because the regression did not repeat, profiling was not triggered and
no unrelated source change is justified.

Logs are retained under `%TEMP%` with SHA-256 values: C1
`8081AECD8C4BA9705F162706D7783B53072617442F60984E8408EAF04DDBF20F`, B1
`8F6433AF6E6B6A145ADB11C555935AD9521C0D8C18AB08C3CA372AB1148677F3`, C2
`9BDD553CCF1A73235705E8E84758EC99EF48F729CC4C9877763110680A7F55F8`, and B2
`C82FB264193508D8467BE41FF2FE12F4C8A0BDA514EB05599E52F75DDCE5D458`.
Raw estimates remain under the named `investigation-c1`, `investigation-b1`,
`investigation-c2`, and `investigation-b2` Criterion directories. The corrected radial
classification leaves every measured provisional latency threshold passing. Cache
byte watermark evidence remains unverified. At this benchmark stage there was no
startup, idle, or live native claim; the subsequent evidence is recorded below.

### M6 live Windows host and idle-resource evidence

The opt-in ignored probe
`radial::native::tests::live_radial_host_probe` ran on the candidate commit with
`MULTI_LAUNCHER_RADIAL_LIVE_PROBE=1` and passed 1/1 (`3,845` filtered out) after a
`2m26s` debug build; the test itself completed in `0.05s`. This creates the real
Windows layered visual HWND and shaped input-proxy HWND on an interactive desktop,
asserts both are visible and distinct, proves creation does not change the foreground
window, proves `WindowFromPoint` selects the proxy over an owned wheel point, proves
an exterior point is not owned by the proxy, and destroys the surface before exit.
This is direct native HWND/input-shape evidence, but it does not claim delivery of a
physical click into another process or visual-quality inspection.

Environment: Windows `10.0.19045`, candidate
`da0215fb08d8f4dff198147dbfe0b7daf667a247`, NVIDIA GeForce GTX 1080 driver
`32.0.15.6094`, one active `2560x1440@59 Hz` display at `(0,0)`, and applied DPI
`192` (200%). A Parsec Virtual Display Adapter was installed but exposed no active
resolution. The host was configured at 200%, but the isolated live probe deliberately
constructs a `ScaleFactor::new(1.0)` layout and the idle launches do not open the lazy
native surface. The evidence therefore covers real Win32 ownership on a 200%-configured
single-monitor host, not 200%-scale rendering/input, mixed DPI, or negative-origin
coverage. Native desktop automation was unavailable in the Codex session: the exposed
Computer Use inventory contained browser surfaces only, and the lower-level Windows
target service reported that it was not configured.

The exact isolated baseline and candidate release binaries were also launched from
separate disposable profiles with `MULTI_LAUNCHER_PERF=1`. Readiness is elapsed time
from `Start-Process` until a nonzero responsive main-window handle. The first run used
an empty profile (cold profile); the second reused its generated settings/index state
(warm profile). Each readiness point was followed by a 30-second idle sample, then the
exact process was stopped before the next run.

| Revision / profile | Ready | Idle CPU | Handles | Threads | Working set | Private bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| baseline `0d0acaf`, cold | 775.5 ms | 1.578125 s | 667 | 55 | 88,276,992 | 99,897,344 |
| candidate `da0215fb`, cold | 978.0 ms | 4.281250 s | 672 | 57 | 93,208,576 | 104,038,400 |
| baseline `0d0acaf`, warm | 413.3 ms | 0.562500 s | 664 | 55 | 81,805,312 | 93,401,088 |
| candidate `da0215fb`, warm | 419.8 ms | 0.515625 s | 667 | 56 | 82,644,992 | 93,089,792 |

Cold profile work includes one-time default persistence/indexing and is recorded as an
observation, not a stable regression signal. The matched warm readiness delta is
`+1.57%`; candidate idle CPU is slightly lower, with +3 handles, +1 thread,
+839,680 working-set bytes, and -311,296 private bytes. Direct UI search could not be
driven because native Computer Use was unavailable. The gate is therefore explicitly
revised to use the serialized, purpose-built baseline/candidate Criterion search target
recorded above for the search workload; it exercises the actual completion, dynamic,
command-cache, and static search implementations without unsafe UI input synthesis.

The release binary was launched twice from the disposable data root
`.radial-acceptance-candidate` with `MULTI_LAUNCHER_PERF=1`, once with the persisted
radial setting disabled and once enabled. Both remained responsive. A 30-second idle
sample after initial startup produced:

| Mode | CPU delta | Handles | Threads | Working set | Private bytes |
|---|---:|---:|---:|---:|---:|
| disabled | 0.015625 s | 663 | 55 | 83,165,184 | 93,822,976 |
| enabled | 0.000000 s | 667 | 56 | 82,735,104 | 93,085,696 |

The enabled-minus-disabled point sample is four handles and one thread, with no
measurable idle CPU increase; memory differences are restart noise rather than a
regression claim. The earlier settled enabled sample independently observed 658
handles, 51 threads, a 93,401,088-byte working set, a 103,501,824-byte private set,
and 0.015625 CPU seconds over 30 seconds. These are bounded process observations, not
per-resource ownership attribution or a multi-run statistical startup benchmark.

The manual checklist in `docs/manual-smoke-tests.md` remains authoritative for the
parts no automated probe can establish: physical 0/100/349/350/351-ms shared-chord
behavior and suppression, real second-process click delivery, visual contrast,
keyboard-only authoring, Narrator/AccessKit speech, high-contrast rendering,
mixed/fractional-DPI and negative-origin monitors, real target churn/UIPI, and physical
capture handoffs. Those rows are intentionally not marked passed. M6 remains
`in_progress` until a human records the applicable manual results (with explicit N/A
reasons for unavailable hardware) or the release owner accepts them as a separate
release-validation gate.

| Requirement | Direct source/test evidence |
|---:|---|
| 39 | `universal_actions::persisted_resolver::custom_identity_survives_reorder_but_never_retargets_a_deleted_slot`; menu stable-ID edits |
| 40 | `bindings::ephemeral_clipboard_list_browser_and_window_targets_revalidate_exactly`; `target::ephemeral_targets_do_not_claim_persistent_identity` |
| 41 | `universal_action_executor::unavailable_radial_action_is_rejected_before_confirmation_or_execution`; controller button-specific availability |
| 42 | universal executor destructive-confirmation tests, including consumed cancel and confirmation-time resolver path |
| 43 | radial invocation context/root-state tests and native-preview no-history boundary |
| 44 | `ordinary_radial_execution_preserves_root_launcher_state`; explicit command executor tests |
| 45 | context own-process exclusion and native-preview owned/editor-window sanitization tests |
| 46 | `context::priority_then_configured_order_is_deterministic`; captured controller menu/context tests |
| 47 | dynamic removed-entry/revisit tests and bindings/controller stable pagination tests |
| 48 | handoff close/release ordering and controller release-alias/wait matrices |
| 49 | existing command/executor runtime-failure diagnostics plus handoff fail-closed tests; UIPI remains live-only |
| 50 | handoff/controller stale-session, stale-generation, replacement, and exact-once tests |
| 51 | skin precedence/clear/provenance tests and full-scope editor round-trip test |
| 52 | exhaustive 71-field consumer audit plus geometry/render/compositor/audio/native behavior tests |
| 53 | asset decode/budget/cache/fallback tests, font fallback tests, straight-alpha compositor tests |
| 54 | import accepted-field registry and typed apply-or-diagnose outcome tests |
| 55 | read-only supplied-archive tests and explicit synthetic-evidence diagnostics in `radial::import` |
| 56 | package canonical round-trip, collision remap, dependency closure, and asset-reference tests |
| 57 | package hostile-path, ZIP-limit/header, link/reparse-point, and case-collision tests |
| 58 | store invalid-save/package-failure rollback and malformed/newer-retention tests |
| 59 | store revision/SHA conflict tests and authoring captured-baseline conflict tests |
| 60 | authoring/menu clone/link/copy/move/delete/reference-remap tests |
| 61 | authoring coalescing/undo bounds/Apply/Save/Cancel/dirty-close and staged-resize tests |
| 62 | native-preview lease/non-dispatch tests and separate authoring Test-action revalidation path |
| 63 | radial settings cofire/conflict tests and main fail-closed route replacement tests |
| 64 | model starter/settings-default tests and store missing/empty/malformed distinction tests |

The field audit is exhaustive over the serialized full-scope schema. “First consumer”
names the boundary at which changing the value becomes observable; later consumers may
also retain the value in an immutable snapshot.

| First consumer | Serialized style fields |
|---|---|
| Geometry | `menu_scale`, `item_size`, `radius_scale`, `center_size`, `outer_ring_margin`, `outer_rim_width` |
| Scene | `item_glow`, `menu_outer_rim`, `menu_background`, `item_background`, `item_foreground`, `item_shadow`, `menu_foreground`, `center_background`, `center_image`, `submenu_indicator`; all eleven corresponding opacity fields including `icon_opacity`; `center_image_scale`, `item_image_scale`, `item_image_y_ratio`, `item_background_scale`, `item_foreground_scale`, `item_shadow_scale`, `menu_background_scale`, `menu_foreground_scale`, `center_background_scale`, `submenu_indicator_size`, `submenu_indicator_y_ratio`, `item_background_on_center`, `item_background_on_items`; every text field (`visible`, indicator text, family, size, color, bold, italic, underline, strikeout, shadow enable/color/offset, box scale, vertical ratio); `glow_enabled`, `tooltip_mode`, and all three menu-shadow fields |
| Compositor | `text` quality, `shape` quality, `interpolation` quality |
| Audio | `on_show`, `on_close`, `on_select`, `on_submenu_show`, `on_submenu_close` |
| Input shaping | `fill_center_hit_zone`, `fill_item_hit_zones` |
| Native window | `always_on_top`, `activate_on_show` |

### M5 substep 1 — authoring coordinator foundation

`radial::authoring` owns the pure revisioned draft/session model and typed GUI-to-main
protocol. A session retains an `Arc<RadialDocument>` baseline with revision and exact
disk SHA-256, a generation-tagged draft and clean checkpoint, stable-ID selection,
pending managed-asset additions/deletions, one pending request, explicit external
conflict state, and count/byte-bounded undo/redo. Field, slider, and drag edits coalesce
by stable entity and field until end; complete ring resize, duplication, and
import-shaped document replacements are one atomic undo entry. Apply resets the clean
baseline while remaining open, Save closes, and Cancel-after-Apply sends a checked
inverse commit for the last successful Apply. Dirty close is an explicit prompt
decision. Clean external publication refreshes automatically; dirty state retains all
three documents for compare/reload/discard/rebase, with conservative three-way merge
and no force-overwrite path. Request ID/generation checks reject stale replies.

The GUI receives only an `AuthoringClient`; `main` owns the endpoint, validates live
preview without persisting or rebinding hotkeys, and is the sole caller of the store
commit. Successful commits close/reconfigure the controller, invalidate resources and
GUI leases, refresh macro/global trigger reservations, and begin the existing
generation-safe invocation route handoff before replying with the published revision.
`RadialStore::commit_authoring` checks revision and exact disk bytes, validates the
candidate plus supplied asset hashes/media, guards live references and shared managed
paths, stages deletions, publishes assets plus JSON transactionally, rolls back only
transaction-created/staged files on failure, and returns the inverse asset mutation
needed by Cancel-after-Apply. Malformed/newer or semantically changed disk bytes cannot
be paired with the retained runtime snapshot for editing.

Focused source tests cover stable-ID rename selection, atomic duplication/ring resize,
coalescing, undo count/byte bounds, asset undo/redo, clean-versus-dirty external
publication, non-overlapping/conflicting rebase, stale replies, Apply/Save/Cancel and
dirty-close semantics, typed service routing, revision/SHA no-partial-file rejection,
asset+document publish/inverse rollback, malformed snapshot rejection, and asset
reference guards. Per the assigned substep, no Cargo/Nextest gate was run. Direct
`rustfmt --edition 2024` on the touched Rust files and `git diff --check` passed. M5
remains `in_progress`; editor rendering, settings, commands, and starter expansion are
later M5 substeps.

### M5 substep 2 — Universal Action authoring catalog and Test Action boundary

`gui::universal_action_catalog` now owns one immutable, already-loaded target snapshot
shared by radial preparation, dispatch-time revalidation, and editor action discovery.
It combines launcher and command catalogs with current dashboard Notes, Snippets,
Favorites and clipboard entries, MkMacro definitions, live windows, recent history,
and Crop's read-only provider results. Screen Draw and dashboard commands enter through
the existing command-provider inventory. All operations continue to resolve through
`UniversalActionRegistry`; the catalog introduces no execution implementation.

Picker rows expose the exact target command and semantic action ID, effective radial
presentation and availability reason, safety, interaction/handoff requirement, and
after-action compatibility. Assignment returns only a durable
`PersistedUniversalActionRef` or typed contextual window selector. Runtime window
handles, clipboard/list indexes, browser IDs, and other ephemeral targets remain
visible as explicitly nonpersistable rows. Contextual selectors remain assignable even
when their current preview target is unavailable and resolve against a fresh invocation
context at use time.

Preview catalog construction is side-effect free. The separate Test Action facade
re-resolves the selected binding and invokes the existing Universal Action executor
with `ActionSurface::RadialMenu`, normal confirmation/safety/history behavior, no
radial dispatch lease, and no session-close path. Source tests cover stable/contextual
serialization, ephemeral rejection, HWND churn, provider presentation/policy parity,
non-executing preview browsing, explicit executor routing, destructive confirmation
exactly once with one history record, and cancellation with no execution/history.
Per the assigned substep, no Cargo/Nextest gate was run.

### M5 substep 3 — menu editor and embedded preview

The launcher now registers a normal `RadialEditor` panel with the existing focus,
pinning, stack, close, and forced-close lifecycle. Its three coordinated areas are a
stable-ID menu/ring/cell tree, an embedded compositor-backed preview, and a selected
entity inspector. Widget identity is derived only from entity kind, persisted entity
ID, and field; labels and list indexes never participate. Drag/drop records an ID-only
command during rendering and applies it afterward. The inspector exposes menu/ring/
cell CRUD, spacers, duplication, bounded undo/redo, guarded deletion, explicit ring
shrink resolution, and the shared Universal Action authoring picker.

Pure graph changes live in `radial::authoring::menu`. Submenu duplication distinguishes
linking existing descendants from cloning the complete closure with deterministic fresh
IDs and rewritten internal references, rejects cyclic graphs, and reports reference
impact before deletion. Ring shrink first returns a `ResizePlan` listing every removed
populated cell; it cannot apply until the caller chooses relocation, an overflow ring,
or explicit destructive discard. Each complete operation remains one atomic entry in
the substep-1 bounded history.

The preview uses the production layout, scene builder, M4 compositor, and navigation
`SessionReducer` with synthetic preview state. Submenu and Back navigation therefore
follow runtime transitions. Any `Dispatch` intent is intercepted inside the preview;
it has no Universal Action executor, history, confirmation, audio, or native-window
lease. Apply, Save, Cancel-after-Apply, dirty close, and external conflict choices route
through the typed authoring service. Source tests cover clone/link/cycle behavior,
shrink without implicit loss, reference guards, undo, selection/widget identity across
rename/reorder, and preview navigation/back/non-dispatch. Per the assigned substep, no
Cargo/Nextest gate was run; direct Rust formatting/check and diff checks were used.

### M5 substep 4 — skin, asset, import/package, and audio authoring

The shared editor now exposes every full M4 style override and every legal restricted
ring/cell override through the serialized typed schema, with explicit Inherit, Set,
Clear, and typed-value controls. Effective values retain visible `StyleSource`
provenance from the production skin compiler. Controls cover media, geometry/scaling
and alignment, opacity, text/system-font and RGBA color data, rim/background/center/
glow/tooltip, rendering quality, sound, window behavior, skin CRUD, per-menu selected
skin, and user-default scope. No font or reference asset is copied or bundled.

Managed/external/search-path/icon resource choices retain their M4 portability
semantics and diagnostics. Selected image/WAV data passes the same bounded M4 decoder;
managed bytes are staged in the authoring session while external resources remain
explicitly nonportable. Asset and skin deletion first queries the production reference
walker and displays every blocking path. Sound audition is an explicit button routed
through a uniquely scoped `RadialAudioSession` and the bounded audio mailbox; ordinary
editor rendering, hover, and all preview presets have no audio path.

Package and legacy controls construct immutable M4 `ImportPlan`/`ImportPreview`
reviews before changing the draft. The review displays deterministic mappings,
warnings, and destination. Acceptance rechecks revision, disk identity, draft
generation, and conflict state, then merges the graph and managed bytes through one
bounded undo entry. Malformed, hostile, over-budget, and unsupported packages remain
rejected by the M4 decoder/planner. Export is a generation-correlated request to the
main-owned `RadialStore`; it resolves persisted managed bytes below the private store
root, rejects missing/tampered/link content, and returns a dependency-complete M4
package for an atomic write to the explicitly selected destination. The GUI neither
owns the store nor assembles packages from partial draft bytes. Create New remains the
default import action. Replace requires a separate destructive confirmation and
user-selected backup destination; main atomically creates and verifies the exact disk
backup before entering the revision/SHA-guarded transactional replacement path.
Stale/conflicting replies cannot mutate editor state, and replacement success reports
the verified backup path. Representative preview presets cover current/one-ring/multi-ring/submenu/long
labels/high-DPI; zoom and preset are UI-only state and never alter the draft.

Focused source tests cover all 71 full-scope fields across Inherit/Set/Clear plus
round-trip and provenance, zoom/preset non-dirty behavior, skin/asset reference guards,
portable/nonportable resource diagnostics, side-effect-free import preview, atomic
accept/undo and stale-draft conflict rejection, malformed/unsupported package
rejection, exactly-once explicit audition, persisted-asset export round-trip with
missing/tamper diagnostics, GUI ownership boundaries, confirmation/backup enforcement,
replacement success/rollback, and stale replacement/export replies. Per the assigned
substep, no Cargo/Nextest gate was run.

### M5 substep 5 — cancellable native desktop preview lease

The editor can explicitly start, update, or stop a native desktop preview through a
session/request/generation-correlated authoring lease. `main` alone owns the dedicated
preview coordinator and lazy native host. It validates the draft against the current
store revision/disk identity, uses the production desktop geometry, layout, shared
preview frame builder, vector scene, compositor input, native host, and `SessionReducer`,
and never installs a global or item trigger route. The coordinator has no Universal
Action executor, history sink, audio session, or reservation owner; click/key navigation
may select, enter submenus, page, or go Back, while every `Dispatch` intent is counted
and discarded.

Only one native surface is active, with per-editor monotonic generation/request
tombstones preventing stale starts or replies from resurrecting it. A newer draft
generation replaces the lease; close, Apply, Save, external publication/conflict, and
package replacement stop it fail-closed. A stop request supersedes an in-flight start,
so closing the editor cannot strand a late native surface. Context is synthetic unless
the user explicitly enables sampling. Sampled foreground/pointer identities exclude
the launcher/editor/preview process and explicit preview HWNDs, while preserving the
last external foreground fallback. The GUI reports pending/active/stopped state,
sampled context, and correlated errors.

Focused source tests cover exclusive/stale leases, stop superseding an in-flight start,
generation/save/conflict cancellation, dispatch interception, the absence of audio,
history, executor, and hotkey ownership, HWND/context sanitization, and identical native
and embedded `PreviewFrameInput` construction. M5 remains `in_progress`; settings,
commands, and starter expansion are later substeps. Per assignment, no Cargo/Nextest
gate was run.

### M5 substep 6 — typed radial commands and launcher integration

The command domain now owns `RadialCommand` and parses/displays the stable `radial`,
`radial show <id-or-name>`, `radial close`, `radial edit`, and `radial skins` wire
forms. The command bus uses a dedicated host facade: Show and Close enqueue exactly
one typed request to the main-owned radial control endpoint, while Edit and Skins
focus the existing stable editor panel (with Skins selecting its shared resource
area). Outcomes keep launcher query, results, selection, visibility, and focus intact
and record the user activation once; opening an editor remains the one explicit panel
transition. Disabled runtime presentation fails before enqueue, and Close remains
idempotent.

`main` resolves an exact menu ID before attempting a unique case-insensitive name,
reports missing and ambiguous matches with actionable IDs, and routes accepted
requests through the existing controller `OpenRadial`/`CloseRadial` intent and
preparation path. No simulated key input or parallel execution path was added. A
registered Radial plugin supplies launcher search, command/help discovery, and dynamic
Show actions. The Universal Action authoring catalog also includes stable Generic
targets for the four static radial controls, so radial bindings use the normal
Universal Action resolver/executor.

Focused source tests cover parse/display/serde round trips, malformed input,
ID-before-name resolution, missing/ambiguous diagnostics, disabled Show and idempotent
Close, exactly-once control enqueue, editor/skins panel routing, launcher-root
preservation with one history record, and plugin search/help inventory. M5 remains
`in_progress`; settings and starter expansion are later substeps. Per assignment, no
Cargo/Nextest gate was run.

### M5 substep 7 — radial settings and reconfiguration

`RadialFeatureSettings` now preserves legacy behavior while adding typed defaults for
the selected menu, new-menu interaction and submenu presentation, destructive-action
safety, and new item-input scope. The settings editor maps every radial field
explicitly in both directions. Its Radial menus section explains tap-on-release versus
hold-threshold crossing, shows the current trigger and conflicts, validates threshold,
default-menu existence, and process-wide input co-fire, and opens the existing menu or
skin editor without changing the published document or dirtying its draft.

The main-owned immutable document view supplies menu choices and diagnostics without
giving the GUI store ownership. Settings Save still uses the established atomic
settings update and hotkey restart. Main validates startup and reload settings,
disables invalid radial runtime configuration fail-closed, advances one coherent
generation, and uses one route plan for service handoff, legacy-listener restoration,
and service stop/start decisions. The configured default menu feeds both shared
tap/hold invocation and the typed `radial` command. New menus consume the configured
interaction/submenu defaults; radial-only forced destructive confirmation remains
scoped to `ActionSurface::RadialMenu`.

Focused source tests cover legacy deserialization, all-field settings/editor
round-trips, validation language and conflicts/co-fire, default-menu invocation,
enable/disable route plans, new-menu defaults, scoped safety confirmation, and clean
editor opening. M5 remains `in_progress`; starter expansion is the remaining substep.
Per assignment, no Cargo/Nextest gate was run.

### M5 substep 8 — starter content and capability-aware empty states

`RadialDocument::starter` now provides an editable root and child menus for Favorites,
Apps, Windows, Macros, Notes, Snippets plus Clipboard, Screen Tools, and Dashboard.
Every child retains explicit Back and Close controls even when its collection has no
items. User collections remain typed dynamic sources; Applications and Dashboard join
the existing invocation-frozen snapshot boundary and never serialize process handles,
clipboard indexes, result indexes, or live provider state. Screen Draw, screenshot,
and Crop seeds are exact stable Universal Action references and use `CloseTree` before
their established capture/handoff executor runs. Text and collection selections use
the same safe close policy.

Frozen dynamic entries distinguish action, manage, empty, loading, and unavailable
presentation. Empty and unavailable sources remain visible and non-dispatchable.
Only manage commands proven by the current provider catalog are appended (Favorites,
MkMacro, Notes, Snippets, Clipboard, and Dashboard Settings); disabled Dashboard state
explains how to enable it. All candidates are captured once with the preparation
generation, so later provider or foreground changes cannot alter an open invocation.

Authoring Save/Apply and Create New/Replace package publication now reject a candidate
that would remove the settings-selected default menu, including while radial runtime
is disabled. The check occurs before store mutation or replacement backup application.
Successful publication reconfigures through the shared `RadialRoutePlan`; a handoff
start failure disables the radial endpoint/controller, stops the service, and restores
the legacy listener fail-closed. Source tests cover starter graph IDs/references,
Back/Close and capture bindings, typed frozen states/manage entries, per-invocation
freezing, and the settings-selected publication invariant.

M5 implementation is complete but remains `in_progress` until the combined focused
gate and independent review succeed. Per assignment, no Cargo/Nextest command was run
during this substep. Direct `rustfmt --edition 2024` on the touched Rust modules and
`git diff --check` passed. The workspace-wide rust-analyzer diagnostic scan completed
nonzero because the cumulative tree still reports broad type-inference diagnostics;
two concrete M5 annotations in editor columns and submenu clone mapping were corrected,
while the focused Cargo gate remains the authoritative compile/test check.

### M5 focused gate and root-correction record

The required gate was run from `G:\Repos\rust\Multi_Launcher` with the default
Nextest profile and the exact command
`cargo nextest run -E 'test(/radial/) | test(/settings_editor/) | test(/command/) | test(/universal_action/) | test(/plugin/)'`.
Only one Cargo tree was active at a time. Each failed tree fully exited before its
root correction and replacement. The gate attempts were:

| UUID / requested start | Result | Root correction |
|---|---|---|
| `4bdedf7e-72de-4f64-9c8e-534dc6dd3fe0`, `2026-09-14T20:39:56.4784761-04:00` | exit `1` after about `1m 06s`; PTY retained no run ID or test count | Repeated non-interactively to obtain an authoritative failure. |
| `338d6b0e-75ec-4d90-81b9-1f67b26e1d68`, `2026-09-14T20:41:34.4689615-04:00` | Nextest `e17a2f92-3c9e-4978-bde2-bca85c55d59c`; compile `0.95s`; 537 run, 536 passed, 1 failed, 366 cancelled, 3,438 skipped | Corrected the control test fixture so a shared display name no longer lowercases to an exact menu ID. |
| `0d54138d-4a92-4de3-9917-aac69c27e04a`, `2026-09-14T20:42:42.2952481-04:00` | Nextest `b76eafb1-248c-48cb-ba60-0676a2766113`; compile `20m 22s`; 583 run, 578 passed, 5 failed, 3,438 skipped | Added a private, single-menu legacy import template instead of treating the expanded nine-menu starter graph as the legacy compatibility candidate. |
| `2fff9642-6b3d-4abc-b50b-218ea60b0757`, `2026-09-14T21:06:42.2491958-04:00` | Nextest `1ecf3fa7-3f63-49ff-b506-e0b4a47d993a`; compile `17m 54s`; 650 run, 647 passed, 3 failed, 3,438 skipped | Isolated package closure fixtures from the expanded starter and updated the starter render expectation from `Recent` to `Apps`. |
| `2b7d4e3c-0a90-473b-9793-19cf94e5ac3a`, `2026-09-14T21:26:31.0720371-04:00` | Nextest `01be5387-71db-4730-b268-9221c9f821a6`; compile `18m 22s`; 684 run, 682 passed, 2 failed, 3,438 skipped | Updated deletion-impact coverage for all nine starter skins and isolated the imported-plan fixture. |
| `49fcdd0b-e9aa-4700-bb4d-63c13d82f9db`, `2026-09-14T21:46:34-04:00` | preflight only; formatting failed and no gate launched | Applied `cargo fmt --all`. |
| `ce007f3b-682d-4d3a-a285-26b2a6ef3c64`, `2026-09-14T21:47:10.3096546-04:00` | Nextest `264d7354-94c9-4982-be86-25c1a586bd3f`; compile `18m 29s`; 715 run, 713 passed, 2 failed, 3,438 skipped | Made paging and shared-DAG validation fixtures explicit rather than inheriting starter submenu edges. |
| `f43f643a-f4a2-40bd-95b8-95100de3c155`, `2026-09-14T22:06:57.1034975-04:00` | Nextest `2974c8ad-6f16-44e4-89a3-135305caebb8`; compile `18m 06s`; 903 passed, 0 failed, 3,438 skipped; tests `27.932s` | Focused gate passed. |

The first post-gate `cargo check` used UUID
`28264fd6-d9fb-45f8-9932-3c2bdf6e87df`, started
`2026-09-14T22:26:16.8686439-04:00`, and exited 0 in `1m 14s`, but reported
nine unused-assignment warnings. The compiler-proven dead `shared_invocation`
assignments were removed without changing route behavior. A replacement exact gate
was launched as `a56ee143-3fef-430c-8ca7-9befeed513fa` at
`2026-09-14T22:29:10.0561629-04:00`, source identity
`3abb262e3ba1c60d441c7663c45f875345dc3d88`; it compiled in `16m 21s` and
reported Nextest run `260eec82-b241-4d03-9978-cf09e4dbf3e9`, but its expired
session handle did not retain an authoritative terminal summary. No process remained,
so the same source was rerun rather than treating that attempt as evidence.

The authoritative source-identical replacement used UUID
`343521ea-0eac-4ea9-ae5a-a775c34d4104`, started
`2026-09-14T22:47:45.2381153-04:00`, source identity
`3abb262e3ba1c60d441c7663c45f875345dc3d88`, and passed as Nextest run
`b5d4ede8-4e33-45e4-831f-28a437c0cac2`: compile `1.19s`; 903 passed,
0 failed, 3,438 skipped; tests `6.795s`. The final `cargo check` used UUID
`765e9aca-5b5c-4247-968b-edad2db2e0bc`, started
`2026-09-14T22:48:18.5729634-04:00` against the same source identity, and
exited 0 warning-free in `29.43s`.

Every generated root `clipboard_modifiers.json` fixture was removed after its run.
No native interactive probe was run, so desktop preview focus/click-through,
cross-process handoff, capture, and mixed-DPI behavior remain explicitly unverified.
M5 remains `in_progress` until its independent review succeeds.

### M5 independent-review remediation pass 1 — correctness/lifecycle

Status: `complete`. This pass is limited to review findings 1, 2, 3, 6, 7, 8,
and 11. The authoring coordinator now invalidates Apply rollback state after accepted
external Reload/Rebase/Discard and package replacement, preventing Cancel from
publishing over newer external or imported state. Every authoring request and reply
now carries `AuthoringSessionId`; reply acceptance proves session, request ID, draft
generation, and expected reply variant before consuming pending state. Late replies
from a closed editor session and mismatched variants remain inert.

Unpinning the radial editor now uses its ordinary dirty-close decision and explicit
save/discard/keep-editing prompt. New menus are built from an explicit clean typed
definition plus configured interaction/submenu defaults; no center/background action,
control, style, shortcut, hotstring, or cell metadata is cloned from another menu.
Destructive Test Action confirmation re-resolves its stable request against a fresh
catalog and the retained invocation context, so target deletion/churn fails without
execution or history. A shared fail-closed radial-route helper now owns startup and
reload service-start failure handling: settings/control/controller/service are
disabled and the legacy listener is restored, causing Show requests to be rejected.

While the initial authoritative Snapshot request is pending, authoring mutations,
selection, asset staging, undo/redo, post-render edits, and the editor draft/tree/
preview/inspectors/resources are disabled. Focused source tests cover external
Reload/Rebase then Cancel, package replacement then Cancel, reused request IDs across
sessions, late replies, reply-variant mismatch, pending-Snapshot mutation blocking,
dirty unpin, clean-menu construction, destructive confirmation target deletion, and
fail-closed Show rejection. Per assignment, no Cargo build or test gate has run in
this pass. Direct `rustfmt --edition 2024 --check` over all seven touched Rust files
and `git diff --check` exited 0; no Cargo/Nextest/rustc process, Git index lock, or
generated `clipboard_modifiers.json` artifact remained at handoff.

### M5 independent-review remediation pass 2 — complete authoring surfaces

Status: `complete`. This source pass addresses review findings 4, 5, 9, 10,
and 12. The normal editor now exposes typed menu layout, geometry, interaction,
dwell, submenu, after-action, center/background left/right action and control,
cell alternate gesture/control, dynamic-source, shortcut/hotstring/scope, context
rule, custom trigger, media search-root, copy/move, submenu link/clone, ring
relocation, and confirmed destructive ring-removal controls. Action-picker rows show
their exact command, semantic action ID, persistence, availability, safety,
interaction requirement, and supported close policies.

The style editor no longer presents serialized JSON. Every one of the 71 full-scope
style fields has an explicit ergonomic control classification (toggle, scalar,
opacity, color, font/text, offset/alignment, enum, rendering quality, or resource),
with scope reset, skin duplicate/gallery selection, provenance, managed/external/
search-path/icon-resource media selection, full-package and selected-skin export
entry points, and persisted managed-sound audition routed through the main-owned
authoring service. The v1 package format remains menu-rooted; an unreferenced skin is
truthfully rejected by the UI until assigned to a menu rather than silently emitting
an incomplete package.

Published menu IDs/names now generate stable `radial show <id>` plugin and Universal
Action candidates dynamically, including an empty `radial show` discovery query;
arbitrary typed names are no longer synthesized. Embedded preview resets by stable
menu/skin/preset selection token, follows the selected menu without a draft edit, and
can apply an unreferenced selected skin to its transient document. Applications use
the full launcher/application catalog (with `custom_len` only typing its prefix),
Favorites resolve every stable persisted provider favorite directly, and Dashboard
includes stable note/snippet/process actions plus its truthful manage/unavailable
state. Focused tests cover all 71 explicit style controls, stable selected-menu/skin
previewing, dynamic named-menu discovery, full production Applications population
beyond the custom prefix, populated-ring relocation, and every left/right action
assignment slot. Per assignment no Cargo command ran in this pass. Direct
`rustfmt --edition 2024` and its `--check` form over the nine touched Rust files,
plus `git diff --check`, exited 0. The Cargo/Nextest/rustc process audit was empty;
no Git index lock or generated `clipboard_modifiers.json` artifact was present.

### M5 independent-review remediation replacement gate

Status: `complete`. All jobs used the exact focused command
`cargo nextest run -E 'test(/radial/) | test(/settings_editor/) | test(/command/) | test(/universal_action/) | test(/plugin/)'`
from `G:\Repos\rust\Multi_Launcher`, with one Cargo tree active at a time against
`HEAD a1366b6c877138a05b2301108e7dd182bc3f4f14` plus the shared uncommitted worktree.
The final pre-ledger tracked-diff identity was
`999614bfde722d249974cd4bbe8cbf91ff3455b2`. Failed or unrecorded jobs fully exited
before source correction or replacement:

| Attempt | Result and correction |
|---|---|
| `904cba61-42b1-4026-acfd-f195e12c56fb` | Compile failed before tests: corrected a shadowed style-control kind and normalized `CellContent` match arms to unit. |
| `4c1427f9-e5ce-4376-b3af-f599d73712ba` | Exited 1 after `18m 14s`; the PTY did not retain a diagnostic summary, so it was repeated non-interactively. |
| `e884c582-2666-4117-8468-389a404a7307` / Nextest `e8803de4-38f0-49f1-adc7-e0a277432d54` | Compile `0.96s`; 168 run, 167 passed, 1 failed, 747 cancelled, 3,439 skipped. Corrected the Applications production fixture to prepare the menu that owns the discovered cell. |
| `ed4605c8-7929-4530-bc65-67d94c784e62` / Nextest `851e02fd-96ac-4488-92ca-e2718a3a9269` | Compile `18m 49s`; 517 run, 515 passed, 2 failed, 398 cancelled, 3,439 skipped. Corrected native-preview and package-reply fixtures to model production pending-request invalidation explicitly. |
| `ebf4e0a0-4c27-4b4d-a17c-f943d58886b5` | Process fully exited, but its retained output overflowed and no authoritative terminal result survived; it was not counted as evidence. |
| Nextest `6259d064-99ea-4d0f-9b20-680e3f95320d`, started after preflight at `2026-09-15T00:50:27.2445147-04:00` | Authoritative source-identical replacement passed: compile `1.00s`; 915 run, 915 passed, 0 failed, 3,439 skipped; tests `7.251s`, invocation wall `10.86s`. |

The single post-gate `cargo check` (exec session `85375`) exited 0 warning-free:
compiler-reported time `57.45s` and observed wall `55.52s` across the retained
session. Every generated root `clipboard_modifiers.json` fixture was removed via a
targeted patch after its run. Final `cargo fmt --all -- --check` and
`git diff --check` exited 0; no Cargo/Nextest/rustc process, Git index lock, or
clipboard fixture remained. The authoring raw-JSON regression scan found no
`Typed JSON`, `serde_json::from_str(encoded`, `style_inputs`, or
`take(custom_len)` path. The two retained `radial show Work Menu` strings are
intentional parser/host test fixtures for space-preserving typed command handling,
not named-menu catalog synthesis.

No native interactive probe was run. Desktop preview focus/click-through, persisted
managed-audio playback on a real device, cross-process handoff, capture, and
mixed-DPI behavior remain manual/native evidence gaps and roll into M6.

### M5 closure remediation — identity, authoring completeness, and portable skins

Status: `complete`. The authoring domain now owns a session-lifetime deterministic
allocator for menu, ring, cell, shortcut, hotstring, context-rule, and custom-trigger
IDs. Deleted IDs stay reserved, and menu/cell duplication remaps every nested input
identity. Mapping controls now clear conflicting action/control choices, remove
alternates, expose every after-action policy, report ring resize/delete errors, and
show path-addressed `validate` diagnostics before enabling Save or Apply.

Real egui text, drag, slider, color, and style-field responses now enter the stable
entity/field coalescing boundary with explicit update/end phases. A typed SkinBundle
package payload exports an unreferenced selected skin plus its exact managed-asset
closure and imports it collision-safely as one undo unit while retaining legacy menu
package compatibility and the hostile path/checksum checks. The action picker uses a
captured editor invocation context; destructive contextual confirmation must still
resolve the exact captured target in the current catalog. Dashboard preparation now
materializes favorite, clipboard, todo, calendar, gesture, system, recycle-bin,
note, snippet, process, and settings families with actionable frozen bindings where
available and truthful status/manage rows otherwise. Font-family selection comes
from the main-correlated cached `SystemFontCatalog`, and icon-resource style controls
preserve and edit a validated nonzero resource index.

Focused tests cover nested input remapping and deleted-ID holes, widget dispatch
coalescing with older undo history, selected-skin canonical/dependency/collision/
security behavior and menu-package compatibility, unreferenced-skin merge/undo,
production Favorites/Dashboard materialization, font request correlation, and icon
resource index round-trip. `cargo fmt --all` and `git diff --check` passed before the
replacement gate; no Cargo/Nextest/rustc process was active.

Closure gate attempts all used the exact focused expression from the preceding M5
gate, one Cargo tree at a time, against
`HEAD a1366b6c877138a05b2301108e7dd182bc3f4f14`. Every failed or unrecorded job fully
exited before a root-only correction or replacement:

| Attempt | Result and correction |
|---|---|
| exec `6890`, preflight `2026-09-15T01:24:33.7215746-04:00`, tracked diff `e1f4c9fcd83aabab6cba19b8c34e85ab227c8e7a` | Compile failed before tests: corrected the font-catalog fixture constructor, supplied the required capture process ID, and removed one unused binding. |
| exec `32428`, requested `2026-09-15T01:27:48.2345782-04:00`; Nextest `477499f3-e82c-4399-af85-ac3027828eea` | Compile `17m 35s`; 172/923 run, 171 passed, one failed, 751 cancelled, 3,439 skipped; corrected the unreferenced-skin fixture to add a skin instead of renaming a skin still referenced by menus. |
| exec `46270`, requested `2026-09-15T01:46:26.7019028-04:00`, same tracked diff | Process fully exited and produced the known clipboard fixture, but its session expired before the final output could be retained; no result is claimed and it was repeated. |
| exec at `2026-09-15T02:06:39.2435709-04:00`; Nextest `3e19f532-000d-48d8-9187-eebbf7ce8f0a` | Compile `0.99s`; 668/923 run, 667 passed, one failed, 255 cancelled, 3,439 skipped; the real legacy-menu compatibility path exposed invalid synthetic image bytes. |
| focused diagnostic exec `79442` | `cargo test` compiled in `17m 56s` and confirmed `InvalidMedia { asset: skin-image, reason: UnsupportedFormat }`; the fixture now uses a valid PNG, and skin-bundle imports validate packaged media bytes through the same boundary as menu imports. |
| exec `89841`, preflight `2026-09-15T02:26:51.1558722-04:00`, tracked diff `d06d5adbe98fc229503a8a823de584d00559ad49`; Nextest `9e3cbd3e-e93e-4a23-a7ed-d355be0bede3` | Authoritative replacement exited 0: compile `19m 58s`; 923 passed, 0 failed, 3,439 skipped; tests `64.410s`. |

The required single post-gate `cargo check` (exec `31495`) exited 0 warning-free in
`3m 18s`. Every generated root `clipboard_modifiers.json` was removed by a targeted
patch after its producing run. Final formatting, diff, process, artifact, index-lock,
stale-owner, and raw-JSON audits passed. No native interactive probe ran; preview
focus/click-through, real-device managed-audio audition, cross-process handoff/capture,
and mixed-DPI behavior remain manual/native evidence gaps for M6.

### M5 final closure remediation — sampled identity, asset reuse, and continuous edits

Status: `complete`. The action picker and Test path now consume only the latest
explicitly requested, sanitized native-preview context; opening the editor no longer
captures an implicit context, and an unsampled preview update cannot erase the last
explicit sample. Captured window identity includes HWND, PID, executable, full process
path, and class. Destructive confirmation compares that identity with the fresh window
catalog, so deletion and same-HWND process reuse fail closed without execution/history.

SkinBundle import now reuses an existing asset with identical media kind, full digest,
and byte length. A conflicting generic asset ID is remapped to a validation-safe
`asset-{kind}-{fullsha}` identity and all skin references are rewritten; packaged bytes
still pass the same media decoder boundary as menu packages. Favorites and Dashboard no
longer discard ephemeral resolved targets. Window favorites freeze the catalog's PID,
executable/path/class identity and dispatch revalidation disables the original slot on
deletion or HWND churn instead of retargeting it.

Continuous menu dwell/center, ring geometry, cell dynamic/query/media/icon-resource,
shortcut/hotstring, context-rule priority/pattern, custom-trigger, font/text/resource,
style numeric/color/offset, and media-search-path controls now use stable entity/field
edit keys. Discrete checkboxes/combos remain atomic. Ring cell counts are staged outside
the document during the gesture and create/apply a loss-safe `ResizePlan` only at edit
end. Focused tests cover explicit-sample retention and availability, exact window
identity deletion/PID churn, canonical asset reuse plus mismatched collision remap,
ephemeral favorite freezing/churn, and representative continuous-control wiring/history.
The first exact-gate attempt (exec session `86223`, requested
`2026-09-15T03:34:20.6299881-04:00`, HEAD
`a1366b6c877138a05b2301108e7dd182bc3f4f14`, tracked-diff identity
`a4743fe1823ec00b197b7fa49013a942a3aec0db`) fully exited `1` during compilation,
before tests ran. It exposed an incorrect module path in the new SkinBundle code and
move-after-use errors in the new multi-widget continuous-edit accumulator. The source
fix was limited to the correct model path and a first-event accumulator helper; the
failed tree fully exited before formatting and the replacement run.

The authoritative replacement used the exact command
`cargo nextest run -E 'test(/radial/) | test(/settings_editor/) | test(/command/) | test(/universal_action/) | test(/plugin/)'`
as the sole Cargo tree in exec session `98760`. Preflight found no Cargo/rustc/Nextest
process and no test artifact; `cargo fmt --all -- --check` and `git diff --check`
exited 0. Nextest run `6805bfad-598e-47e0-8bf6-afc0cb2ceb8d` exited 0 after an
`18m 45s` compile: 928 run, 928 passed, 0 failed, 3,439 skipped; test execution was
`34.472s`. The generated `clipboard_modifiers.json` test artifact was removed via the
approved patch path. A single subsequent `cargo check` exited 0 warning-free in
`59.31s`.

### M5 live single-HWND safety remediation

Status: `complete`. `WindowCatalog` now owns a bounded `describe_current(hwnd)`
boundary backed in production by the existing `IsWindow`-validated single-handle
descriptor query, without scheduling or enumerating the window catalog. Tests can
inject this provider independently from a deliberately stale published snapshot.
Destructive authoring confirmation and frozen radial runtime-window dispatch both
query this live boundary and compare the captured HWND/PID/executable/process
path/class `WindowTargetIdentity`; deletion and same-HWND process reuse fail closed.
Runtime window candidates capture the live descriptor while materializing, and a
failed capture leaves the visible slot unavailable and non-dispatchable rather than
creating an identity-less fail-open binding.

The authoritative exact replacement started after a clean preflight at
`2026-09-15T04:13:12.6262628-04:00` from HEAD
`a1366b6c877138a05b2301108e7dd182bc3f4f14` and ran as the sole Cargo tree in exec
session `82037`. Nextest UUID `47146892-2b1c-442e-92f5-d6735b6132dd` exited 0 after
a `17m 47s` compile: 928 run, 928 passed, 0 failed, 3,439 skipped; tests took
`22.561s`. No generated clipboard artifact remained. The single subsequent
`cargo check` exited 0 warning-free in `50.74s`.

Final review required the freeze layer itself, rather than only current producers, to
mark every identity-less runtime window unavailable. That source-only invariant and a
direct regression test were added after the recorded passing run, so the prior run is
retained as evidence but is not the final source-identical acceptance gate.

The final source-identical replacement started after clean format/diff/process/artifact
preflight at `2026-09-15T04:34:57.0401793-04:00` as the sole Cargo tree in exec
session `96623`. Nextest UUID `45172f01-6847-46fd-82ff-35d059e002f7` exited 0 after
a `34m 31s` compile: 929 run, 929 passed, 0 failed, 3,439 skipped; tests took
`109.376s`. The added test is the direct identity-less runtime-window freeze
regression. The subsequent `cargo check` exited 0 warning-free in `2m 35s`.

### M5 contextual dispatch closure

Status: `complete`. Invocation-frozen contextual bindings now have a dedicated
typed representation carrying selector, action ID, and captured
`WindowTargetIdentity`; persisted bindings cannot accidentally manufacture that
state. Normal cells, alternate clicks, and center/background primary and secondary
mappings all use the same freezing helper. Every contextual dispatch performs a fresh
single-HWND descriptor comparison before resolution, and the controller refuses to
reconstruct contextual bindings from an unprepared fallback. Authoring Test Action
performs the same live check before immediate execution regardless of safety or
confirmation preference, then retains and rechecks the exact identity if confirmation
is required. Tests cover all radial surface categories, stale HWND reuse, immediate
non-destructive rejection, confirmation-disabled destructive rejection, and
confirmation-time churn.

The first retained replacement result, Nextest
`89833420-9988-4c3b-b019-bb1ffb687f66`, compiled in `1.05s` and exited 1 after
572/932 tests: 571 passed, one failed, 360 cancelled, and 3,439 skipped. The failure
was a controller handoff test that still used an unprepared contextual binding; its
fixture was corrected to use the persisted action appropriate to the handoff behavior
under test. No production fallback was restored. An earlier compile-complete session
expired without an authoritative terminal result and is not claimed as evidence.

After clean format/diff/process/artifact preflight, the authoritative exact command
`cargo nextest run -E 'test(/radial/) | test(/settings_editor/) | test(/command/) | test(/universal_action/) | test(/plugin/)'`
ran as the sole Cargo tree from HEAD
`a1366b6c877138a05b2301108e7dd182bc3f4f14`. Nextest
`a9eebc6c-d54c-4adf-ae8e-4e01418cbffe` exited 0: compile `16m 30s`; 932 run,
932 passed, 0 failed, 3,439 skipped; tests `24.182s`. The required subsequent
`cargo check` exited 0 warning-free in `40.39s`. Generated
`clipboard_modifiers.json` fixtures were removed by targeted patches after each run.

## Verification and job records

Read-only tooling inspection: `cargo-nextest 0.9.135`; `cargo nextest run --help`
confirmed expression filters, target selection, captured output, status levels, and
no-capture serialization. At initial plan approval no build or test job had run.
Before every gate, record
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

M4 substeps 1-5 static verification: direct `rustfmt --edition 2024` over the
touched Rust sources and `git diff --check` exited 0. Substep 2 adds a pure
six-layer effective-style compiler with per-field provenance plus styled
layout/pagination/scene inputs. Substep 3 extends the retained synchronous keyboard
service with bounded, provenance-filtered menu-local/global item shortcuts and
hotstrings, routes all five gestures through the prepared action handoff, and applies
per-menu topmost/explicit activation while retaining no-activate as the safe default.
Substep 4 adds preparation-only asset and font/layout services: managed paths are
canonicalized beneath `radial_assets`; configured filename search roots are explicit;
external/search/resource paths are marked nonportable; and Windows Media fallback is
WAV-only and opt-in. PNG/JPEG/BMP/ICO/TIFF decode as bounded static images (the
reviewed image 0.24 ICO codec selects its best directory entry), while GIF animation
is bounded by source bytes, dimensions, decoded bytes, frames, and total duration.
EXE/DLL/CPL icon access uses `LOAD_LIBRARY_AS_DATAFILE |
LOAD_LIBRARY_AS_IMAGE_RESOURCE`, converts persisted one-based indices to the native
zero-based index, and owns module/icon/GDI cleanup. Deterministic caches include
asset identity/version, effective-style fingerprint, DPI, logical size, and quality;
unavailable results are negatively cached. Font discovery is performed once outside
paint and bounded text-layout snapshots preserve Unicode graphemes with script-aware
fallback and truncation diagnostics. Paint/hit-test inputs are immutable prepared
snapshots and perform no filesystem access or decoding.

The existing `image = 0.24` dependency remains pinned with default features off;
only its narrowly reviewed built-in `ico`, `gif`, and `tiff` codec features were
enabled alongside PNG/JPEG/BMP. Their already-locked transitive crates remain under
the existing permissive Rust ecosystem licenses; no decoder executes imported code.
The data-only Windows resource path is platform-gated and no reference asset was
copied or redistributed. Per the substep boundary, no Cargo, Nextest, native
interactive probe, import/package work, or M5 editor work was run.

Substep 5 adds the shared straight-alpha software compositor used by both native
presentation and the future preview boundary. Ordered scenes accept immutable
prepared background/rim/item/glow/icon/indicator/text/shadow/tooltip inputs; custom
media omission or failure retains the plain native fallback, and unsafe frame bounds
fail closed. The premultiplied-BGRA conversion used by Screen Draw was moved to the
shared platform pixel boundary and is now also used by radial
`UpdateLayeredWindow(ULW_ALPHA)` presentation. Navigation and hover use an in-place
`Present` command that retains the HWND, capture/suppression ownership, session, and
Closed semantics while atomically replacing the scene, dimensions, and shaped input
region. Animated prepared GIF frames use one window-owned one-shot timer correlated
to the current state and are cancelled on replacement/destruction.

The application sound API now feeds one lazy retained output worker through a
bounded queue instead of spawning one stream/thread per sound. Radial audio contracts
carry session/generation tags, exact-once lifecycle cues, selection debounce, bounded
stale-generation tombstones, and prepared WAV bytes; queue saturation drops work
rather than allocating without bound. Static layer/frame cache keys include scene
generation/content, DPI, and animation time, so hover performs no media decode or
filesystem probing. Direct formatting and diff checks covered this substep source
boundary; M4 remained `in_progress` there pending the later focused milestone gate.

Substep 6 adds a side-effect-free legacy import preview boundary. `ImportPreview` owns
deterministic planned menu/skin IDs, discovered-file roles, source/evidence and
line-level provenance, typed mappings, collision/missing-asset/portability warnings,
and a complete compatibility-registry coverage snapshot. Preview accepts caller-owned
bytes and paths. Review remediation additionally compiles that immutable preview into
a typed `RadialDocument` and content-addressed `ImportPlan`; the callable
`RadialStore::apply_legacy_import` boundary performs all writes through the same
revision-checked package transaction and explicit create-new/replace-backup policy.
Radify JSON keeps
default and per-skin objects independently and retains JSON `false`, numeric zero,
empty string, and null rather than defaulting them away. The supplied Radify archive
contains no generated `Preferences.json`, `Settings.json`, or sound data, so tests of
those shapes are explicitly tagged `SyntheticFixture`; preview of supplied evidence
also reports those absences rather than implying real samples.

The RM4 parser recognizes only registered assignment fields, literal text/finite
numbers/booleans/clear, and the narrow `fac+N`, `fac-N`, `add+N`, and `add-N` forms.
`Close`, `CloseMenu`, and `Drag` become typed controls. Unknown fields, unsupported
expressions, matrices/ARGB transforms, callbacks, and non-assignment script content
receive exact file/line/field diagnostics and are never evaluated. Familiar skin
media discovery is ASCII-case-insensitive for `ItemBack.png`, `ItemGlow.png`,
`MenuBack.png`, `MenuOuterRim.png`, `CenterImage.png`, and
`SubmenuIndicator.png`. `ItemBack.png` is required only for a named imported legacy
skin, never for a native vector skin.

Read-only archive inspection covered all five supplied RM4 definitions:
`Anectra's target` (869 bytes), `Cell` (834), `Merlock's device` (826),
`Metal plate` (913), and `Orb` (1,304). Representative media inspected in place were
`Anectra's target/ItemBack.png` (5,454 bytes, 76×76),
`Cell/MenuOuterRim.png` (12,266, 600×600), `Merlock's device/MenuBack.png`
(116,365, 305×306), `Metal plate/ItemGlow.png` (14,636, 122×122), and
`Orb/MenuFore.png` (11,965, 543×543); each had a valid PNG signature/IHDR. No
archive member was executed, extracted, copied,
or added to product resources. Every compatibility registry field is either mapped
through its registered native/translated policy or represented by an explicit
incompatible/not-applicable diagnostic policy.

Substep 7 adds versioned portable `.mlradial` packages and an equivalent plain-folder
entry model. A manifest covers the complete requested menu/submenu closure, referenced
skins and managed media, root IDs, notices, byte lengths, and SHA-256 checksums.
Export/import planning is side-effect-free and strips local media search roots and
free-form metadata rather than packaging paths or potential secrets; runtime sessions,
dynamic snapshots, pointer/drag state, HWNDs, clipboard values, and mutable provider
indices are not document/package fields. External/search-path/icon-resource media is
rejected as nonportable instead of being silently embedded.

The archive codec intentionally emits and admits only deterministic UTF-8, unencrypted
ZIP stored entries. It does not enable a general compression or encryption surface.
Admission cross-checks local and central headers and rejects unsupported compression,
encryption, links/reparse metadata, malformed/truncated records, CRC/checksum mismatch,
absolute/UNC/drive/device/ADS/traversal paths, backslashes, Windows-illegal characters,
trailing dots/spaces, DOS device names, excessive depth/path/file/entry/total compressed
or expanded sizes, and case-insensitive collisions. Expansion is copied through a
per-entry bounded reader. Packaged images and WAV files pass the same bounded decoder
used by runtime preparation, including decoded dimensions/bytes/GIF frames/duration.
Plain-folder adapters require caller-provided no-follow entry metadata and reject
symlink/reparse entries before forming the immutable byte map.

Import deterministically remaps colliding menu, skin, ring, cell, asset, shortcut,
hotstring, context-rule, and trigger IDs, then rewrites submenu, skin, trigger/rule,
and managed-asset references before revalidation. `RadialStore` owns the only apply
transaction: under its mutation lock it rechecks revision, expected radial bytes, and
previewed asset digests; replacement additionally requires a distinct existing backup
whose bytes and receipt digest match the current source. Managed assets are written
first under content-addressed immutable names using create-new semantics. Reused files
must match and are never transaction-owned. Atomic `radial.json` publication is last;
pre-commit failure/cancellation removes only files created by that transaction and
never shared assets. Typed menu/skin/asset usage queries prevent dependency deletion.

The existing `hex` dependency remains the encoding boundary and direct `sha2 = 0.10`
was added for its RustCrypto SHA-256 implementation (MIT/Apache-2.0, no native code or
default executable surface). No general ZIP dependency was added: the required stored
format is implemented as a deliberately narrow codec so unsupported methods fail
closed rather than compiling unused compression/encryption backends. No Cargo or
Nextest command was run in this substep, so lockfile resolution and executable tests
remained for the later serialized M4 gate. Direct rustfmt and diff checks were the
substep's static boundary; M4 remained `in_progress` there.

Substep 8 makes radial recovery an explicit atomic group: `RadialDocument` and
`RadialAssets` are selected, fingerprinted, materialized, installed, and rolled back
together. Current and legacy single-member restore descriptors are rejected. The
generic recovery confirmation names the complete group, while canonical document
reset installs `RadialDocument::starter()` and deliberately leaves the asset tree
untouched; asset deletion remains an explicit reference-aware operation. Catalog
privacy is `Sensitive` for the document and `UserContent` for managed assets. Existing
MkMacro grouping semantics and serialized identity remain unchanged.

The process main loop remains the sole live radial persistence owner. One recursive
`notify` watcher rooted at the existing application-data directory filters only
`radial.json` and `radial_assets`, coalesces callback bursts, and wakes the existing
event loop. External document bytes are decoded through the shared version migration
and validator before any publication. A real change closes the active radial tree,
invalidates GUI leases/prepared resource generations, publishes one monotonic store
revision, refreshes MkMacro/global shortcut reservations, and transfers invocation
configuration through the acknowledged launcher route. Asset changes invalidate the
same invocation-local style/media/font/static presentation boundary. Malformed,
empty, missing, or newer-schema candidates leave both the last-valid snapshot and
original bytes untouched and surface a GUI recovery diagnostic. Atomic self-save
echoes and semantically identical external rewrites are ignored by content identity.

Focused regressions cover radial group serialization and membership, catalog privacy,
dialog/data-service routing, incomplete-group rejection, second-member rollback,
starter reset with retained assets, valid external publication exactly once,
malformed/newer retention, self-save suppression, watch filtering/coalescing, and
controller resource-generation invalidation. Static review found no additional
persistence owner or polling daemon. No Cargo or Nextest command was run during the
substep itself; the combined M4 gate subsequently passed as recorded below.

M4 combined focused gate and root-only replacements:

| Attempt | Start/session/source | Result |
|---|---|---|
| 1 | UUID `7a82728b-1f07-440d-bb04-e1c717e90c2b`; `2026-09-14T01:05:55.0737603-04:00`; tracked diff `c5f52461d74b49440aea0706671856f0656c4c5f` | exit `1` after about 27.5s before compilation: crates.io Schannel `SEC_E_NO_CREDENTIALS` resolving `ab_glyph`; no tests |
| 2 | escalated network replacement; exec session `22115`; same source | exit `101` after dependency resolution and about 100s compilation: five first-party compile errors in new M4 sources/tests (missing test import, `Arc<str>` conversion, generic clone bound, redundant mutable reborrow, missing fixture argument) |
| 3 | UUID `63650dfd-704c-435b-964f-c513c0287d30`; `2026-09-14T01:10:12.6793331-04:00`; session `22779`; tracked diff `5a1ff165453460cadbe2f7f51c781b85267ac04c` | exit `100` after compile `11m 13s`; PTY did not retain the failing-test summary |
| 4 | UUID `7973e83b-e1b7-46a4-a6c1-0b3823ba80d4`; `2026-09-14T01:23:09.5906102-04:00`; session `52999`; same source; Nextest `4bc1d8d3-a085-46d0-863e-7395a7ea3287` | exit `100`; compile `15m 52s`; 174 run, 172 passed, 2 fixture failures, 3,816 skipped |
| 5 | UUID `d4f0accd-3b9e-4f9d-b1e7-580bb0504351`; `2026-09-14T01:41:19.9861472-04:00`; session `33573`; tracked diff `3baea4091c03ebcd13b578406b1a8073d588e2b7`; Nextest `100e082e-daf8-4a0b-a27e-02ee3c03e99e` | exit `100`; compile `16m 28s`; 163 run, 162 passed, one legacy-v1 fixture failure, 3,816 skipped |
| 6 | UUID `e9a64c88-f94e-4ec9-9fc3-23b0378e2664`; `2026-09-14T02:00:12.2748294-04:00`; session `6897`; tracked diff `3baea4091c03ebcd13b578406b1a8073d588e2b7`; Nextest `bd0a2c2e-69bd-42a3-abb4-3891283e1871` | exit `100`; compile `16m 08s`; 239 run, 237 passed, 2 controller fixture/lifecycle failures, 3,816 skipped |
| 7 | UUID `5704adfe-cc84-4dfd-8444-c488e5e8939c`; `2026-09-14T02:19:18.0001245-04:00`; session `89804`; tracked diff `b01a0b190981021f9e35c7501b5531967c98dbc1`; Nextest `3fc0fb1d-2e1f-423b-9611-1edb1679ea8f` | exit `100`; compile `16m 48s`; 238 run, 237 passed, one shortcut-handoff assertion failure, 3,816 skipped |
| 8 | UUID `7fafe7d8-cdf3-4bdb-bc89-8fc56f12b8d1`; `2026-09-14T02:39:47.1579815-04:00`; session `17348`; tracked diff `953c3a9204a29a12c5c501191777db7154ea0348`; Nextest `fa8fabc2-390e-4ec2-b085-16d30183ba88` | exit `100`; compile `17m 30s`; 239 run, 238 passed, one shortcut-handoff release failure, 3,816 skipped |
| 9 | UUID `867520f6-e59e-43ed-870a-cf6e34cc02ee`; `2026-09-14T02:59:17.5254245-04:00`; session `17780`; tracked diff `2689fe9fa480c9aa6713624d8e85221446edd033`; Nextest `cd56dc29-4d74-40c7-b43a-2f540dd89982` | exit `100`; compile `15m 53s`; 240 run, 239 passed, one incompatible fixture-policy failure, 3,816 skipped |
| 10 | UUID `be067d2a-6882-4285-a149-0a37e1256d3f`; `2026-09-14T03:18:56.0749693-04:00`; session `76524`; tracked diff `751eef6493d1b369ecfa7e5ed354b0e2179e229a`; Nextest `57ffd46c-d28d-4f0c-8632-389b4a3bc517` | exit `100`; compile `17m 26s`; 312 run, 311 passed, one invalid package remapping fixture, 3,816 skipped |
| 11 | UUID `29c235e8-356f-49de-87ce-1226fadd1435`; `2026-09-14T03:38:52.4878510-04:00`; session `95606`; tracked diff `751eef6493d1b369ecfa7e5ed354b0e2179e229a`; Nextest `0924dc45-741f-4e2a-9f79-7821cc7837d0` | exit `0`; compile `17m 03s`; 408 passed, 0 failed, 3,816 skipped; test execution `8.347s` |

Every Cargo/Nextest tree fully exited before the next root-only correction and
replacement. The successful command was exactly
`cargo nextest run -E 'test(/radial/) | test(/launcher_invocation/) |
test(/persistence::(catalog|recovery|data_service|backup)/) |
test(/gui::(radial_actions|watch|data_recovery_dialog|actions)/) |
test(/settings::/) | test(/sound::/) | test(/platform::pixels/)'` from
`G:\Repos\rust\Multi_Launcher`, using the default Nextest profile and the existing
incremental target. Output was retained in the unified exec transcript rather than a
separate log file; exec session identifiers are recorded above, while OS process PIDs
were not separately sampled. The successful gate retained the production fix that keeps an
item shortcut's owned release separate from the original menu-opening chord; the
remaining corrections were compile mechanics or invalid test fixtures. No native
interactive probe was run, so the evidence gap below remains. Because several new M4
modules were not yet tracked, the post-gate full `src` plus `Cargo.toml`/`Cargo.lock`
content identity was also recorded as
`b5b75d1d1314c7b06d8a83971d69643536d19100` across 545 files; no Rust or Cargo source
changed between the successful gate and that identity capture.

M4 independent-review remediation is now `in_progress`. The remediation connects
the previously pure style/media/font/audio contracts to the production controller:
every open prepares immutable media, real glyph coverage, tooltip layouts, and WAV
cues before the native Open command; retained navigation/hover presentation consumes
those resources without paint-time filesystem or decode work. Serialized opacity,
scaling/alignment, image glow, item-background visibility, text shadow/quality,
tooltip, menu shadow, and hit-zone settings now alter scene/layout/input behavior.
The rectangular layered visual surface retains the complete visual extent while
WM_NCHITTEST independently returns transparent outside owned input geometry.

Security remediation hashes managed bytes against their declared SHA-256 before
cache lookup (including same-length changes), parses RIFF/WAVE chunks and bounds
format/channel/rate/data/duration, rejects pre-existing radial asset roots that are
files/symlinks/junctions/reparse points, and requires stored ZIP local and central
CRC/sizes/names/offset ranges to agree without gaps, aliases, overlaps, optional
records, or unreferenced bytes. Recovery treats an absent `radial_assets` member in
an otherwise complete fresh-install snapshot as a typed empty directory while still
installing/rolling back the radial group atomically. Local hotstring history is
discarded when navigation relinquishes ownership and ordinary external typing is not
retained when no eligible local/global binding exists. The replacement combined gate
has not yet run; M4 remains `in_progress` pending that source-identical result.

M4 independent-review replacement gate launch record: UUID
`e6077b01-9f5e-42f9-b7cb-5a78d65f7d21`, requested start
`2026-09-14T04:48:21.0446356-04:00`, cwd
`G:\Repos\rust\Multi_Launcher`, pre-ledger tracked-diff identity
`2c4e83d4fc9ec3e03c742d0bd947598af742c547`. The preflight direct rustfmt and
`git diff --check` exited 0, and `Get-Process cargo,cargo-nextest,rustc` returned
no process. Source SHA-256 values: controller `E76414DE...`, assets `72EC3BA8...`,
package `8032EE1F...`, import `CD399783...`, font cache `9AA57AA6...`, native
`5DF71A41...`, recovery `B6CCFEC3...`. The unified exec transcript is the retained
log; session/PIDs/result are recorded after process exit.

The first remediation replacement used exec session `44121` and exited `1`
after about 151 seconds (`cargo test --no-run` exit `101`) with two root compile
errors: an unqualified `BTreeMap::new()` in the new legacy store adapter and the
Windows `ScreenToClient` symbol absent from the native import namespace. No tests
ran. The permitted replacement changes only those two compile roots.

The source-changed replacement launch UUID is
`45d57822-39f6-41d4-992d-7a65efe5ee80`, requested start
`2026-09-14T04:52:34.8446071-04:00`, pre-ledger tracked-diff identity
`24aa6f23afc4dc2a2f8a0f38762d71733619bb9f`; store SHA-256
`BC53DB64...`, native SHA-256 `F26C2524...`. Direct rustfmt/diff checks pass and
the process preflight again found no Cargo tree.

That replacement compiled successfully in `16m 47s` but exited `1`; its PTY
lost the test identity. A read-only direct invocation of the already-built unit-test
binary isolated the sole failure to
`persistence::recovery::tests::radial_group_restore_accepts_snapshot_with_missing_assets_as_empty_member`:
the virtual-empty fingerprint included a framed empty byte segment while a real empty
directory fingerprint correctly contains only the directory discriminator. Assets,
controller, and the other recovery tests passed in the same diagnostic binary. The
root correction centralizes the exact empty-directory fingerprint for virtual group
members and canonical empty-directory reset candidates.

The second root-only replacement UUID is
`0590a6ee-bed8-4f0f-8e1a-e20cdbffdd9b`, requested start
`2026-09-14T05:12:37.2070342-04:00`, pre-ledger tracked-diff identity
`89a93a32196378cd52cde2169dee1df8d273cb13`, recovery SHA-256
`D48B2B30...`; formatting/diff checks pass and no Cargo tree is active.

The second root-only replacement ran in exec session `93989` and exited `0`
after `14m 52s` compilation/execution using the exact combined focused expression
recorded above. The PTY retained Cargo's successful exit but not Nextest's per-test
count/run identifier, so neither is invented here; process exit is the authoritative
gate result. Post-gate `cargo fmt --all -- --check` and `git diff --check` exited 0,
the Cargo/Nextest/Rustc process audit was empty, and the only generated repository
artifact (`clipboard_modifiers.json`, created by an existing test) was removed.
The post-gate accepted-field audit found one remaining semantic gap: shape quality was
present in the immutable layout contract but was not consumed by rasterization. M4
returned to `in_progress`; the compositor now uses deterministic 1x1/2x2/4x4 edge
coverage for Fast/Balanced/HighQuality circles and wedges, with a focused pixel-edge
regression. This is the only source change after that successful gate and requires a
source-identical replacement. The existing native cross-process/interactive evidence
gap remains explicitly unverified.

Final accepted-field replacement launch: UUID
`834ddbeb-41c3-4eb0-ac1c-b16af3ea258b`, requested start
`2026-09-14T05:33:20.3158599-04:00`, pre-ledger tracked-diff identity
`fd9a0a27d0cf8fc093ea0859534c49b72e45eedc`, render SHA-256 `3A6E4743...`,
compositor SHA-256 `87B9C61E...`; direct rustfmt/diff and no-process preflight pass.

The final accepted-field replacement ran as the sole Cargo tree in exec session
`15086` and exited `0` after `18m 31s` with the exact combined focused expression.
As with the prior PTY run, Nextest's per-test count/run identifier was not retained,
so the ledger records no invented values. Post-gate `cargo fmt --all -- --check` and
`git diff --check` exited 0; the process audit was empty and the existing test-created
`clipboard_modifiers.json` artifact was removed again. Product source is unchanged
after that successful source-identical gate. M4 is complete.

M4 closure remediation then addressed the final integration findings. Legacy Radify
and RM4 mappings now mutate the typed candidate document and transactional apply plan
(including explicit false/zero, selected-skin precedence, scalar/style/geometry/media,
quality, and independently typed primary/secondary center/background controls). RM4
`IconTrans` compiles into a dedicated icon-opacity field which participates in style
precedence, validation, immutable layout, and render alpha. Font preparation now
discovers bounded system-font paths once, caches loaded `FontArc` values, selects real
requested families, performs per-glyph CJK/emoji fallback, and diagnoses missing glyphs
instead of drawing tofu. Radial audio replaces the complete child effective sound set,
emits submenu-close, and atomically retires stale scoped cues while preserving one
terminal close cue. Native owned-region re-entry restores local item inputs; exterior
movement does not. Resource cache variants use a stable effective-style/media
fingerprint rather than layout generation. Missing/corrupt preparation diagnostics flow
through the normal GUI error/toast-log path even with debug logging disabled and preserve
launcher query/selection. The recovery UI accepts the backend's virtual-empty radial
asset member for a fresh-install snapshot without weakening grouped atomicity.

Closure combined-gate attempts used the exact command/expression and cwd recorded above,
with the unified exec transcript as the retained log. Preflight direct rustfmt,
`git diff --check`, and `Get-Process cargo,cargo-nextest,rustc` were clean before each
attempt; no Cargo trees overlapped.

| Closure attempt | Session / source | Result |
|---|---|---|
| 1 | exec `60555`; requested around `2026-09-14T06:30-04:00`; non-ledger tracked diff `8b2ba907ead28c580e134219212d426d5baf6d77` | exit `1` after the compile tree fully exited: four new-test type errors (`ValidationErrors.0` and optional GUI selection) plus one unused generation parameter; no tests ran |
| 2 | exec `83077`; requested `2026-09-14T06:35:59-04:00`; tracked diff `c01ad1510aa27379c1f3a3e25ee37e1b6fc7cb11`; Nextest `ec0542d5-9f6e-4558-8d97-a77c6c117105` | exit `1`; compile `16m 02s`; 259 run, 258 passed, one stale synthetic-font expectation failed, 176 cancelled, 3,816 skipped |
| 3 | exec `58257`; requested `2026-09-14T06:53:20-04:00`; same production source, test-only correction; Nextest `7cfeedc5-ba91-4114-a967-14e9d577bcbd` | exit `0`; compile `15m 19s`; 435 passed, 0 failed, 3,816 skipped; tests `11.461s` |

Final closure source SHA-256 identities include import
`3E2CFEDEFBDEB8565CE3269BFCBA8813318D23B4FE5826241232EFE50604BBB8`, font cache
`BF0E85698056BC862C1BBE2303171A3187871DE9357BB8EFEA66752A0F2F6EAD`, audio
`E4A53FC6B74A9F4A0BF7A636EFF383270F838EC841FE339E155B478C1249A5DF`, controller
`B045779C1BA88F234CF597DC344C17D9808446F02C1EF63EF76247DDFCF51723`, model
`80A09404CCADC3F190F5C834E469F7FDE29F49A360BBD9D0808093F8539A2F20`, and sound
`6BA622A3955092EC5EBB0202A3CBB0C94A402E83436CC344845D36C9A7638E6A`.
The only generated repository artifact, `clipboard_modifiers.json`, was removed after
the gate. Post-gate `cargo fmt --all -- --check` and `git diff --check` exited 0,
and the Cargo/Nextest/Rustc process audit was empty. No native interactive probe was
run, so the documented live evidence gap remains.

## Known evidence gaps

Terminal-token/input-override replacement UUID
`0d272b43-92dc-4c7d-a2f2-83aafebddec0`, requested start
`2026-09-14T10:30:38.9216998-04:00`, pre-ledger diff identity
`89de08f47091226046c7bf71c44fdb1ccfc74221`, import SHA-256
`95E5E62EFBC9A325AB48686CC744511C858490D5150F268985D23C7F1556DDD2`, sound
SHA-256 `52C8E0196E530FCB1501AB25C6AFA32A0CAAF704B02DFB98891A6A97916971DC`.
Terminal reservations are now per-scope reference counts shared across queued Finish
commands and active terminal sinks. Imported shortcuts/hotstrings upsert stable gesture
slots so selected-skin mappings override defaults. Submenu graph diagnostics have one
typed owner and are emitted exactly once per field.
The source-identical combined M4 gate ran as the sole Cargo tree in exec session
`56619` and exited `0` after `18m 42s` using the required expression. The PTY retained
the Cargo completion and exit status but not a Nextest run identifier or test count, so
neither is invented. Post-gate `cargo fmt --all -- --check` and `git diff --check`
exited `0`; the Cargo/Nextest/Rustc process audit was empty and the sole generated
`clipboard_modifiers.json` artifact was removed. Import and sound hashes remained
source-identical to the recorded gate inputs. M4 is complete; native interactive
evidence remains explicitly unverified.

Transactional-validation/audio-reservation replacement UUID
`ad5b767d-bdd3-425c-bf66-d3d6f0b8b7df`, requested start
`2026-09-14T09:44:26.5537242-04:00`, pre-ledger tracked-diff identity
`bae9acbc4732e5b20da493265951d76399e70e09`, import SHA-256
`9D2687BCF18B845334A1169C361D61A8512C1CF04798100A9F5AD8168BBD9D8D`, sound
SHA-256 `251287596DD9B6A98C5DBDE33BD423E23A06B5F982CCC72F2A33BBDD10DBB5C3`.
Each legacy mapping now mutates a cloned candidate and commits only after the complete
runtime document validator accepts it. Import provenance carries an explicit dialect
for Radify selected-skin versus RM4 definition-sibling media precedence. Audio finish
reservations are shared mailbox/worker state covering queued and active terminal
scopes, with release on completion, stop, decode/output failure, and backend absence.
The first attempt exited `1` in exec session `50013` before tests: the registry test's
collision-preserving media fixture needed an explicit
`BTreeMap<String, Vec<MediaReference>>` annotation. No production compiler error was
reported. Root-only replacement UUID `3ccc473c-7b97-41fd-ba37-dd03244d37f3`, requested
start `2026-09-14T09:49:15.3399137-04:00`, pre-ledger diff identity
`162989f5fc4ca4c1f1d9779d1f82870b3df0e42b`, import SHA-256
`C446BF887EB275D38E6A78F2E4AB789159CE867F794AC1B9D8230D259506817B`.
That replacement ran in exec session `76745` and exited `1` after `15m 56s`.
Direct compiled-binary runs passed all 7 sound tests and 21/22 import tests; the only
failure exposed the existing validator's contradiction with the compatibility promise
to preserve explicit `ItemSize = 0`. The root correction changes that style bound from
`1.0..=2048.0` to `0.0..=2048.0`; negative/out-of-range values remain rejected.
Replacement UUID `6fb419b9-19bb-4a41-98fa-0fcd993b4bf9`, requested start
`2026-09-14T10:07:23.1004978-04:00`, pre-ledger diff identity
`ae57d2a5f7992eabc99264752114b9a42b612667`, validation SHA-256
`B6E70959CEEE7F9B4D1E03CF0FDEE0BED0349F9DA652D586A4CE7C8DB83DD007`.
The final replacement ran as the sole Cargo tree in exec session `76861` and exited
`0` after `14m 43s` compilation with the exact combined M4 expression. The PTY did
not retain Nextest's count/run identifier, so neither is invented. Post-gate
`cargo fmt --all -- --check` and `git diff --check` exited 0, the process audit was
empty, and the generated `clipboard_modifiers.json` was removed. Import, sound, and
validation hashes remained source-identical to the successful gate. M4 is complete;
native interactive evidence remains unverified.

Final-invariant replacement UUID `080bcdde-21aa-42b7-9e6a-353a084a2192`, requested
start `2026-09-14T08:34:06.4163274-04:00`, pre-ledger tracked-diff identity
`04e978978044b05e4d9af291d73cd7b278b9c1c1`, import SHA-256
`F1727390DA3CCE0A1BA302901DA66656C954608116089B8605866597ADE3F05A`, sound
SHA-256 `76AE0E1798E84B8CCD6792A13842C78AF4C37A7F6CFD777B86CD52D77CBE1459`,
controller SHA-256 `F1072AF0D3D63E8C58F5C398679840063609F947F9FA93F09BA236209D13D4E2`.
The audio mailbox has separate strict nonterminal, terminal-scope, and absolute bounds;
accepted terminal work is never evicted and close cues have a bounded sink reserve.
Import application now records typed Changed/AlreadyEqual/Invalid/MissingAsset/
ExplicitlyDiagnosed outcomes. Normalized path storage is a collision-preserving
multimap and source skins match the immediate media parent, not arbitrary components.
That attempt ran in exec session `61415` and exited `1` after `18m 52s`: compilation
succeeded and one new registry semantic test treated a valid idempotent false value as
having no outcome. Direct execution of the completed test binary identified
`every_accepted_field_has_a_semantic_valid_value_outcome`; the mailbox-bound and
case-fold collision regressions passed directly. The root-only test correction now
asserts the typed `AlreadyEqual` outcome explicitly instead of requiring mutation.
Replacement UUID `cea11807-c056-4f58-8d0d-3a01c4cbadf7`, requested start
`2026-09-14T08:55:24.6494090-04:00`, pre-ledger diff identity
`b715121c46840bd6c1ebc0eb6cfcaebc8b6afed0`, import SHA-256
`14BD4F08627077BB7CBF52C505226C21A8307C8E35F380D60548E29962AF845E`.
That replacement ran in exec session `79281` and exited `1` after `18m 23s`.
Direct execution of the compiled `sound::tests` suite passed all 6 tests and
`radial::import::tests` passed 19/20; the isolated failure showed that an explicit
source skin structurally rejected another skin but then fell through to a globally
unique basename. The root-only correction makes explicit source selection
authoritative. Final replacement UUID `835faa9b-9752-4bff-b58a-f90ce92ff6d6`,
requested start `2026-09-14T09:15:45.7809781-04:00`, pre-ledger diff identity
`dddd2db7c2746a18d41946e41c9d9de5c5dcd641`, import SHA-256
`AFE10B6C407374A0E883CFCBD0E375862C296664D61BFAB55E0DB1F16910CFCE`.
The final replacement ran as the sole Cargo tree in exec session `31081` and exited
`0` after `16m 54s` compilation with the exact combined M4 expression. The PTY did
not retain Nextest's count/run identifier, so neither is invented. Post-gate
`cargo fmt --all -- --check` and `git diff --check` exited 0, the process audit was
empty, and the generated `clipboard_modifiers.json` was removed. The import hash is
source-identical to the successful gate. M4 is complete; native interactive evidence
remains unverified.

Semantic-closure replacement UUID `def6c321-ef5e-4f06-b8b0-054e7beaf424`,
requested start `2026-09-14T08:06:07.6925566-04:00`, pre-ledger tracked-diff
identity `d9e786db1279ff49841c12baf0ac70db61e5f7de`, import SHA-256
`73AABE032CAC8EA94DEBE45AB98FBCCB90831B8F3A77E166A52CEE05E5A3B7D9`, sound
SHA-256 `F56FD5B0AEFB95658D879119EC885835AA8F84895623B2F838AC0A12FCEDA9A2`,
validation SHA-256 `3976AF8FE572DE190CC0CB93CBA99687F82F2AF78A68645F193DE9F83496FBA5`.
The bounded audio mailbox now reserves `MAX_ACTIVE_SINKS` terminal slots beyond
ordinary play capacity, coalesces terminals only for the same scope, never evicts an
accepted terminal, and reports impossible reserve exhaustion. Import application
records a field-local outcome for each selected accepted value, recursively normalizes
archive provenance/source-skin components, refuses ambiguous basename fallback, and
uses validated full-digest media-kind-qualified imported asset IDs.
The replacement ran as the sole Cargo tree in exec session `65783` and exited `0`
after `16m 33s` compilation with the exact combined M4 expression. The PTY did not
retain Nextest's per-test count/run identifier, so neither is invented. Post-gate
`cargo fmt --all -- --check` and `git diff --check` exited 0, the Cargo/Nextest/Rustc
process audit was empty, and the known generated `clipboard_modifiers.json` was
removed. Import, sound, and validation hashes remained source-identical to the
successful gate. M4 is complete; native interactive evidence remains unverified.

Final bounded import/audio remediation is source-complete pending the combined gate.
Accepted legacy fields now have an explicit apply-or-diagnose policy; typed alternate
controls and all local shortcut/hotstring gesture variants are retained, explicit
false tooltip/right-close values survive, and submenu graph settings produce a visible
field diagnostic rather than a dangling ID. Radify selection carries a source skin
identity independently of the renamed destination, and media lookup uses definition/
skin path provenance before basename fallback. Managed bytes deduplicate by full
SHA-256 plus media type. Audio terminal finish commands evict/coalesce nonterminal
work and cannot be rejected by a saturated play queue.

Planned replacement UUID `e18f7bad-a9e8-4ed6-a811-7bff6b90f63a`, requested start
`2026-09-14T07:29:11.7887520-04:00`, pre-ledger tracked-diff identity
`2c32b16c1c748d1d3e2fa1496776396143ff01c9`, import SHA-256
`82D1889D46F62D88C16F595C6763B4E86C9A119F7EB8E221AEA9D7762C9B55E1`, sound
SHA-256 `7DBD3BDA3CF8DB8492039FF59EEF62A82B24E52BF15D13FBB5703E7F1912D442`.
The sole replacement ran in exec session `34509` and exited `0` after `19m 03s`
of compilation using the exact combined M4 expression recorded above. The PTY did
not retain Nextest's per-test count/run identifier, so neither is invented; the
successful process exit is the authoritative result. Post-gate
`cargo fmt --all -- --check` and `git diff --check` exited 0, the process audit was
empty, and the known test-generated `clipboard_modifiers.json` artifact was removed.
The import and sound source hashes above are source-identical to the successful gate.
M4 is complete; native interactive evidence remains unverified.

- No `Preferences.json`, Radify sounds, or Radify `Settings.json` is supplied;
  source-defined defaults and generated shape are evidence, and fixtures must be
  labelled synthetic.
- Real dual-HWND creation, nonactivation, owned/exterior input-region routing, release
  startup/idle process samples, search benchmarks, and radial microbenchmarks are
  verified as recorded above. Physical cross-process click delivery, mixed-DPI and
  negative-origin behavior, real chord suppression, visual quality/accessibility,
  lifecycle repetition, and per-resource attribution remain manual release checks.
- Radify stock icon/emoji rights and all RM4 redistribution rights are insufficient
  for bundling; references remain user-provided compatibility inputs.
