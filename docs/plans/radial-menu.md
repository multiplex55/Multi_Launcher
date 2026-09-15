# Native radial menu implementation ledger

Status: approved; M0-M5 complete

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
| M6 hardening/performance/full verification/review | pending | format/check/diff, full Nextest, native evidence, serialized baseline/candidate measurements, independent review |

Each milestone is implemented and committed sequentially by one writer. Tests are
written with each coherent batch and run only at gates. One Cargo/build/test job may
use this checkout/target at a time.

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
- Native cross-process click-through, nonactivation, mixed-DPI behavior, real chord
  suppression, visual quality, and resource/performance measurements are unverified
  until their stated live gates run.
- Radify stock icon/emoji rights and all RM4 redistribution rights are insufficient
  for bundling; references remain user-provided compatibility inputs.
