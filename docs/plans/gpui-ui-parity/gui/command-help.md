# Command Help

G05 owns `command-help`. Help → Command List and Linking Guide, or the typed `help:show` result, open the retained Command Help utility.

| Criterion | Source-backed behavior |
| --- | --- |
| COMMAND-HELP-01 | Preserve actual registered plugin names, capability, description, usage, commands and optional examples. Apply the common retained filter. Linking Guide seeds its specific query. Save Show Examples only through the existing successful persistence path. |

Select `command_help_filters_real_plugin_commands_without_static_tutorials` and `bounded_help_scroll_keeps_controls_and_full_content_reachable`.

`G05-EGUI-ACTION-SHEET-HELP` verifies the typed panel opens. `G05-EGUI-HELP-MENU-ROUTES` verifies the Linking Guide populates the exact filter and the scoped empty result, then closes on Escape. `G05-EGUI-HELP-SHARED-FILTER-EXAMPLES` verifies a real filtered command entry, the immediate Show examples write, and the shared filter with Quick Help. The save-error path and GPUI remain pending. [Validation log](../VALIDATION_LOG.md#g05-native-help-filter-and-examples-baseline)

## First native Command Help checkpoint

One retained GPUI `command-help` panel serves Help → Command List, Linking Guide and the live typed `help:show` result. Its Kit filter keeps exact text through reentry and close. Linking Guide sets the exact `todo note cal @note: @todo:` seed. The focus handoff follows the existing root popup owner. Accepted typed activation keeps the launcher query, caret, selected results and history. Keep the filter, Show examples and Close reachable while the command body scrolls. The panel yields Escape to any more-specific native input or popup owner, then returns focus to its still-live caller. Its Tab path stays out of Search completion admission.

Share the existing match, prefix and example rules in `src/launcher/command_help.rs`, while egui retains its own renderer and actual plugin manager. Match the full case-folded substring with no trimming or token split across plugin name, description and capabilities and command label, description or action ID. Prefixes are unique sorted first words of command labels. Usage retains command order. Examples use the trimmed literal `query:` suffix or the command label. A nonempty miss displays **No commands match the filter.**

The current GPUI installed Hash Generator, Settings and Help unit providers supply actual side-effect-free metadata through one provider list. Use the same list for search, cached commands and Command Help. Cached commands follow plugin enablement, genuine search results also follow Search capability, and Help lists installed descriptors without using those search gates. Filter out Settings' unimplemented Theme and Dashboard commands. Keep GPUI's current Hash layout rule. The complete egui catalog, dynamic plugin metadata, Quick Help overlay, and Command Help size persistence follow in their accountable checkpoints. The GPUI startup path stays before the egui Plugin Manager and its services.

Show examples uses the committed `Settings::show_examples` at startup and an acknowledged immediate write on toggle. Share the canonical raw Boolean patch and registered cancellable writer with query autocomplete through a closed typed preference choice. Preserve full effective defaults for missing and whitespace-only stores, and keep exact unowned values in existing files. The live example state follows the acknowledged value. A failed write preserves the filter and permits retry. Pending Help writes have their own close and restart ownership. EgUI can reuse the same leaf writer for this control while keeping its existing feedback. The broader egui ordinary-close writer remains under G06-D.

Verify the existing real egui matching and bounded-scroll cases, typed Help dispatch, and the touched GPUI Search/popup routes. Cover the shared preference transaction and read/save receipts with narrow filters. On Windows use an owned installed-provider profile with both Paste Sequence shortcuts disabled. Verify mixed-case Hash matching, exact labels/capability/prefix/usage, menu and typed entry, Guide seed, scoped Escape, Scroll and retained filter. Exercise actual Show examples acknowledgement, opaque-value preservation, write failure and retry, and a same-profile reopen. Replay one actual Toast menu item to verify the adjacent routes after adding Help rows.

The first installed-provider slice passed native evidence and all 21 focused Rust tests. It covers the typed, Command List and Linking Guide routes, retained filter, compact controls, acknowledged examples, write failure and retry, and a same-profile read and leaf write across both frontends. Complete plugin metadata, Quick Help and shared utility geometry remain open. [Validation log](../VALIDATION_LOG.md#g05-native-help-filter-and-examples-baseline)
