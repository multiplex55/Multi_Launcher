# Goal register

Status: planning package prepared; implementation has not started. All work goals are pending execution and acceptance. Read the master Goal for shared contracts and the packet for concrete checkpoints.

The initial program has 27 goals. All remain on **one migration branch**, with one active source writer. Ordering below is a default, not a requirement to block independent work behind an unavailable unrelated native check. Record any resequencing and preserve dependency/evidence ownership.

| Goal | Outcome and packet | Dependencies | Current stage |
| --- | --- | --- | --- |
| G00 | [Source baseline, complete discovery, and execution records](goals/G00-baseline-and-inventory.md) | None | Not started |
| G01 | [Same-executable GPUI Kit and Windows feasibility](goals/G01-windows-and-kit-feasibility.md) | G00 initial source/branch checkpoint | Not started |
| G02 | [Shared commands, state, services, and adapters](goals/G02-shared-engine-boundaries.md) | G00; use G01 findings | Not started |
| G03 | [Frontend selection, instance ownership, restart, and data identity](goals/G03-frontend-selection-and-lifecycle.md) | G01, G02 relevant seams | Not started |
| G04 | [GPUI design system and complete Hash Generator pilot](goals/G04-design-system-and-pilot.md) | G01–G03 relevant contracts | Not started |
| G05 | [Main launcher, search, navigation, Jump Mode, and universal actions](goals/G05-main-launcher.md) | G00–G04 | Not started |
| G06 | [Settings, plugin configuration, authoring, and recovery](goals/G06-settings-and-authoring.md) | G00–G05 | Not started |
| G07 | [File Search, previews, filters, and window modes](goals/G07-file-search.md) | G00–G05 | Not started |
| G08 | [Text, binary, and folder Diff with all child flows](goals/G08-diff.md) | G00–G05; G07 for search handoffs | Not started |
| G09 | [Batch Rename and Search & Replace](goals/G09-rename-and-replace.md) | G00–G05; shared file contracts | Not started |
| G10 | [ZIP, duplicate finding, folder size, and file locks](goals/G10-file-utilities.md) | G00–G05; G04 owns Hash Generator | Not started |
| G11 | [File Operations Center, Downloads Triage, and Explorer Tabs](goals/G11-file-workflows.md) | G00–G05; relevant G07–G10 handoffs | Not started |
| G12 | [Notes, Markdown Studio, links, export, and printing](goals/G12-notes-and-markdown.md) | G00–G05 | Not started |
| G13 | [Clipboard history, Clipboard Modify, snippets, and prompts](goals/G13-clipboard-and-snippets.md) | G00–G05 | Not started |
| G14 | [JSON, XML, Regex, and structured text tooling](goals/G14-structured-text.md) | G00–G05; G13 where clipboard handoffs apply | Not started |
| G15 | [Calculation Pad, editors, reference, and companion windows](goals/G15-calculation-pad.md) | G00–G05 | Not started |
| G16 | [Table Utility, date/time, and conversion interfaces](goals/G16-tables-and-conversion.md) | G00–G05; G15 only for shared chart/data handoffs | Not started |
| G17 | [Diagram Studio, visual/Mermaid editing, and child tools](goals/G17-diagram-studio.md) | G00–G05 | Not started |
| G18 | [Screenshot editor, crop, comparison, colors, and QR](goals/G18-images-and-screenshots.md) | G00–G05; G01 native capability findings | Not started |
| G19 | [MkMacro, authoring children, macro tools, and recording](goals/G19-mkmacro-and-recording.md) | G00–G05; reuse relevant G18/G22 capabilities | Not started |
| G20 | [Radial runtime, designer, preview, and assets](goals/G20-radial-interfaces.md) | G00–G05; G01 native capability findings | Not started |
| G21 | [Paste Sequences, gestures, and Hot Folder](goals/G21-automation-tools.md) | G00–G05; G13 clipboard contracts | Not started |
| G22 | [Pinboard, Screen Draw, OCR, and coordinate surfaces](goals/G22-native-overlays.md) | G00–G05; native proof started in G01 | Not started |
| G23 | [Dashboard, calendar, todos, timers, notifications, and activity](goals/G23-dashboard-and-organizers.md) | G00–G05; relevant feature adapters | Not started |
| G24 | [MultiManager, layouts, processes, shell, and system controls](goals/G24-system-tools.md) | G00–G05; G01 native integration | Not started |
| G25 | [Cross-feature integration and exhaustive parity reconciliation](goals/G25-integration-and-parity-reconciliation.md) | Applicable feature goals; incremental checks begin earlier | Not started |
| G26 | [Performance, production packaging, final review, and delivery](goals/G26-final-acceptance-and-delivery.md) | G00–G25 applicable acceptance | Not started |


## Rules for activating a goal

Read its packet and actual source; reconcile moved paths/partially existing work. Create a bounded checkpoint packet using the template. Assign every GUI and child to a single evidence owner. Do not implement only the easiest examples in the packet.

G00 discovery expands the inventory and can split a large goal into recorded child checkpoints/goals without asking routine permission. Scope may grow to account for baseline interfaces, not unrelated product features. Goals can execute overlapping **read-only** discovery; overlapping source writes remain sequential.

## Shared and feature settings ownership

G06 owns the global settings host, navigation, registry integration, common save/apply/cancel behavior and common configuration surfaces. The corresponding feature goal owns each specialized plugin/settings control and its child interfaces. G23 similarly owns dashboard-widget settings through the registry.

G06's host can be accepted with later specialized implementations explicitly pending in their owner goals; the entire settings experience cannot be declared complete until G25 reconciles those rows. Avoid a circular requirement that all later features must already be finished to implement G06.

G04 exclusively owns the Hash Generator pilot. G10 links to it for file workflows, without counting a second port. G16 owns Password Generator. G06 owns shell-command authoring; G24 owns system/shell execution integration. G06 owns temporary-file alias configuration; G13 owns temporary-file contents. Source discovery may refine boundaries, but every moved row keeps an accountable owner and evidence.

## Acceptance and continuation

A source/code checkpoint may finish before all native evidence exists, if the stage accurately remains Validation pending. A goal is Accepted only when all applicable requirements assigned to it are accepted or a clearly recorded scope exception changes the claim.

After an accepted checkpoint, continue immediately to its next eligible checkpoint or goal. Unavailable push is not a blocker. An unavailable platform check blocks its claim, not all unrelated work.

At final reconciliation, no new or awkward surface may be assigned to an ownerless “later” bucket. G25 accounts for every inventory row; G26 delivers the whole program.
