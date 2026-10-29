# Command and capability catalog — Goal 02 / M1-A

Source pin: `59156d96adcc73939c7171487a0cb582e3286587`. This is a static research catalog of **15 existing fixed tool openers across 11 owners**, with five explicitly non-executable request classes. No provider, current settings store, application index, clipboard or native tool was called. Source verification establishes existing construction/dispatch paths; it does not establish present runtime availability or successful native behavior.

The machine-readable authority for exact fields is [catalog/catalog.json](catalog/catalog.json). Its IDs, descriptions of eligibility, proposed aliases and subset tags are research metadata, **not existing registered discovery records**. The existing full `Action {label,desc,action,args}` is preserved separately from that synthetic ID. [Current architecture](CURRENT_SEARCH_ARCHITECTURE.md) explains canonical routing/score/pending/activation; the closed [audit handoff](../01-static-audit/SEARCH_HANDOFF.md) supplies deeper source and privacy boundaries.

## 1. Research coverage and proposed shipping scope

| Subset | Count / owners | Status and later evidence rule |
|---|---|---|
| `SEARCH_F001_INITIAL` | 11 descriptors / 9 builtin owners | Exactly the planner's raw [SEARCH-F001 proposal](roles/planner/follow-on/SEARCH-F001-built-in-discovery-metadata/GOAL.md). Proposed metadata foundation only; no production implementation, ranking or UI integration. |
| `RESEARCH_ONLY_EXTENSION` | 4 descriptors / 2 more builtin owners | Empty Diff plus File Search open/filename/content-mode openers. Useful existing capabilities for research comparison; outside the raw foundation's source/test scope. No shipping expansion accepted here. |
| Non-executable request classes | 5 | Explicit unsupported/out-of-scope meanings, separate from the executable descriptor list. No dummy action or fallback execution. |

Future relevance reports must compare full-research and initial-11-only runs on the **same case denominators and labels**, state absent-target cases explicitly, and report coverage separately. A case targeting a research-only descriptor must not silently disappear from initial-subset metrics. Gains using extra descriptors cannot justify an eleven-descriptor shipping implementation. M3-A must either retain these four as nonshipping research or amend the canonical foundation with their actual owners, payload contracts and focused tests; raw planner files stay unchanged. No evaluation cases or scores are produced at M1-A.

## 2. Existing catalogs are different domains

| Domain | Actual owner / what it contains | Relation to this catalog |
|---|---|---|
| Application/object Actions | `actions::Action`, indexed/custom application catalog | Real launch targets, labels, paths and args; local nonempty scoring is under `app`. No user application records were copied. [D1] |
| Live provider content | `Plugin::search`, including Omni's configured folders/bookmarks/apps/notes/todos | Dynamic content/results, pending work and provider-specific query grammar. A note or file result is not a tool descriptor. Do not invoke providers to harvest metadata. [D2] |
| Command shortcuts | `Plugin::commands`, `commands_filtered`, GUI command cache | Existing browse/fallback/completion inputs. Usually `query:...`; not the same Action as a tool's returned opener. Preserve current set/order behavior. [D3] |
| Settings metadata | `PluginCatalogSnapshot`, `SettingsCatalog` | Immutable identity/origin/epoch/enablement/declared capabilities, plus a separate Settings navigation projection. Reuse identity/ambiguity ownership, not Settings search entries. [D4] |
| Contextual actions | Universal Actions and current target adapters | Availability and operations on selected existing targets; source/surface/confirmation authority remains there. No second action dispatcher is implied. [D5] |
| This research tool catalog | Synthetic IDs + exact fixed opener Actions + proposed vocabulary | Opens an existing tool after explicit selection. Does not resolve a user document, start a search, interpret a path or grant mutation permission. |

## 3. The eleven foundation descriptors

All actions below have `args: null` (`None` in Rust). Labels/descriptions and shortcut full Actions are recorded exactly in JSON. The shorter table names are explanatory. These plugins declare `search` and use default `ActionFilters`; the matching query head routes to the owner. The defaults do not establish complete protected-prefix admission for future discovery. [G1]

| Synthetic ID | Builtin owner / existing explicit query | Exact opener `Action.action` | Existing bounded capability / limitation |
|---|---|---|---|
| `tool.json.open` | `json_utility` / `json` | `json_utility:open` | Open strict JSON formatter/minifier; no natural-language payload. [T1] |
| `tool.json.format` | `json_utility` / `json format`, `json pretty` | `json_utility:format` | Open format intent. `pretty` is an existing query alias, not another descriptor. [T1] |
| `tool.json.minify` | `json_utility` / `json minify` | `json_utility:minify` | Open minify intent; no automatic clipboard/text load. [T1] |
| `tool.hash.open` | `hash_generator` / `hash` | `hash:open` | Text, explicitly loaded clipboard text or selected local file; SHA-256/SHA-512/legacy SHA-1. No automatic input capture. [T2] |
| `tool.xml.open` | `xml_toolkit` / `xml` | `xml:open` | Existing bounded offline UTF-8 XML browsing/query/validation/format/save. No inferred XPath, source or output. [T3] |
| `tool.regex.open` | `regex_tester` / `regex` | `regex:open` | Existing local Rust Regex Tester; no inferred arbitrary pattern/input or PCRE compatibility claim. [T4] |
| `tool.table.open` | `table_utility` / `table` | `table:open` | Existing CSV/TSV inspection/filter/export profile. Materialized source/view/selection authority remains session-owned. [T5] |
| `tool.csv.open` | `csv` / `csv` | `csv:open` | Strict comma-delimited viewer; not a general spreadsheet engine. [T6] |
| `tool.image_compare.open` | `image_compare` / `imgdiff` | `imgdiff:open` | Exact RGBA image comparison with explicit inputs; no semantic similarity. [T7] |
| `tool.duplicates.open` | `duplicate_finder` / `dff` | `dff:open` | Byte-identical-file discovery in explicitly selected folders; no inferred deletion. [T8] |
| `tool.activity.open` | `activity` / `activity` | `activity:open` | Bounded payload-free background-work snapshots; not Macro run-history browsing. [T9] |

Current source connects these through `commands/parser.rs`, typed domain handlers and existing GUI hosts. General/format/minify are distinct JSON intents; the table/CSV profiles are distinct existing commands. Catalog metadata must reuse those payload producers rather than derive an executable action from a label. The parser's handling of optional args differs by command; **all catalog entries deliberately carry None**, without claiming a universal parser rejects every non-None arg. [X1]

## 4. Four research-only existing openers

| Synthetic ID | Existing explicit query | Exact fixed wire and decoded payload | Scope guard |
|---|---|---|---|
| `tool.diff.open` | `diff` | `diff:open:eyJsZWZ0IjpudWxsLCJyaWdodCI6bnVsbH0` → `{"left":null,"right":null}` | Empty opener only, no path canonicalization, read, comparison or file mutation performed by catalog construction. [T10] |
| `tool.file_search.open` | `fs` | `file_search:open`; no payload | Open current local filename/content tool. Runtime executable discovery/configuration is not probed. [T11] |
| `tool.file_search.filename` | `fs file` | `file_search:mode:eyJraW5kIjoiZmlsZSJ9` → `{"kind":"file"}` | Mode-only opener. `kind_label` presents `filename`; the serialized enum value is `file`. [T11], [T12] |
| `tool.file_search.content` | `fs content` | `file_search:mode:eyJraW5kIjoiY29udGVudCJ9` → `{"kind":"content"}` | Mode-only opener. No root/search text is supplied; not `file_search:start:`. [T11], [T12] |

Diff's source serializes `DiffOpenPayload` using compact UTF-8 JSON and URL-safe base64 without padding. File Search modes use the same encoding method with their own typed payload. The JSON strings here were derived from those fixed source structs using standard-library text/base64 tools, not by invoking Rust or a provider. No natural-language words can become a root, left/right path, pattern, content, `args` or query override. [T10], [T12]

`FileSearchHost` distinguishes Open, SetMode and StartSearch. GUI `open_with_mode` changes mode and opens; `open_and_start` separately assigns root/text and starts. Diff empty payload opens the existing dialog; supplied-path branches remain outside discovery. Opening may reveal an existing retained draft/session; this research does **not** promise all existing dialogs erase prior content or retire all prior work on open. Existing lifecycle/preview/native/output ownership continues unchanged. [X2]

## 5. Existing shortcuts versus returned opener Actions

| Provider | Existing `commands()` labels | Shortcut wire / difference from opener |
|---|---|---|
| JSON | `json`, `json format`, `json pretty`, `json minify` | Each `query:<label>`, `args: None`; descriptions `JSON formatter and minifier`. Query reenters canonical routing rather than directly invoking JSON intent. [T1] |
| Hash/XML/Regex/Table/CSV/Image/Duplicates/Activity | `hash`, `xml`, `regex`, `table`, `csv`, `imgdiff`, `dff`, `activity` | Each `query:<label>`, `args: None`. Exact current descriptions remain in JSON, not replaced by proposed aliases. [T2]–[T9] |
| Diff | `diff` | **Direct encoded empty Diff opener**, not `query:diff`; command description `Open file and folder comparison` differs from search opener description. JSON preserves both full Actions. [T10] |
| File Search | `fs`, `fs file`, `fs content`, `fs here`, `fs here file`, `fs here content` | Each `query:<label>`, `args: None`. The first three map to catalog openers; the `here` routes remain existing shortcuts outside this fixed-opener list, with their own subsequent grammar. [T11] |

The catalog's `existing_shortcuts` lists shortcuts associated with each included descriptor, not every command across the repository. File Search also accepts filename/content aliases in its parser; a start/request-directory Action derived from explicit query text is not a new safe fixed opener. JSON-only `pretty` is existing syntax; `proposed_aliases` such as natural-language phrases are model-authored research vocabulary, neither current command registrations nor frozen evaluation labels. [T13]

## 6. Eligibility and protected syntax

Every included owner declares `search`. Current canonical provider admission separately checks an enabled-provider set and configured capability restriction: missing restriction is unrestricted; a present list lacking `search` is denied. This current function does not itself require a declared `search` string. Snapshot `enabled`/declared metadata and caller configuration must not be confused. Command enumeration currently filters enabled names, not that capability map; preserving shortcuts and designing stricter discovery admission are separate decisions. No real user policy was loaded here. [G2]

The planner's future discovery foundation requires a unique current builtin owner, enabled snapshot/caller policy, declared and configured search eligibility, immutable exact Action and source epoch/revision. Future UI integration additionally needs fresh-user list/grid origin, complete reserved-route metadata, no pending/deferral/unstable catalog, no canonical rows, and activation revalidation. These are **proposed requirements**, not current root-row guarantees, and do not follow merely from this JSON catalog. `has_enabled_query_prefix` omits disabled/denied providers and uses provider read locks; it cannot certify all reserved forms. [G1], [G2]

| Protected class / representative current forms | Current owner / reason to preserve |
|---|---|
| Explicit `app` and `g` | Existing local-app route and special web provider-selection route. Preserve corrected fallback distinctions; never infer discovery just because a provider returns no rows. [Architecture route matrix](CURRENT_SEARCH_ARCHITECTURE.md#2-exact-current-routefallback-matrix). |
| Provider heads: `json`, `hash`, `xml`, `regex`, `table`, `csv`, `imgdiff`, `dff`, `activity`, `diff`, `fs` | Existing source-backed entries above. They remain reserved when disabled; these eleven owner heads are a **partial inventory**, not a universal prefix registry. |
| `o`, `=` and other current provider grammar | Omni content search and calculator already own these interpretations. Calculator search can append history under existing settings. Metadata enumeration must not execute them. [D2], [G3] |
| `kind:` / `id:` filters, quote/escape/raw spelling | Shared ActionFilters owns classification/reconstruction; matching-prefix Literal bypass is per-provider. AUD-F001 addresses only terminal raw escape; no general rewrite is performed. [Closed grammar handoff](../01-static-audit/SEARCH_HANDOFF.md#2-existing-routes-and-query-grammar). |
| QR literal input, snippets and other payload-bearing prefixes | QR can contain spaces/quotes/filter-looking text/newlines. Snippet no-tag byte preservation is helper-local; upstream ActionFilters may already have reconstructed text. Preserve current owner policy and unresolved full registry. [Closed review corrections](../01-static-audit/REVIEW_AND_LIMITATIONS.md). |
| Raw paths, URLs, shell/process text, regex/pattern-like text, selected content | Future discovery admission must conservatively protect these from natural-language prefill or guessed execution. This row is an admission requirement, not a claim every such string already has a universal parser or identical runtime route. |
| Programmatic/Macro/queryexec/saved/radial/dashboard/hotfolder queries | Preserve exact consumer cardinality, surface/source, saved identity and unattended eligibility. Programmatic text visible in the root is not automatically fresh-user origin. [Architecture consumer map](CURRENT_SEARCH_ARCHITECTURE.md#6-automatic-and-programmatic-consumers--why-shared-vectors-remain-unchanged). |

No destructive or unattended capability is conferred by discovering/opening a tool. A request to delete, replace, launch, extract or mutate input is not rewritten into a guessed operation. Later UX/ranking stages must specify opener-versus-abstention behavior, ambiguity and negative labels before evaluation; M1-A records source limits only.

## 7. Unsupported meanings and deferred operation authority

| Request class | Evidence and current disposition |
|---|---|
| Extract a ZIP archive now | `archive/mod.rs` explicitly bounds itself to metadata/catalog and pure extraction planning before entry payload/destination access. **No executable catalog capability verified.** Do not equate a planning module with extraction UI/command. [N1] |
| Open Macro run-history browser | Bounded summary collector exists; no run-history UI command established by this targeted source scope. Activity opener does not prove one. **No executable catalog capability verified.** [N2] |
| Export every unmaterialized row of an arbitrarily large file | Existing table export acceptance ties source/view/selection/basis/resource authority. **Outside verified capability**; no full-file export inferred from a preview or generic table label. [N3] |
| Automatically delete duplicates | Catalog exposes byte-identical-file discovery after explicit folder selection. **No inferred operation** and no deletion Action in this catalog. This is a catalog limit, not a global absence proof for every unrelated file command. [T8] |
| Find visually similar images using AI | Existing Image Compare is exact RGBA comparison. **Outside verified capability**, not a perceptual/image-search implementation. [T7] |

These classes retain source citations and non-executable status in JSON. They are future synthetic negative/adversarial material, not a backlog of five requested features. No actionable new recommendation is accepted for them.

## 8. Data schema, provenance and remaining limits

Each executable record requires unique synthetic `id`, source owner/origin, subset, full current Action, typed-command trace, existing queries/shortcuts, declared/current/proposed eligibility, no-prefill statement, target/operation limits, explicit proposed-alias provenance and pinned source evidence. Encoded records additionally store the fixed decoded payload. The unsupported list has a distinct non-executable status/reason/evidence shape; reference domains are descriptive and contain no private records.

At M1-A `freeze_state` was `DRAFT_NOT_EVALUATION_FROZEN`; source SHA and deterministic IDs were provenance, not a freeze receipt. M1-C now records `FROZEN_FOR_EVALUATION_V1` and the [canonical UTF-8/LF manifest](evaluation/freeze_manifest.json), preserving every other byte of decoded text rather than reformatting JSON. Windows CRLF/Git normalization is not treated as data drift; no raw working-copy/Git-byte equivalence is asserted.

Coverage is intentionally bounded: eleven builtin owners, exact fixed opener construction, relevant parsers/handlers/hosts and representative source tests read statically. Dynamic plugins, every command/subcommand, current user enablement, real files/content/executable availability, native success, relevance, resource usage and latency remain unverified. Future ~300-case work is a planned synthetic evaluation, not measured human need or evidence that these 15 descriptors cover the whole application. SEARCH-F001 stays proposed; all ranking/UI/extra-owner implementation decisions await later evidence and complete packages.

## Source references

[D1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/actions/mod.rs#L12-L19
[D2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/omni_search.rs#L129-L165
[D3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1174-L1185
[D4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/settings_editor/catalog.rs#L272-L318
[D5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/universal_actions/model.rs#L104-L145
[G1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L100-L148
[G2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1310-L1437
[G3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins_builtin.rs#L58-L110
[T1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/json_utility.rs#L8-L82
[T2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/hash_generator.rs#L6-L44
[T3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/xml_toolkit.rs#L6-L44
[T4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/regex_tester.rs#L6-L44
[T5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/table_utility.rs#L6-L44
[T6]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/csv.rs#L6-L44
[T7]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/image_compare.rs#L6-L44
[T8]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/duplicate_finder.rs#L6-L44
[T9]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/activity.rs#L6-L44
[T10]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/diff.rs#L14-L122
[T11]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/file_search.rs#L661-L738
[T12]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/file_search/actions.rs#L12-L76
[T13]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/file_search/query.rs#L16-L78
[X1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/commands/parser.rs#L380-L538
[X2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/command_host.rs#L886-L948
[N1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/archive/mod.rs#L1-L22
[N2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/mkmacro/runtime/summary.rs#L158-L229
[N3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/table_utility_session.rs#L1917-L1961

Additional exact payload/dispatch evidence: [Diff struct and prefix](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/diff/query.rs#L5-L11), [Diff encoding](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/diff/query.rs#L88-L97), [File Search shortcuts](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/file_search.rs#L58-L72), [File Search dialog mode/open/start separation](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/file_search_dialog.rs#L698-L731), [JSON typed intent parser](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/commands/parser.rs#L943-L950), [File Search/Diff handlers](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/commands/handlers/file_search_diff.rs#L24-L150).
