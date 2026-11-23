# GUI source registration crosswalk

Source checked at `98bbc0ccc22a297e8eafae7e844c2f7e45854856`. Stable GUI IDs link to owners and production routes in [GUI inventory](GUI_INVENTORY.md). These are source registration counts. Several sources refer to the same surface, and some embedded or independently hosted surfaces have no entry in these registries.

## Panel (77 registrations)

`src/gui/mod.rs::Panel` includes root-pass, utility-host, organizer and authoring routes. Some variants represent multiple runtime instances, such as per-note or per-image views. Recording maps to its setup surface, with the controller tracked under G19.

| Variant | GUI ID |
| --- | --- |
| `AliasDialog` | `alias-editor` |
| `BookmarkAliasDialog` | `bookmark-alias-editor` |
| `TempfileAliasDialog` | `tempfile-alias-editor` |
| `TempfileDialog` | `create-tempfile` |
| `AddBookmarkDialog` | `add-bookmark` |
| `HelpOverlay` | `quick-help` |
| `HelpWindow` | `command-help` |
| `TimerDialog` | `timer-manager` |
| `CompletionDialog` | `timer-completion` |
| `ShellCmdDialog` | `shell-command-editor` |
| `SnippetDialog` | `snippets` |
| `PasteSequenceDialog` | `paste-sequences` |
| `HashGeneratorDialog` | `hash-generator` |
| `PasswordGeneratorDialog` | `password-generator` |
| `FileOperationsCenterDialog` | `file-operations-center` |
| `DownloadsTriageDialog` | `downloads-triage` |
| `NotificationInboxDialog` | `notification-inbox` |
| `SnippetPromptDialog` | `snippet-prompt` |
| `MacroDialog` | `legacy-macros` |
| `MkMacroDialog` | `mkmacro` |
| `MouseGesturesDialog` | `mouse-gestures` |
| `MouseGestureSettingsDialog` | `mouse-gesture-settings` |
| `MouseSettingsDialog` | `mouse-settings` |
| `ThemeSettingsDialog` | `theme-settings` |
| `HotkeyDirectoryDialog` | `hotkey-directory` |
| `FavDialog` | `favorites-editor` |
| `FileSearchDialog` | `file-search` |
| `FileSearchPreviewDialog` | `file-preview` |
| `NotesDialog` | `quick-notes` |
| `NoteGraphDialog` | `note-graph` |
| `UnusedAssetsDialog` | `unused-note-assets` |
| `NotePanel` | `note-editor` |
| `ImagePanel` | `image-viewer` |
| `ScreenshotEditor` | `screenshot-editor` |
| `CropDialog` | `image-crop-editor` |
| `TodoDialog` | `todo-list` |
| `TodoViewDialog` | `todo-view` |
| `ClipboardDialog` | `clipboard-history` |
| `ClipboardModifyDialog` | `clipboard-modify` |
| `JsonUtilityDialog` | `json-utility` |
| `ZipExtractorDialog` | `zip-extractor` |
| `RegexTesterDialog` | `regex-tester` |
| `CalculationPadDialog` | `calculation-pad` |
| `ProgrammerInspectorDialog` | `programmer-inspector` |
| `CalculationDependencyDialog` | `calculation-pad-dependencies` |
| `CalculationChartDialog` | `calculation-pad-charts` |
| `CalculationSensitivityDialog` | `calculation-pad-sensitivity` |
| `ExplorerTabsDialog` | `explorer-tabs` |
| `DateTimeStudioDialog` | `date-time-studio` |
| `DiagramStudioDialog` | `diagram-studio` |
| `MarkdownStudioDialog` | `markdown-studio` |
| `BatchRenameDialog` | `batch-rename` |
| `SearchReplaceDialog` | `search-replace` |
| `XmlToolkitDialog` | `xml-toolkit` |
| `QrDialog` | `qr-generator` |
| `ConvertPanel` | `convert` |
| `VolumeDialog` | `volume` |
| `BrightnessDialog` | `brightness` |
| `CpuListDialog` | `cpu-list` |
| `DataRecoveryDialog` | `data-recovery` |
| `ToastLogDialog` | `toast-history` |
| `CalendarPopover` | `calendar` |
| `CalendarEventEditor` | `calendar-event-editor` |
| `CalendarEventDetails` | `calendar-event-details` |
| `Editor` | `actions-editor` |
| `Settings` | `settings-main` |
| `Plugins` | `plugin-editor` |
| `MultiManagerDialog` | `multi-manager` |
| `MultiManagerSettingsDialog` | `multi-manager-settings` |
| `RadialEditor` | `radial-designer` |
| `TableUtilityDialog` | `table-utility` |
| `ImageCompareDialog` | `image-comparison` |
| `DuplicateFinderDialog` | `duplicate-finder` |
| `FolderSizeDialog` | `folder-size` |
| `FileLockDialog` | `file-lock` |
| `ActivityDialog` | `activity-center` |
| `RecordingDialog` | `screen-recorder-setup` |

## Jump Mode (60 registrations)

`src/gui/jump_mode/ui.rs::NavigationDialogKind::ALL` maps each kind through `surface_kind`. Use the inventory ID on the right when it differs from the source key. Confirmation and popup ownership remains in `NavigationOwner` and each owning GUI criterion.

| Kind | Source key | GUI ID |
| --- | --- | --- |
| `FileSearch` | `file-search` | `file-search` |
| `Snippets` | `snippets` | `snippets` |
| `ClipboardHistory` | `clipboard-history` | `clipboard-history` |
| `JsonUtility` | `json-utility` | `json-utility` |
| `RegexTester` | `regex-tester` | `regex-tester` |
| `XmlToolkit` | `xml-toolkit` | `xml-toolkit` |
| `MouseSettings` | `mouse-settings` | `mouse-settings` |
| `MouseGestures` | `mouse-gestures` | `mouse-gestures` |
| `BatchRename` | `batch-rename` | `batch-rename` |
| `SearchReplace` | `search-replace` | `search-replace` |
| `DashboardEditor` | `dashboard-editor` | `dashboard-editor` |
| `RadialDesigner` | `radial-designer` | `radial-designer` |
| `MouseGestureSettings` | `mouse-gesture-settings` | `mouse-gesture-settings` |
| `HotkeyDirectory` | `hotkey-directory` | `hotkey-directory` |
| `TableUtility` | `table-utility` | `table-utility` |
| `ImageCompare` | `image-compare` | `image-comparison` |
| `DuplicateFinder` | `duplicate-finder` | `duplicate-finder` |
| `Activity` | `activity` | `activity-center` |
| `ZipExtractor` | `zip-extractor` | `zip-extractor` |
| `ClipboardModify` | `clipboard-modify` | `clipboard-modify` |
| `CalculationChart` | `calculation-chart` | `calculation-pad-charts` |
| `MarkdownPrint` | `markdown-print` | `markdown-print` |
| `PasswordGenerator` | `password-generator` | `password-generator` |
| `FileOperations` | `file-operations-center` | `file-operations-center` |
| `DownloadsTriage` | `downloads-triage` | `downloads-triage` |
| `NotificationInbox` | `notification-inbox` | `notification-inbox` |
| `FilePreview` | `file-preview` | `file-preview` |
| `HashGenerator` | `hash-generator` | `hash-generator` |
| `QrGenerator` | `qr-generator` | `qr-generator` |
| `ExplorerTabs` | `explorer-tabs` | `explorer-tabs` |
| `Convert` | `convert-panel` | `convert` |
| `FileLock` | `file-lock` | `file-lock` |
| `FolderSize` | `folder-size` | `folder-size` |
| `DateTimeStudio` | `date-time-studio` | `date-time-studio` |
| `Calendar` | `calendar` | `calendar` |
| `CalendarDetails` | `calendar-event-details` | `calendar-event-details` |
| `CalendarEditor` | `calendar-event-editor` | `calendar-event-editor` |
| `HotFolder` | `hotfolder-manager` | `hot-folder-manager` |
| `DataRecovery` | `data-recovery` | `data-recovery` |
| `DataRecoveryConfirmation` | `data-recovery-confirmation` | `data-recovery-confirmation` |
| `NoteTemplates` | `note-templates` | `note-templates` |
| `TemplateDelete` | `note-template-delete` | `note-template-delete` |
| `UnusedNoteAssets` | `unused-note-assets` | `unused-note-assets` |
| `ImageViewer` | `image-viewer` | `image-viewer` |
| `CalculationDependencies` | `calculation-dependencies` | `calculation-pad-dependencies` |
| `CalculationSensitivity` | `calculation-sensitivity` | `calculation-pad-sensitivity` |
| `ProgrammerInspector` | `programmer-inspector` | `programmer-inspector` |
| `NotePanel` | `note-panel` | `note-editor` |
| `NoteFormTable` | `note-custom-table-builder` | `note-custom-table-builder` |
| `NoteFormLink` | `note-insert-link` | `note-insert-link` |
| `NoteFormNewNote` | `note-link-new` | `note-link-new` |
| `NoteClosePrompt` | `note-close-prompt` | `note-close-decision` |
| `NoteConflictPrompt` | `note-conflict-prompt` | `note-save-conflict` |
| `NotePlainLinkPrompt` | `note-plain-link-prompt` | `note-plain-link-conversion` |
| `MarkdownStudio` | `markdown-studio` | `markdown-studio` |
| `CalculationPad` | `calculation-pad` | `calculation-pad` |
| `CalculationReference` | `calculation-reference` | `calculation-pad-reference` |
| `DiagramStudio` | `diagram-studio` | `diagram-studio` |
| `DiagramViewer` | `diagram-viewer` | `diagram-viewer` |
| `PluginSettings` | `plugin-settings` | `plugin-editor` |

## Utility geometry (110 keys)

`src/settings/utility_dialog_sizes.rs::STABLE_TOOL_KEYS` is the persisted key set. The following table lists differing identities. Every other key matches an inventory ID directly. Preserve the existing persisted keys when porting.

| Persisted key | GUI ID or disposition |
| --- | --- |
| `action-sheet` | `universal-action-sheet` |
| `activity` | `activity-center` |
| `bookmark-alias` | `bookmark-alias-editor` |
| `cpu-usage` | `cpu-list` |
| `custom-apps` | `actions-editor` |
| `favorites` | `favorites-editor` |
| `folder-alias` | `alias-editor` |
| `gesture-binding-editor` | `mouse-gesture-bind-action` |
| `gesture-settings` | `mouse-gesture-settings` |
| `gesture-settings-window-picker` | `mouse-gesture-window-picker` |
| `hotfolder-manager` | `hot-folder-manager` |
| `mkmacro-parameters` | `mkmacro-parameter-prompt` |
| `screen-recording` | `screen-recorder-setup` |
| `ocr-result` | `ocr-results` |
| `plugin-settings` | `plugin-editor` |
| `screenshot-pin-list` | `pinboard` |
| `settings` | `settings-main` |
| `shell-commands` | `shell-command-editor` |
| `tempfile-alias` | `tempfile-alias-editor` |
| `timer` | `timer-manager` |
| `toast-log` | `toast-history` |
| `todos` | `todo-list` |
| `unicode-inspector` | Reserved. No production GUI route. |

## Built-in plugin routes (91 Windows registrations)

`src/plugin.rs::PluginManager::reload_from_dirs` registers the following providers on Windows. G05 owns the common launcher projection and activation of result rows. The feature goal in the table owns any specialized settings, target view, operation or external handoff. A provider can contribute a result row without opening a standalone GUI. The [inventory](GUI_INVENTORY.md) identifies each dedicated surface and settings callback.

| Owner | Registered provider types | Disposition |
| --- | --- | --- |
| G04 | `HashGeneratorPlugin` | Hash Generator. |
| G05 | `WebSearchPlugin`, `RunescapeSearchPlugin`, `YoutubePlugin`, `RedditPlugin`, `WikipediaPlugin`, `OmniSearchPlugin`, `HistoryPlugin`, `AsciiArtPlugin`, `EmojiPlugin`, `TextCasePlugin`, `LoremPlugin`, `HelpPlugin` | Root result rows, generated text, web handoffs, history and help. |
| G06 | `BookmarksPlugin`, `FoldersPlugin`, `FavPlugin`, `MissingPlugin`, `DataPlugin`, `SettingsPlugin` | Settings, authoring, saved-target repair and recovery routes. |
| G07 | `FileSearchPlugin` | File Search and preview route. |
| G08 | `DiffPlugin` | Diff modes and child routes. |
| G09 | `BatchRenamePlugin`, `SearchReplacePlugin` | Rename and replacement workspaces. |
| G10 | `ZipExtractorPlugin`, `DuplicateFinderPlugin`, `FolderSizePlugin`, `FileLockPlugin` | Archive, scan and lock tools. |
| G11 | `ExplorerTabsPlugin`, `FileOperationsPlugin`, `DownloadsTriagePlugin` | Explorer workflows and Windows-only transfer/triage registrations. |
| G12 | `NotePlugin`, `MarkdownStudioPlugin`, `LinkPlugin` | Notes, Markdown and link resolution. Todo destinations remain G23. |
| G13 | `ClipboardPlugin`, `ClipboardModifyPlugin`, `SnippetsPlugin`, `TempfilePlugin` | Clipboard, snippets and temporary-file content. |
| G14 | `JsonUtilityPlugin`, `XmlToolkitPlugin`, `RegexTesterPlugin` | Structured-text workspaces. |
| G15 | `CalculatorPlugin`, `DropCalcPlugin` | Inline calculation behavior and the Calculation Pad. |
| G16 | `UnitConvertPlugin`, `BaseConvertPlugin`, `TimestampPlugin`, `DateArithmeticPlugin`, `RandomPlugin`, `ConvertPanelPlugin`, `TableUtilityPlugin`, `CsvPlugin` | Inline and hosted conversion, dates, table profiles and password generation. |
| G17 | `DiagramStudioPlugin` | Diagram Studio and viewer. |
| G18 | `ScreenshotPlugin`, `CropPlugin`, `ImageComparePlugin`, `QrPlugin`, `ColorPickerPlugin` | Capture, image tools, QR and color. |
| G19 | `MacrosPlugin`, `MkMacroPlugin`, `RecordingPlugin` | Legacy macros, MkMacro and recorder. |
| G20 | `RadialPlugin` | Radial run and authoring handoffs. |
| G21 | `PasteSequencePlugin`, `MouseGesturesPlugin`, `HotFolderPlugin` | Editor routes, runtime states and underlying automation. |
| G22 | `PinboardPlugin`, `ScreenDrawPlugin`, `MousePlugin`, `OcrPlugin` | Native capture/control tools, Mouse Settings and overlays. |
| G23 | `TodoPlugin`, `CalendarPlugin`, `NotificationInboxPlugin`, `ActivityPlugin`, `TimerPlugin`, `StopwatchPlugin`, `WeatherPlugin` | Organizers, status surfaces and dashboard integrations. |
| G24 | `SystemPlugin`, `ProcessesPlugin`, `SysInfoPlugin`, `NetworkPlugin`, `ShellPlugin`, `KeysPlugin`, `MultiManagerPlugin`, `RecyclePlugin`, `MediaPlugin`, `IpPlugin`, `VolumePlugin`, `BrightnessPlugin`, `TaskManagerPlugin`, `WindowsPlugin`, `VirtualDesktopPlugin`, `BrowserTabsPlugin`, `LayoutPlugin` | System results, native controls and established external handoffs. |

The same reload path uses `load_dir` for configured directories. G06 owns supported dynamic plugin settings capability and lifetime. Preserve built-in, registered and dynamic origin identities. The runtime enumeration must be repeated against an actual supported dynamic library before accepting that route.

## Separately constructed Windows mouse surfaces

| Native constructor | Source disposition |
| --- | --- |
| `mouse_gestures/overlay.rs::HintOverlaySurface` | G21 [Mouse Gesture Hint Overlay](gui/mouse-gesture-hint-overlay.md) |
| `mouse_gestures/overlay.rs::TrailOverlaySurface` | G21 [Mouse Gesture Trail Overlay](gui/mouse-gesture-trail-overlay.md) |
| `coordinate_tool/native.rs::WindowsSurfaceBackend` HUD `LayeredSurface` | G22 [Coordinate overlay](gui/coordinate-overlay.md) |
| `coordinate_tool/native.rs::WindowsSurfaceBackend` crosshair, guide and click `LayeredSurface` instances, and `WindowsEffectOperations` halo and zoom hosts and children | G22 [Mouse Effects Overlays](gui/mouse-effects-overlay.md) |
