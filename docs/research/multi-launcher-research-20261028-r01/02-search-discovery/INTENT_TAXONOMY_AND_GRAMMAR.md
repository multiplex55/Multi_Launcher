# Intent taxonomy and grammar — M1-B design

Status: **PROVISIONAL_RESEARCH_DESIGN**, not implemented or accepted as production scope. Source baseline `59156d96adcc73939c7171487a0cb582e3286587`; accepted M1-A checkpoint `80b2050a6e6c0a78b90bbfb60db822d399d2c82d`, normally pushed (parent-reported). Current facts remain in [architecture](CURRENT_SEARCH_ARCHITECTURE.md) and [catalog](COMMAND_AND_CAPABILITY_CATALOG.md). All new types/rules below are proposed. No ranker, provider or prototype was executed.

The feature discovers an **existing tool opener** after a fresh user edit. It does not infer an operation over supplied data. The same admission rules apply to P0/P1/P2; ranking never recovers a candidate rejected by authority, protected syntax or unavailable ownership. The [numeric policy file](evaluation/candidate_policies.json) and [ranking specification](RANKING_SPECIFICATION.md) define bounded English research behavior; [runtime design](RUNTIME_AND_INTEGRATION_DESIGN.md) owns the missing integration contracts.

## 1. Taxonomy grounded in existing tools

| Intent/object family | Catalog mappings / desired opening | Confusable or unsupported meaning / explicit boundary |
|---|---|---|
| Inspect structured data | JSON general, XML, Table, CSV | CSV versus general CSV/TSV Table is a real profile choice. Open a tool; do not infer a file or load query text. |
| Format/minify JSON | JSON format/minify intents | Format and minify are distinct fixed mode Actions. YAML/JSON5 conversion, archive compression and automatic clipboard formatting are not supplied. |
| Compute a digest | Hash | Existing hash algorithms and explicit later input; no signature verification, password cracking or inferred local-file read. |
| Validate/query XML | XML | Opens the bounded local XML tool; no inferred XPath/schema/network resource. `xpath` is vocabulary for discovery, not evaluated input. |
| Test a regular expression | Regex | Opens the Rust Regex Tester; literal pattern/input remains protected, no PCRE/script compatibility claim. |
| Filter/inspect tabular data | Table or strict CSV | The row/view/export owner remains authoritative. Full export of unmaterialized rows is not advertised. |
| Compare images/files/folders | Image Compare or empty Diff | `compare` alone is ambiguous. Exact RGBA differs from native file/folder comparison; neither is semantic/AI matching. Diff remains a research-only extension. |
| Find byte-identical files | Duplicate Finder | Read-only discovery after explicit folder choice. Delete/remove/replace intent abstains under this design. |
| Find filenames/file contents | File Search general/filename/content openers | Research-only mode selection, never implicit start or root/pattern prefill. Existing `fs` grammar keeps supplied inputs. |
| Inspect background work | Activity | Activity snapshots are not a Macro run-history UI or process-kill tool. |
| Capture/configure/automate/mutate | Existing application owners outside this catalog | No descriptor invented for broad taxonomy completeness. Canonical commands, Universal Actions and unattended policies remain unchanged. |

Intent and object tags are finite model-authored research metadata, not a general semantic parser. `entry_roles`, `intent_vocabulary` and `object_vocabulary` in the policy file enumerate every membership. A query can carry several recognized roles; P2 measures their coverage rather than treating one verb as proof of intent. P0/P1 do not use role vocabulary. Alias/taxonomy metadata is maintained beside the source-owned descriptor definitions, versioned and reviewed with them; it never becomes an executable string.

## 2. Ordered admission: raw input stays owned by canonical search

Canonical `search()` still runs exactly as today. Discovery observes a completed/current outcome and adds **no** rows to `self.results`, `LauncherSearchOutcome.actions`, command cache, completion or automatic resolvers. Its independent gate returns a reason, not a rewritten query. Evaluate the following order; the first failure is the stable diagnostic (all gates are still hard requirements):

1. Feature enabled by acknowledged settings, app accepting input, no settings reconciliation/publication transition; actual surface is LauncherList/LauncherGrid and proposed origin is FreshUserEdit in the current visible root session. Programmatic query replacement, Macro, saved/radial/dashboard/hotfolder paths fail here.
2. Current canonical query generation matches; no pending providers, no provider deferral, stable result catalogs, current provider/result/metadata/policy revisions, and **zero canonical rows**. Canonical Results with zero rows is distinct from Pending. A capacity-deferred empty result is not eligible. Low-scoring existing rows suppress discovery and must be counted later.
3. Reserved-route snapshot is Complete/current for every relevant registered owner and reviewed builtin grammar; no unclassified global grammar, missing registration, stale prefix metadata, uncertain coverage or owner transition. See runtime coverage design. Missing complete coverage blocks admission even if `has_enabled_query_prefix` returns false.
4. Raw size/shape and explicit-route protection below. Check raw text before any token normalization. Known prefixes remain reserved when disabled/denied; exact and Literal inputs never pass through the discovery tokenizer.
5. Natural-content token bounds, finite unsupported/dangerous-intent rules, then current descriptor eligibility. No eligible descriptor means abstention; do not relax owner gates to fill a list.

Gate results distinguish `ExcludedSurfaceOrOrigin`, `Disabled`, `BusyOrStale`, `CoverageUnknown`, `ProtectedSyntax`, `InputBound`, `UnsupportedIntent` and `NoEligibleOwner`. These are proposed typed reason variants, not new command protocols. Runtime/UI may omit diagnostics for normal protected routes; tests and synthetic evaluation record the actual reason. Later metric denominators retain all gated cases.

## 3. Protected grammar and regression boundaries

| Raw form / recognition | Discovery behavior / byte boundary | Existing authority and regression example |
|---|---|---|
| Complete snapshot contains first whitespace-delimited head, ASCII case-insensitive; special raw-leading grammar such as `=` | Bypass discovery before normalization; never remove a disabled owner's reserved head. | PluginManager per-provider plan and special root routes. `g checksum` stays web; `app hash` stays app; `json format` stays exact JSON. [S1], [S2] |
| `kind:` / `id:` and any colon-bearing input | Protected, not natural-language metadata. | Shared ActionFilters and each provider's policy. `json kind:file` is not interpreted as a discovery request. Colon-wide conservatism also protects unknown namespace/shell/drive forms. [S1] |
| Matching-prefix Literal input, e.g. QR | Protected by head inventory, unchanged raw text. Literal is only active for matching declared head; no global parser change. | QR/parser owns `qr kind:file "a b"` and multiline payload. [S1], [S3] |
| Paths, UNC, extensions, URLs, process/shell syntax | Any `/`, `\`, `:`, `.`, leading hyphen, or shell/metacharacter punctuation fails natural grammar. | Existing exact Action/parser/native owners; no path normalization/canonicalization or filesystem stat. `C:\\tmp\\a.json`, `https://x.invalid`, `report.csv`, `--flag`, `echo x | ...` remain canonical. |
| Quoted arguments, regex, JSON/XML-looking payloads, bracket syntax | Any quote/apostrophe/backtick, bracket/brace/parenthesis, angle delimiter, operator or punctuation outside the allowlist is protected. | Existing provider/typed payload owners. `"compare files"`, `/a.+b/`, `a*b`, `{"x":1}`, `<x/>` are never stripped into an intent. |
| Calculator, Macro and clipboard pipeline syntax | Reserved heads/raw markers and punctuation fail above; no syntax-specific fuzzy repair. | Existing calculator/Macro/clipboard owners. `= 2+2`, `mkmacro ...`, a saved Action or `queryexec:` must retain exact meaning. [S4], current architecture consumer map. |
| Raw text with controls/newline/tab/non-ASCII | Protected for initial English-only discovery; no Unicode folding, transliteration or removal. | Raw canonical text remains unchanged. `café`, fullwidth `ｊｓｏｎ`, Turkish dotted I, emoji, NBSP, bidi/zero-width characters and pasted multiline data do not become ASCII guesses. |
| Programmatically assigned root query | Origin exclusion, even when text looks natural and root is visible. | Existing automatic consumer controls execution. A later actual user insertion/deletion is the only origin transition; focus/show alone does not change it. |

The initial admitted raw alphabet is ASCII letters/digits, U+0020 spaces and **internal hyphens**. A hyphen must have an ASCII alphanumeric immediately on both sides; leading/trailing/repeated/spaced hyphens are protected. Other punctuation, including a trailing question mark or period, is deliberately rejected. This conservative loss of ordinary punctuated questions is a measurable coverage limitation, not a claim those strings are unsafe in the current application. Do not silently broaden the alphabet after seeing holdout errors.

No complete live prefix inventory is claimed by this document. The catalog's eleven owner heads, web/calculator/Omni examples and current enabled-prefix helper are partial evidence. A future integration package must close the source-reviewed manifest/registration coverage requirement before its UI can be enabled; otherwise all candidates remain unavailable with `CoverageUnknown`. This does not block pure metadata/ranking research.

## 4. Natural-language normalization and bounds

Apply only after raw admission, in this exact order:

1. Reject raw UTF-8 length over **256 bytes**; split by U+0020, ignore empty pieces, reject over **24 raw tokens**. Reject any raw token longer than **24 ASCII characters**. Empty/all-space input is canonical browse, excluded.
2. Lowercase ASCII A–Z only. Replace whole hyphenated raw tokens `sha-256`, `sha-512`, `sha-1` with `sha256`, `sha512`, `sha1` respectively. Split remaining internal hyphens as token separators; tokenize with `[a-z0-9]+`.
3. Apply the policy file's **finite** inflection map (files→file, images→image, rows→row, etc.), no general stemming, accent folding or locale processing. Remove its exact stopword set, including generic open/show/tool/utility words. This is vocabulary processing, not removal from canonical raw text.
4. Build phrase sequence by retaining the first occurrence of each remaining token in original order; build scoring set by sorting those tokens in ASCII order. Repetition contributes no score. Reject more than **12 unique content tokens**, fewer than **4 total ASCII letters across unique content tokens**, or a single content token shorter than **4 characters**. Digit-only text consequently abstains. Bounds apply before/after dedup as specified, so unlimited repetition cannot evade work limits.

Field phrases/role vocabulary use the same lowercase/compound/inflection/stopword processing, but source metadata size limits are validated at projection construction. Do not apply user protected-prefix rejection to metadata labels. Phrase equality uses the deduplicated ordered phrase, while coverage uses the unique set. `open checksum tool`, `checksum checksum` and `checksum` therefore have the same content phrase/score when raw bounds admit them. `sha-256` is a narrowly documented compound; arbitrary punctuation is not repaired.

The stopword/inflection/vocabulary lists are executable research **data**, entirely enumerated in the JSON policy file. No embeddings, dictionary downloads, spell checker, language detection, morphological dependency or unrestricted synonym expansion is introduced. English-first means unsupported locale input abstains; it does not mean English semantics are complete.

## 5. Operation intent, ambiguity and no-match

After normalization, any exact token in the common dangerous-verb set (delete/remove/erase/wipe/replace/overwrite/execute/run/kill/terminate/extract/unzip/compress/encrypt/decrypt/send/upload/download/paste/write) yields `UnsupportedIntent`, with **no executable discovery row**. This intentionally suppresses even a harmless tool opening for those phrases. Catalog's draft minify alias changed from `remove json whitespace` to `strip json whitespace` to avoid an unreachable alias; exact production Action/shortcut fields are unchanged.

Three finite negative rules also abstain: a macro/macros + history combination; image/photo/pixel plus ai/semantic/perceptual/similar; export plus all/full/unmaterialized plus row/file. A rule matches when at least one token from each of its groups occurs; normalization/inflections precede matching. This is a documented conservative classifier, not proof arbitrary destructive English is understood. Even a missed paraphrase cannot manufacture mutation args because every proposed row is an existing fixed opener and never auto-activates.

`compare` may yield an explicit chooser for file/folder versus image tools; `find files` can remain broad among File Search and Duplicate Finder. Low lexical evidence abstains. A strong suggestion means “these fields support opening this tool,” never a calibrated confidence probability or instruction to execute an operation. Missing/disabled/stale owner cannot be recommended. M1-C will define exact visible text/keys; the result-state contract and numbers are already fixed in the ranking spec.

## 6. Versioning, scope and next evidence

At M1-B policy revision `g02-m1-b-v1` and catalog `g02-m1-b-draft-2` were draft. M1-C freezes the unchanged numerical policy and catalog metadata as `g02-eval-v1`; see the evaluation freeze manifest. Alias and role mappings are author-visible hypotheses chosen before a frozen corpus; they are not independently judged labels. Full 15-descriptor and initial 11-descriptor evaluation must use the same cases and denominators, report absent targets and gated coverage, and never use research extras to justify the narrower foundation. M1-C freezes inputs/candidates/protocol before M2-A; no prototype or final shipping algorithm is accepted here.

## Pinned current owners

[S1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1310-L1437
[S2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/search.rs#L1125-L1392
[S3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/qr.rs#L6-L68
[S4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins_builtin.rs#L58-L110
