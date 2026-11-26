# GPUI migration user priorities

This is the authority for user-defined migration scheduling on `feat/gpui-ui-parity`. Deliver a genuinely usable GPUI daily driver as early as possible by migrating frequently used workflows in breadth. Complete the original GUI-by-GUI parity program afterward, using the shared application engine and preserving egui compatibility, data integrity, verified behavior and all original acceptance criteria. G00–G26 retain their existing scope and owners.

## Implementation maturity

| Stage | Name | Evidence required |
| --- | --- | --- |
| 1 | Specified | Source implementation, authoritative engine, intended workflows and dependencies identified. Functional GPUI delivery is still outstanding. |
| 2 | Minimum Functional | An actual GPUI interface or command route runs the primary workflow against real data and the authoritative engine, with basic controls and errors. Relevant compilation and focused functional verification pass. Registration, placeholders, mocks, disabled routes and hidden egui fallback do not qualify. |
| 3 | Daily-Driver Functional | Primary workflows, common controls and input, configuration and persistence, essential cancellation, errors, lifecycle and Windows integration work. Focused automated evidence and essential native smoke evidence support ordinary use. |
| 4 | Feature Parity | Secondary flows, advanced controls, configuration, keyboard interactions, companion windows, edge cases and visual refinements are in place, with broader matched egui and GPUI verification. |
| 5 | Fully Accepted | Every applicable original functional, native, accessibility, performance, compatibility, lifecycle, regression and delivery gate is satisfied. |

Stages 1–4 are interim maturity. Formal acceptance continues in the original [parity matrix](PARITY_MATRIX.md).

## P0 · Daily-driver essentials

Target Stage 3 or higher. Use this preferred order for remaining work and retain working implementations.

| Order | Workflow | Current-pass scope |
| --- | --- | --- |
| 01 | Main Launcher and general Search | Reliable entry and focus, fuzzy ranking, ordinary actions, history and autocomplete. |
| 02 | Application Search, indexed and saved applications | Search and execute with arguments and working directory, index updates, native browsing, Add, Edit and Remove. |
| 03 | Folder Search, opening, management and aliases | Find and open existing folders, Add, Edit, Remove, aliases, watched persistence and compatibility. |
| 04 | Bookmark Search, opening, management and aliases | Find and open existing bookmarks, Add, Edit, Remove, aliases, watched persistence and compatibility. |
| 05 | Windows Explorer drag-and-drop | Drop applications, files and folders onto the launcher with the existing egui authoring and prefill behavior. |
| 06 | Omni Search | Combine available app, folder, bookmark, note, todo and calendar sources. Start with available sources and record each unavailable category explicitly. |
| 07 | Clipboard Modify | Load current clipboard, built-ins, pipelines, Preview, Apply, Copy, Undo, templates, saved pipelines and direct execution with checked acquisition and publication. |
| 08 | Clipboard Snippets | Search, Add, Edit, Remove and Run with parameters, live preview, exact Copy and supported execution, watched canonical persistence. |
| 09 | Notes, Quick Notes and essential Markdown | Browse and search existing notes, create, edit and save, Quick Notes, essential Markdown editing/viewing, keyboard and clipboard, safe dirty-state handling. Graphs, specialized templates and printing follow later. |
| 10 | MultiManager | P0. Inspect actual commands, settings, window-management operations and Windows integration, then deliver primary ordinary-use workflows. |
| 11 | Calculation Pad | Real workspace and stock language, variables, expressions, functions, live/manual evaluation, diagnostics, editing, Undo/Redo, Open/Save, Copy, common keys and layout/configuration. Advanced charts, sensitivity, dependencies and companions follow later. |
| 12 | Keyboard and numpad navigation | Keyboard-only navigation, physical numpad, Num Lock and Page navigation. |
| 13 | Global hotkey, visibility, focus and virtual desktops | Reliable invocation, show/hide, restoration on the current virtual desktop and Search focus. |
| 14 | Essential Settings, application authoring and plugin configuration | Canonical writes and live acknowledgement, ordinary authoring and safe frontend selection and recovery. |
| 15 | Google Search, general web and essential Search providers | Checked ordinary provider execution and results needed for normal use. |
| 16 | Temporary Files | Create/edit where supported, save, open, search and manage through canonical storage, association, aliases and configuration. |
| 17 | Timers and Alarms | Create and edit durations or times, active list, cancel/remove, completion, notification and persistence. Dashboard refinement can follow. |
| 18 | Ordinary Mouse Gestures | Normal gesture execution, authoring and configuration. Radial connectivity belongs to P4 and has no prerequisite role here. |
| 19 | Jump Mode | Common keyboard-driven target discovery and activation. |

## P1 · Frequently used functionality

Target Stage 3 after P0. Dashboard and Dashboard Editor lead P1. Establish reusable Dashboard infrastructure and prioritize widgets useful to P0 and P1 before exhaustive widget coverage.

1. Dashboard
2. Dashboard Editor
3. Frequently useful Dashboard widgets
4. File Search and Preview
5. Favorites
6. Window Search
7. Paste Sequences
8. MkMacro
9. Hash Generator, preserving existing advanced behavior
10. JSON Utility
11. XML Toolkit
12. Regex Tester
13. Screenshot workflows
14. Image Crop
15. Screen Color Picker
16. Inline Calculator and conversions

## P2 · General-purpose functionality

Target Stage 2 or Stage 3 in this order, grouping related work where appropriate.

1. Folder Size Analyzer
2. File Lock Inspector
3. Duplicate Finder
4. ZIP Extract and Create
5. Diff tools
6. Batch File Renamer
7. Search & Replace
8. Explorer Tabs
9. Downloads Triage
10. File Operations Center
11. Hot Folder
12. Calendar
13. Todo List
14. OCR
15. Screen Draw
16. Screenshot Pinboard
17. Other general-purpose plugins

## P3 · Lower-priority functionality

Initially target Stage 2 in this order.

1. Table Utility
2. CSV Viewer
3. Diagram Studio
4. Mermaid Viewer
5. Advanced conversion workspaces
6. Date & Time Studio
7. Password Generator
8. Screen Recorder
9. Image Comparison
10. Specialized system utilities
11. Less frequently used Dashboard widgets
12. Specialized plugin settings
13. Advanced companion dialogs and secondary workflows

## P4 · Lowest priority

Defer while higher priorities need functional coverage. Preserve existing implementation and retain the complete parity requirement for every item.

1. Clipboard History
2. Radial Menu runtime
3. Radial Menu Designer
4. Radial submenu management
5. Radial skin editor
6. Radial asset picker
7. Radial import and export
8. Radial authoring previews
9. Radial-specific Mouse Gesture integrations
10. Other Radial Menu secondary interfaces

Shared clipboard services required by higher-priority features follow their actual consumers. Ordinary Mouse Gestures remain P0 independently of Radial Menu runtime, design or integration.

## Scheduling and future revisions

Use preferred user order for remaining work. Concrete dependencies, correctness or essential architecture can justify limited resequencing. Record the reason and the higher-priority workflow it serves. Material changes to assignments require explicit user direction. Prior implementations and evidence carry forward. When a workflow meets its present target, record its evidence, gaps, deferred refinements and correctness issues, then move to the highest-priority appropriate incomplete workflow. Preserve safety, data integrity, essential persistence, cancellation, error handling, resource ownership, lifecycle and required native checks throughout.

Every startup, resumption, compaction, role assignment and next-batch decision follows the startup steps in the [Kickstart](../../CODEX_KICKSTART_GPUI_UI_PARITY.md). Maintain the [maturity register](PLUGIN_MATURITY_REGISTER.md), [daily-driver readiness](DAILY_DRIVER_READINESS.md) and original [parity matrix](PARITY_MATRIX.md) alongside the [execution strategy](EXECUTION_STRATEGY.md). Incorporate future explicit usage changes here, date the revision, and reconcile the registers and schedule.

## Revision history

| Date | User revision | Scheduling effect |
| --- | --- | --- |
| 2026-11-26 | Established usage priorities, five maturity stages, and the distinct Daily-Driver Readiness milestone. | Finish the active read-only Duplicate Finder increment, then move to Pass A and the highest appropriate incomplete P0 work. Preserve G00–G26 and final acceptance. |
