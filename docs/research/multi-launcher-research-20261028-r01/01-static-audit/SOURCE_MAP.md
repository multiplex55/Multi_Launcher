# Source map — pinned owners and first-party boundaries

Checkpoint: G01/M1-A, 2026-10-28. Source: `59156d96adcc73939c7171487a0cb582e3286587`. This is a static map of the checked source; achieved inspection depth is in [COVERAGE_MATRIX.md](COVERAGE_MATRIX.md). Later command, input, cancellation, mutation and change-cost findings are not implied by this checkpoint.

## Inventory method and boundaries

The parent generated [SOURCE_INVENTORY.md](evidence/SOURCE_INVENTORY.md) from tracked paths and text reads: 105 buckets and 790 first-party Rust files under `src/`, `crates/` and `tools/`. This author cross-checked the manifest boundaries, `src/lib.rs` declarations, startup bodies and selected facades. Supporting `tests/`, `benches/`, `dev/`, manifests and build resources are additionally dispositioned in the matrix. Inventory totals establish presence, not audited lines, runtime load or build cost.

No Cargo metadata, macro expansion, rust-analyzer, compiler or runtime was used. Bulk vendor/dependency/generated review is excluded. The application's local eframe/egui patches remain relevant interface context: where later reasoning relies on patched lifecycle behavior, the patch needs a targeted read. Module declarations prove exposure; a `use` proves a syntactic dependency; neither proves an executed call. Trait objects, dynamic libraries, `cfg`, macros and test-only substitutions make this an intentionally incomplete Rust call graph.

| Build or source boundary | Observed current declaration | Architectural meaning and limit |
|---|---|---|
| Root `multi_launcher` package | Edition 2024, `build.rs`, `autobins = false`, default launcher binary; 12 explicit binary targets, feature gates, four benches and three grouped integration targets [M1] | `src/bin` contains more tracked files than declared binary targets. Tracked presence does not establish a target is built. Root top-level integration tests remain auto-discovered; explicit suites group selected cases rather than replacing all targets. |
| Root library | Public subsystem declarations and selected crate-private owners in `src/lib.rs` [M2] | Most features share one application crate. A file/module split is not a crate or independent build boundary. Production main links the library via `multi_launcher`. |
| `multi_launcher_table_core` | Workspace member and root path dependency; dependencies `csv` and `serde_json` [M1], [M3] | Existing extracted table boundary. `src/table_utility.rs` re-exports it, preserving application import/type identity [T1]. The core exports file-source and resource types as well as in-memory parsing/projections; do not infer the whole crate is file-free from its opening comment [T2]. |
| `tools/cursor_effects_smoke` | Independent manifest with its own empty `[workspace]` and Windows dependency [M4] | Separate tool, outside root workspace membership. Inspected as source only; no tool was run. |
| Build resources/local patches | `build.rs` calls `embed_resource::compile`; root patches epaint, egui, eframe, image and egui-toast [M1], [M5] | Build/native dependency context, not executed research machinery. No third-party implementation audit is claimed. |

## Bootstrap and process/thread owners

Normal launcher startup follows these inspected bodies. The internal brightness-helper early exit precedes the normal launcher ownership boundary, so the single-instance statement below is scoped to launcher startup rather than every invocation of the executable. [A1]

| Stage | Owner and actual transition | Edge type / evidence |
|---|---|---|
| Process/data admission | `main` derives `AppDataRoot` from `settings.json`, acquires and retains `SingleInstanceGuard`, returns on `AlreadyRunning`, and calls target-repair startup recovery [A1] | Call plus process-lifetime ownership. `AppDataRoot` records relative-path behavior and normalized identity; it does not relocate user data [A2]. The guard owns a Windows handle and closes it on drop [A3]. |
| Recovery/settings preload | `load_startup_preload` calls pending recovery before `load_startup_settings`. The settings loader retains invalid persisted bytes, uses temporary defaults, and publishes a migration candidate only after successful save [A4] | Ordered calls and returned diagnostic state; no crash-durability or external-writer guarantee is asserted. |
| Saved actions | `main` calls `load_startup_actions`, retains a diagnostic for temporary empty startup state, then constructs index configuration [A1] | Call and value transfer; detailed load/publication safety belongs to M2-A. |
| Initial index | `startup_indexed_actions` returns an empty indexed list/no coordinator for empty roots. Otherwise it creates the coordinator, submits config, waits, checks generation/config, takes result, validates acknowledgment, and returns `StartupIndexingTransfer` [A5] | Call, completion acknowledgment and move of coordinator/config/generation. Waiting occurs before GUI creation. This does not establish measured startup latency. |
| Canonical startup catalog | `startup_action_catalog` records `custom_len`, appends indexed actions, and wraps the combined vector in `Arc` [A5] | Data assembly: saved custom prefix followed by indexed tail. The same `Arc<Vec<Action>>` reaches plugins and the GUI; no mutation-in-place claim follows. |
| Plugin/internal services | `main` constructs `PluginManager::new_with_reserved_hotkeys`; that opens the macro store, installs shared macro runtime reservations and creates internal catalogs [A6], [P1] | Calls and shared service ownership. `PluginManager` holds macro/window/workspace/clipboard-modifier services; constructors are effectful and differ from inert test constructors. |
| Main native services | `main` owns hotkey listener/reconfiguration endpoint, radial store/controller/resources, authoring/control endpoints and native preview. Callback closures send wake messages to `event_tx`; GUI clients are installed before app construction [A6] | Native/process owner plus message endpoints. Native resources are not all fields of `LauncherApp`. Detailed service/lease transitions remain M1-B/M2-A work. |
| Registration before GUI spawn | `spawn_gui` calls `plugins.reload_from_dirs` and sets missing-target custom-action input before `GuiWorker::spawn` [A7] | The registration code runs on the caller/main thread despite the function name. The plugin manager then moves into the GUI closure. |
| GUI thread and app | `GuiWorker::spawn` launches a thread; `eframe::run_native` uses Glow and the Windows any-thread event-loop option. The app creator calls `LauncherApp::new`, installs target-repair, revisions/bridges/settings-save registry, and installs the acknowledged startup index transfer [A7] | Thread creation, moves and shared handles. No native-thread-affinity correctness is certified. |
| GUI events/index attachment | App construction creates the GUI event channel and registers an owner-scoped event sink. `IndexingOwner::attach` installs a notifier that sends `WatchEvent::IndexReady` then requests a viewport wake [G1], [I1] | Message flow plus UI-owned coordinator. Main's `channel::<()>` wake channel and the GUI `WatchEvent` channel are separate mechanisms. |
| GUI exit | `LauncherApp::on_exit` explicitly shuts down domain/dialog owners, cancels paste playback, retires capture/input owners, closes bridges, prepares settings-save shutdown, and saves usage through target repair or fallback [X1] | Ordered calls only. Whether every callee finishes/join-releases resources is deliberately deferred to M2-A. |
| Main exit | GUI completion/channel closure stops admission to settings-save work, cancels/stops native services/listener, joins GUI, then joins save workers. Quit-hotkey path closes the viewport before join and has a separate explicit sequence [X2], [X3] | Completion/wake distinction, cancellation requests and joins. Presence of a `shutdown` call alone is not retirement proof. |

## Shared facades and state owners

| Owner / source | Responsibility observed at this checkpoint | Consumers, adapters and boundaries |
|---|---|---|
| `actions::Action`, `ActionIdLookup` [F1] | Serialized action model and catalog identity lookup | Startup/catalog, plugin-produced rows and command invocation refer to `Action`; wire compatibility and identity semantics require M1-B trace. |
| `commands` facade; `CommandInvocation`, `ActivationSource`, `Command` [C1] | Owned typed domain request retains original `Action`, source and optional query override | `CommandBus` explicitly matches command domains through a `dyn CommandHost` [C2]. Already an existing typed boundary, not an absent architecture. |
| `CommandHost` and domain host traits [C3] | GUI-independent interfaces for domain-specific operations | `gui/command_host.rs` implements these traits for `LauncherApp`; GUI dispatch forwards to its shared bus [C4]. `commands/headless.rs` is a separate effect adapter, not proof all surfaces share identical admission. |
| `Plugin`, `PluginManager`, `PluginSlot` [P1] | Provider interface, catalog/service ownership, configured enablement and dynamic reload bookkeeping | Providers expose search/commands/prefixes/capabilities and literal-versus-filter query policy. `load_dir` scans DLLs; exact dynamic implementations cannot be resolved by this audit [P2]. |
| `PluginSearchSnapshot` [P1] | Immutable captured provider ownership plus cloned enablement/capability inputs and shared update registry | Bounded workers can retain `Arc<PluginSlot>` without borrowing mutable app state. Snapshot retention is not a guarantee of current execution authority; later traces must follow revision checks. |
| `LauncherApp` [G2] | Actions, caches, query/results/selection, plugin manager, index owner, command bus, activity center, settings-save state, recovery context and tool/dialog owners | GUI modules are methods/adapters around this state. `selected: Option<usize>` is a presentation field; the semantic selection/activation contract is not established by this field alone. |
| GUI constructors and services [G1], [G3] | Construct dashboard/widget registry, attach watcher/repaint callbacks, configure clipboard-modifier runtime/watcher, attach workspace catalog, macro authoring context and Hot Folder adapter/runtime | Production-versus-isolated initialization policy is explicit. Do not treat arbitrary app construction as a pure ranking fixture. |
| `universal_actions` facade [U1] | Separate target/reference, resolution context/provider/registry, availability/safety/presentation and operation types | GUI has `universal_action_catalog` and executor modules; surface-independent declarations do not prove caller parity or correct confirmation on every route. M1-B owns that trace. |
| `IndexingOwner` / `IndexCoordinator` [I1] | GUI owns desired configuration, expected generation, notifier and coordinator attachment after startup handoff | `install_startup` rejects closed/already-attached/config-mismatched transfer and validates acknowledgment; shutdown revokes notifier before coordinator shutdown. Replacement/result-publication internals remain M2-A. |
| `AppDataRoot`, persistence facade/catalog/data service [A2], [D1], [D2] | Root identity plus typed persistent-store inventory, metadata/health/recovery/backup interfaces; `DataService` owns a named worker/channel/state | `PersistenceCatalog::new` includes configured-path discovery; startup bootstrap avoids dashboard discovery. Metadata construction is not a blanket no-I/O claim. Domain stores still own their formats. |
| Settings facade and save worker registry [G2], [F2] | Settings model/I/O/store modules; registry is re-exported through GUI | Current transaction, intent, watcher and live-application contracts await M2-A. No new save coordinator is proposed. |
| `table_utility` / `TableUtilitySession` [T1], [T2] | Application facade to extracted core; app session supplies source/view/selection revisions and export receipts | GUI/dialog/session integration remains in root crate. Core constants and resource/source types are present; bounded-runtime claims require body/lifecycle review. |
| `activity::ActivityCenter`, `thread_reaper` [F3], [F7] | Explicit activity lifecycle and retirement vocabulary; separately reusable join/reap owner | This is navigation/counterevidence to an absent-lifecycle-framework claim. M2-A must verify integration per worker; activity state alone does not prove resource release. |
| Query metadata, completion, usage/history [F4], [F8], [F9], [F10] | Query filtering/tokenization, FST suggestion facade, usage persistence and history/pins APIs | Actual ranking/prefix/literal/retention behavior belongs to M1-B/M2-A; only API presence is surveyed here. |

## Remaining domain navigation

The library's module declarations, GUI adapter declarations and complete [coverage matrix](COVERAGE_MATRIX.md) are the exhaustive entry point index [M2], [F5]. Feature/plugin families are not separate packages merely because they have directories.

- File Search is `src/file_search.rs` plus coordinator/discovery/backend/matching/identity/preview/export modules. Indexing is the separate `src/indexer.rs` plus `indexer/coordinator.rs` owner [F6]. Do not confuse launcher catalog indexing with file-content search sessions.
- Mutation owners include `file_operations`, `batch_rename_transaction`, `replace_executor`, `search_replace`, Diff operations, archive and target repair; automation adapters include MkMacro, Hot Folder and Paste Sequence [M2]. Only presence and startup wiring are covered here.
- Native input/overlay owners span `hotkey`, radial, Jump GUI, visibility/parking, mouse gestures, coordinate/screen-draw/capture tools and window/workspace services [M2], [F5]. A common Windows dependency does not make them one authority.
- Notes/records, dashboard/widgets and pure utilities retain their existing domain modules. Survey-only entries are explicit in the matrix; their lower checkpoint priority is not a claim that user content or resource behavior is safe.

## Tests, development support and exclusions

The root manifest groups `plugin_queries`, `domain` and `pure` suites and keeps top-level integration targets. The inspected suite files use explicit `#[path]` case inclusion; `command_bus_architecture` checks source shape for exhaustive typed dispatch and canonical activation parsing. These are source contracts, not passing test results [M1], [V1].

`IsolatedAppWorkspace::create_app` scopes a profile root and calls `LauncherApp::new_isolated_for_test_at`; `PluginManager` has inert constructors with an empty store path separate from production shared-runtime construction [V2], [P1]. Future app tests must still clear Paste Sequence `next_hotkey` and `cancel_hotkey` before construction as required by current AGENTS; this research constructs no app.

`dev/README.md` and tracked development scripts describe target-first test, process containment and historical performance/native workflows. They are inventory/context only. No historical command in them was adopted or run. Standalone smoke/perf targets, benches, resource files and vendor patches are survey/out-of-scope entries with reasons in the matrix [V3], [M1].

M1-A introduces no new accepted action item. Later checkpoints evaluate current safeguards and concrete failure mechanisms before accepting recommendations; the planner's AUD-F001 remains a separately attributed proposal.

## Pinned source references

[M1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/Cargo.toml#L1-L300
[M2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/lib.rs#L19-L127
[M3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/crates/multi_launcher_table_core/Cargo.toml#L1-L9
[M4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/tools/cursor_effects_smoke/Cargo.toml#L1-L17
[M5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/build.rs#L1-L6
[A1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/main.rs#L4225-L4304
[A2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/platform/app_data.rs#L5-L60
[A3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/platform/single_instance.rs#L16-L82
[A4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/startup.rs#L22-L86
[A5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/startup_catalog.rs#L7-L125
[A6]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/main.rs#L4332-L4609
[A7]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/main.rs#L4030-L4223
[G1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/mod.rs#L3061-L3164
[G2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/mod.rs#L954-L1054
[G3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/mod.rs#L3434-L3556
[X1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/render.rs#L10043-L10125
[X2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/main.rs#L4643-L4667
[X3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/main.rs#L6228-L6253
[F1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/actions/mod.rs#L13-L95
[F2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/settings/mod.rs#L1-L12
[F3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/activity.rs#L201-L240
[F4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L4-L61
[F5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/mod.rs#L1-L138
[F6]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/file_search.rs#L1-L19
[F7]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/thread_reaper.rs#L231-L318
[F8]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/completion.rs#L1-L51
[F9]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/usage.rs#L11-L96
[F10]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/history.rs#L12-L356
[C1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/commands/model.rs#L13-L104
[C2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/commands/bus.rs#L14-L100
[C3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/commands/host.rs#L335-L390
[C4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/command_host.rs#L2143-L2215
[P1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L104-L768
[P2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1227-L1267
[U1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/universal_actions/mod.rs#L1-L29
[I1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/indexing.rs#L11-L139
[D1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/persistence/catalog.rs#L186-L265
[D2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/persistence/data_service.rs#L249-L309
[T1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/table_utility.rs#L1-L7
[T2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/crates/multi_launcher_table_core/src/lib.rs#L1-L60
[V1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/tests/domain_cases/command_bus_architecture.rs#L1-L100
[V2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/test_fixtures.rs#L61-L109
[V3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/dev/README.md#L1-L75
