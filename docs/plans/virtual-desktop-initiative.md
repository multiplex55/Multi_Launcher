# Virtual Desktop initiative execution ledger

## Authority and baseline

- Original specification: `C:\Users\Jay\.codex\attachments\f7f7d135-5fb3-41e6-b749-0f8123500198\pasted-text-1.txt`, read completely before implementation.
- Repository policy: root `AGENTS.md`; the checked-out repository remains the source of truth.
- Branch: `virtual-desktops`.
- Baseline: `415aa68e` (`origin/virtual-desktops` and `origin/master` when work began), with a clean working tree.
- Baseline verification: `cargo check` passed before source changes.
- Cadence: production code and its focused tests are implemented in four code-heavy tranches. Expensive full Nextest validation is deferred until the integrated feature exists, while every tranche must compile and pass its narrow verification before commit.

## Cross-cutting architecture and invariants

1. `virtual_desktop` owns desktop identity, selection, capabilities, native operations, and structured errors. Desktop GUIDs are authoritative; display indices are transient; cached names never retarget stale bindings.
2. Raw undocumented shell COM declarations remain isolated in the Windows backend. Only verified interface layouts/slots may be called. Unsupported shell builds return capability diagnostics instead of invoking guessed vtable entries.
3. `window_activation` owns general foreground activation. `FollowWindow`, `CurrentDesktopOnly`, and `MoveToCurrentDesktop` are explicit policies. Generic activation never relocates a window; only launcher restoration may request relocation.
4. `window_catalog` owns the one bounded, nonblocking, single-flight top-level-window enumeration worker shared by `win` and `vd`. No second worker, periodic VD polling, eager startup enumeration, or non-`vd` COM work is introduced.
5. `vd` uses the normal built-in plugin, settings, typed-command, command-bus, and background-execution architecture. Values containing arbitrary text use typed JSON payloads and malformed `vd:` actions never fall through to external launch.
6. Multi Manager owns workspace persistence and bind/unbind mutations. Layout keeps its existing string schema. MkMacro keeps its existing persisted action representation. All consume the shared services through adapters.
7. Auto rules are opt-in and event-driven. No enabled rules means no hook or idle worker. Native callbacks only enqueue/coalesce events; workers perform metadata and COM operations with narrow loop suppression.
8. Launch discovery and other bounded waits run off the egui thread. Shared locks are not held across COM calls, activation, process waits, or sleeps.
9. One implementation writer owns the repository at a time. Each construction milestone is verified, diff-inspected, and committed before its dependent milestone begins. Independent review is read-only until remediation returns to one writer.

## Pipeline status

| ID | Milestone | Depends on | Status | Commit / verification |
| --- | --- | --- | --- | --- |
| M01 | Shared virtual-desktop domain/native service and window activation | baseline | complete | pending commit; `cargo check`, 26 desktop tests, and 13 activation tests passed; independent tranche review approved |
| M02 | Shared window catalog, typed commands, and standalone `vd` plugin | M01 | in_progress | pending |
| M03 | Multi Manager binding and Layout migration | M02 | pending | pending |
| M04 | Event-driven rules, MkMacro consolidation/extensions, and discovery polish | M03 | pending | pending |
| M05 | Test completion, targeted validation, and full authoritative Nextest | M04 | pending | pending |
| M06 | Independent review, remediation, final verification, and ledger completion | M05 | pending | pending |

## M01 — Shared Windows foundations

**Objective:** replace the current MkMacro-coupled/duplicated virtual-desktop and foreground helpers with reusable, typed services while preserving all existing callers.

**Architectural intent:** add `src/virtual_desktop/{mod,model,selection,windows}.rs` and `src/window_activation.rs`; move the existing raw interfaces behind the new backend; keep COM apartment ownership RAII-correct and thread-local; expose build/capability failures safely; adapt existing launcher and MkMacro entry points without retaining competing normal-execution paths.

**Required behavior:** coherent snapshots; GUID/name/index model and resolver; ambiguity/stale-binding safety; current/next/previous; capability-gated create/close/rename/switch; window membership/movement via public `IVirtualDesktopManager`; deterministic close fallback; activation validation, cross-desktop follow without move, restore, direct foreground attempt, bounded attached-input fallback, verified failure, and elevation context. Existing MkMacro VD tags and Activate Window payloads remain unchanged.

**Tests:** pure ID/selector/binding/adjacency/close-plan tests; native validation/error conversion; injectable activation state-machine ordering and cleanup; MkMacro adapter/serialization regression coverage.

**Acceptance:** generic activation does not move; launcher-only relocation is explicit; all legacy desktop helpers are migrated or reduced to temporary compatibility adapters with no duplicated native implementation; `cargo check`, formatting for touched code, focused tests, and diff inspection pass.

**Commit:** `refactor(windows): centralize desktop and window activation services`.

## M02 — Window catalog, typed commands, and standalone plugin

**Objective:** provide the complete `vd` launcher surface without introducing another enumeration worker or untyped execution path.

**Architectural intent:** extract `WindowCatalog` from `plugins/windows.rs` into a shared internal service; have `PluginManager` own it and inject it into both plugins; add `VirtualDesktopCommand`, parser, bus/host/handler routing, and `VirtualDesktopPlugin`; extend reusable external launch primitives so background discovery can retain PID when possible.

**Required behavior:** `vd` overview/list/current; number/name switching; previous/next; create/close-current/rename; dynamic switch and move-active actions; cross-desktop window listing/labels/activate/move/move-follow; launch and launch-follow with bounded PID-first/single-instance discovery; settings entry and command discovery. Non-`vd` queries return before desktop/catalog work.

**Tests:** preserved catalog lifecycle tests; shared-consumer/single-worker behavior; plugin prefix/overview/filter/action tests with fakes; every `vd:` typed protocol and malformed-payload rejection; move/follow separation; launch discovery new-process/reuse/timeout/unrelated-candidate/no-kill cases.

**Acceptance:** `win` behavior/cache latency is preserved; one catalog worker exists; launch waiting never blocks egui; no periodic/eager VD work; focused compilation/tests and diff inspection pass.

**Commit:** `feat(vd): add virtual desktop launcher integration`.

## M03 — Workspace and Layout integration

**Objective:** integrate stable desktop ownership into Multi Manager and migrate Layout to the shared resolver/service without changing existing storage formats.

**Architectural intent:** add an optional serde-defaulted `VirtualDesktopBinding` to `MmWorkspace`; keep binding mutation in Multi Manager typed commands/state; centralize UI/hotkey target activation through one plan; snapshot state before COM work; keep Home operations geometry-only. Resolve Layout desktop strings through the shared snapshot and carry typed IDs into apply operations.

**Required behavior:** bind/rebind/clear UI with stale status; `vd bind workspace` and unbind by stable workspace ID; target/toggle/rotate/hotkey placement onto the bound desktop with one switch; Send Home/All Home do not desktop-bounce; old workspaces load unchanged. Layout capture remains compatible, restore accepts GUID or unique legacy name, and missing/ambiguous targets produce per-window diagnostics rather than current-desktop fallback.

**Tests:** old/new MM serialization, stale binding, dirty/autosave mutation, target/home/send-all/hotkey parity; Layout GUID/name/missing/ambiguous restore planning and existing JSON compatibility.

**Acceptance:** no workspace lock spans COM/window movement; stale GUIDs never name-retarget; no new Layout store/schema; focused checks/tests and diff inspection pass.

**Commit:** `feat(vd): integrate desktops with workspaces and layouts`.

## M04 — Rules, MkMacro completion, and discovery polish

**Objective:** complete opt-in application-to-desktop automation, finish MkMacro consolidation/extensions where architecturally natural, and expose all supported commands/settings/help.

**Architectural intent:** add serde-defaulted VD settings/rules and a lifecycle-owned WinEvent runtime patterned after the recorder observer; callback only enqueues; worker matches process/path/title/class and runs an explicit move-switch-activate plan; narrowly suppress only matching self-caused events. Finish eliminating shortcut/direct foreground production paths and add named/move MkMacro variants only if they preserve additive serialization compatibility cleanly.

**Required behavior:** zero rules means no runtime; enabled foreground rule moves only the configured foreground window, switches, and restores activation; already-correct/disabled/stale rules are no-ops with diagnostics; disable/reload/drop unhooks and joins; no oscillation or swallowed unrelated foreground event. Settings and help list all supported `vd` surfaces.

**Tests:** pure rule matching/planning/suppression; empty/enable/disable lifecycle; stale target; subsequent unrelated event; MkMacro adapter, authoring, validation, and serialization coverage for any additive actions.

**Acceptance:** no polling/orphan hook; no duplicate desktop or activation backend remains; `rg` cleanup audit, compilation, focused tests, and diff inspection pass.

**Commit:** `feat(vd): add event driven desktop rules`.

## M05 — Integrated tests and authoritative verification

Finish and migrate the full deterministic test matrix without adding unnecessary test binaries. Run `cargo check`, `cargo fmt --all --check`, `git diff --check`, then targeted Nextest filters for virtual desktop, activation, window catalog/windows, MkMacro, Multi Manager, Layout, rules, and launch discovery. Classify/fix failures at the correct layer. Run `cargo nextest run --no-fail-fast` and record passed/failed/skipped counts. No milestone becomes complete while task-caused failures remain.

## M06 — Independent review and completion

Assign a reviewer that did not implement the primary milestones. Review the original specification, this ledger, cumulative commits/diff, surrounding source, and tests for unsafe COM ABI/lifetimes, activation relocation or false success, GUID semantics, duplicate workers, rule lifecycle/suppression, MM Home/hotkey parity, Layout compatibility, MkMacro serialization, launch/UI blocking, and idle/search performance. Remediate every substantive finding with one writer, rerun affected targeted tests and the full suite whenever shared code changes, perform the requested Windows smoke checks if the available UI surface supports them, inspect final Git state, update the ledger, and deliver the exact required report headings.
