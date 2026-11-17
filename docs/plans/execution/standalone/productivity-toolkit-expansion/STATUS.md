# Productivity Toolkit Expansion

| Git identity | Value |
| --- | --- |
| parent_branch | feat/calc-pad-dynamic-vectors |
| parent_tip_sha | a9de4ab4be045bdadd27a910425d51dd1c6f0779 |
| Task branch | feat/productivity-toolkit-expansion |
| baseline_sha | 768fbe9a93268f21509c89fe55f54b96c909f8ad |
| Upstream | origin/feat/productivity-toolkit-expansion |

M0-A verified the direct parent and clean checkout. The intentional baseline adopted the two approved Toolkit documents from a89a1e6cd. Initial push succeeded with exit 0. The single existing checkout was used throughout.

| Checkpoint | State |
| --- | --- |
| M0-A | complete |
| M0-B | complete |
| M1-A | pending |
| M2-A, M2-B | pending |
| M3-A, M3-B, M3-C | pending |
| M4-A, M4-B, M4-C | pending |
| M5-A, M5-B | pending |
| M6-A, M6-B | pending |
| M7-A, M7-B | pending |
| M8-A, M8-B | pending |
| M9-A, M9-B, M9-C, M9-D | pending |
| M10-A, M10-B | pending |
| M11-A, M11-B, M11-C, M11-D | pending |
| M12-A, M12-B | pending |
| M13-A | pending |

## M0-B platform decisions

- PDF uses a shared point-based page model with positioned text, graphics, links, and resolved local assets. Use a single Rust PDF writer with embedded Unicode font subsets and annotations (Krilla 0.8 is the selected candidate), and retain the existing Markdown parser and asset permission checks. Choose redistributable font assets and disclose missing glyphs. Render representative exports with Poppler and inspect page size, page count, extracted text and links.
- Print uses the Win32 PrintDlgExW printer context and GDI's StartDoc, StartPage, EndPage, EndDoc and AbortDoc sequence, driven by the same page model. Validate printable bounds and report scaling or hard-margin differences. Use Microsoft Print to PDF as a disposable submission target. The spooler is running, and the printer is visible when queried outside the sandbox. No print job was submitted during setup.
- External notifications use UserNotificationListener with the manifest's uap3:userNotificationListener capability. Probe the optional signed external-location identity route first with a single foreground consent call. The process has to hold the expected package identity, consent has to run on the UI thread, and access must be checked before and after toast snapshot reads. A registered package alone does not validate the consent host. If that host fails, the supported fallback is an optional UWP foreground listener with same-session, package-SID-scoped local IPC. Keep any third-party text off disk by default.
- Windows SDK 10.0.26100 MakeAppx and SignTool are installed. No matching MultiLauncher package was found by Windows PowerShell, and no existing current-user signing certificate was reported. Packaging/consent proof remains unverified until a trusted signed test package can be registered in an isolated test context. No trust or system policy was changed. Ordinary builds must stay independent of these tools.

Sources: [notification listener](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/notification-listener), [manifest capability](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-uap3-capability-manual), [optional package identity](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps), [UWP/Win32 IPC](https://learn.microsoft.com/windows/uwp/communication/interprocess-communication), [printer context](https://learn.microsoft.com/en-us/windows/win32/printdocs/printer-output), [GDI print API](https://learn.microsoft.com/en-us/windows/win32/printdocs/gdi-print-api-functions), [Krilla API](https://docs.rs/krilla/0.8.2/krilla/).

## M0-B initial bounds

Use existing Pad/vector/archive and Markdown source and asset limits. New tools start with these caps and report saturation explicitly.

- PDF: 256 pages, 64 MiB serialized output and 64 MiB decoded assets per render, with checked dimensions and cancellation during layout.
- Transfer: 32 active or queued jobs, 10,000 entries per reviewed job, 128 directory levels, 32,768 UTF-16 units per Windows path and 32 MiB of receipt metadata. Keep unresolved recovery records when compacting completed history. Stream contents independently of metadata limits.
- Downloads: 10,000 direct children per explicit scan, 10,000 reviewed-state entries, 16 KiB per origin record. Surface truncated scans and read errors.
- Inbox: 7 days and 1,000 entries by default, settings bounded to 1–365 days and 100–10,000 items, including pins. Cap external snapshot payloads to 512 entries and 1 MiB, and cap retained title/body lengths. Publish terminal/attention events once and disclose truncation.
- Secrets: 8–256 password characters or 3–32 words, with at most 1,024 UTF-8 bytes per transient result.

## M0-B source reconciliation

- Pad: calculation_pad/document.rs and loops.rs both prepare/evaluate through script.rs. Extend the Function catalog and vector.rs CollectionOperation, including collection-failure rollback and the separate ordinary and vector fuel. Named callbacks must receive a distinct source role in rename.rs and syntax.rs, with real definition origins surfaced by DependencySnapshot. Examples & Reference retains CalculationExample; explicit insertion uses the existing source-edit transaction. ChartDefinition/ChartDataset/ChartScene in chart.rs, persisted view validation and Pad session rename/undo share the existing ChartRuntime/ChartActions and its current evaluation stamp.
- Markdown: parse_headings currently recognizes ATX headings. Section ranges need actual heading-end offsets for setext. NotePanel uses apply_prepared_markdown_edit_with_selection and Studio uses apply_prepared. Studio needs its first outline adapter. Shared parsing and export belong in notes_markdown with the existing Markdown export asset resolver and publication rules.
- ZIP: ZipExtractorDialog fences picker results by host, mode and draft revision. Advance destination through the real draft and ArchiveSession. The archive destination currently requires an existing held directory. Admit an absent exact child with a held parent for preview, then create it exclusively in the confirmed writer after validation. Preserve no-clobber and token fencing.
- Secrets: plugins/random.rs currently publishes a clipboard action string. Use typed ephemeral invocation ahead of gui/actions.rs record_history_usage and history.rs. ClipboardPlugin update_history and Clipboard Modify snapshots both need native same-clipboard-item exclusion.
- Files and inbox: file_operations.rs provides native_file_identity, guarded access/publication and cancellable scanning. File Search's SelectedFileSearchResult provides exact selections. Transfers own approvals and execution, publish monitored ActivityTask outcomes and issue terminal events to the inbox. Preserve timer completion identity at the timer owner, use calendar and Todo domain adapters, and integrate sanitized launcher toasts, the macro notification backend and Hot Folder terminal events.
- Keyboard M12-A: existing Jump registrations cover File Search, Batch Rename, Search/Replace, JSON/XML/Table, Duplicate Finder, Image Compare, ZIP, Clipboard History/Modify, Activity, Snippets, Regex, mouse forms, Hotkey Directory, Dashboard and Radial. Specialized owners include settings, timer, Todo, Paste Sequences and macro. Audit their remaining ordinary controls and add coverage to File Search Preview, Hash, QR, File Lock, Folder Size, Explorer Tabs, Date/Time, Convert, Hot Folder, Data & Recovery and Calendar.
- Keyboard M12-B: audit Notes/NotePanel and its Template, Unused Assets and Image Panel companions, Markdown Studio, Pad/Reference/Dependencies/Chart/Sensitivity, Programmer Inspector, Diagram, Plugin Editor and the newly introduced dialogs. Ordinary controls surrounding canvases are in scope. Native picker internals and canvas pan/draw/resize gestures use their existing input ownership.
- Source differences from the planning reference include minor ZIP and archive lint corrections. The exact parent has an inherited clean-build defect: drawio.rs has a test-time include of a missing archived fixture. Master already repairs it with a source-adjacent fixture. Adopt that narrow test fixture repair before library Nextest, independently of feature code.

No application build was run in M0-B. The Nextest binary is 0.9.135 and the active Rust toolchain is 1.99.1.
