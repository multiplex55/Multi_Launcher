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
