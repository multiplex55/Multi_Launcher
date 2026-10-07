# Regex Tester

Type `regex` in Multi Launcher and press Enter to open the resizable native tester. The command opens the tool; inline pattern and input arguments are not supported.

The tester uses Rust's `regex` engine entirely locally. It requires no website, cloud service, or AI service. Look-around and traditional backreferences are unsupported and produce inline compiler feedback.

## Edit and evaluate

Enter a raw pattern in the top field and text in the multiline editor. The displayed slashes and flag suffix are decoration, not part of the stored pattern. Literal slashes need no delimiter escaping.

Matching updates about 150 ms after pattern, flag, or text edits. Invalid or incomplete expressions show inline feedback without interrupting editing. All matches are evaluated implicitly; there is no `g` flag.

| Flag | Meaning |
| --- | --- |
| `i` | Case-insensitive matching |
| `m` | Multiline `^` and `$` anchors |
| `s` | Dot also matches newline |
| `u` | Unicode matching; initially enabled |
| `x` | Ignore pattern whitespace and allow comments |

Rust inline and scoped flags also apply.

Ordinary matches and the selected match have distinct highlights. Zero-width matches use position markers. Previous/Next and F3/Shift-F3 wrap through displayed matches while the tester owns keyboard focus. Clicking a match row selects it and brings it into view where practical. Navigation preserves the text and editing cursor.

Match details show full text, numbered and named captures, and optional captures that did not participate. An unmatched capture is distinct from a capture containing empty text. Locations use one-based lines and Unicode scalar columns; spans are end-exclusive UTF-8 byte offsets.

Use Tab/Shift-Tab to move between controls. Escape, Ctrl+W, and the native close button close the tester. Reopening within the running application retains the draft and substitution mode. Unsaved draft buffers are not restored after application restart.

The information area can be collapsed and restored. It moves below the editor in narrow windows.

## Substitution and clipboard

Select Substitution and explicitly enable the preview. Replacement uses Rust syntax: `$1`, `$name` or `${name}`, and `$$` for a literal dollar. The preview is read-only and never changes the input, files, another application, or the clipboard automatically.

The result preview displays up to 16 KiB. Copy Replacement Result copies the complete accepted result, including text beyond the preview.

Use Clipboard Text explicitly replaces only the test text, preserving the pattern, flags, and replacement. Copy actions are available for the pattern, selected match, displayed/all matches, selected capture, replacement result, and readable match information. All Matches joins values with newlines, including empty zero-width values. When evaluation is limited, the action is labeled Copy displayed matches.

## Explanation, reference, and examples

Explanation uses local parsed Rust syntax and conservative structural descriptions. Invalid or pending patterns do not show stale explanations.

Quick Reference supports local search and category filtering. Append syntax adds to the end of the pattern; it does not insert at the editing cursor or replace the whole expression. Copy syntax writes only through that explicit action.

Load example deliberately replaces the draft's pattern, flags, test text, and replacement. The bundled catalog includes email-like text, IPv4, HTTP(S)-like URLs, UUIDs, dates, times, hex colors, integers, decimals, whitespace cleanup, file extensions, and key=value pairs. Read each example's limitations: practical extractors and format checks are not standards-complete validators.

## History, presets, and privacy

`regex_history.json` and `regex_presets.json` live beside the configured settings file. A nondefault settings directory also contains these stores.

History retains at most 50 exact pattern/flag pairs, newest first. Reusing a pair promotes it. Accepted nonempty patterns are recorded even if they find no matches. History never stores test text or replacement buffers.

Presets save a name, pattern, and flags. Sample text and replacement inclusion use explicit checkboxes, initially unchecked; these choices persist within the session. Loading an omitted optional buffer preserves the current draft value; loading an explicitly saved empty buffer clears it. Save new preset, Load, Rename, Update, and confirmed Delete are available. Rename and content Update are separate actions.

Malformed, unreadable, or invalid stores are not silently reset. Their original bytes and the last valid in-memory snapshot are preserved, and writes are blocked until the problem is repaired and the store is explicitly reloaded. Use Reload history and Retry record where applicable, or Reload presets to refresh presets. These stores are separate from the current Data & Recovery backup catalog.

## Interactive limits

This tool is intended for interactive text and logs, with no file-loading or huge-file processing workflow.

| Budget | Limit |
| --- | --- |
| Pattern / replacement warning | Above 1 KiB each |
| Test text warning | Above 16 KiB |
| Pattern / replacement hard limit | 4 KiB each |
| Test text hard limit | 64 KiB |
| Capture groups | 100 |
| Stored matches | 1000 |
| Materialized match/capture data | 2 MiB |
| Replacement output | 1 MiB |

Limited results use qualified lower-bound counts such as “at least”; navigation and copy operate on displayed rows. Excessive substitution suspends instead of publishing partial output.

Oversized pasted drafts are retained, with bounded read-only previews and explicit clear actions to resume editing. Limits suspend expensive work rather than truncating the original buffer.

