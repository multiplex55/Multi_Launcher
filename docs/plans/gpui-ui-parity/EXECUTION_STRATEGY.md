# GPUI migration execution strategy

Read and follow this strategy before planning, implementing, reviewing, verifying, or resuming any GPUI migration work. It governs infrastructure priorities, implementation and verification scheduling, batching, and build efficiency. The [Master Goal](../../CODEX_GOAL_GPUI_UI_PARITY.md) defines the objectives, full functional scope, and final acceptance. The [Kickstart](../../CODEX_KICKSTART_GPUI_UI_PARITY.md) defines initialization, resumption, and coordination. Repository [AGENTS.md](../../../AGENTS.md), nested instructions, and the current agent configuration govern repository engineering and agent conventions. Use the [Validation Strategy](VALIDATION_STRATEGY.md) for evidence design and record results in the [checkpoint ledger](CHECKPOINTS.md).

## Continue the existing migration

Resume the current checkpoint, branch, and goal from the existing migration records. Preserve existing work and plans. Read the current repository instructions, goal packets, checkpoint records, and actual role configuration. Use the established Planner, Implementer, and Reviewer workflow. This methodology applies through context compaction, agent or model replacement, session restart, checkpoint or milestone transition, goal continuation, and upstream integration. Include the link and relevant requirements in every new sub-agent assignment.

Deliver complete GUI-by-GUI parity with the egui application through official GPUI Kit. Preserve behavior, controls, workflows, configuration, data, the functional egui fallback, and a single authoritative shared engine. Implementation scheduling may change while the original scope, safety requirements, and final acceptance criteria continue to govern.

## Prioritize infrastructure and broader coverage

Address foundational blockers and reuse existing abstractions before extracting common infrastructure. Use Hash Generator and the implemented launcher and utility interfaces as references. Prioritize broadly applicable capabilities according to real dependencies.

- Global hotkeys, visibility, foreground restoration, virtual desktops, native placement, and window lifecycle.
- Shared docked, popout, pinned, independent, and retained utility hosting.
- Keyboard navigation, focus, Escape, Jump Mode, accessibility semantics, and native interaction patterns.
- GPUI Kit components, layout, typography, sizing, styling, editors, tables, virtualized lists, filters, menus, dialogs, and confirmations.
- Typed command dispatch, plugin admission, result activation, and universal actions.
- Canonical Settings ownership, checked persistence, and preservation of unrelated data.
- Background ownership, cancellation, progress, retirement, and Activity Center integration.

Extract abstractions for actual consumers or established requirements. Keep substantial features in their own modules with retained views and clear ownership. Use the main frontend for application coordination. Large GPUI render trees have caused native stack overflows. Address that pressure through independently rendered components and child views while preserving the normal main-thread stack size.

Fix cross-cutting defects at the responsible boundary. Distinguish work that blocks implementation, cross-family regressions, isolated plugin defects, minor presentation work, environment-limited evidence, and deferred acceptance criteria. Give each deferred criterion an owner, explicit acceptance conditions, and a scheduled verification stage. Continue safe independent work while isolated or environment-dependent work waits. Group small control or appearance refinements after broader implementation when safe.

## Select coherent GUI batches

The Planner selects related work by dependencies, shared infrastructure, and concrete reuse. A typical batch is three to five smaller surfaces or several meaningful increments of a substantial family. Adjust scope for complexity, safety, and link cost. Use G00–G26 and the existing packets as the coverage and ownership ledger. Document dependencies and GUI ownership when scheduling independent work out of register order.

Examples include structured text with JSON, XML, and Regex. Clipboard History, Clipboard Modify, and Clipboard Snippets share clipboard workflows. File Search, Folder Size, File Locks, Duplicate Finder, and ZIP utilities share file operations. Diff, Batch Rename, and Search & Replace can share preview and apply controls. Notes and Markdown Studio share document workflows. Calculation Pad, converters, and table tools share input and data presentation. Dashboard and organizer widgets, screenshot and native overlays, and Automation, MkMacro, Radial Menu, and related authoring interfaces offer further families. Choose concrete batches from the source and documented dependencies.

## Pass 1 · Functional implementation and coverage

Inspect the relevant egui implementation and map its real controls, modes, state, actions, and effects. Implement usable GPUI workflows using the authoritative engine, commands, canonical data, and persistence. Each shipped increment must exercise its actual feature. Count empty shells, mock data, and nonfunctional controls as unfinished.

Write meaningful tests alongside the implementation. Use scoped `cargo check --locked --profile fast-dev --bin multi_launcher` and include affected library or test targets when appropriate. Check meaningful test code before the linked batch when practical. Use these checks for types, ownership, API and integration issues. Obtain independent source review and resolve blocking architecture, safety, and compatibility issues during the increment. Record tests as written, type-checked, or executed according to the actual evidence.

Use one active source writer and keep small coherent checkpoint commits. Safe independent implementation can proceed with explicit verification gaps. Maintain separate functional and acceptance status for every GUI.

## Pass 2 · Grouped verification, refinement, and acceptance

Combine focused Nextest selectors by affected package and target and confirm nonzero intended selection. Reuse linked binaries for additional selectors or failed-test retries when source and configuration match. Record compilation, linking, and execution costs separately. Some focused links have taken 20–30 minutes while tests run in seconds. Favor meaningful implementation between those builds and preserve compiler flags, profiles, features, toolchain, environment, target directory, and caches unless the work needs a change. Full-suite testing and release builds require a concrete scope, risk, or existing acceptance gate.

Where practical, build one native Windows application candidate for the batch and exercise the related workflows in egui and GPUI using that same artifact. Compare behavior and appearance against recorded baseline outcomes. Collect related failures before building a correction if that is safe and leaves the cause clear. Combine related fixes and cosmetic adjustments into scoped correction cycles and revalidate the affected behavior. Verify each batch before allowing excessive unverified implementation to accumulate. Formally accept only the criteria whose required evidence has passed.

## Verify high-risk changes at the affected boundary

Use immediate narrow verification for startup, shutdown, shared engine or service ownership, configuration writes or data loss, destructive operations, native window identity and foreground authority, global hotkeys and input routing, synchronization, cancellation, threading and retirement, unsafe Rust and Windows FFI, resource lifetimes, architecture used by dependent work, and regressions that stop meaningful testing. Resolve ordinary compilation failures promptly. Use grouped linking for independent lower-risk presentation work. Apply the narrowest useful check to each risk based on judgment and evidence.

## Preserve visual and functional verification

Use physical **1516 × 937 pixels** at matched display scaling and equivalent application state for primary egui and GPUI behavior comparisons and screenshots. Inspect the stock egui layout. Preserve the GPUI query field at the top center during resize, scrolling and changes in result count. Keep results and docked content in the available scrolling area below it. Preserve query focus, navigation, autocomplete, and Jump Mode. Compare information density, control placement, spacing, alignment, and available workspace. Compact and minimum-size checks are supplemental. Apply the comparison baseline through fixtures and preserve default size, supported minimum, and saved geometry.

Use disposable owned native test profiles with Paste Sequences' Next and Cancel shortcuts unset before app construction. Keep production configuration semantics. Record native scenarios separately from headless or shared-engine tests. Continue eligible implementation if native access is temporarily unavailable and keep the affected verification and acceptance open.

## Record progress and coordinate ownership

Keep the existing goal, checkpoint, validation, and parity documents current at meaningful boundaries. Distinguish specified, implemented, compilation checked, awaiting grouped verification, automated tests passed, native verification passed, formally accepted, and blocked or deferred. Record implemented interfaces and behavior, introduced or reused infrastructure, tests written and still unrun, required automated and native checks, known defects and unfulfilled criteria, compilation and linking costs, and the next eligible batch. Keep evidence proportional and link the canonical receipt. Track implementation breadth separately from accepted GUI counts.

The Planner identifies the real source owner, high-leverage infrastructure, related GUI scope, batch sizes, and targeted evidence. The Implementer delivers coherent functionality and reusable components, preserves compatibility, writes focused tests, and keeps changes buildable with inexpensive checks. The Reviewer examines ownership, behavior, safety, and regression risk, and distinguishes blocking fixes from work for grouped refinement. The parent coordinates one source writer, Cargo and desktop ownership, evidence, commits, and best-effort pushes. Read-only work may run independently. Continue autonomously through the next eligible implementation until the original parity goal is complete or every remaining path has a genuine recorded blocker.
