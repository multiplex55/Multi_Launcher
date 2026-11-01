# SEARCH-F002 source admission manifest v1

Authored by `/root/contract_author`, 2026-10-29. Source/base: `59156d96adcc73939c7171487a0cb582e3286587`. Historical comparison: `cc94dbf4f98f155bcdef81165f647cbafb472eb0:docs/research/multi-launcher-research-20261028-r01/02-search-discovery/roles/orchestrator/ROUTING_METADATA_SURVEY.md` (read with local `git show`).

## Denominator and meaning

The current `reload_from_dirs` contains **81 unique builtin registration types: 36 HEAD_GATED and 45 REVIEWED_GLOBAL_GRAMMAR**. There are **0 REVIEWED_GLOBAL_CONTENT and 0 UNKNOWN builtin admission envelopes** in this finite roster. There are 37 nonempty declared prefix sets, of which Snippets alone overrides `always_search=true`; 44 inherit empty prefixes. Historical type/name/prefix/always-search deltas: **none**. This is not a live roster, provider health measurement, full operand-parser audit or claim that arbitrary registered plugins are covered.

The single pass examined 45/45 global `search` entry points, including Web and Calculator. Each was bounded by its entry body and at most one direct grammar-helper layer. The shared `strip_prefix_ci` primitive is read once at `src/common/mod.rs:2`: it is ASCII case-insensitive prefix matching with UTF-8-safe slicing, **without a token-boundary requirement**. `CI-start` below means precisely that primitive; trim behavior is specified per row. Helpers behind an already proved outer rejection boundary are not recursively audited. Co-located conversion functions visible in a source excerpt do not constitute a recursive conversion audit.

Categories describe a **conservative reserved admission envelope**, not exact successful syntax: reserve the whole outer prefix family, including malformed operands that currently return no rows. This can safely exclude more queries than the provider accepts. Deep operand/effect/data semantics remain unreviewed and cannot be inferred from the category. The reviewed outer rejection boundary is sufficient to establish that those deeper helpers cannot accept a query outside that envelope. If that outer boundary itself depends on unresolved code/settings, the row must instead be UNKNOWN. None of the current 45 outer boundaries does after the bounded helper reads below. Do not generalize that conclusion to changed source or arbitrary plugins.

`HEAD_GATED` is declaration plus manager evidence only, not a body review. Every such head stays reserved while disabled; its internal syntax is deliberately opaque but contained by the manager gate. `REVIEWED_GLOBAL_CONTENT` would mean unprefixed content matching, not a command grammar; no current global entry qualifies. Data matching behind `cb`, `hi`, `tab`, `cs`, etc. remains global **grammar** for admission purposes.

## Shared routing and identity policy

- Every roster row has origin **Builtin**, through `register_with_settings` (`src/plugin.rs:1056`), and the registration-line column identifies its call. The manager supplies name/origin/epoch; the document does not fabricate live epochs.
- `E/C` applies to every row: enabled-provider policy and configured per-provider search capability at `src/plugin.rs:1417–1427`; absent capability entry does not equal denied search. `PluginCatalogSnapshot` (`382–422,1120–1159`) contains declared capabilities and runtime enabled facts, not the separate configured capability policy or complete grammar. Those identities and revisions must be combined, not conflated.
- Head gating is first `split_whitespace` token, case-insensitive, after the existing per-plugin query plan. `ActionFilters` is the trait default. QR alone declares `Literal`; the original matching declared head preserves its raw payload and bypasses action filters (`src/plugin.rs:1363–1386`). Otherwise filter extraction precedes the manager head and provider call. Candidate raw guards therefore run before any normalization; filter-bearing input is excluded by the proposed contract.
- Literal prefixes, `always_search`, and the head-gated names in this roster are fixed declarations. No claim is made that unrelated plugin settings or result contents are fully audited. `E/C` changes eligibility, never removes reserved syntax. The global detail table identifies settings visible at the reviewed entry boundary.
- A matching source hash establishes evidence identity only. It is not an admission capability or substitute for current origin, epoch, roster, policy, pending and catalog facts.

## Exact registration/declaration roster

Each hash is SHA-256 of the named entire source file encoded as its existing UTF-8 bytes with **CRLF converted to LF only**. No other text normalization. Source locators are relative to the repository at the source revision above; `impl/search` gives exact starting lines. `[]` and false may be inherited trait defaults. `HG` = HEAD_GATED; `GG` = REVIEWED_GLOBAL_GRAMMAR. Global protected syntax, negatives, settings and depth are in the keyed detail table; all HG rows have depth D (declarations + manager), protected forms = all their declared heads, negative = manager refuses other heads absent a matching query plan, settings = E/C and stated query policy. All rows use the shared disabled/origin policy above.

| Type / exact name | Origin; registration line | Prefixes; always_search; query policy | Category; impl/search locator | Source SHA-256 (LF) |
|---|---|---|---|---|
| `WebSearchPlugin` / `web_search` | Builtin; plugin.rs:877 | `[]`; false; ActionFilters | GG; `src/plugins_builtin.rs:10/11` | `2d2f55dfcd32304f5fee5ff1fc548776d76aa91d95b6304040dc9011abc0c24c` |
| `CalculatorPlugin` / `calculator` | Builtin; plugin.rs:878 | `[]`; false; ActionFilters | GG; `src/plugins_builtin.rs:58/59` | `2d2f55dfcd32304f5fee5ff1fc548776d76aa91d95b6304040dc9011abc0c24c` |
| `UnitConvertPlugin` / `unit_convert` | Builtin; plugin.rs:879 | `[]`; false; ActionFilters | GG; `src/plugins/unit_convert.rs:70/71` | `88bbe83b1676a2bd7e4004860911c8cf0024339d7c920ac0235332cc83b582fd` |
| `BaseConvertPlugin` / `base_convert` | Builtin; plugin.rs:880 | `[]`; false; ActionFilters | GG; `src/plugins/base_convert.rs:143/144` | `6bc0f3fe78607c7da52433f7075a0321417f0d88144d298da3722693ea91d235` |
| `DropCalcPlugin` / `dropcalc` | Builtin; plugin.rs:881 | `[]`; false; ActionFilters | GG; `src/plugins/dropcalc.rs:19/20` | `c0d8009becf6de1d9a33d716820458d22aa24c26b488b8a6c3aae68ffd6949ad` |
| `RunescapeSearchPlugin` / `runescape_search` | Builtin; plugin.rs:882 | `[]`; false; ActionFilters | GG; `src/plugins/runescape.rs:7/8` | `6dea0575af43a305cfdf272c466fd75afa7f6b7c9a98791802653b1c3715f20b` |
| `YoutubePlugin` / `youtube` | Builtin; plugin.rs:883 | `[]`; false; ActionFilters | GG; `src/plugins/youtube.rs:7/8` | `cf2866a61da0a3cbf8d7a056c18cab12bdb9713303087d881c3d1aee9ec7eb81` |
| `RedditPlugin` / `reddit` | Builtin; plugin.rs:884 | `[]`; false; ActionFilters | GG; `src/plugins/reddit.rs:7/8` | `02ca174c6531b80cf3121b44e682aeec0b53ef132170354a421134c08b97ef6d` |
| `WikipediaPlugin` / `wikipedia` | Builtin; plugin.rs:885 | `[]`; false; ActionFilters | GG; `src/plugins/wikipedia.rs:7/8` | `3ee43e354c8f461111563156259d27c36d0c3e3119251cc61b48bc1d6c681267` |
| `ClipboardPlugin` / `clipboard` | Builtin; plugin.rs:886 | `[]`; false; ActionFilters | GG; `src/plugins/clipboard.rs:259/260` | `ab597356b867dabff730d002cabfc90442d033880099ff5b20c535d591068db6` |
| `ClipboardModifyPlugin` / `clipboard_modify` | Builtin; plugin.rs:888 | `cm`; false; ActionFilters | HG; `src/plugins/clipboard_modify.rs:49/50` | `144670d42125264f98697b5190cf703c4fb7e4767c3e27cb6dc9c2133a1a2aa7` |
| `BookmarksPlugin` / `bookmarks` | Builtin; plugin.rs:892 | `bm`; false; ActionFilters | HG; `src/plugins/bookmarks.rs:392/393` | `1bcd12e8603b6a60e20821b131af0123145bd140b86e415a4f5519db1dd4138c` |
| `FoldersPlugin` / `folders` | Builtin; plugin.rs:893 | `f`; false; ActionFilters | HG; `src/plugins/folders.rs:316/317` | `6bea17b1e8da0a07e575b26860a8dc5202545f7db0d481726ed0e63a3c9b9aa6` |
| `FileSearchPlugin` / `file_search` | Builtin; plugin.rs:894 | `fs`; false; ActionFilters | HG; `src/plugins/file_search.rs:33/34` | `807231f34f51e849057c2da92bc337876af91e0be28340a78ea8d65cce9b2ea3` |
| `DiffPlugin` / `diff` | Builtin; plugin.rs:895 | `diff`; false; ActionFilters | HG; `src/plugins/diff.rs:13/14` | `8ad48d3ac8771c57b329a1092ff9be7273c29c3f938e533b288eb0f0206db6c7` |
| `OmniSearchPlugin` / `omni_search` | Builtin; plugin.rs:896 | `o`; false; ActionFilters | HG; `src/plugins/omni_search.rs:129/130` | `6e523ea3d5611c4b4e49515c3dd5ba46614aec3de654af64c3ed738f22c231ee` |
| `SystemPlugin` / `system` | Builtin; plugin.rs:897 | `[]`; false; ActionFilters | GG; `src/plugins/system.rs:6/7` | `89a35a4837af15faf39bedf54ab3761a08667c8df3fbbff5cc63a1a5353fd136` |
| `ProcessesPlugin` / `processes` | Builtin; plugin.rs:909 | `[]`; false; ActionFilters | GG; `src/plugins/processes.rs:93/94` | `db0f0a416dfcadda6a8750f7a311f54724eecbc6499f57490bd17bdeb32f26b2` |
| `SysInfoPlugin` / `sysinfo` | Builtin; plugin.rs:910 | `[]`; false; ActionFilters | GG; `src/plugins/sysinfo.rs:79/80` | `5cfc303aa9e0231fc197b6b9ba989ae937df25def8448461ba9b26be37295227` |
| `NetworkPlugin` / `network` | Builtin; plugin.rs:911 | `[]`; false; ActionFilters | GG; `src/plugins/network.rs:85/86` | `53cfe184f2c0a89d9b001a3f1cf9ee5a418181b9e7292f4f8e0b1e6f1b1478ae` |
| `ShellPlugin` / `shell` | Builtin; plugin.rs:912 | `[]`; false; ActionFilters | GG; `src/plugins/shell.rs:141/142` | `db3a90f897a3759257642d0ccb2914da8836bb616c6b30b003441f4814397b7e` |
| `HistoryPlugin` / `history` | Builtin; plugin.rs:913 | `[]`; false; ActionFilters | GG; `src/plugins/history.rs:21/22` | `a392806743d527cf2fa670944bcc69823eaf4d6ffcdf6fb88c5d9e98035e1cac` |
| `NotePlugin` / `notes` | Builtin; plugin.rs:914 | `note, notes`; false; ActionFilters | HG; `src/plugins/note.rs:2558/2559` | `4cf89db17e779d3eca41771915313e3880c0f579bbc70ea24b751b856f9e29bb` |
| `TodoPlugin` / `todo` | Builtin; plugin.rs:915 | `todo`; false; ActionFilters | HG; `src/plugins/todo.rs:1104/1105` | `4abb38634be95df2692a278d18d62bb0c714c0c416eef37b2f8fc65820c913ef` |
| `CalendarPlugin` / `calendar` | Builtin; plugin.rs:916 | `[]`; false; ActionFilters | GG; `src/plugins/calendar.rs:1253/1254` | `42f1b9c63e6cd88ef7e03469d855fb5b0ac8ac6d053d2443a401a67f3b17496c` |
| `SnippetsPlugin` / `snippets` | Builtin; plugin.rs:917 | `cs`; true; ActionFilters | GG; `src/plugins/snippets.rs:748/749` | `61bd2980d35a9f2b2c7279dacc84c6ffdf0a30ba51e27f33bd5f5290bf8e464d` |
| `PasteSequencePlugin` / `paste_sequence` | Builtin; plugin.rs:918 | `paste`; false; ActionFilters | HG; `src/plugins/paste_sequence.rs:22/23` | `96c8221c9877e13737028a08861f7b0635d685102af80d9ec5d8831ab78678d0` |
| `MacrosPlugin` / `macros` | Builtin; plugin.rs:919 | `[]`; false; ActionFilters | GG; `src/plugins/macros.rs:383/384` | `3771e6132ad532b46c03a70980729232d0b12768e4a88aaf3bfbc0e492c9b9bd` |
| `MkMacroPlugin` / `mkmacro` | Builtin; plugin.rs:920 | `mkmacro`; false; ActionFilters | HG; `src/plugins/mkmacro.rs:80/81` | `a93bdac47dd369455382a13f2626e9eee2597e0e81a382ce6011dd4683118934` |
| `KeysPlugin` / `keys` | Builtin; plugin.rs:924 | `[]`; false; ActionFilters | GG; `src/plugins/keys.rs:7/8` | `83eff6709660a71a30b337d3da823f1445f1f1b2adf56f0d9c201d6e417350af` |
| `MouseGesturesPlugin` / `mouse_gestures` | Builtin; plugin.rs:925 | `[]`; false; ActionFilters | GG; `src/plugins/mouse_gestures.rs:623/624` | `ea327ce2d1c13078abb434a7893cc99db6c0351f5f133a7824fd5ada9e42d6de` |
| `MultiManagerPlugin` / `MultiManager` | Builtin; plugin.rs:926 | `mm`; false; ActionFilters | HG; `src/plugins/multi_manager.rs:12/13` | `bff1007e79e87655a345bfb078a9ad6f206aad6b4302b3d4d0424429cf92d408` |
| `FavPlugin` / `favorites` | Builtin; plugin.rs:927 | `[]`; false; ActionFilters | GG; `src/plugins/fav.rs:326/327` | `1f347c5390b1bf68d93bec72029d05a0ff7098e223bdaac02ab59a731cf28481` |
| `MissingPlugin` / `missing` | Builtin; plugin.rs:928 | `[]`; false; ActionFilters | GG; `src/plugins/missing.rs:252/253` | `19519481ced2d9e5c6c6ad4f1e9c1a715afbe61c31c71e4382e4467f74f398a6` |
| `RecyclePlugin` / `recycle` | Builtin; plugin.rs:936 | `[]`; false; ActionFilters | GG; `src/plugins/recycle.rs:6/7` | `0133d53ea2f45a77c2ccf5b340d8da3a06514e5510b1c749f074cbbdb3cd08e7` |
| `TempfilePlugin` / `tempfile` | Builtin; plugin.rs:937 | `[]`; false; ActionFilters | GG; `src/plugins/tempfile.rs:135/136` | `c8df250aee4c4315d544bfc8692420b0665bfa515520c761b640ebbf984a8e77` |
| `MediaPlugin` / `media` | Builtin; plugin.rs:938 | `[]`; false; ActionFilters | GG; `src/plugins/media.rs:6/7` | `39ee1ebefb816b3eae746f8101616884ef0dd208b436f8355cd3fe3b544ee5a0` |
| `AsciiArtPlugin` / `asciiart` | Builtin; plugin.rs:939 | `[]`; false; ActionFilters | GG; `src/plugins/asciiart.rs:29/30` | `23af9d27720e27294c96485562f8ab5766104ba6fb4b77ad76bced6dd450a9ec` |
| `EmojiPlugin` / `emoji` | Builtin; plugin.rs:940 | `[]`; false; ActionFilters | GG; `src/plugins/emoji.rs:120/121` | `6e08b4316aa353c1df23545c655167f0aa2519d3df8ac0f7b1f918c5431098b5` |
| `TextCasePlugin` / `text_case` | Builtin; plugin.rs:941 | `[]`; false; ActionFilters | GG; `src/plugins/text_case.rs:7/8` | `9e7e884eb3514b320d3d89af1531ecf346f3b2e0d11bb0bd675bb381a8b19631` |
| `ScreenshotPlugin` / `screenshot` | Builtin; plugin.rs:942 | `[]`; false; ActionFilters | GG; `src/plugins/screenshot.rs:113/114` | `f225bacede286b4ca7d0ea5ceab4ff5d82eaaad778cfe04c5939bef08ccf1de6` |
| `PinboardPlugin` / `pinboard` | Builtin; plugin.rs:943 | `pinboard`; false; ActionFilters | HG; `src/plugins/pinboard.rs:6/7` | `da36bf0f19003e022916abb739600c01e82c224f0786da196f5eeb6f321612cc` |
| `ScreenDrawPlugin` / `screen_draw` | Builtin; plugin.rs:944 | `sd, sa`; false; ActionFilters | HG; `src/plugins/screen_draw.rs:29/30` | `12e7dd153fdc769d2b04969d3ea001fcc5cfa17744ca12e66be93d9b48ca726a` |
| `CropPlugin` / `crop` | Builtin; plugin.rs:945 | `crop`; false; ActionFilters | HG; `src/plugins/crop.rs:27/28` | `ca0e1a65613f752e95d80d554ffcd0de9f13d9b9a2b78d51a53d1e6b0fe2d47b` |
| `DataPlugin` / `data` | Builtin; plugin.rs:946 | `data`; false; ActionFilters | HG; `src/plugins/data.rs:25/26` | `ad470a0c25f7fb3e5445e4a9df6867af7bc1ebf575cee6f2585e0113ac15278b` |
| `TimestampPlugin` / `timestamp` | Builtin; plugin.rs:947 | `[]`; false; ActionFilters | GG; `src/plugins/timestamp.rs:50/51` | `5807bebb13c307819f0c39c4eb9b6baff2e2a4eae25b26f4f58fa135d4bbd8ef` |
| `DateArithmeticPlugin` / `date_arithmetic` | Builtin; plugin.rs:948 | `date`; false; ActionFilters | HG; `src/plugins/date_arithmetic.rs:61/62` | `edd8292f72c9d00176fad2f3a543e933948f239094754d00d1b226eaa4813a7b` |
| `IpPlugin` / `ip` | Builtin; plugin.rs:949 | `[]`; false; ActionFilters | GG; `src/plugins/ip.rs:289/290` | `565243a18d9883ff0357c836817869f1ef45b512a193506c1aa52207245ff1bf` |
| `RandomPlugin` / `random` | Builtin; plugin.rs:953 | `[]`; false; ActionFilters | GG; `src/plugins/random.rs:34/35` | `66df1fc55211e2bb55b2ee3b3e6cc044082105e8517981b285ec7d7527346bd9` |
| `RadialPlugin` / `radial` | Builtin; plugin.rs:954 | `radial`; false; ActionFilters | HG; `src/plugins/radial.rs:53/54` | `0c65a4d38c4e73114339bc457dfcec6b2c6c641c3892b07a40f0e03202ddcce0` |
| `LoremPlugin` / `lorem` | Builtin; plugin.rs:955 | `[]`; false; ActionFilters | GG; `src/plugins/lorem.rs:22/23` | `74f16a67dac1138ff4633cde9d1de15231b665ce842140d2854f76af77df0119` |
| `ConvertPanelPlugin` / `convert_panel` | Builtin; plugin.rs:956 | `[]`; false; ActionFilters | GG; `src/plugins/convert_panel.rs:7/8` | `b111f00af8dbf805178041e3f9790209cca185a98a2ac5e4fd9b24eb491d74e3` |
| `JsonUtilityPlugin` / `json_utility` | Builtin; plugin.rs:957 | `json`; false; ActionFilters | HG; `src/plugins/json_utility.rs:7/8` | `6335f5dcc322edbaadaa29f739177f4044ebc25172ad9857d9888b6044738a07` |
| `XmlToolkitPlugin` / `xml_toolkit` | Builtin; plugin.rs:958 | `xml`; false; ActionFilters | HG; `src/plugins/xml_toolkit.rs:6/7` | `8c935af23578956faf6393d8791cbdadfa2aebb2c39e622891039ac4e41fa973` |
| `ImageComparePlugin` / `image_compare` | Builtin; plugin.rs:959 | `imgdiff`; false; ActionFilters | HG; `src/plugins/image_compare.rs:6/7` | `b0af5f7e202709b0e63c8c17aa32b57eecd16f7225ef957a2093b4ce5f0db3f1` |
| `DuplicateFinderPlugin` / `duplicate_finder` | Builtin; plugin.rs:960 | `dff`; false; ActionFilters | HG; `src/plugins/duplicate_finder.rs:6/7` | `e7e74eaf21887c85259519be546dee8091b16c2dc42b1b11d76491678003b4ce` |
| `ActivityPlugin` / `activity` | Builtin; plugin.rs:961 | `activity`; false; ActionFilters | HG; `src/plugins/activity.rs:6/7` | `ae7ce95027f8030b2d5b4af819cd69f09a0e039a74ccaea960eefb0990c29b2d` |
| `TableUtilityPlugin` / `table_utility` | Builtin; plugin.rs:962 | `table`; false; ActionFilters | HG; `src/plugins/table_utility.rs:6/7` | `186f4072e1c583ed3f1ddd18e59fc715b0e2d6250b6d4ae7d512991f9e631b25` |
| `CsvPlugin` / `csv` | Builtin; plugin.rs:963 | `csv`; false; ActionFilters | HG; `src/plugins/csv.rs:6/7` | `e0297ad0d8734087cdb805ec007903f1ffb11c387d412eae18c6e7ad99c6be00` |
| `BatchRenamePlugin` / `batch_rename` | Builtin; plugin.rs:964 | `rename`; false; ActionFilters | HG; `src/plugins/batch_rename.rs:6/7` | `d6bd69e819f3be8bafbce28b910ac879cad21bd90d331ba7260440d6e9452790` |
| `SearchReplacePlugin` / `search_replace` | Builtin; plugin.rs:965 | `replace`; false; ActionFilters | HG; `src/plugins/search_replace.rs:6/7` | `9e94cf0b253a48add72e0c107913f4696968ec93f7546f92c3a42792108bb2ee` |
| `HotFolderPlugin` / `hotfolder` | Builtin; plugin.rs:966 | `hotfolder`; false; ActionFilters | HG; `src/plugins/hotfolder.rs:6/7` | `d0ac66b24b6b84543edcbdb4244fb3f74b3d4afcfe54423a19e00fbc4598ae8d` |
| `HashGeneratorPlugin` / `hash_generator` | Builtin; plugin.rs:967 | `hash`; false; ActionFilters | HG; `src/plugins/hash_generator.rs:6/7` | `0de08dbf149de0f942edb967088e39c20b1661dfcaef997d70d1e73f9c7189dc` |
| `RegexTesterPlugin` / `regex_tester` | Builtin; plugin.rs:968 | `regex`; false; ActionFilters | HG; `src/plugins/regex_tester.rs:6/7` | `1aa286173a34d633154f6eb4248a2c85c9479b22395f0a9bde40863020af42d5` |
| `QrPlugin` / `qr` | Builtin; plugin.rs:969 | `qr`; false; Literal | HG; `src/plugins/qr.rs:6/7` | `1576fc0654d5c3c425d4b18b5a359d7ae69eb8a55fddbb289f6660595c9d868a` |
| `ColorPickerPlugin` / `color_picker` | Builtin; plugin.rs:970 | `color`; false; ActionFilters | HG; `src/plugins/color_picker.rs:24/25` | `cff744489e4b986e884e0c32a13619f1ddbc4853002451a010fe321912976dcf` |
| `MousePlugin` / `mouse` | Builtin; plugin.rs:971 | `mouse`; false; ActionFilters | HG; `src/plugins/mouse.rs:71/72` | `c777f076345ba3be8e14af2b499e066d4fcb6cced74221af813f07fab918ce96` |
| `OcrPlugin` / `ocr` | Builtin; plugin.rs:972 | `ocr`; false; ActionFilters | HG; `src/plugins/ocr.rs:16/17` | `ca5804f69d1b75a8b9346cfd9bad218c0e1581f11af8979b05b9f5d86a5fccd6` |
| `VolumePlugin` / `volume` | Builtin; plugin.rs:973 | `[]`; false; ActionFilters | GG; `src/plugins/volume.rs:31/32` | `eb2aea39c8550cb6f7935d669e33196e466d2612208833b105fd9f9dd9e6ef04` |
| `BrightnessPlugin` / `brightness` | Builtin; plugin.rs:974 | `[]`; false; ActionFilters | GG; `src/plugins/brightness.rs:18/19` | `0cb7d8f07f2c24ed27ed131e329629577c2b46c7c0f6c4327704a1b04cdd5711` |
| `TaskManagerPlugin` / `task_manager` | Builtin; plugin.rs:975 | `[]`; false; ActionFilters | GG; `src/plugins/task_manager.rs:6/7` | `9fd18cb0b23187d5b96708eef5a26831e9bfba048dd657259af579e7a14fc1f6` |
| `WindowsPlugin` / `windows` | Builtin; plugin.rs:976 | `win`; false; ActionFilters | HG; `src/plugins/windows.rs:49/50` | `55ecb30b0a0d123a84e8bf5153485a4365637c7b618d7efa53043f3c89f95730` |
| `VirtualDesktopPlugin` / `virtual_desktop` | Builtin; plugin.rs:980 | `vd`; false; ActionFilters | HG; `src/plugins/virtual_desktop.rs:565/566` | `8275b7e073294ff99c656f500913fe66a7fe5824b65cf0da342fa38c139faf7b` |
| `BrowserTabsPlugin` / `browser_tabs` | Builtin; plugin.rs:988 | `[]`; false; ActionFilters | GG; `src/plugins/browser_tabs.rs:799/800` | `d2c64f5068312db9272d355e614c2d4aa887dfaae120ad5d48b4a9bfb856f326` |
| `SettingsPlugin` / `settings` | Builtin; plugin.rs:992 | `[]`; false; ActionFilters | GG; `src/plugins/settings.rs:6/7` | `d3a5e05d1495fad247f86ee9d759154c7b4c19d2e1f4009d2f15f06b8c9ba150` |
| `HelpPlugin` / `help` | Builtin; plugin.rs:993 | `[]`; false; ActionFilters | GG; `src/plugins/help.rs:6/7` | `dc05222ae971343cfc4fa37371cba01278b0d7f1bd4281245522921efc438dfa` |
| `LayoutPlugin` / `layout` | Builtin; plugin.rs:994 | `[]`; false; ActionFilters | GG; `src/plugins/layout.rs:152/153` | `d49bb0d3bb080f11a4b0d601a5359a8ecb2ed1a0b2820217a2de66e6e05704ae` |
| `LinkPlugin` / `link` | Builtin; plugin.rs:995 | `[]`; false; ActionFilters | GG; `src/plugins/link.rs:37/38` | `5ea5f05e980232038fbe99cbec59568ed0b10e1db3744fd4f911bec8e8791f3e` |
| `TimerPlugin` / `timer` | Builtin; plugin.rs:996 | `timer, alarm`; false; ActionFilters | HG; `src/plugins/timer.rs:704/705` | `f482a5347b6a0761215f03089285a5faa25d2728e4cc88a16144299548a6247f` |
| `StopwatchPlugin` / `stopwatch` | Builtin; plugin.rs:997 | `[]`; false; ActionFilters | GG; `src/plugins/stopwatch.rs:247/248` | `9de471f06f8108187456af5f0d715e8f3162f84a642cfdc0d0109c6e6b88b882` |
| `WeatherPlugin` / `weather` | Builtin; plugin.rs:1002 | `[]`; false; ActionFilters | GG; `src/plugins/weather.rs:8/9` | `bb02723635579a4f02ef19731baf69898cde07cc18e6d6ae62c13340df2bd61f` |

## Global detail: protected envelopes and negative evidence

`B` = entry body only; `B+C` = entry plus shared direct `strip_prefix_ci`; `B+H` = entry plus named direct local guard helper. A named helper may call the already reviewed common prefix primitive; no further domain helper is followed. A bound is an outer conservative exclusion, not a claim that every protected string succeeds. In every row, query outside the listed envelope returns no actions before any downstream operand/content/effect helper can return a row. For guarded data-backed routes, empty/missing/error data may also yield zero rows **inside** the envelope; that is never permission to offer discovery there. All rows inherit E/C.

| Owner name | Protected raw family / negative condition | Route-relevant settings or result-state limit | Evidence depth / unresolved scope |
|---|---|---|---|
| web_search | Raw case-sensitive `g ` with len>2; otherwise empty. Reserve g/G head also at GUI level. | No route setting; GUI special g routing is separate. | B; URL encoding is not audited. |
| calculator | Trimmed leading `=`; CI exact `calc list`, `= history`, `= list`. Empty/invalid expression can be empty; reserve all `=` and calc head. | `save_on_enter` changes action/history side effects, not outer guard (`plugins_builtin.rs:149–160`). | B; exmex and history internals unreviewed, contained by guard. |
| unit_convert | Trim-start first whitespace-separated head CI `conv`/`convert`; no split or another head returns None. Empty operand/base conversion may be empty. | No declared settings; evaluator can return errors/actions inside envelope. | B+H `unit_convert.rs:9–17` conversion_query; deep units/evaluator not audited, whole conv/convert reserved. |
| base_convert | Same conv/convert whitespace guard; parse/convert failure is empty. | No declared settings; quoted conversion operands stay protected. | B+H `base_convert.rs:37–45` conversion_query; directly visible parse_query/convert do not establish a complete inner grammar. |
| dropcalc | Trimmed CI-start `drop `; two operands, parse failures return empty. | No settings at entry. | B+C; probability helper not followed because entire drop family reserved. |
| runescape_search | Raw CI-start `rs ` or `osrs ` and nonempty trimmed suffix; else empty. | No settings at entry. | B+C; encoding not audited. |
| youtube | Raw CI-start `yt ` and nonempty trimmed suffix; else empty. | No settings at entry. | B+C. |
| reddit | Raw CI-start `red ` and nonempty trimmed suffix; else empty. | No settings at entry. | B+C. |
| wikipedia | Raw CI-start `wiki ` and nonempty trimmed suffix; else empty. | No settings at entry. | B+C. |
| clipboard | Raw CI-start `cb`, including adjacent cbtext; otherwise empty. Bare/edit, clear, list and filtered history remain protected. | `max_entries` controls history size (`clipboard.rs:355–366`); no outer guard change. | B+C; history/clipboard helpers not followed. |
| system | Raw CI-start `sys`; suffix filters shutdown/reboot/lock/logoff; unmatched suffix empty. | No settings at entry. | B+C; destructive action payloads protected. |
| processes | Raw CI-start `psk`, `pss`, then `ps`; otherwise parse_query returns None. No snapshot/matches may be empty. | System cache/refresh affects rows, not outer parser. | B+H `processes.rs:37–49` parse_query; process enumeration/effects unreviewed. |
| sysinfo | Raw case-sensitive starts_with info, then trimmed lowercase tokens info/cpu/mem/disk/cpu-list-count; others empty. | Snapshot may be unavailable; no dynamic prefix. | B; reserve whole CI info family conservatively. |
| network | Raw CI-start `net` and whitespace-only suffix; else empty. Lock failures also empty. | refresh_rate/unit (`network.rs:202–225`) alter data, not guard. | B+C; network sampling unreviewed. |
| shell | Trimmed sh exact or CI-start `sh add `, `sh rm`, `sh list`, `sh `; outside empty. Missing files/matches may be empty. | open_in_wezterm affects execution (`shell.rs:297–305`), saved commands/autocomplete affect payloads within sh. | B+C; all sh suffixes reserved; saved data not read. |
| history | Raw CI-start `hi`, including adjacent suffix; outside empty. clear/list/search history protected. | History data/availability controls rows; apply_settings is empty (`history.rs:88`). | B+C; history contents not read. |
| calendar | Trimmed CI-start `cal`, including adjacent suffix; outside empty. day/week/month/upcoming/find/add/snooze/date branches protected. | Local time and parser/data can affect inner grammar; no settings-controlled outer prefix. | B+C; date/calendar parsing not followed; entire cal family reserved. |
| snippets | Trimmed CI-start `cs`, including csfoo, cs rm/add/edit/list; outside empty. | Stored snippets/tag filters affect rows; always_search=true bypasses declared-head gate. | B+C; all adjacent syntax reserved; tag parser/matcher not recursively inspected. |
| macros | Trimmed CI exact macro or CI-start `macro list`/`macro `; outside empty. | Saved macros and list helper affect rows within guarded family. | B+C; macro execution/query resolution not inferred from list body. |
| keys | Trimmed CI-start keys or key, including adjacent suffix; outside empty. Bare form emits query hint. | No settings at entry. | B+C; arbitrary key payload remains protected, execution not reviewed. |
| mouse_gestures | Trimmed mg exact or CI-start mg settings/setting/edit/add/find/where/conflicts/list/space; otherwise empty. | Settings/runtime binding DB affect results/effects (`mouse_gestures.rs:736–745`), no outer-prefix field. | B+C; DB/list helper bodies not followed. |
| favorites | Trimmed fav exact or CI-start fav add/rm/list/space; otherwise empty. | Stored favorite records/list helper affects rows; no outer-prefix field. | B+C; saved content not inspected. |
| missing | Trimmed CI exact check missing or any nonempty proper ASCII-CI prefix of that phrase; otherwise empty. | Scan inputs/status affect rows only after guard. | B+H `missing.rs:169–179` explicit_scan_query/partial_query; scheduling not followed. Reserve check head and partial c/ch/... as specified. |
| recycle | Trim-start CI-start rec, including adjacent recreation; outside empty. | No settings at entry. | B+C; rec family deliberately broad and destructive. |
| tempfile | Trimmed tmp exact or CI-start tmp new/create/open/clear/rm/alias/list; outside empty. | Files/aliases/validation affect inner rows; no dynamic outer prefix. | B+C; whole tmp family reserved; filesystem not accessed. |
| media | Raw CI-start media; suffix filters play/pause/next/prev; unmatched suffix empty. | No settings at entry. | B+C. |
| asciiart | Raw CI-start `ascii `, nonempty text, available font/conversion; otherwise empty. | Font availability affects rows; outer prefix fixed. | B+C; font conversion not followed. |
| emoji | Trimmed CI-start emoji list/space; suffix matches fixed content table, may yield no rows. | Matcher/content affects rows inside emoji only. | B+C; guarded content, not unrestricted global content. |
| text_case | Trim-start CI-start `case `; otherwise empty. | Legacy transform may produce zero rows inside case. | B+C; transform_query not followed; whole case family reserved. |
| screenshot | Trimmed CI-start ss, including adjacent suffix; outside empty. cancel/delay constraints may return empty; other ss suffixes return screenshot options. | Screenshot directory/config affects execution, not outer route (`screenshot.rs:249`). | B+C; delayed_actions effect/payload internals not audited. |
| timestamp | Trim-start CI-start `tsm ` or `ts `; empty/failed conversion can be empty. | Local timezone/date parsing changes conversion, not guard. | B+C; whole ts/tsm families reserved. |
| ip | Trimmed CI-start ip, including adjacent suffix; outside empty. | Adapters/public-IP cache may be empty; no prefix config. | B+C; no adapter/network call executed. |
| random | Trimmed CI-start `rand ` or `random `; otherwise empty. number/num, dice, pw/password branches; invalid args/lock failure may be empty. | RNG state changes payload, not routing. | B+C; no RNG invoked. |
| lorem | Trim-start CI-start `lorem `, kind w/s/p and unsigned count with no extra token; else empty. | No settings at entry. | B+C; text generation not followed. |
| convert_panel | Trimmed CI-start convert or conv, including adjacent conversation; outside empty. | No settings at entry. | B+C; broad overlap with conversion providers preserved. |
| volume | Trimmed vol exact or CI-start `vol `; otherwise empty. ma/level/pid/name branches protected. | Process snapshot availability affects name lookup only. | B+C; process/OS helpers unreviewed. |
| brightness | Trimmed lowercase proper prefixes of bright of length>=2, plus CI-start bright; otherwise empty. | No settings at entry; parser can produce usage rather than empty. | B+C; br/bri/brig/brigh reserved as well as bright family. |
| task_manager | Trim-start CI-start tm, including adjacent suffix; otherwise empty. | No settings at entry. | B+C; native launch not run. |
| browser_tabs | Trimmed CI-start tab, including adjacent suffix; otherwise empty. clear/cache/filter protected. | recalc_each_query/cooldown/cache affect refresh/results (`browser_tabs.rs:878–893`). | B+C; cache helper not followed. |
| settings | Trim-start CI-start settings/dashboard/theme plus has_command_token_boundary; outside empty. Settings suffix whitespace remains transport data. | Settings query is carried in Action.args; no dynamic outer route. | B+H `settings.rs:91–93` plus shared common primitive; boundary empty, ASCII space prefix or wholly whitespace. |
| help | Trimmed CI-start help followed by empty or ASCII-space suffix; otherwise empty. | No settings at entry. | B+C; helpfoo does not match, but full help head stays reserved. |
| layout | Trimmed CI-start layout, including adjacent suffix; otherwise empty. list/save/load/show/edit/rm protected. | Config path existence is checked before guard; inner flags/layout data affect rows. | B+C; **no empty-result proof of absence of provider side effects**; layouts not read by this research. |
| link | Trim-start CI link exact or ASCII-lower starts_with link-space; otherwise empty. | Notes/todos/link catalog affect resolution within link; uppercase suffix handling not changed. | B; resolve_link and catalog unreviewed; entire link head protected. |
| stopwatch | Trimmed CI-start sw, including adjacent suffix; otherwise empty. start/list/pause/resume/stop/show protected. | precision/refresh_rate (`stopwatch.rs:432–448`) affect display/runtime, not outer guard. | B+C; stopwatch state helpers not followed. |
| weather | Raw CI-start `weather ` and nonempty trimmed suffix; otherwise empty. | No settings at entry. | B+C; URL encoding not audited. |

## Unknown, disabled and lifetime cost

The zero UNKNOWN builtin-envelope count is narrow and **does not mean runtime Complete**. Unknown operand semantics remain excluded by the reserved envelopes; unknown *outer routes/identity* cannot be excluded that way. These identity classes are not extra builtin registration rows:

| State | Proposed conservative disposition / cost |
|---|---|
| Reviewed builtin disabled or denied search capability | Keep its HG/GG reserved envelope; it contributes no eligible descriptor. A candidate cannot skip its syntax merely because disabled. |
| New/changed source, changed declaration, unreviewed builtin, missing source binding | UNKNOWN outer coverage until a new explicit review; reject discovery for the entire configuration, even if owner disabled. |
| DynamicLibrary or Registered, including disabled owner or same builtin name | UNKNOWN; no reviewed builtin identity inheritance; reject all discovery for that configuration. |
| Duplicate origin/name or ambiguous identity/epoch | UNKNOWN/ambiguous coverage and no eligible descriptor. Reject, never first/last-name-wins. |
| Removed/unloaded owner, deferred dynamic retirement or stale roster snapshot | Invalidate rows immediately; unresolved coverage/tombstone blocks discovery. Removal is not evidence its reserved syntax vanished. |
| Policy/capability/registration/catalog generation changed during or after search | Discard admission and selected binding. Re-read owner-authoritative facts; never relabel a stale empty result current. |
| Failed provider, poisoned/unknown provider state, pending ticket or capacity deferral | Empty is insufficient; do not issue a completed admission receipt. No polling/retry/provider execution is added by this document. |

The restricted candidate is explicitly a builtin-only profile from a fresh reviewed manager lifecycle with empty plugin-directory configuration, unique current registered builtin identities, stable policy and no unresolved prior external owner. Merely disabling or unloading an unknown owner in an existing session does not meet it. Native/source-current facts for that profile remain unobserved. The source loop permits no configured directories and still registers all 81 builtins; that is source plausibility, not a live environment measurement.

## Shared evidence hashes

These support the same conservative boundary and the companion consumer trace. Hash format is identical to the roster; line ranges are in the companion documents.

| Source | SHA-256 (LF) |
|---|---|
| `src/plugin.rs` | `be2e64f904ee4ad18758d55409d6ebe9e9b4ac633c6314c89115d5bb1dd0de0a` |
| `src/common/mod.rs` | `56b2c95566eb6005593fe23c4e48edef8141fc4e5955f1fb2fbd535867af923f` |
| `src/common/query.rs` | `311c387f5b69d11ea35059b37d1548d39ff32680a570168b7a3e04781708bdbc` |
| `src/gui/search.rs` | `b5724ef258bae10c05c05faa318d0b39a46bf905477957475b920e0bebbd1ba8` |
| `src/gui/render.rs` | `8dba9d66ed33528bd034d8e996c4a9e116d3449953f63fccd46ff7fb7cca534d` |
| `src/gui/actions.rs` | `5ea1081ae46584d2c7b3e2449f1bc7c181f61b9c2156baef0ec96f7f401d1674` |
| `src/actions/mod.rs` | `8260583a6f19f66f63c1d30f7336eb0083f244b76670efbd11821c8a2d092cb5` |
| `src/commands/model.rs` | `b7328a8ec44fb0dc80a2406d6de628a93b0101066e7d5459878b7ee4dcaa529d` |
| `src/commands/parser.rs` | `65a1aa2eb99e9a7990b50310b9d4f3ca29ef2e1ec4dcb82f9fe3d29fef1a3ec3` |
| `src/commands/handlers/launcher_query.rs` | `60cded8790be5c2a3d5e61d9a6805d0de0cf72509029d4d893d824edd46de9e5` |
| `src/universal_actions/model.rs` | `0c1a841b696df58accb73af8223d960727b214b0ee2ea050c1fe423db01d7675` |
| `src/settings/model.rs` | `2b79db302e3bc03449f72e51c0440df39108dd431c1e41bc818520c22bac7c8e` |
| `src/settings_editor/save.rs` | `6bd45a9da671808ed7febe97865c25ffe43a4636723ec99683fbfabe4636f2cf` |
| `src/settings_save.rs` | `912def1bade4cce61a73000d762f3f232c2cee37f531244c232dfa0e17942ffd` |

## Limits and stopping point

Roster 81/256; global entries 45/45; helper depth at most one domain layer; consumer configurations 2/2. No further global-body pass is authorized by this result. No provider, parser, app, native API or ranking prototype was executed. No private settings/data was read. A Python source-text extraction was used only for registration/declaration transcription and documentation generation; it is not the optional admission model. Existing tests were read as source at the compatibility seams; none was run.

This manifest establishes reserved-envelope evidence. It does not establish finality signaling for arbitrary providers, default-like canonical emptiness, live epoch/source agreement, native input provenance or product value. Those distinctions determine the [contract](CONSUMER_COMPATIBILITY_CONTRACT.md) and [decision](ADMISSION_DECISION.md).
