# Source baseline and research limits

## Planning reference

| Field | Verified preparation value |
| --- | --- |
| Repository | [multiplex55/Multi_Launcher](https://github.com/multiplex55/Multi_Launcher) |
| Branch inspected | master |
| Exact commit | [f7bc610a86bcad1c14b69dce65b1d804df1b7934](https://github.com/multiplex55/Multi_Launcher/commit/f7bc610a86bcad1c14b69dce65b1d804df1b7934) |
| Commit timestamp | November 21, 2026, 5:28:04 p.m. America/New_York (22:28:04 UTC) |
| Commit subject | Merge PR #1466; Restore Clipboard Modify reads on Windows |
| Recursive tree response | 2,373 entries; not truncated |
| Preparation method | Read-only repository/API and primary toolkit documentation inspection |
| Execution branch | Not created by this preparation task |
| Execution baseline | Not yet established; fetch latest origin/master at G00 |
| Application tests/builds/native checks performed for this package | None |

The following immediately preceding inspected commits are relevant context, not fresh test results: `769709c` integrates master into the clipboard fix; `95bd68f` adds real protected-publisher STA exercise; `6f78380` restores protected ordinary text reads. Preserve the resulting behavior, not merely the historical commit messages.

## Verified architecture anchors

All paths below refer to the planning commit. Source entry points are starting points; G00 must inspect complete local implementations and runtime routes.

| Source | Observed fact | Goal implication |
| --- | --- | --- |
| Cargo.toml | Package/bin `multi_launcher`, Rust 2024, eframe 0.36.2 with Glow; shared table core workspace; fast-dev/iteration profiles; local egui/eframe/epaint patches | Preserve real executable/build behavior; prove coexistence instead of assuming it |
| src/main.rs | Existing GUI worker calls eframe run_native and allows a Windows any-thread event loop; internal brightness helper has an early entry path | GPUI thread ownership and selector placement need deliberate proof |
| src/platform/single_instance.rs | Windows mutex identity is based on canonical AppDataRoot and retained for process lifetime | Both frontends keep one data/service owner; explicit override needs visible existing-instance behavior |
| src/main.rs | Data ownership precedes persistent loading/recovery/services; AlreadyRunning currently returns success | Do not move migrations before ownership or silently ignore explicit UI choice |
| src/gui/mod.rs | LauncherApp takes egui context and owns application/service state | A renderer-only rename is insufficient; extract real operation/service ownership |
| src/commands/host.rs | Typed host interfaces include default methods that can do nothing | Audit actual method coverage, not only compilation |
| src/commands/outcome.rs | Outcomes include query, history, visibility, restoration/focus, caret and notifications | Preserve all outcomes alongside command side effects |
| src/plugin.rs and src/plugin_editor.rs | Plugin settings render through egui and maintain edit/save identity | Each settings surface needs its own GPUI implementation/contract |
| src/dashboard/widgets/registry.rs | Widget settings callbacks also expose egui | Inventory all dynamic widget settings outside src/gui |
| src/gui/utility_host.rs and utility_window_host.rs | Host/session identity, deferred intents and accepted action semantics | Preserve semantics with new frontend adapters, not egui frame APIs in core |
| src/gui/utility_presentation.rs and src/settings/utility_presentations.rs | Remembered modes/revision acknowledgements and unknown-data retention matter | Cross-frontend mode persistence must preserve these protections |
| src/visibility.rs | Wake behavior includes egui viewport identity | Extract targeted semantic wake ownership; do not replace with constant repainting |
| src/gui/query_navigation.rs | Physical numpad handling and paired generated-digit consumption have explicit semantics | Native numpad verification is a separate requirement from ordinary arrows |
| src/radial/render.rs and native.rs; src/screen_draw/native_runtime.rs | Useful native/domain substrates are separately owned | Reuse compatible native behavior while recreating all egui control surfaces |
| .config/nextest.toml; scripts/nextest-test-stack.ps1 | Existing Nextest Windows wrapper/stack-floor fix | Preserve current test environment; avoid reproducing old stack failures |

## Deliberate limits of this planning survey

The tree includes 237 Rust files under `src/gui/` and 96 under `src/plugins/`. These are **not GUI counts**. Source can contain many independently meaningful interfaces, while a GUI can span several modules.

Large `src/gui/render.rs` is about 3.6 MB in the tree and did not return usable complete content through one research retrieval path. It must be read locally during G00. The survey read other important entry points, feature files, tests and documentation, but did not inspect every control or execute the app. The seed inventory must not be represented as the final exhaustive inventory.

No missing source response is evidence that a file has no behavior. No historical acceptance log is a current pass. No pre-existing issue is newly reproduced merely because its old ledger remains in the tree.

## Execution baseline fields to fill at G00

Record fetched master SHA/time, task parent and baseline commits, branch/upstream, current instructions/roles, source changes since planning, Rust/Cargo/Nextest versions, Windows build target/tools/SDK, dependency/features/profile, current environment and disposable fixture strategy. Record baseline test/native failures only with provenance.

Preserve this planning reference separately from the new execution baseline. After kickoff, do not silently change the behavioral reference when master advances. Reconcile any deliberate upstream integration with affected GUI/spec/test records.

## Primary sources

- [Planning commit](https://github.com/multiplex55/Multi_Launcher/commit/f7bc610a86bcad1c14b69dce65b1d804df1b7934).
- [Cargo manifest](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/Cargo.toml).
- [Repository instructions](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/AGENTS.md).
- [Current README](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/README.md).
- [GPUI Kit installation](https://gpui-kit.com/docs/installation/), [getting started](https://gpui-kit.com/docs/getting-started/), [testing](https://gpui-kit.com/docs/test/), [multi-window](https://gpui-kit.com/docs/multi-window/), [accessibility](https://gpui-kit.com/docs/accessibility/).
- [Official GPUI Kit source](https://github.com/longbridge/gpui-kit).

Every source path in the architecture table can be resolved under the planning commit; use the execution checkout after baseline reconciliation.
