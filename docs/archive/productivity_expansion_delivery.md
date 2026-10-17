# Productivity expansion delivery evidence

Source and focused verification milestones M0–M13-B are complete on `big-changes`. Final documentation is M13-C. **Final Clippy is pending M13-D**; its actual command/result and any lint-cleanup commit belong in the final delivery report, because no documentation or other verification follows the last Clippy pass.

Scope follows the [approved master plan](multi_launcher_productivity_expansion_master_plan.md). The [execution ledger](productivity_expansion_progress.md) records each checkpoint, scoped review, actual commands/results, repaired failures, and evidence limits. The [README](../../README.md) describes the user commands and safety rules.

## Feature evidence

Implemented means the feature has its production path, scoped review, and the listed automated evidence. It does not imply that native GUI interaction, target input delivery, or every hardware configuration was observed.

| Feature | Implemented behavior and evidence |
| --- | --- |
| Search scopes | Five session scopes, scope-aware results/cache, explicit-prefix override and main/radial compatibility; M1 domain/routing/GUI-state cases. |
| Dynamic snippet variables | Namespaced `sys.*` values captured once per invocation, legacy prompted fields, explicit preview privacy; M2-A/B contextual rendering and invocation tests. |
| Snippet tags | Additive normalized multi-tag persistence, search and editor filters; M2-C/D legacy-store, hidden-body, Save/Cancel and GUI-state tests. |
| Paste Sequence | Saved/memory-only sequences, prompted/dynamic mixed preflight, automatic/manual playback, guarded target and conditional clipboard restoration; M3 domain, app and fake-platform playback evidence. |
| Hash Generator | SHA-256/SHA-512/SHA-1 exact-byte text/file hashing, bounded streaming/cancellation, explicit copy/compare; M4 vectors, file-error and retained-dialog lifecycle tests. No MD5. |
| JSON tree | Read-only authoritative document/path model, virtualized expansion/selection and explicit copies; M5-A/B/D model and 2,048-item GUI-state fixture. |
| JSON search | Full-value key/value/both search, cached highlights, ancestor reveal and navigation; M5-C/D domain and GUI-state tests, with original transforms retained. |
| Regex test cases | Match/no-match expectations, isolated evaluation, versioned preset migration, explicit case persistence and GUI pass/fail; M6 tests include legacy presets and invalid-pattern recovery. |
| XML Toolkit | Bounded local MSXML6 DOM, secure parsing, format/minify, tree/XPath and explicit local XSD; M7 includes actual MSXML engine/XSD/namespace/DTD/network-denial fixtures plus GUI-state integration. |
| Batch Renamer | Explicit roots, ordered preview, protected extensions, frozen confirmation, source/target rechecks and guarded Apply/Undo/recovery; M8 actual Windows native-file disposable fixtures and app routing. |
| Search & Replace | Explicit scoped discovery, stable per-match approval, byte-faithful diffs, fresh-source checks, durable backup and explicit Restore/recovery; M9 actual native-file and app fixtures include stale/occupied/ADS refusal. |
| Hot Folder | New/imported/edited rules disabled, explicit validated activation, bounded event/settle/action ownership, no startup backfill, read-only Test and history; M10 actual notify/native-file Move plus correlated launcher/macro admission evidence. |
| Screenshot Pinboard | Session-only independent images/windows, child-DPI Fit/Actual/zoom, explicit Copy/PNG Save, per-pin topmost/pass-through and keyboard List recovery; M11 tracked-budget/capture/ACK/deferred-child tests and two-pin app fixture. Native window effects are simulated in verification. |
| Click Visualization | Physical left/right/middle observations on the shared hook, Ripple/Circle/Flash, finite bounded passive surfaces, saved appearance/session enablement and idle blocking; M12 hook-target→actual worker→fake-render, settings, failure/exclusion/DPI-order and gesture preservation tests. Native input delivery is simulated. |

## Verification closeout

- M13-A used one current-registry provider→search→parser→typed-bus slice for XML, rename, replace and Hot Folder: **4 passed**. A stale rename prose assertion was corrected while preserving payload and safety assertions; the same bounded rerun passed.
- M13-A reused the recorded scope/hotkey, mixed snippet/sequence, file-operation, pinboard and click integration evidence rather than rerunning every feature test.
- M13-B formatted the explicit **130 maintained Rust paths** changed since the clean baseline using `rustfmt --edition 2024 --config skip_children=true`. The matching format check passed; 40 files changed. Independent review confirmed formatting/import ordering only and no vendor or untouched-file edits.
- The final ordinary production check, `cargo check --bin multi_launcher`, passed (exit 0, 1m22s). Maintained warnings were deliberately deferred to M13-D. No full-repository Nextest run or intermediate Clippy baseline was performed.
- The last-stage scope is `cargo clippy --workspace --all-targets -- -D warnings` on the Windows toolchain. Baseline metadata identified only `multi_launcher` as a workspace member; M13-D must confirm membership and exclude any actual vendored workspace member if that changes. Ordinary vendored dependencies are not separate lint targets.

## Native UI evidence and limitations

**No feature UI screenshots were captured.** The owned M1 instance was discovered, but computer-use app inspection/activation timed out with “Computer Use app approval timed out.” One bounded retry found no window after the containment timeout. Later milestones reused this `not_run_environment` blocker without repeated launches, capture attempts, or user-run validation.

The retained process result is [M1 native process result](../../target/productivity_expansion/M1-native-process/result.json), a generated Git-ignored local artifact. It proves the process containment timeout, `RootAlive: false`, and owned PID 35224 `Absent` after cleanup; it does not itself record the separate computer-use approval error. That error and the no-retry decision are recorded in the execution ledger.

Evidence therefore has these practical limits:

- Pin windows, click-through/no-activation, real input delivery, native picker interaction, focus transitions, and mixed-monitor rendering were not exercised through the desktop. Deterministic egui/platform fakes prove the coordination and request paths, not OS behavior.
- Windows MSXML parsing/validation and native file/notify operations were exercised under disposable fixtures. Real user folders and clipboard contents were not used. Guarded profile-ancestor access was denied by this host; native file fixtures were physically rooted under `target/productivity_expansion` without weakening guards.
- Cross-volume move/copy safety was verified through the injected protocol; an actual second-volume fixture was not run.
- Test evidence is scope-proportionate. The ledger preserves failed/canceled attempts and exact corrected reruns; a passing subset is never presented as a full-suite result.

## Verified checkpoint commits

Generated from the actual Git history after the clean baseline `677f7ba0cb49104dbf0d8397a8f4eaa3703019b0`, through M13-B. Subjects below are verbatim; hashes are verified 12-character prefixes. M0-A is the initial clean-state/toolchain inspection, with no separate source commit.

| Checkpoint | Commit | Subject |
| --- | --- | --- |
| M0-B | `946a30066fbf` | docs(plan): [M0-B] initialize expansion ledger |
| M1-A | `263330a6fa87` | feat(search): [M1-A] identify launcher result scopes |
| M1-B | `49629e25794c` | feat(search): [M1-B] add launcher scope controls |
| M1-C | `6ae20e8705f8` | test(search): [M1-C] protect scoped and unscoped result behavior |
| M2-A | `d544fbfc6dd7` | feat(snippets): [M2-A] add namespaced dynamic variables |
| M2-B | `de5aa773a190` | feat(snippets): [M2-B] resolve variables in snippet execution |
| M2-C | `5ee03531bd08` | feat(snippets): [M2-C] persist searchable snippet tags |
| M2-D | `01ed79038789` | feat(snippets): [M2-D] expose variables and tags in existing UI |
| M3-A | `a80be47adaf0` | feat(paste): [M3-A] add typed sequence model and persistence |
| M3-B | `22c534f4f879` | feat(paste): [M3-B] preflight and resolve sequence steps |
| M3-C | `024afbfe5648` | feat(paste): [M3-C] implement guarded sequence playback |
| M3-D | `9c16f3b27366` | feat(paste): [M3-D] add sequence editor and launcher commands |
| M3-E | `68c385121a31` | test(paste): [M3-E] verify mixed steps and cancellation |
| M4-A | `1e39e3173a20` | feat(hash): [M4-A] add streaming hash and compare engine |
| M4-B | `7820db6c5125` | feat(hash): [M4-B] add hash generator dialog and actions |
| M4-C | `5c7111985270` | test(hash): [M4-C] cover UI lifecycle and file errors |
| M5-A | `3f756260c7e3` | feat(json): [M5-A] add structured read-only JSON tree model |
| M5-B | `65aaf84fb072` | feat(json): [M5-B] integrate virtualized JSON tree view |
| M5-C | `6eef86a9cf6c` | feat(json): [M5-C] add cached key and value search |
| M5-D | `c0732c5ed3ed` | test(json): [M5-D] protect tree, search and transform flows |
| M6-A | `dbff248cb72c` | feat(regex): [M6-A] add typed test cases and isolated evaluation |
| M6-B | `f51c43862c78` | feat(regex): [M6-B] persist cases with safe preset migration |
| M6-C | `ee2a0e43a0aa` | feat(regex): [M6-C] add test case editor and pass/fail display |
| M6-D | `72fb0b3065b1` | test(regex): [M6-D] verify test cases and legacy presets |
| M7-A | `2c1a41157b47` | feat(xml): [M7-A] establish secure XML engine boundary |
| M7-B | `eacefa837a4a` | feat(xml): [M7-B] add syntax diagnostics and formatting |
| M7-C | `f28a4398d19b` | feat(xml): [M7-C] add XML tree and node inspection |
| M7-D | `c885f881fea5` | feat(xml): [M7-D] add XPath query and navigation |
| M7-E | `10461a41d99a` | feat(xml): [M7-E] add local XSD validation |
| M7-F | `9cc3531323a3` | feat(xml): [M7-F] integrate XML Toolkit dialog and commands |
| M8-A | `e3f6a24f65ea` | feat(rename): [M8-A] model renames and compute dry-run preview |
| M8-B | `88459ba255b3` | feat(rename): [M8-B] add scoped candidate enumeration |
| M8-C | `5085c7195f74` | feat(rename): [M8-C] add batch rename preview interface |
| M8-D | `aa9d12fb2a20` | feat(rename): [M8-D] execute renames and safe undo |
| M8-E | `09dc79d349fb` | test(rename): [M8-E] validate preview, apply and undo |
| M9-A | `41638dbf9eff` | feat(replace): [M9-A] add scoped text search and match model |
| M9-B | `171003edd404` | feat(replace): [M9-B] compute selective replacement previews |
| M9-C | `82bc4b6ce1af` | feat(replace): [M9-C] add verified write and restore operations |
| M9-D | `f649e91950d3` | feat(replace): [M9-D] add multi-file diff preview dialog |
| M9-E | `305dcc09fc4c` | test(replace): [M9-E] cover selective replacements and recovery |
| M10-A | `6b82e7912a60` | feat(hotfolder): [M10-A] add validated, disabled-by-default rules |
| M10-B | `ec93606175af` | feat(hotfolder): [M10-B] add bounded watcher and file-settle service |
| M10-C | `cd0d754590e6` | feat(hotfolder): [M10-C] run safe file actions without loops |
| M10-D | `3827d05e333a` | feat(hotfolder): [M10-D] connect validated launcher and macro actions |
| M10-E | `b9001f6f92af` | feat(hotfolder): [M10-E] add rule editor and execution history |
| M10-F | `1e2c7fa670e0` | test(hotfolder): [M10-F] verify rules and watcher lifecycle |
| M11-A | `bfabd4329dec` | feat(pinboard): [M11-A] capture and manage floating screenshot pins |
| M11-B | `59c9b4d307e0` | feat(pinboard): [M11-B] add movable pinned image windows |
| M11-C | `70b0b42bc741` | feat(pinboard): [M11-C] add native topmost and click-through controls |
| M11-D | `9d30a309331b` | test(pinboard): [M11-D] validate pin isolation and native lifecycle |
| M12-A | `25af70c266d2` | feat(mouse): [M12-A] add click visualization state and commands |
| M12-B | `7cec285fbbbb` | feat(mouse): [M12-B] render configurable click indicators |
| M12-C | `1a836a790099` | test(mouse): [M12-C] protect existing input and effect behavior |
| M13-A | `c787aa23c1da` | test(productivity): [M13-A] verify shared provider command routes |
| M13-B | `32d4068dbeeb` | chore(quality): [M13-B] finish integration cleanup and evidence |

These 55 checkpoint commits precede the M13-C documentation commit. The final report identifies M13-C and any M13-D lint-cleanup commit, along with the actual final Clippy outcome.

