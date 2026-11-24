//! Renderer-neutral copy and filtering for the Quick Help guide.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideBlock {
    Heading(&'static str),
    Paragraph(&'static str),
    WrappedParagraph(&'static str),
    Monospace(&'static str),
    Separator,
    Hotkeys,
    PasteSequenceSummary,
    SequenceStarts,
    Providers,
}

pub const QUICK_HELP_GUIDE: &[GuideBlock] = &[
    GuideBlock::Heading("Hotkeys"),
    GuideBlock::Hotkeys,
    GuideBlock::Heading("Hotkey Directory"),
    GuideBlock::Paragraph("Open Settings > Hotkeys > Hotkey Directory to search configured shortcuts and actions, such as End, paste, or macro. Active means the runtime acknowledged the route; Pending is still being applied, and Unknown means no reliable acknowledgement is available. Other statuses include Disabled, Invalid, Conflict, Unsupported, Omitted by precedence, and Unavailable. Conflict details name competing routes. Not configurable entries have no current editor. Edit opens the owning settings or editor, where its existing Save and Cancel controls apply."),
    GuideBlock::Paragraph("In Settings, Search settings finds existing controls. Choose a result to reveal and focus that control; search does not change or save it, and it does not enable unavailable controls. Use the owning section's normal Save, Apply, or Cancel controls."),
    GuideBlock::Heading("Saved target repair"),
    GuideBlock::Paragraph("Type missing and choose the exact check missing command to inspect saved targets. From a saved custom app or folder result, choose Repair target… to edit a replacement path and confirm it. Custom apps can use a file/application or folder; saved folders require a folder. Choosing a path does not launch it. Unavailable or ambiguous saved records are reported without changing them."),
    GuideBlock::Heading("Table Utility"),
    GuideBlock::Paragraph("Type table for the general viewer or csv for the strict CSV viewer; opened files are never modified and clipboard reads happen only after Paste Text. Table defaults to Auto delimiter, headers off, lenient row widths, and numeric sorting when all values are finite (10 MiB, 50,000 rows, 128 columns, 48 MiB of cells, 16 MiB per output). CSV defaults to comma, headers on, strict widths, and Unicode-lowercase lexical sort (16 MiB, 100,000 rows, 256 columns, 64 MiB of cells, 64 MiB per output); both profiles share a one-million-cell limit. Cell values remain strings, preserving leading zeroes, large identifiers, and whitespace. Equal-sort ties retain original row order. Opening .csv/.tsv selects comma/tab, while other extensions and pasted text use the visible delimiter. Filter is case-insensitive literal text. Copy CSV/TSV, JSON, and Save JSON use selected visible rows and source-order columns. JSON Auto explains when it falls back to arrays for missing, blank, or duplicate headers. After a failed CSV reparse, Copy Previous Valid CSV explicitly exports the last successful options once reads/parses finish. After a failed table edit, the last successful data remains visible but outputs stay disabled until the current source parses successfully. Large sources retain full text and show a bounded preview."),
    GuideBlock::Heading("Image Comparison"),
    GuideBlock::Paragraph("Type imgdiff to compare two session-only images. Open A… and Open B… choose PNG, JPEG, BMP, or GIF files (16,777,216 pixels per image maximum); Paste A and Paste B read bounded CF_DIB images only when selected. A failed replacement keeps the last accepted image. A, B, Wipe, and Difference show top-left-aligned images without resizing; the exact RGBA comparison includes alpha and transparent-pixel RGB values. Inputs and results are cleared when the dialog closes."),
    GuideBlock::Heading("Duplicate Finder"),
    GuideBlock::Paragraph("Type dff to open Duplicate Finder, select folders, and explicitly start a read-only scan. It is bounded to 32 roots, 250,000 files, 350,000 entries, 100,000 directories, depth 256, and 64 MiB of path metadata. Reparse points and cloud placeholders are excluded. Same-sized files are checked with streamed hashes and exact byte comparison; hard-link aliases count as one file identity. Matches without reliable identity are separated from verified totals. Open, Reveal, and Copy Paths revalidate each exact displayed alias; logical duplicate bytes are not guaranteed recoverable disk space. No files are changed."),
    GuideBlock::Paragraph("Older query: Ctrl+Up"),
    GuideBlock::Paragraph("Newer query / restore draft: Ctrl+Down"),
    GuideBlock::Heading("Jump Mode"),
    GuideBlock::Paragraph("Jump Mode is off by default. Enable Jump Mode in Settings and save. On supported screens, Shift+Space shows hints for visible, enabled controls. Typing a hint only focuses or highlights its control; a later, separate Enter, Space, or normal typing interacts with it. Escape cancels hints. Existing Save, Apply, and confirmation steps still require their normal action."),
    GuideBlock::Paragraph("Coverage includes File Search; Snippets; Clipboard History; Dashboard commands and simple widgets; JSON, Regex, and XML tools; Mouse Settings and Gestures; Rename and Replace; and Dashboard and Radial editors. Pointer-only canvas drags, capture or recording, native picker internals, unknown popups, and hidden, clipped, or disabled controls are not hint targets."),
    GuideBlock::Separator,
    GuideBlock::Heading("Launcher search"),
    GuideBlock::Paragraph("Search applications, indexed files and enabled providers in one result list. Use app <query> for the action catalog or a plugin prefix such as fs, note, o, cs, paste or replace for its commands."),
    GuideBlock::Paragraph("Empty queries can show the dashboard. Pinned queries and Omni Search provider controls remain available. File Search uses its own Global/Directory search roots."),
    GuideBlock::Paragraph("Inline result descriptions show up to seven words and can shorten further to fit. The full action and its existing help and details remain available."),
    GuideBlock::Paragraph("Ordinary utility windows remember Dock or Pop Out across close and restart without reopening automatically. Children inherit a popped-out parent; direct and docked-parent opens use the child's preference. Reopening a live tool preserves its draft."),
    GuideBlock::Separator,
    GuideBlock::Heading("File Search"),
    GuideBlock::Paragraph("Clear results cancels the current search and clears rows, selection, preview, refinement and diagnostics. Query, roots, filters and sort remain; press Search to run again. Filename contains matches a literal contiguous part of the leaf name, including its extension."),
    GuideBlock::Paragraph("Filters includes Date Modified and File Size. Changes take effect when you press Search. The content-reading size limit remains separate from the File Size filter. When a filtered filename search would otherwise use Everything, it uses WalkDir instead and may be slower."),
    GuideBlock::Separator,
    GuideBlock::Heading("Calculation Pad"),
    GuideBlock::Paragraph("Dependencies… opens a read-only explorer of current Pad rows and their upstream inputs and downstream dependents. Select a relationship or use Go to source to navigate. Inspect bits… offers labels-only Import labels… and Export labels…; these do not change Pad source or working bits."),
    GuideBlock::Separator,
    GuideBlock::Heading("Folder Size"),
    GuideBlock::Paragraph("Table, Treemap and Both share the current directory and selection. Click a tile to select it, double-click a directory to drill down, and use Up to return. Changing views or size reuses the current scan; only Scan or Refresh starts another. Partial, unknown and skipped entries remain identified."),
    GuideBlock::Separator,
    GuideBlock::Heading("Screen Recorder"),
    GuideBlock::Paragraph("Duration accepts 0 for Unlimited or 1–1,000 seconds; arrows and the wheel step by 25 while typed values stay exact. Unlimited removes the automatic time stop, not Stop or target and encoder safeguards. The active limit is frozen when recording starts; the countdown is separate."),
    GuideBlock::Separator,
    GuideBlock::Heading("Snippets"),
    GuideBlock::Paragraph("Use cs to edit saved snippets. Tags are metadata; select multiple tag chips to require all of them. The editor's Test / Preview resolves a draft without saving or copying."),
    GuideBlock::Paragraph("Automatic values: {{sys.date}}, {{sys.time}}, {{sys.clipboard}}, {{sys.window_title}}. Date/time default to %Y-%m-%d and %H:%M:%S and accept :<format>; unavailable clipboard/title values are empty. {{date}} is still an ordinary prompted field."),
    GuideBlock::Separator,
    GuideBlock::Heading("Paste Sequences"),
    GuideBlock::Paragraph("Use paste to open the editor. Run Once keeps a draft in memory; Save stores authored literal text as plaintext and clipboard steps as expiring history references."),
    GuideBlock::Paragraph("Add Snippet and Add Clipboard Item open an inline source picker. Search snippets by alias, tags, and visible text; hidden bodies stay concealed. A selected template preview is raw and unevaluated. Insert selected adds a reference to the draft; it does not copy or run the source, and the source is rechecked at insertion."),
    GuideBlock::Paragraph("Commands: paste <query>, paste:run:<id>, paste:start, paste:next, paste:cancel. Saved sequence shortcuts use the application's chord syntax."),
    GuideBlock::Paragraph("Choose Automatic, Manual advance, or Paste-to-Advance (Ctrl+V). Each step uses the current eligible application; invocation context only freezes template values. Paste-to-Advance uses one real Ctrl+V per step, with time to settle between presses. After completion the final item stays on the clipboard for ordinary pasting."),
    GuideBlock::PasteSequenceSummary,
    GuideBlock::SequenceStarts,
    GuideBlock::Paragraph("Settings > Paste Sequences, also reached through Playback Settings in the editor, configures playback controls and optional Escape cancellation. Cancel Run or configured Cancel ends early; Esc also cancels when enabled and available. Empty shortcuts are disabled. Conflicting defaults can be inactive; custom conflicts retain the previous routes. Each saved sequence can have its own optional Start shortcut."),
    GuideBlock::Separator,
    GuideBlock::Heading("Explorer Tabs"),
    GuideBlock::Paragraph("Search explorertabs to manage saved folder lists or run a saved list. Commands explorertabs:open, explorertabs:run:<id>, and explorertabs:cancel can also be assigned to actions or shortcuts. Runs use the saved list and report verified, unverified, and remaining folders."),
    GuideBlock::Separator,
    GuideBlock::Heading("Reusable macro parameters"),
    GuideBlock::Paragraph("In the Signature Editor, String parameters can be Text, File picker, Folder picker, or an ordered Choice list with an optional default. Browse fills a path, and selecting a choice fills its value; neither runs the macro. Review the values and choose Run separately. Canceled or unavailable file and folder choosers leave the path editable."),
    GuideBlock::Paragraph("Add Action → Programs & Launcher → Open Folder or Open URL to author a direct open. Folder Browse only fills the path; Run opens it in Explorer. No folder or bookmark record is created. For example, use C:\\Users\\You\\AppData\\Local\\Temp\\DirectOpenDemo or https://example.test/. URLs must be explicit HTTP(S)."),
    GuideBlock::Paragraph("Templates use ${name} for a current runtime value and $${name} for literal ${name}. String, Number, Boolean, and Point values use the shared formatting; Null is rejected. Folder existence and access are checked when the step runs. A URL handoff to Windows does not prove the site loaded, and canceling after handoff does not close the external window. Direct opens are unavailable in unattended Hot Folder runs."),
    GuideBlock::Paragraph("Read Clipboard Text reads into a named String variable when its step runs. Write Clipboard Text writes an exact literal or String variable only when its step runs; neither action pastes text or restores earlier clipboard contents."),
    GuideBlock::Separator,
    GuideBlock::Heading("Diff text workflows"),
    GuideBlock::Paragraph("In Diff, choose Compare Text… to type both sides or use Paste Left/Paste Right for explicit clipboard input. Text input is limited to 8 MiB and 100,000 lines per side. Edit Inputs… changes that memory comparison; Save Left As… and Save Right As… choose explicit destinations, while file comparisons keep their ordinary Save behavior."),
    GuideBlock::Paragraph("To align, select a source line and choose Align from selected line, then select a line in the other pane and confirm. A multi-line selection uses its focused line. Cancel abandons the pending endpoint; accepted Left ↔ Right alignments can be revealed or removed. Alignment changes layout only and never edits or saves text. A hidden alignment target reveals the full row view."),
    GuideBlock::Paragraph("Ignore selected lines adds reversible whole-line ranges; regex rules can instead mark matching differences unimportant. Raw equality describes the original text, while effective equality reflects comparison rules and ignored ranges. Bookmarks belong to the current comparison. Identical only and the Differences only context control change the view; navigation to a hidden target reveals all rows."),
    GuideBlock::Paragraph("Compare selected lines with Clipboard reads plain text once, only after you choose it, and opens a temporary memory comparison with the captured source on the left. Back returns to the original comparison with its edits and annotations. The clipboard text is not added to recents or saved history."),
    GuideBlock::Separator,
    GuideBlock::Heading("Screenshot pinboard"),
    GuideBlock::Paragraph("Each session-only pin has an Opacity control from 10% to 100% (default 100%) in its toolbar and Pin List row. Pending and failed requests are distinguished from the confirmed native value. Opacity changes the displayed window, not the source image used by Copy Image or Save As."),
    GuideBlock::Separator,
    GuideBlock::Heading("Brightness"),
    GuideBlock::Paragraph("Type bright and choose the bright: edit brightness result to open the named physical-monitor list. Refresh explicitly reads monitor information; choose a supported device, adjust its draft, and press Set. Unsupported devices have no invented percentage. The legacy bright <0–100> action remains a separate numeric route that targets all enumerated physical monitors."),
    GuideBlock::Separator,
    GuideBlock::Heading("Markup editor"),
    GuideBlock::Paragraph("Zoom and Text Size share a row above Tools when the editor is wide enough; at narrow widths they wrap onto separate lines."),
    GuideBlock::Separator,
    GuideBlock::Heading("Dashboard"),
    GuideBlock::WrappedParagraph("Open Settings → Dashboard → Customize Dashboard... to edit widget layout plus plugin-aware settings such as note/todo queries, context links, or weather location."),
    GuideBlock::Paragraph("Show preview uses matching S markers in the slot list and grid. Hover highlights the partner; the tooltip identifies its widget and position. * marks selection and ! marks a conflict."),
    GuideBlock::Separator,
    GuideBlock::Heading("Linked context"),
    GuideBlock::Monospace("todo add Draft release @note:release-plan"),
    GuideBlock::Monospace("note add Release Notes @todo:todo-123"),
    GuideBlock::Monospace("cal add tomorrow 09:00 Kickoff @todo:todo-123"),
    GuideBlock::Paragraph("Use note/todo right-click menus to link existing items."),
    GuideBlock::Separator,
    GuideBlock::Heading("Commands"),
    GuideBlock::Providers,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickHelpProvider {
    pub name: String,
    pub description: String,
}

pub fn matches_provider_filter(filter: &str, name: &str, description: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let needle = filter.to_lowercase();
    name.to_lowercase().contains(&needle) || description.to_lowercase().contains(&needle)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpFilterPeer {
    QuickHelp,
    CommandHelp,
}

impl HelpFilterPeer {
    fn other(self) -> Self {
        match self {
            Self::QuickHelp => Self::CommandHelp,
            Self::CommandHelp => Self::QuickHelp,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SharedHelpFilter {
    value: String,
}

impl SharedHelpFilter {
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn seed(&mut self, value: &str) -> String {
        self.value.clear();
        self.value.push_str(value);
        self.value.clone()
    }

    /// Accept the latest edit from one retained widget and return a replacement
    /// only when its peer is behind. The caller can update the inactive widget
    /// without disturbing the source widget's selection or caret.
    pub fn synchronize(
        &mut self,
        source: HelpFilterPeer,
        source_value: &str,
        peer_value: &str,
    ) -> Option<(HelpFilterPeer, String)> {
        if self.value != source_value {
            self.value.clear();
            self.value.push_str(source_value);
        }
        (peer_value != self.value).then(|| (source.other(), self.value.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::{HelpFilterPeer, QuickHelpProvider, SharedHelpFilter, matches_provider_filter};

    #[test]
    fn quick_help_provider_filter_matches_only_case_insensitive_literal_metadata() {
        let provider = QuickHelpProvider {
            name: "Hash Generator".into(),
            description: "Compute a file digest".into(),
        };
        assert!(matches_provider_filter("hAsH", &provider.name, &provider.description));
        assert!(matches_provider_filter("DIGEST", &provider.name, &provider.description));
        assert!(!matches_provider_filter("Compute Hash", &provider.name, &provider.description));
        assert!(!matches_provider_filter(" hash ", &provider.name, &provider.description));
        assert!(matches_provider_filter("", &provider.name, &provider.description));
    }

    #[test]
    fn quick_help_filter_syncs_changed_peers_and_guide_seed() {
        let mut shared = SharedHelpFilter::default();
        let seeded = shared.seed("todo note cal @note: @todo:");
        assert_eq!(seeded, "todo note cal @note: @todo:");
        assert_eq!(
            shared.synchronize(HelpFilterPeer::QuickHelp, &seeded, ""),
            Some((HelpFilterPeer::CommandHelp, seeded.clone()))
        );
        assert_eq!(
            shared.synchronize(HelpFilterPeer::CommandHelp, "haSH", &seeded),
            Some((HelpFilterPeer::QuickHelp, "haSH".to_owned()))
        );
        assert_eq!(shared.value(), "haSH");
        assert_eq!(
            shared.synchronize(HelpFilterPeer::QuickHelp, "haSH", "haSH"),
            None
        );
    }
}
