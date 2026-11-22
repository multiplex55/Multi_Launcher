# Codex Goal — Productivity Toolkit Expansion

**Repository:** `multiplex55/Multi_Launcher`  
**Goal ID:** `productivity-toolkit-expansion`  
**Companion:** `CODEX_KICKSTART_PRODUCTIVITY_TOOLKIT.md`  
**Planning reference:** `master` at `c4170d0853ce462b44840be54c25c4a8d1b339ac`, checked November 17, 2026.  
**Approval:** Q1-A — include the internal notification inbox **and a working, opt-in external Windows notification capture route**; optional Windows registration/packaging and permission setup are authorized.  
**Execution shape:** One Goal, one shared task branch, one existing checkout, sequential implementation checkpoints. The thematic milestones below are not separate Git categories or branches.  
**Status:** Implementation contract. Source inspection was read-only; this document does not claim that application builds, tests, printing, or native notification capture have been executed.

## A0. Mandatory Chained Git Branch Bootstrap — Before All Application Work

### M0-A — Exact parent → fresh child branch → baseline → initial push

Read `AGENTS.md`, applicable local instructions, this entire Goal, and its Kickstart before starting. Repository instructions govern engineering; this Goal governs this feature's approved scope. Historical plans and acceptance campaigns are reference material, not additional mandatory work.

1. **Inspect without changing application state.** In the single existing checkout inspect `git status -sb`, current branch, `git rev-parse HEAD`, remotes/upstream, staged/unstaged/untracked files, and any queue/orchestrator/execution record identifying the immediately preceding completed Goal/category. Do not edit application code or run implementation builds/tests before completing M0-A.
2. **Resolve the required parent, not merely the current checkout.** For a continuing sequence, use the exact `final_tip_sha` of the immediately preceding completed Goal/category branch. Record `parent_branch` and `parent_tip_sha`; verify that the commit exists and that the predecessor's intended work is committed. Fetch a remote-only predecessor safely, but do not substitute a stale remote tip for a newer recorded local tip. Conflicting ancestry records are a bounded blocker, not permission to guess.
3. **Only the first Goal in a genuinely new chain may default to the up-to-date primary integration branch**, normally `master`. A safe fetch may establish its tip without pulling or changing user work. The planning reference above is evidence about source inspected, **not an instruction to bypass a predecessor or reset to that SHA**. A documented current-HEAD fallback is allowed only for a genuinely new chain with no predecessor and no loss or accidental inclusion of user work.
4. Choose a fresh, unique lowercase kebab-case branch, for example `feat/productivity-toolkit-expansion`, adding a suffix when needed. The Goal and Kickstart describe **the same one branch**. An already-active branch does not waive creation of this Goal's fresh child branch.
5. Protect all pre-existing work. Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, overwrite user changes, or stage unrelated files to make the transition convenient. If switching to the required parent would endanger local work, report the precise blocker instead of choosing a different parent.
6. Create and switch directly from the verified SHA: `git switch -c <unique-task-branch> <parent_tip_sha>`. Verify active branch and HEAD. Do not implement on the predecessor or on `master`/`main`.
7. Make this branch's own **M0-A baseline commit**. Prefer a small task ledger at `docs/plans/execution/standalone/productivity-toolkit-expansion/STATUS.md`, or an existing authoritative ledger if the orchestrator provides one. An intentional empty baseline is acceptable: `git commit --allow-empty -m "chore: [M0-A] establish productivity toolkit branch baseline"`. Stage only explicitly adopted task documents/ledger changes. Record `baseline_sha` separately and verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`.
8. Attempt **one ordinary non-force upstream push**, normally `git push -u origin HEAD`. If credentials, permissions, remote access, or policy prevent it, record the exact failure and continue safe local work. Do not bypass restrictions, rewrite history, or retry in a loop.
9. All later checkpoints remain on this branch. Attempt one normal best-effort push after each checkpoint. A push failure never invalidates the local commit or prevents independent work.
10. At completion, record the actual final branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, significant checkpoint SHAs, `final_tip_sha`, push state, and checkout state. The next Goal must branch from **this exact final tip**, not from `master` or a common ancestor. Intermediate merges, rebases, squashes, and cherry-picks are neither needed nor permitted merely to advance the chain. Final integration is a separate approved boundary; do not merge this work into master automatically.

**Single-checkout prohibition:** Never use `git worktree add`, `git worktree move`, `git worktree prune`, auxiliary checkouts, secondary clones, or agent-specific branches for this workflow. Planners/reviewers may perform safe read-only work concurrently; one implementer owns source writes, and only one build/test process should use the shared checkout at a time.

**Bootstrap acceptance:** The active branch is new and unique; its recorded parent matches the predecessor or legitimate initial integration tip; the baseline is a direct child of that parent; the initial push was attempted or its environmental impossibility recorded; no unrelated work was staged/lost; no worktree or parallel implementation branch was created. M0-A is a hard prerequisite for M0-B and every feature checkpoint.

**Commit:** `chore: [M0-A] establish productivity toolkit branch baseline`.

## A. Goal / Desired Outcome

Deliver all 14 selected improvements as one coherent expansion of Multi Launcher's existing Rust/egui application. Improve numerical analysis and discoverability, keyboard navigation, Markdown authoring/export, password generation, ZIP convenience, downloads review, managed file transfers, and persistent notifications. Preserve existing workflows rather than replace their implementations with parallel systems.

| ID | Deliverable | Principal checkpoints |
|---|---|---|
| L11 | More complete keyboard coverage for ordinary controls | Every new UI checkpoint; M12-A/B |
| P5 | Vector sorting, uniqueness, filtering, mapping, matching counts/indices | M2-A/B |
| P6 | Cumulative, rolling, and adjacent-change calculations | M3-A |
| P7 | Percentiles, dispersion, covariance, correlation, linear fits/prediction | M3-B/C |
| P10 | Histograms and an optional cumulative distribution view | M4-A/C |
| P11 | Log axes, bounds, reference lines, ticks, appearance, labels | M4-B/C |
| CP-DOC | Complete searchable, function-oriented in-app API reference | M5-A/B |
| N5 | Move Markdown sections from their outline | M6-A/B |
| N3 | Paginated PDF export, preview, and printing with formatting | M7-A/B |
| U13 | Configurable password/passphrase generator with privacy protections | M8-A/B |
| ZIP-DEST | Archive-adjacent destination suggested on ZIP selection | M1-A |
| X9 | Queued file operations with conflicts, progress, verification, history | M9-A/B/C/D |
| X8 | Downloads Triage Center with reviewed state and explicit actions | M10-A/B |
| X25 | Internal notification inbox plus optional-setup external capture | M11-A/B/C/D |

Success means these features are usable through the launcher and relevant existing tools, work in docked/popped-out presentations where ordinary utilities do, have accurate documentation, and preserve established data and focus behavior. A menu item, mock provider, disabled control, or explanatory stub does not substitute for implemented behavior.

## B. Current Codebase Findings

These are observed source facts at the planning reference, not guesses about future files. Reconcile only affected differences after M0-A. New module names later in this plan are explicitly **proposed additions**, not existing source locations.

| Area | Inspected source and established foundation | Planning consequence |
|---|---|---|
| Repository orchestration | `AGENTS.md`, `.codex/config.toml`, `.codex/agents/{planner,implementer,reviewer}.toml` define bounded planning, single-writer implementation, review, and checkpoint rules. | Use the configured agents and model settings; do not replace them with model names from old conversations. |
| Numeric domain | `src/calculation_pad/{document,script,loops,vector,limits}.rs`. `CalculationVector` has finite dimensionless `f64` components and a 1,024-component cap. `CollectionOperation` implements immutable vector operations. `WorkBudget` distinguishes ordinary and vector work and cancellation. | Extend the existing value/evaluation model; preserve budgets and failure publication rules. |
| Local functions | `src/calculation_pad/README.md` and `script.rs` describe previously declared, bounded formula functions. Function bodies cannot capture document variables. | Reuse formula calls for map/filter; extra values must be explicit callback arguments. |
| Existing statistics | The Pad guide documents `sum`, `average`, `median`, `min`, `max`, and population `stdev`; empty sum is zero, other listed statistics reject emptiness. | New statistics must not reinterpret old names or empty-input behavior. |
| Charts | `src/calculation_pad/chart.rs` defines `ChartKind`, `ChartDefinition`, `ChartSource`, `ChartVectorReference`, `ChartDataset` and bounded rendering. `src/gui/calculation_chart_dialog.rs` uses `ChartRuntime`, `ChartStamp`, `ChartActions`, cached scenes and host-owned commits. | Extend one definition/data/scene pipeline for screen and image export; retain document/evaluation identity checks. |
| Pad persistence and navigation | `src/calculation_pad/{session,persistence,dependencies,rename,syntax}.rs` and `src/gui/calculation_pad_dialog.rs` own sessions, definitions and editor integration. | New calls, references and chart settings must remain compatible with rename, saved documents, undo and stale results. |
| Reference content | `src/calculation_pad/examples.rs` contains `CalculationExample`, grouped examples, search, expected results and executable expectations. | Add API metadata/navigation around existing examples; do not discard the examples catalog. |
| Markdown structure | `src/notes_markdown/{mod,headings,sections,parse,authoring,mutations}.rs` supplies `MarkdownHeading`, `MarkdownSection`, `OutlineRow`, and prepared edits. `parse_sections` ends a section at the next equal-or-shallower heading. | Move source ranges with nested sections; do not regenerate Markdown from a rendered tree. |
| Markdown presentation/export | `src/gui/{note_panel,markdown_studio,markdown_export,markdown_preview,note_mutation}.rs`; `src/notes_markdown/{html,preview}.rs`. `export_note_html` and shared publication code already enforce bounded, local asset handling. | PDF/print and reorder must share authored-source and asset policies across Notes and Markdown Studio. |
| Keyboard and hosting | `src/gui/jump_mode.rs`, `src/gui/jump_mode/{ui,paint,config}.rs`, `src/gui/{utility_host,utility_window_host,utility_presentation,utility_dialog_size}.rs`. Semantic target identities and surface/generation ownership already exist. | Reuse focus ownership and hint registration; do not add a second keyboard hook/router. |
| ZIP | `src/gui/zip_extractor_dialog.rs` has `ZipExtractorDialog`, `ZipPickerKind`, `UiAction`, source/destination text/path, revision and confirmation state. `src/archive/` owns archive operations. | A suggested destination must update the real extraction draft and invalidate stale approval, not merely the visible text. |
| Random and clipboard | `src/plugins/random.rs` currently creates alphanumeric password results. `src/plugins/clipboard.rs` reads text into persisted history through `ClipboardPlugin::update_history`; `src/history.rs` stores action history. | Password privacy needs protection at both action/history and clipboard-ingestion boundaries. Hiding the GUI alone is insufficient. |
| Filesystem safeguards | `src/file_operations.rs` provides `NativeFileIdentity`, root/parent proofs, guarded native operations, and `NativeHotFolder*` result/partial-outcome types. Rename/Replace/Hot Folder use related safeguards. | Reuse applicable file identity/publication helpers, but do not treat the existing file as a ready-made general transfer queue or reuse Hot Folder authorization for manual transfers. |
| Activity | `src/activity.rs` provides `ActivityCenter`, owner/operation identities, lifecycle, progress and explicit terminal outcomes; `src/gui/activity_dialog.rs` presents them. | Transfers own execution; Activity reports it; Notifications retain selected outcomes. Avoid three controllers for the same job. |
| Existing notifications | `src/mkmacro/notifications.rs` defines `DESKTOP_AUMID`, `toast_xml`, `WindowsNotificationBackend` and own-toast registration/presentation. Existing timer/calendar/todo plugins are separate domains. | Sending an own toast is not external capture. Integrate domain events without replacing reminder scheduling or scraping toast logs. |
| Ordinary build | `Cargo.toml` targets `multi_launcher`, uses `eframe` 0.36.2 and `windows` 0.58, has optional developer binaries and normal `cargo build --release`. | Keep the normal executable build independent of optional external-listener packaging tooling. |

**Pinned repository references:** [commit](https://github.com/multiplex55/Multi_Launcher/commit/c4170d0853ce462b44840be54c25c4a8d1b339ac), [AGENTS](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/AGENTS.md), [Pad guide](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/calculation_pad/README.md), [vector domain](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/calculation_pad/vector.rs), [charts](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/calculation_pad/chart.rs), [chart UI](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/gui/calculation_chart_dialog.rs), [Markdown sections](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/notes_markdown/sections.rs), [Markdown export](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/gui/markdown_export.rs), [ZIP dialog](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/gui/zip_extractor_dialog.rs), [clipboard history](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/plugins/clipboard.rs), [file operations](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/file_operations.rs), [activity](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/activity.rs), [native own-toast sender](https://github.com/multiplex55/Multi_Launcher/blob/c4170d0853ce462b44840be54c25c4a8d1b339ac/src/mkmacro/notifications.rs).

## C. Approved Functional Requirements

The selected feature scope and Q1-A are approved. The precise API names, numerical conventions, privacy defaults and bounds below are **planning decisions made under the authorization to resolve routine details**. They describe required new behavior, not APIs claimed to exist already. If current source reveals a genuine name collision, choose one unambiguous compatible spelling, update all examples/tests/reference entries, and record it without reopening the product questionnaire.

### C1. Cross-feature behavior and keyboard coverage — L11

Retain the main launcher's single query workflow and its ability to take focus while ordinary utility windows are popped out. New ordinary utility windows use existing Dock/Pop Out, geometry, theme and ownership conventions; reopening a live utility focuses its retained draft rather than duplicating it.

Tab/Shift+Tab follow logical visible controls; arrows navigate lists/tables/options without hijacking text editing; Enter/Space act according to the focused control; Escape closes the innermost owned interaction first. Multiline Enter remains a newline. Navigation scrolls the focused item into view once when needed, not continuously on every frame.

Keep Jump Mode's existing opt-in setting and leader. Hints target only visible, enabled, current-surface controls; choosing a hint focuses/highlights without executing. A later ordinary activation still runs existing confirmation rules. Support keyboard row selection, context menus or an equivalent visible actions menu, dropdowns, checkboxes, number controls, and footer actions. No hint targets for hidden/clipped rows, native picker internals or arbitrary canvas drags.

Inventory existing ordinary utilities in M0-B, recording already covered / needs extension / intentionally pointer-only controls. Cover all ordinary tools in that inventory, not merely the new dialogs. Existing workspace/layout management is unchanged and receives no new workspace features. Native pickers retain normal OS keyboard behavior. New UIs acquire keyboard behavior when built; M12 closes remaining existing-UI gaps.

### C2. Vector transformations — P5

Add pure vector functions `sort(v)`, `sort_desc(v)`, `unique(v)`, `map(v, function_name, extra_args...)`, `filter(v, function_name, extra_args...)`, `count_if(v, function_name, extra_args...)`, and `find_indices(v, function_name, extra_args...)`.

`function_name` is a previously declared document-local formula identifier in this specific call position, not an arbitrary string to evaluate or a general first-class function value. Each callback receives the current element followed by explicitly supplied extra arguments. This supports document thresholds without changing the existing no-capture rule. No new lambda, mutable closure, external script engine or dynamic `eval` is required.

**Proposed runnable language after this feature:**

```text
values = [7, 2, 7, 4, 10]
threshold = 5
factor = 2

fn above(value, limit) = value > limit
fn scale(value, multiplier) = value * multiplier

ordered = sort(values)
ordered_descending = sort_desc(values)
distinct = unique(values)
selected = filter(values, above, threshold)
doubled = map(values, scale, factor)
matching_count = count_if(values, above, threshold)
matching_indices = find_indices(values, above, threshold)
```

Expected results are `[2, 4, 7, 7, 10]`, `[10, 7, 7, 4, 2]`, `[7, 2, 4, 10]`, `[7, 7, 10]`, `[14, 4, 14, 8, 20]`, `3`, and `[0, 2, 4]` respectively. The original `values` is unchanged.

Sorting uses ascending/descending numeric comparison and stable equal-value ordering. Uniqueness preserves first occurrence using ordinary numeric equality, treating positive and negative zero as equal and preserving the first representative. Predicates must return Boolean values; numeric truthiness is not invented. Map returns finite ordinary dimensionless scalars only. Empty map/filter/sort/unique/find_indices return `[]`; empty count is zero. Unknown callback and invalid arity are diagnosed even for empty input. Do not evaluate a callback body for nonexistent elements.

Preserve vector limits, source limits, work/cancellation accounting, local-function call-depth/recursion restrictions and line-local diagnostics. Charge callback work as well as collection traversal. Fail the operation without publishing a partial output. Callback identifiers must participate correctly in dependency tracking, function/parameter rename and function-body edits.

### C3. Cumulative, rolling and adjacent changes — P6

Add `cumsum(v)`, `cumprod(v)`, `diff(v)`, `percent_changes(v)`, `rolling_sum(v, window)`, `rolling_average(v, window)`, `rolling_min(v, window)`, and `rolling_max(v, window)`.

Cumulative operations preserve length; adjacent differences and percentage changes return `max(n-1, 0)` values. `percent_changes` uses `(current-previous)/previous` and returns ratios, matching the existing scalar `percent_change` convention: `0.2` means 20%, not 0.2%. A zero prior value yields a diagnostic for the operation, never infinity, a fabricated zero, or an undocumented gap.

Rolling windows are **trailing, complete windows only**, defaulting to a result length of `max(n-window+1, 0)`. Window 1 is valid; a window larger than the vector returns `[]`; zero, negative or fractional windows are errors. Empty input yields an empty result for valid windows. Cumulative operations on `[]` yield `[]`.

For `[10,20,15,30]`, `cumsum` yields `[10,30,45,75]`, `diff` yields `[10,-5,15]`, and `rolling_average(...,2)` yields `[15,17.5,22.5]`. Rolling result position zero corresponds to original position `window-1`. Document use of `slice(x, window-1, len(x))` for aligned chart X values when a complete window exists; do not silently modify chart axes or pretend shortened results retain their original indices. Nonfinite intermediate results fail clearly. Results remain ordinary reusable vectors.

### C4. Statistical helpers — P7

Use clear scalar-returning functions rather than adding a regression-record value type solely for this feature.

| Proposed API | Required definition and valid data |
|---|---|
| `quantile(v,q)` | `q` in `[0,1]`; sort a copy and interpolate at `h=(n-1)*q` between neighboring values; reject empty input. |
| `percentile(v,p)` | `p` in `[0,100]`; same convention with `q=p/100`. |
| `variance_population(v)` | Sum of squared deviations divided by `n`; require `n>=1`. |
| `variance_sample(v)` | Divide by `n-1`; require `n>=2`. |
| `stdev_sample(v)` | Square root of sample variance; existing `stdev` stays population standard deviation. |
| `covariance_population(x,y)` | Paired centered products divided by `n`; equal lengths, `n>=1`. |
| `covariance_sample(x,y)` | Same paired products divided by `n-1`; equal lengths, `n>=2`. |
| `correlation(x,y)` | Pearson linear correlation; equal lengths, `n>=2`, both datasets nonconstant. |
| `regression_slope(x,y)` | Ordinary least-squares straight-line fit with intercept; equal lengths, `n>=2`, nonconstant X. |
| `regression_intercept(x,y)` | `mean(y)-slope*mean(x)` under the same conditions. |
| `regression_r2(x,y)` | `1-SSE/SST` for that fit; constant Y gives an explicit undefined diagnostic. |
| `predict_linear(x,y,x_new)` | Scalar prediction from that fitted line at finite `x_new`; extrapolation allowed and documented as an estimate. |

Keep existing scalar and vector `min`/`max`, collection statistics and exact-number semantics unchanged. New helpers take the current finite numeric vectors; supporting additional value types is not a hidden expansion of this goal. Use numerically stable centered/scaled calculations where appropriate; do not return a misleading finite answer after overflow or degenerate inputs. Results are ordinary scalars, so watches and later expressions can use them. An error in R-squared must not prevent separately requested valid slope/intercept calculations for constant Y.

Reference fixtures: `[1,2,3,4]` has median/50th percentile `2.5`, 25th percentile `1.75`, population variance `1.25` and sample variance `5/3`. With `x=[1,2,3]`, `y=[3,5,7]`, slope is `2`, intercept `1`, correlation and R-squared are `1`, prediction at `4` is `9`. Use appropriate numeric tolerances, not fragile string comparisons, for inexact results.

### C5. Histograms and distributions — P10

Add Histogram to the existing Charts companion and an optional cumulative-distribution display. Choose one numeric vector for the histogram, auto bins or explicit bin count/width, and Count or Percentage on the Y axis. Do not overload ordinary bar charts or reinterpret their labels.

Use a deterministic auto-bin rule, documented in the reference; the selected default is `ceil(sqrt(n))` for nonconstant data. Accept 1–1,024 explicit bins, finite positive widths, and enforce rendering/work bounds. Width mode derives enough bins to cover the data and rejects a request that exceeds the cap rather than silently changing the width. Bin intervals are left-inclusive/right-exclusive except the last, which includes the maximum. Every value is counted exactly once. Empty data shows No data; a constant dataset uses one clearly labeled bin containing that value without inventing spread.

Hover shows exact interval membership, count, percentage and optionally cumulative count. The cumulative view is an empirical cumulative distribution: at a threshold, show the proportion of observations at or below it, combining duplicate observations correctly. Do not imply a fitted normal distribution or add a probability-density estimation project. The counts/percentages must be available in an accessible textual/table summary as well as the visual plot.

Use the existing current-evaluation rules: an outdated vector produces an outdated chart, not a silent recalculation. Image export must show the same bins, cumulative steps and labels as the screen.

### C6. Chart controls — P11

Provide a collapsible **Chart Options** section in the current companion. Support independently selected automatic or fixed X/Y limits; linear or base-10 positive-log scales; tick interval/density controls; horizontal/vertical reference lines with labels from literals or current named scalar values; gridline visibility; per-series color, line style, marker and thickness; legend placement; title and axis labels.

Retain existing defaults for old charts. Fixed bounds require finite `min < max`; log bounds and plotted coordinates must be positive. Explain incompatible values; do not silently discard zero/negative samples to make a log chart appear valid. A default index axis includes zero, so log-X requires explicit suitable positive X values or another valid domain. Zero-based bar/count baselines are not representable on a log axis: disable incompatible bar/histogram scale combinations with a reason rather than rendering fictitious baselines. Ordinary line/scatter log scales must work.

Use base-10 decade/subdivision or tick-density settings for log axes rather than applying linear tick steps. Cap ticks, markers and reference lines so extreme settings cannot hang the UI. Selectable display bounds may clip visible points but must not alter data or numerical calculations. Keep Reset View (pan/zoom) distinct from resetting definition options to Auto. Named reference lines are bound to the same current evaluation as the chart; stale/missing values are reported.

Persist chart options in named documents using compatible defaults for prior definitions. Preserve settings during Dock/Pop Out and close/reopen according to existing chart-definition lifetime. Match screen/export rendering, series identity across reordering, labels, and keyboard operation.

### C7. Complete in-app API reference — CP-DOC

Expand **Config Menu → Tools → Examples & Reference** into an offline, searchable API reference without removing grouped examples. Cover all currently supported functions, overloads, aliases, operators, statements, value types, conversions and limits, plus every API introduced here. Group by Mathematics, Statistics, Vectors, Control Flow/Functions, Programmer/Exact Numbers, Units, Calendar/Time, and Chart Data/Usage as appropriate.

Every entry has a stable identity, spelling/signature, summary, parameter and return types, argument ranges/defaults, behavior, limitations/errors, self-contained executable example, expected output and related entries. Search names, aliases, descriptions, types and concept keywords. An exact function-name match ranks ahead of incidental mentions. Searching `pop` or `remove last element` opens its actual entry, which explicitly says it returns a shortened vector rather than the removed scalar.

Support keyboard selection, Back to results, related-entry navigation, Copy Example, and **Insert Example into Pad**. Insertion is explicit, one undoable edit at the current caret/selection, never replacement of an entire draft without authorization. If example-local names could collide, warn or offer a new scratch document; do not silently rewrite unrelated variables. Copy remains available when insertion is inappropriate. Preserve search, selected entry and scroll on Dock/Pop Out. Navigating help does not run examples, edit the Pad or read the clipboard.

Maintain a coverage inventory of language operations versus documented entries and executable examples. Keep metadata connected to actual supported call signatures where practical, without turning the interpreter into a new generic plugin framework. Update the Pad README and top-level discovery text for the delivered features. Syntax displayed as runnable must execute in the shipped evaluator.

### C8. Outline section reordering — N5

In Notes and Markdown Studio, move a heading together with its content and all nested subsections. Provide both a visible drag destination indicator and keyboard-accessible Move Up/Move Down actions. The default operation reorders sibling sections under the same parent; it does not promote/demote headings, change heading levels, or move a section inside itself. Unsupported structural drops show why they are refused.

Derive boundaries from parsed headings/source positions, not text search on a heading title. Preserve duplicate titles, preamble before the first heading, code fences containing apparent headings, CRLF/LF, Unicode, links, tables, images, comments and unsaved text. Handle supported ATX/setext headings correctly; inspect current parser ranges rather than assuming every heading occupies one line. Preserve final-newline intent, adding only the minimal separator necessary when moving a final section would otherwise concatenate Markdown blocks.

Commit one source transaction through the existing edit/undo path. Recalculate outline/preview and keep the moved selection visible without stealing another window's focus. One Undo restores the prior complete source. Do not force Save or rewrite cross-document links. Where duplicate-heading anchors are order-dependent, warn before applying a move that changes those anchors; ordinary unrelated link text remains untouched.

### C9. PDF and print export — N3

Add **Export / Print** to both Notes and Markdown Studio, using the current draft, not a stale saved file. Capture a source revision for preview; edits/options changes invalidate old output. Do not force a source save.

Provide page preview, zoom/page navigation, Letter/A4/custom page size, portrait/landscape, preset/custom margins, optional/custom title, header/footer text with title/date/page-number tokens, body/code font sizing, readable code wrapping, and explicit page breaks. Use the supported canonical full-line marker `<!-- pagebreak -->` for export-only breaks; outside print export it remains ordinary preserved Markdown comment text. A token like Page X of Y resolves to actual pagination, not placeholder text.

Render supported headings, paragraphs, emphasis, lists, task lists, quotes/callouts, tables, links, code and local images. Keep headings with following content where possible; split genuinely long blocks/tables across pages instead of clipping or looping; repeat table headers on continuation pages. Scale oversized images to the printable region. Show actionable warnings for assets or constructs that cannot render; do not silently drop material. Preserve existing local-asset permissions and limits and do not introduce network fetches. Handle supported Mermaid content through the existing bounded render path where available; otherwise disclose a readable source fallback before export.

**PDF Export** writes an actual local PDF through explicit Save As with replacement confirmation, selectable text where supported, embedded usable fonts, page numbers and functional links. **Print** opens an explicit printer/settings workflow and submits the same paginated content; it must not require manually opening an exported file in another application. Preview, PDF and print share pagination/content, with printer hard-margin differences disclosed. Cancelled dialogs do nothing. Report printer submission, not guaranteed physical completion. Retain existing Markdown/HTML exports unchanged. Settings may persist; document contents and rendered pages must not leak into configuration/history.

### C10. Passwords and passphrases — U13

Add a compact hosted generator with Password and Passphrase modes. Password configuration includes length, upper/lowercase, digits, symbols, and optional ambiguous-character exclusion. Passphrases include word count, separator, capitalization and optional number/symbol additions. Select a redistributable, audited local word list with its license recorded. No network service is required.

Proposed bounds/defaults: passwords 8–256 characters, default 20; passphrases 3–32 words, default 6 with hyphens; generated text capped at a documented safe size. Use explicit Generate/Regenerate, Copy, and reveal/conceal controls. Opening, searching, repainting or editing settings must not continuously generate secrets. Required character classes are actually represented when selected; impossible combinations show a validation message.

Use cryptographically secure OS-backed randomness and unbiased selection. Persist only settings, never results, generated examples, seeds or secret-derived diagnostics. Estimate entropy only from the actual generation scheme; fixed capitalization/separators do not create randomness, and constrained password pools cannot be scored as unconstrained `alphabet^length`. Label estimates, not security guarantees.

Exclude output from action history, favorites/pins, usage payloads, toast/inbox bodies, clipboard undo snapshots, crash/debug receipts and Multi Launcher's clipboard-history ingestion. A successful secret copy must be marked/excluded **before** history can ingest it, including delayed reads and an app restart while that same clipboard item remains. Suppression must follow clipboard ownership/generation, not skip the next unrelated copy. A failed Copy keeps the result available and does not affect other clipboard content. Never clear a newer clipboard value as cleanup. Do not claim control over unrelated clipboard managers or guarantee forensic memory erasure.

Preserve existing `rand` number/dice behavior. Keep legacy password commands discoverable and able to generate/copy, but route newly generated secrets through the protected invocation path rather than serializing them into `clipboard:<secret>` action/history strings. Existing user-authored literal actions are not silently rewritten or purged.

### C11. Automatic ZIP destination — ZIP-DEST

After a new archive selection is accepted, populate the actual extraction Destination draft with `archive_parent / archive_filename_without_final_zip_extension`.

```text
Selected:    C:\Users\multi\Downloads\gezik-0.1.0-rc.7-windows-x64.zip
Destination: C:\Users\multi\Downloads\gezik-0.1.0-rc.7-windows-x64
```

Preserve earlier dots and case in the stem; recognize `.zip` case-insensitively. No empty/invalid derived folder is accepted silently. A cancelled/rejected picker keeps the existing draft. A different accepted archive gets its own suggestion even if the previous archive had a custom destination; the new suggestion remains editable. Reopening the same live dialog or a preview refresh does not repeatedly reset a manual override.

Do not create the folder on selection, preview, typing or picker return. Permit preview of the proposed not-yet-existing child under an explicitly known parent; create it only after confirmed extraction, through the existing guarded publication path. Existing directories and file/name conflicts are reported under current no-clobber rules. Never overwrite merely because the default is convenient. Show the final layout, including nested archive folders; do not auto-flatten a top-level folder. Any source/destination change invalidates an old preview/confirmation and rejects late picker/session results. ZIP Create mode is unchanged.

### C12. Advanced File Operations Center — X9

Offer Copy, Move and Duplicate from a file picker, File Search selection, and Downloads Triage. New actions are explicit additions; do not globally intercept Explorer file operations. Add jobs as reviewed source/destination drafts, then explicitly queue/start. Default to one active transfer job, retaining queued order and enabling reorder for not-started jobs.

The center shows Queued, Preparing, Running, Pause requested, Paused, Cancelling, Completed, Partial, Cancelled, Failed and Interrupted states as applicable. Display source/destination, current item, files and logical bytes completed, totals once known, elapsed time, transfer/verification phase, and ETA only when meaningful. Do not fabricate percentage totals for unfinished enumeration or label requested cancellation as finished.

Conflicts present **Skip**, **Keep both / choose another name**, or **Replace** after explicit review. Folder merge is a separate clearly labeled decision and never authorizes undisclosed overwrites. Applying a choice to multiple conflicts is scoped to the reviewed job/class. Default is no overwrite. Source and target changes after review invalidate affected approvals. Reject copying a folder into itself/descendants, source-equals-destination, overlapping recursive selections, duplicate source identity, and unsafe aliases/cycles.

Pause/resume is required within a live session and must be responsive during large transfers; surface a pending pause until an actual safe boundary. Cancel stops new admission and preserves precise per-item outcomes. Completed destination files remain, partial files are either safely removed or explicitly listed. Cross-volume move removes a source only after destination publication and verification, with source identity/content revalidation. Failure to remove the source is Partial, not Completed. Same-volume moves may be atomic renames but still require conflict/identity checks.

Optional verification rereads destination content and compares a recorded source digest, reporting Verified / Not requested / Failed / Unsupported distinctly. Account for required alternate data streams and metadata fidelity; never claim a byte-for-byte verified copy when an included stream was dropped. Preserve relevant timestamps and security-zone information where supported, and disclose unsupported fidelity before destructive source removal. File permissions follow a documented destination inheritance/preservation policy; content verification is not a claim of identical ACLs. Do not follow reparse points or hydrate cloud placeholders silently. Unsupported filesystem/provider cases are explicit per-item outcomes, not silent skips.

Persist bounded job summaries and durable receipts for owned artifacts and destructive steps. On restart, unfinished jobs become Interrupted and require explicit review/retry; no automatic transfer resumption. Revalidate a retry and avoid duplicating already completed items. Keep long-lived operation ownership outside dialog lifetimes. Closing/hiding a UI does not cancel work unless explicitly selected; shutdown handles owned work safely and truthfully.

Offer guarded reversal only where the receipt proves it is safe, such as reversing an eligible move or removing an unchanged copy with explicit confirmation. Replacement need not be generically undoable; label that limitation before approval and retain displaced content if offering restoration. Never delete or replace a subsequently edited destination during Undo. Use Activity Center for actual progress and Notifications for terminal/attention events, not a separate parallel engine.

### C13. Downloads Triage Center — X8

Open the user's configured/Windows Downloads folder as a proposed root and show it explicitly; opening/Refresh starts a bounded read-only scan. Support selecting another downloads root. Default to direct children, with no background organization, blanket home-drive indexing, or new always-on watcher. Use searchable/sortable rows grouped by type, age, size and reliable origin metadata when present.

Label timestamps accurately: modified/created time is not automatically the download time. Missing origin is Unknown. Show conventional unfinished-download names and positively known active state as Incomplete/Possibly incomplete, not as an infallible completion detector. Skip unsafe links/placeholders as appropriate without silently recalling content.

Retain Reviewed/Unreviewed state with stable file identity and relevant revision evidence; a replacement occupant or changed file must not inherit stale review status. Marking reviewed changes only app metadata. Show sensible views for large downloads, older installers, archives, and suspected duplicates. Only explicit duplicate verification earns Verified duplicate status. An archive is Previously extracted only when a retained receipt establishes that fact; an adjacent same-named folder is at most a suggestion.

For a current selection offer Open, Reveal, Rename, Move/Copy, Extract ZIP, Inspect/Analyze through applicable existing tools, and Move to Recycle Bin with a clear selection review. Pass exact selections, not a broadened folder scan pretending to preserve selection. Move/copy goes through X9; ZIP extraction retains its preview/confirmation. Destination suggestions use chosen saved folders or explicit user preferences and never move files automatically. Opening a URL or application is always a separate deliberate action.

Display partial scans, errors, inaccessible items and result limits. Scan/cache activity must stop when idle; closing the review window must not start extra work. Preferences and reviewed state persist, but do not record browser histories or invent source metadata.

### C14. Universal Notification Inbox — X25, Q1-A

**Required core:** searchable chronological entries, source/category/time and read/unread filters, details, source grouping, unread indicator, per-source mute/exclusion, explicit local dismissal/clear, pin/retain, and configurable retention. Internal sources include launcher alerts, existing timers/calendar reminders, macro notifications, Hot Folder terminal outcomes, and relevant Activity/transfer completions and failures. Avoid every progress tick becoming an inbox item.

Internal entries carry stable operation/reminder identity and real terminal status. Open related item / details, snooze existing reminders, and Create Todo must use current domain services with revalidation and visible feedback. Converting a notification into a task or snoozing it is explicit and must not fire twice from repeated GUI passes. Reading an entry does not launch anything. Do not make the inbox a new task scheduler or operation controller.

**External capture is required work but opt-in for the end user.** Supply a working documented Windows notification listener route, including the optional setup needed for its identity/capability and consent. A disabled setting, mock listener, or notes about unsupported capture do not fulfill Q1-A. Ordinary launcher builds and internal inbox operation remain independent of installing this optional support.

External coverage is Windows-exposed current toast notifications and changes observed while the supported listener is active, not every app's private notification feed or a historical archive before collection began. Display coverage/start time and connection status: Off, Setup required, Permission not granted, Denied, Active, Disconnected, Unsupported or Error. Recheck permission and stop collection on revocation; an empty read is not proof of permission or absence of notifications. Reconnect only through bounded, nonbusy behavior; avoid flood/replay of entries already observed.

Show source app, source timestamp, title/body where available, and distinguish source removal from local read state. Deduplicate by source identity/notification identity plus relevant version/session data; handle ID reuse and updated content. Suppress recapture of Multi Launcher's own already-recorded toasts and any inbox-generated toast loop. Do not claim arbitrary third-party action buttons/deep links can be replayed. Offer Open source app only when a validated supported target exists; otherwise show details. Never execute commands or URLs parsed from notification body text.

**Privacy defaults selected for this plan:** external collection off; external text history session-only until the user explicitly enables persistence; internal nonsecret inbox history retained for 7 days by default with a 1,000-entry cap, configurable retention/size within documented bounds. Pinned items count toward the cap; reaching it produces a disclosed limit rather than unbounded growth. Offer Clear source/Clear all local history and an option to retain no text. Store no generated passwords, secret clipboard payloads or full debugging context in any inbox entry. Exclude sensitive notification history from default data backups. Logs contain metadata/reasons, not notification bodies.

Mark as read and Clear local history affect Multi Launcher only. Any supported **Dismiss from Windows** action must be separately named, explicitly chosen, and limited to the exact current selected notification; never invoke a system-wide clear for a filtered view. Muting inbox presentation is distinct from excluding collection/retention and from changing the original app's Windows settings. Revoking capture does not silently erase already retained entries; offer explicit deletion.

Optional setup must be discoverable, inspectable and reversible, maintain the same application data ownership, and not require a framework migration. No service installation, privileged database scraping, remote capture, bypassing OS permission, disabling system safeguards or silent certificate trust. A development/test identity may require documented local trust, but production signing prerequisites must be honest; do not commit keys/certificates with private material. Test unavailable/denied/off paths as well as one real permitted external capture path.

## D. Scope and Non-Goals

Include every requirement C1–C14. Preserve existing commands and data except the explicitly approved additions and privacy correction for newly generated passwords. Ordinary tool opening must not start filesystem mutations, print jobs, clipboard writes, external collection or random generation.

Do not add workspaces, cross-device sync, a file-explorer replacement, automatic Downloads cleanup, a general workflow composer, AI features, a new browser/web UI, a new general programming runtime, arbitrary macros from notification text, probability-distribution fitting, or vector element types/limits beyond the existing model. Do not make external notifications or PDF printing depend on an app-wide GUI migration. No full repository cleanup or vendored crate churn.

Do not weaken rename/replace/ZIP/Hot Folder safeguards to accommodate the new transfer center. Do not reinterpret existing file-search roots, change global clipboard capture behavior for unrelated copies, consume keys intended for other windows, or change legacy chart/calculation defaults. No automatic merge to master, release, software purchase, OS security-policy modification or unsigned-install bypass is authorized.

## E. Technical Implementation Strategy

### E1. Ownership and integration

Keep reusable behavior outside rendering code. Extend calculation domain/evaluation/reference modules and the existing chart scene; the existing Pad session remains the authority for document edits, definitions and evaluation revisions. New callback operations should resolve through the bounded formula-function call machinery, not evaluate text in a second interpreter. Add a focused statistics/sequence helper module only where it clarifies domain ownership.

Use prepared Markdown source edits for section moves. Add a **proposed shared paginated-export domain** under the Markdown area; Notes/Studio adapters supply explicit source and permitted assets, while a shared page model feeds preview, PDF and print. Choose the smallest justified PDF/font/printer dependency after checking available capabilities, licensing and native deployment constraints. Do not claim HTML export already supplies pagination. Native print adapters own handles/spool lifetimes, not UI frames.

Add a **proposed password-generator domain** with typed settings, secure generation and transient result ownership. Integrate secret-copy suppression into the real clipboard-history admission path and ephemeral typed command routing. Avoid global content retention or disabling all clipboard history as a substitute.

Add a **proposed file-transfer domain/service** distinct from rename and Hot Folder authorization, reusing audited low-level proof/publication helpers. It owns plans, job IDs, state transitions, cancellation/pause, receipts and recovery. The UI dispatches explicit intents; Activity/Notifications consume facts. Add a **proposed downloads-triage domain/UI** for read-only scans and review metadata, delegating operations to existing tools and the transfer service.

Add a **proposed notification domain/service and hosted inbox**. Internal event adapters publish sanitized notification facts once. A platform listener adapter owns external registration/access/session lifetime; optional support artifacts are isolated from normal Cargo builds. Reuse existing plugin registration, typed commands, help, universal actions, settings navigation and persistence conventions. Enumerate exact call sites after M0-A rather than guessing new command variants in advance.

### E2. Source identity, persistence and resource bounds

Every asynchronous result is tied to its owner/session/revision and rejected after close, reset, owner replacement or source change. Reuse utility host staged actions to prevent duplicate effects across egui passes and dock/pop transitions. GUI frames read snapshots and issue intents; scanning, hashing, transfer, PDF generation and external notification reads must not block them.

Use validated/atomic store patterns, additive optional settings/defaults and explicit schema validation. Preserve malformed existing bytes and show errors instead of replacing them with defaults. Integrate new authored settings and replaceable/private histories with Data & Recovery according to their actual data classification; no silent backup of secrets or external notification text.

Preserve calculation/chart limits. Define finite admission, retained-path, output, queue, history and scan limits for new tools in M0-B and expose reached-limit status. Transfer byte size is not capped merely because metadata/history is bounded; process large file contents incrementally. Do not recurse unboundedly or allocate entire files/trees on the UI thread. Avoid introducing continuous background polling for features that only need explicit opening/refresh. External capture and active jobs are intentional background services, with bounded work and real idle waits.

### E3. Native notification facts and bounded compatibility decision

Microsoft documents a notification-listener manifest capability, explicit access permission, UI-thread consent, access-status checks and current-toast enumeration. Sending own toasts does not supply this capability. Package identity can be supplied without relocating the existing executable, but **identity alone does not prove the specific consent/listener path works in this egui desktop host**; desktop WinRT restrictions must be checked. These are external platform facts, distinct from repository findings.

Use the documented listener API and prefer an optional identity package when a real native proof validates that route. If the consent UI requires a different supported host context, a minimal optional consent/listener companion is authorized solely for this boundary; keep the main application Rust/egui and its ordinary build unchanged. Validate the selected route early and implement one production path, not several speculative backends. Package/bridge tooling remains opt-in and must not become another application-wide framework.

Primary references, checked November 17, 2026:

- [Microsoft notification listener guide](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/notification-listener).
- [UserNotificationListener API and access/notification methods](https://learn.microsoft.com/en-us/uwp/api/windows.ui.notifications.management.usernotificationlistener?view=winrt-26100).
- [Package identity with external location](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps-overview).
- [Manual external-location identity packaging](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps).
- [Desktop WinRT limitations](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/winrt-api-desktop-app-support).
- [Capability declarations](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations).

Do not scrape Windows notification databases, require administrator access to inspect other users, or synthesize consent to claim success. If SDK, signing, permissions or interactive Windows execution is unavailable, implement/test what is independently possible, retain a precise native-validation blocker, and continue independent checkpoints. Do not mark the external portion Complete without real supported-path evidence.

### E4. Dependency order

M0 → M1 → M2 → M3 → M4 → M5 → M6 → M7 → M8 → M9 → M10 → M11 → M12 → M13 is the normal sequential order. M4 uses completed calculation behavior; M5 documents it. M10 uses X9 transfers and existing ZIP/inspect actions. M11 uses actual operation outcomes. Keyboard support is incremental and M12 audits existing gaps. A documented external/environment blocker may allow independent later checkpoints to proceed on the same branch; never erase the blocked requirement or describe a partial Goal as complete.

## F. Detailed Milestones and Tasks

Every checkpoint below is a bounded implementer packet. Its cited C-section is part of its acceptance contract. Verification uses the narrowest affected targets; do not interpret a checkpoint's unit tests as a request for a full-suite run. Checkpoint commits are made after relevant verification and diff inspection; any additional necessary correction gets a new small commit, not rewritten history.

### M0-A — Branch bootstrap

- **Objective:** Establish the correct fresh child branch before application work.
- **Code ownership:** Git state and the task-specific ledger only; no application changes.
- **Tasks:** Execute all steps in A0, including predecessor resolution, direct-child baseline and initial upstream push attempt.
- **Behavior:** One shared task branch for this entire Goal/Kickstart pair.
- **Edge cases:** Dirty/staged user work, conflicting predecessor records, remote-only or unpushed predecessor, unavailable upstream.
- **Regression constraints:** No discarded/staged unrelated work, worktrees, parallel checkouts or ancestry shortcuts.
- **Verification:** Read branch/status/parent SHAs; compare baseline's immediate parent to the selected parent; inspect baseline staged paths.
- **Acceptance:** All A0 bootstrap criteria hold; parent, baseline and push state are recorded separately.
- **Commit boundary:** The mandatory baseline subject specified in A0. Do not reuse the predecessor's final commit as this baseline.

### M0-B — Reconcile source, ownership and native prerequisites

- **Objective:** Give subsequent implementers a precise current-source map and resolve bounded platform choices without changing product scope.
- **Code ownership:** Relevant files in B, actual command/plugin/settings registrations, existing tests, `Cargo.toml`, optional native setup location if one exists, and the task ledger.
- **Tasks:** Compare the execution parent to the planning reference; inventory ordinary utility keyboard coverage; locate exact Pad dispatch/rename/document persistence seams, Markdown undo adapters, ZIP destination admission, clipboard/history ingestion and transfer/activity consumers. Select finite resource bounds. Verify a candidate PDF/print path and external listener identity/consent path against current official docs; use at most one minimal native proof per candidate where needed, within the existing checkout. Record one chosen approach and narrow dependency justifications. Do not build a generic acceptance framework.
- **Behavior:** Approved API conventions and Q1-A remain fixed; routine path/dependency adjustments are documented.
- **Edge cases:** Native SDK or interactive desktop missing, permission UI unsupported in the first host, existing partial features on the parent, command-name collisions.
- **Regression constraints:** No model-config changes, full baseline suite, implementation in another directory/branch, speculative dependency overhaul or silent scope deferral.
- **Verification:** Source/call-site review; targeted compile/native proof only to answer a concrete uncertainty, after M0-A. Record actual evidence and any environment blocker.
- **Acceptance:** Ownership/gap inventory and selected native approach are implementer-ready; unknowns are isolated and independent work can proceed. Native success is not inferred from compilation.
- **Commit boundary:** `docs(plan): [M0-B] reconcile productivity toolkit ownership and native boundaries`.

### M1-A — Suggested ZIP destination

- **Objective:** Complete ZIP-DEST through the real extraction path.
- **Code ownership:** `src/gui/zip_extractor_dialog.rs`, directly involved `src/archive/` destination/preview validation and focused tests.
- **Tasks:** Derive the adjacent destination after accepted archive selection; update text/path/session together; invalidate old previews/confirmations; handle a not-yet-existing child without creating it until extraction approval. Add tests for the precise example in C11 and picker races.
- **Behavior:** User selects a ZIP and can immediately review/extract to its sibling folder; manual changes persist until a different archive is accepted.
- **Edge cases:** Uppercase extension, several dots, Unicode paths, cancelled picker, root path, invalid stem, existing folder/file, late picker result and already nested archive folder.
- **Regression constraints:** Preserve ZIP Create, no-clobber extraction, selected-member behavior and host/session fencing.
- **Verification:** Destination derivation/state tests; one extraction to a disposable absent folder and one occupied-target case through the existing preview/confirmation workflow.
- **Acceptance:** C11 passes, including no folder creation on selection/preview and stale-token rejection.
- **Commit boundary:** `feat(zip): [M1-A] suggest guarded archive-adjacent extraction destinations`.

### M2-A — Pure ordering and deduplication

- **Objective:** Add `sort`, `sort_desc`, and stable `unique`.
- **Code ownership:** `src/calculation_pad/vector.rs`, relevant dispatch in `script.rs`/`document.rs`, and vector-domain tests.
- **Tasks:** Extend the existing collection boundary, finite-value validation, budgets and diagnostics. Preserve input snapshots and return normal vectors. Add C2 fixtures and documentation entries as APIs become available.
- **Behavior:** Ordering and uniqueness compose with current assignments, loops, watches and vector arithmetic.
- **Edge cases:** Empty/singleton vectors, repeated values, signed zero, limit-sized input, budget cancellation and invalid argument types.
- **Regression constraints:** Existing `append/pop/slice/reverse`, scalar sorting-related names, old canonical copy output and limits remain unchanged.
- **Verification:** Focused vector tests plus a source-document evaluation proving original input is unchanged.
- **Acceptance:** Exact C2 ordering/unique outputs and error boundaries pass without partial results.
- **Commit boundary:** `feat(calc): [M2-A] add bounded vector sorting and uniqueness`.

### M2-B — Named-callback collection transformations

- **Objective:** Complete map/filter/count/matching-index operations with explicit parameters.
- **Code ownership:** Existing formula parsing/call dispatch, `script.rs`, `vector.rs`, dependency/rename/syntax/session integration where affected.
- **Tasks:** Recognize the callback identifier only in the designated argument position; resolve a prior formula; pass each element and frozen explicit extra arguments through the existing evaluator. Check callback arity/types, limits and cancellation. Add callback references to dependency/rename paths; identify failed element position in bounded diagnostics.
- **Behavior:** C2's full example executes and changing `threshold`, `factor` or the callback definition updates dependent results normally.
- **Edge cases:** Empty input with unknown callback, callback type mismatch, nested valid formula calls, shadowing, recursion rejection, call depth, callback error on a later element, cancellation and function rename.
- **Regression constraints:** No captured globals, dynamic text evaluation, new recursion permission, altered existing function binding or partial publication.
- **Verification:** Focused expression/function/vector tests and dependency/rename regressions for this new call form.
- **Acceptance:** All transformation results are correct; invalid calls give precise diagnostics; previous input/results remain intact.
- **Commit boundary:** `feat(calc): [M2-B] add bounded named-callback vector transformations`.

### M3-A — Sequential and rolling calculations

- **Objective:** Deliver C3's cumulative, difference, ratio-change and rolling APIs.
- **Code ownership:** Existing calculation domain/dispatch; a focused new sequence helper module if warranted.
- **Tasks:** Implement all specified operations; enforce complete trailing windows and clear alignment; charge traversal/work; add examples usable with chart X vectors.
- **Behavior:** Outputs are reusable vectors with the documented lengths; ratios agree with existing scalar percentage helpers.
- **Edge cases:** Empty/singleton input, window 1/n/n+1, zero/fractional/negative window, zero prior value, negative measurements and overflow.
- **Regression constraints:** Do not pad with zeros/NaNs, change original vectors, silently ignore errors or alter ordinary chart indexing.
- **Verification:** C3 exact fixtures, output-length tests and a chart-data fixture with deliberately aligned X values.
- **Acceptance:** Every API and edge rule in C3 passes and reference text explains alignment and percentages.
- **Commit boundary:** `feat(calc): [M3-A] add cumulative rolling and adjacent-change calculations`.

### M3-B — Quantiles and dispersion

- **Objective:** Add percentile/quantile, population/sample variance and sample deviation.
- **Code ownership:** Calculation statistics dispatch and a focused statistics domain owner; existing statistics tests.
- **Tasks:** Implement C4's interpolation and denominators; use stable numeric calculations; keep existing `stdev` population behavior. Document valid ranges and minimum sample counts.
- **Behavior:** Scalar outputs compose with ordinary Pad formulas and watches.
- **Edge cases:** Single value, equal values, p=0/100, q=0/1, fractional percentile, out-of-range percentile, empty input and large-offset/small-spread data.
- **Regression constraints:** No reinterpretation of `sum([])`, existing average/median/stdev or exact arithmetic conversions.
- **Verification:** Known fixtures, endpoint/interpolation tests and stable-numeric fixtures with appropriate tolerances.
- **Acceptance:** C4's univariate functions are correct and fail explicitly when undefined/nonfinite.
- **Commit boundary:** `feat(calc): [M3-B] add documented quantiles and sample statistics`.

### M3-C — Relationships and linear fits

- **Objective:** Add covariance, correlation, regression coefficients, R-squared and scalar prediction.
- **Code ownership:** Same statistics domain and calculation dispatch as M3-B.
- **Tasks:** Implement paired-data validation and centered/scaled fitting; expose separate scalar APIs; document extrapolation and each undefined case.
- **Behavior:** Users can watch coefficients, predict values and produce fitted vectors with the M2 callbacks without a new object type.
- **Edge cases:** Mismatched lengths, one observation, constant X, constant Y, negative correlation, perfect fit, large offsets and finite overflow.
- **Regression constraints:** Preserve unrelated math grammar and allow valid slope/intercept results even when a separately requested R-squared is undefined.
- **Verification:** C4 regression fixtures, reverse relationship, constant-input and near-degenerate numeric tests.
- **Acceptance:** All paired APIs and diagnostic conditions pass; correlation/R-squared are not fabricated for invalid data.
- **Commit boundary:** `feat(calc): [M3-C] add covariance correlation and linear-fit helpers`.

### M4-A — Histogram and cumulative data model

- **Objective:** Build the bounded histogram/CDF data used by the existing chart system.
- **Code ownership:** `src/calculation_pad/chart.rs` or a focused child domain module; chart tests.
- **Tasks:** Add explicit chart kinds/data as needed, bin generation/counting, percentage normalization and empirical cumulative steps. Produce accessible summaries and stable original-value hover information.
- **Behavior:** Every admitted input value contributes exactly once; empty/constant data behaves as C5 specifies.
- **Edge cases:** Bin boundary values, duplicates, zero/negative measurements, extreme range/tiny width, 1,024 bins and constant data.
- **Regression constraints:** Do not reinterpret ordinary bar charts or exceed existing rendering/work envelopes silently.
- **Verification:** Bin-membership/count invariants, total percentage, CDF monotonicity/endpoints and limits; old chart data fixtures remain valid.
- **Acceptance:** C5's numerical distribution model and diagnostics are independently usable and verified.
- **Commit boundary:** `feat(charts): [M4-A] add bounded histogram and cumulative distribution data`.

### M4-B — Shared chart options, persistence and rendering

- **Objective:** Extend the chart definition and shared screen/raster scene with C6 options.
- **Code ownership:** `chart.rs`, Pad `persistence.rs`/`session.rs`, reference dependencies/rename where required.
- **Tasks:** Add compatible defaults, axis transforms/bounds/ticks, reference-line bindings, styles/title/legend and shared rendering. Validate persisted settings before scene construction. Keep cached scenes keyed by current definition and evaluation.
- **Behavior:** Old charts render as before; new options affect display/export, not calculations.
- **Edge cases:** Missing fields in old data, unknown/invalid new values, fixed inverted limits, zero on log-X, invalid log baselines, reference rename/deletion, series reorder and excessive ticks.
- **Regression constraints:** Preserve source identities, manual calculation, stale-output rejection, existing raster limits and default colors/labels.
- **Verification:** Serialization round trips and old-definition fixtures; transform/bound tests; screen/raster scene assertions; named reference invalidation tests.
- **Acceptance:** C6 settings persist compatibly and the same scene describes the displayed/exported chart.
- **Commit boundary:** `feat(charts): [M4-B] add compatible axis reference and appearance controls`.

### M4-C — Charts companion interaction

- **Objective:** Make C5/C6 usable in the existing Charts companion.
- **Code ownership:** `src/gui/calculation_chart_dialog.rs` and its current host/action commit path.
- **Tasks:** Add Histogram/Cumulative choices, bin and count/percentage fields, collapsible Chart Options, accessible data summaries, validation and reset behavior. Register keyboard controls; preserve active definition across Dock/Pop Out.
- **Behavior:** No new standalone charting app or automatic Pad evaluation; stale source is clearly identified.
- **Edge cases:** Small windows, many series, unavailable named reference, changed definition during export, invalid setting draft and ownership change.
- **Regression constraints:** Do not duplicate commits across UI passes, erase existing labels or permit outdated image copying.
- **Verification:** Focused UI/state tests plus one native dock/pop scenario showing distribution, fixed bounds/reference line, log error and copied image parity.
- **Acceptance:** Every C5/C6 control has usable behavior, keyboard access and matching output.
- **Commit boundary:** `feat(charts): [M4-C] expose distribution and advanced chart options`.

### M5-A — Complete API metadata and executable examples

- **Objective:** Establish comprehensive C7 reference content rather than a partial new-functions list.
- **Code ownership:** `src/calculation_pad/examples.rs`, evaluator function/operator catalogs and a proposed reference metadata module.
- **Tasks:** Enumerate supported APIs/overloads/aliases/statements/types and map each to an entry; add parameter/return/error/limit information, related IDs and concept keywords. Reuse existing examples and validate new examples against the real evaluator.
- **Behavior:** `pop` documents shortened-vector semantics and maps to a working self-contained example; new and old APIs are equally discoverable.
- **Edge cases:** Aliases, overloaded min/max, exact versus ordinary values, documented error examples, removed/unknown functions and incomplete entries.
- **Regression constraints:** Do not advertise unimplemented syntax, import a second language grammar or discard existing useful examples.
- **Verification:** Coverage inventory/check, executable example tests, expected diagnostics and search ranking fixtures.
- **Acceptance:** Every discovered supported API has accurate reference coverage; no placeholder examples remain.
- **Commit boundary:** `docs(calc): [M5-A] provide complete executable API reference content`.

### M5-B — Search, navigation and safe insertion

- **Objective:** Deliver the in-app reference experience in C7.
- **Code ownership:** Existing reference UI within `src/gui/calculation_pad_dialog.rs`, utility hosting and source-editor transaction integration.
- **Tasks:** Add category/type filtering, exact-name-first results, reference detail, related navigation and Back; retain grouped Examples. Add Copy and explicit Insert through normal undoable edits, with collision handling and no automatic execution. Update user guides/discovery.
- **Behavior:** Search `remove last element`, inspect `pop`, copy or insert a valid example, and continue editing the original document safely.
- **Edge cases:** No results, long entries, changed document/selection, name collisions, unavailable insertion target and dock/pop transition.
- **Regression constraints:** Do not overwrite a full draft or read clipboard contents when navigating help; retain search/selection state and ordinary undo.
- **Verification:** Search/navigation/insertion tests and a native reference-to-Pad smoke with Undo.
- **Acceptance:** C7 passes for both an existing function and new APIs, with documentation matching implemented spellings.
- **Commit boundary:** `feat(calc-help): [M5-B] add searchable API navigation and safe example insertion`.

### M6-A — Prepared Markdown section move

- **Objective:** Create one shared, source-preserving section-move operation.
- **Code ownership:** `src/notes_markdown/{headings,sections,authoring,mutations}.rs` and the existing prepared-edit abstraction.
- **Tasks:** Resolve selected source section including descendants; compute valid sibling destinations; reject invalid/stale ranges; prepare one edit preserving bytes except minimal needed separators. Detect anchor changes requiring warning.
- **Behavior:** A section's content/subheadings move together without changing heading levels.
- **Edge cases:** Duplicate headings, setext headings, fenced code, heading-less preamble, final section without newline, CRLF, Unicode and drops within the selected subtree.
- **Regression constraints:** No document reserialization, automatic global link rewrite or source mutation before approval.
- **Verification:** Exact before/after byte fixtures, nested moves, invalid destination and source-revision tests.
- **Acceptance:** C8 source behavior passes and can be used by both editing surfaces.
- **Commit boundary:** `feat(markdown): [M6-A] add source-preserving outline section moves`.

### M6-B — Outline drag and keyboard interactions

- **Objective:** Expose the prepared move through Notes and Markdown Studio.
- **Code ownership:** `src/gui/{note_panel,markdown_studio,note_mutation}.rs`, outline rendering and ordinary undo handling.
- **Tasks:** Add drag indicator and Move Up/Down actions; retain selection/scroll after move; commit through existing draft/autosave policy; update preview/outline and register keyboard access.
- **Behavior:** Drag or keyboard movement yields the same result; one Undo restores the entire move.
- **Edge cases:** Hidden/collapsed subsections, first/last sibling, multiple open notes, stale drag source, unsaved edits and anchor-change warning cancellation.
- **Regression constraints:** Preserve typing, Save/discard policies, focus ownership, backlinks and existing preview behavior.
- **Verification:** Adapter/state tests and a native move/undo in each surface, including one pop-out.
- **Acceptance:** All C8 interactions work without relying exclusively on the mouse.
- **Commit boundary:** `feat(notes): [M6-B] enable outline reordering with undo and keyboard actions`.

### M7-A — Shared pagination and PDF output

- **Objective:** Export a faithful current Markdown draft to a real paginated PDF.
- **Code ownership:** Proposed shared Markdown pagination/PDF domain; `src/gui/markdown_export.rs`, existing asset resolver and source-analysis owners.
- **Tasks:** Implement layout settings/page model, text/table/image/code pagination, title/header/footer tokens and explicit page breaks. Add a narrow PDF writer/font integration justified in M0-B; preserve bounded asset resolution and collision-safe explicit output.
- **Behavior:** The output contains the reviewed draft and selected formatting, with actual page numbers, readable text and links.
- **Edge cases:** Oversized table/code block/image, missing asset/font glyph, custom margins leaving no content space, Unicode, unsupported Mermaid/render fallback, interrupted write and existing output.
- **Regression constraints:** No source save/mutation, network asset access, external-browser dependency or changed HTML/Markdown export behavior.
- **Verification:** Pagination/content fixtures, PDF text/page/link inspection and rendered-page inspection of representative documents; no pixel-perfect campaign.
- **Acceptance:** C9 PDF content/options are implemented with warnings for any unsupported material and no silent clipping or dropped source.
- **Commit boundary:** `feat(markdown-export): [M7-A] add shared pagination and formatted PDF output`.

### M7-B — Export preview and direct print

- **Objective:** Provide the same export/print interaction in Notes and Markdown Studio.
- **Code ownership:** Their existing UI/hosts plus a proposed native printer adapter over the shared page model.
- **Tasks:** Add Export/Print settings, page preview/navigation, Save PDF and explicit printer/settings confirmation; show preparing/submitted/error states. Tie preview/output to source revision and handle cancellation/host close safely.
- **Behavior:** The user previews and exports/prints without manually opening another application; ordinary builds remain usable without a printer.
- **Edge cases:** No printer, unavailable print service, printer hard margins, canceled dialogs, source edits during preparation, output failure and closing the window mid-job.
- **Regression constraints:** No automatic printing, false physical-completion claim or UI-thread rendering/IO stalls.
- **Verification:** UI/state tests, PDF export native smoke from both surfaces, and an actual print submission to an available test/virtual printer. Record unavailable native printing as unverified, not passed.
- **Acceptance:** C9 preview/output agree and every native action is explicit; remaining hardware-only checks are honestly identified.
- **Commit boundary:** `feat(markdown-export): [M7-B] add preview and native print workflows`.

### M8-A — Secure generator domain and privacy admission

- **Objective:** Implement valid password/passphrase generation and prevent retained-secret leakage.
- **Code ownership:** Proposed generator domain, `src/plugins/random.rs`, `src/plugins/clipboard.rs`, affected typed action/history/copy boundaries.
- **Tasks:** Add validated settings and licensed word list; use secure unbiased generation and conservative entropy estimates. Add transient result IDs/ownership and secret-copy admission metadata; suppress generated-value history/diagnostics without disabling normal history. Keep legacy number/dice untouched and migrate newly generated password results to the protected route.
- **Behavior:** Generated values satisfy configured rules and cannot become serialized action strings or clipboard-history entries through normal app paths.
- **Edge cases:** Empty alphabet, impossible class requirements, excluded characters, failed OS randomness, failed clipboard write, delayed history read, clipboard replacement and restart with the same secret clipboard item.
- **Regression constraints:** No stored seeds/results, blanket history shutdown, unrelated-history deletion or changed ordinary paste sequence ownership.
- **Verification:** Deterministic test-source property fixtures, invalid settings, RNG failure, and privacy tests tracing action/history/clipboard admission including restart. Inspect logs/artifacts for fixture markers only.
- **Acceptance:** C10 generation and no-retention guarantees pass for both new dialog and legacy password entry points.
- **Commit boundary:** `feat(passwords): [M8-A] add secure generation and secret-safe publication`.

### M8-B — Hosted password/passphrase utility

- **Objective:** Make C10 generation usable from launcher and keyboard.
- **Code ownership:** Proposed generator dialog/plugin integration, existing utility hosts, settings/help and random command discovery.
- **Tasks:** Add both modes, validated controls, Generate/Regenerate, mask/reveal, Copy, setting persistence and honest strength explanation. Register discovery without putting results in labels/history/notifications.
- **Behavior:** Opening/repainting does not generate or copy; changes to rules are visible; successful Copy reports no secret text.
- **Edge cases:** Small windows, dock/pop, result made stale by settings edit, canceled close, busy clipboard and unsupported requested alphabet.
- **Regression constraints:** Keep active result transient, preserve standard clipboard workflow and do not claim protection against third-party clipboard managers.
- **Verification:** Hosted state/keyboard tests and one native generate/reveal/copy/history-check flow with disposable generated values.
- **Acceptance:** C10 is usable and private; only configuration persists across restart.
- **Commit boundary:** `feat(passwords): [M8-B] expose configurable password and passphrase generation`.

### M9-A — Transfer plan and conflict review

- **Objective:** Define safe explicit Copy/Move/Duplicate plans and conflict decisions.
- **Code ownership:** Proposed file-transfer domain; audited reusable helpers in `src/file_operations.rs`; no repurposing of rename or Hot Folder approval types.
- **Tasks:** Model source identities, destination parents, selection scope, per-item intent, conflict decisions and metadata-fidelity policy. Bound enumeration; identify self/descendant/alias hazards and freeze approved choices for later revalidation.
- **Behavior:** Planning is read-only and defaults to no overwrite; destination replacement/merge is never inferred.
- **Edge cases:** Case-only names, collisions, overlapping sources, hard links, reparse points, placeholders, inaccessible files, unsupported metadata and destination changes.
- **Regression constraints:** Existing rename/replace/ZIP/Hot Folder policy and APIs remain intact.
- **Verification:** Pure planning fixtures plus native identity/reparse cases in disposable roots, using existing helpers where applicable.
- **Acceptance:** C12 planning/conflict decisions are representable, bounded and cannot authorize stale or broader writes.
- **Commit boundary:** `feat(transfers): [M9-A] add explicit transfer planning and conflict review`.

### M9-B — Transfer worker, pause/cancel and verification

- **Objective:** Execute approved plans with truthful state and safe move semantics.
- **Code ownership:** Proposed app-owned transfer service and native file adapter; narrowly shared filesystem helpers.
- **Tasks:** Implement streamed copy/duplicate, same-volume/cross-volume moves, no-clobber or explicitly approved replacement publication, source/target revalidation, verification phase, pause/resume, cancellation and progress. Revalidate source before any cross-volume deletion; include owned-stage outcomes.
- **Behavior:** Large operations leave the launcher responsive; state reflects acknowledged progress and actual per-item results.
- **Edge cases:** Disk full, permission changes, disconnected volume, changed source, destination race, paused large file, cancellation during verification and source-removal failure.
- **Regression constraints:** No source deletion before verified durable destination, no late overwrite, no fabricated rollback or silent fidelity loss.
- **Verification:** Focused worker/fault-injection tests and disposable native transfers including one partial cross-volume-move scenario or a faithful adapter fixture when a second volume is unavailable.
- **Acceptance:** C12's execution rules hold and failures leave discoverable, accurately reported artifacts.
- **Commit boundary:** `feat(transfers): [M9-B] execute bounded transfers with pause and verification`.

### M9-C — Durable receipts, restart and guarded reversal

- **Objective:** Preserve actionable operation outcomes across close/restart without automatic replay.
- **Code ownership:** Transfer service persistence/receipts and existing atomic-store conventions.
- **Tasks:** Persist state around destructive/publication boundaries; reconcile owned artifacts after interruption; retain bounded job history. Add explicit retry from a freshly reviewed plan and eligible undo with identity/content guards. Treat unsupported undo as a visible capability limit.
- **Behavior:** Restart shows Interrupted jobs and precise completed/remaining outcomes; it does not resume transfers automatically.
- **Edge cases:** Crash between publication/removal/receipt update, corrupt receipt, changed output before undo, occupied original name and retention limit with unresolved recovery.
- **Regression constraints:** Do not delete unresolved recovery authority, overwrite malformed records or treat paths alone as proof of ownership.
- **Verification:** Restart/reconciliation fixtures at meaningful boundaries, idempotent retry and refused-unsafe-undo tests.
- **Acceptance:** C12 history/recovery behavior is truthful and recoverable without rerunning successful items blindly.
- **Commit boundary:** `feat(transfers): [M9-C] retain receipts and explicit interruption recovery`.

### M9-D — File Operations Center UI and integration

- **Objective:** Expose complete queue management and entry points.
- **Code ownership:** Proposed transfer dialog/plugin, `src/gui/file_search_dialog.rs`, universal actions, `src/activity.rs` and its UI adapters.
- **Tasks:** Add picker/search-selection intake, plan review, queue order, progress/details, conflict controls, pause/resume/cancel/retry/eligible undo and completed-history actions. Register real jobs with Activity Center; provide domain terminal events for M11.
- **Behavior:** Closing the center leaves approved jobs owned by the app; reopening shows actual progress. Selections remain exact across sorting/filtering.
- **Edge cases:** Multi-file selections, stale search results, controls while paused/cancelling, multiple views of one job and unavailable destination.
- **Regression constraints:** No Explorer interception, duplicated worker, broad source-root substitution, per-frame effects or changed existing File Search shortcuts.
- **Verification:** Focused UI/command tests and a native queue/conflict/pause/reopen/verification smoke in disposable folders.
- **Acceptance:** C12 is usable end-to-end and Activity reports the same job identity/state.
- **Commit boundary:** `feat(transfers): [M9-D] add file operations center and exact-selection entry points`.

### M10-A — Downloads scan and reviewed-state model

- **Objective:** Add bounded, read-only downloads classification and durable review state.
- **Code ownership:** Proposed triage scan/domain/store, existing directory/path metadata utilities and saved-folder lookup.
- **Tasks:** Resolve/display the root; scan direct children; capture stable identity and timestamps; classify type/size/age/incomplete markers; accept origin only from reliable available metadata. Implement Reviewed/Unreviewed with revision checks and conservative archive/duplicate indicators.
- **Behavior:** Open/Refresh reads metadata without moving/deleting/hydrating files; changed occupants invalidate stale reviewed state.
- **Edge cases:** Downloads redirection, unavailable root, replacement file at same path, malformed origin metadata, reparse/cloud items, growing downloads and scan limits.
- **Regression constraints:** Do not scan browser histories, add an always-on watcher or label same-name folders as proven extraction.
- **Verification:** Classification/review persistence fixtures, malformed metadata, identity replacement and partial-scan tests.
- **Acceptance:** C13's metadata and review claims are accurate, bounded and nonmutating.
- **Commit boundary:** `feat(downloads): [M10-A] add bounded triage scanning and reviewed state`.

### M10-B — Triage interface and explicit tool actions

- **Objective:** Deliver Downloads Triage as an everyday review inbox.
- **Code ownership:** Proposed triage UI/plugin; exact-selection transfer, rename, ZIP, duplicate/hash/size/file-user actions already available.
- **Tasks:** Add search/sort/group/filter views, reviewed toggles, selected-item details, saved-folder suggestions and keyboard actions. Send selected files to M9; hand off eligible ZIPs with C11; offer confirmed Recycle Bin action and explicit Open/Reveal/Inspect.
- **Behavior:** Suggestions never execute themselves; incomplete/suspected states remain distinguishable from verified facts.
- **Edge cases:** Selection changes while a picker is open, file disappears/changes before action, batch partial recycle failure and restoring UI after another tool returns.
- **Regression constraints:** No implicit deletion/extraction, broadened selections or duplicate file-operation controller.
- **Verification:** Handoff/selection tests and a native reviewed→move→extract/reveal smoke using fixtures, plus a canceled recycle review.
- **Acceptance:** All C13 actions are reachable, preserve exact intent and reflect real outcomes.
- **Commit boundary:** `feat(downloads): [M10-B] add triage views and reviewed file-action handoffs`.

### M11-A — Notification records, retention and action model

- **Objective:** Establish a bounded persistent inbox without changing existing operation owners.
- **Code ownership:** Proposed notification domain/store, current persistence/data-recovery classification and typed action boundaries.
- **Tasks:** Define stable entry/source IDs, timestamps, severity, read/retained state, provenance and revalidated action descriptors. Implement search/filter/retention/dedupe and separate external session-only versus opted-in durable storage. Keep content out of logs/backups by default.
- **Behavior:** Local read/dismiss never changes an external source; pinned/history caps are explicit.
- **Edge cases:** Corrupt store, repeated event, ID reuse, time changes, retention expiry, pinned-at-cap and excluded source.
- **Regression constraints:** No new scheduler/transfer owner, secret payload storage or automatic command execution from body text.
- **Verification:** Domain/persistence/privacy fixtures and action authorization tests.
- **Acceptance:** C14's record, retention and privacy contract is enforceable independently of native capture.
- **Commit boundary:** `feat(inbox): [M11-A] add bounded notification records and retention policy`.

### M11-B — Internal notifications and inbox UI

- **Objective:** Deliver the full internal inbox and connect current producers.
- **Code ownership:** Proposed hosted inbox/plugin; Activity, timer/calendar/todo, macro-notification and Hot Folder event adapters.
- **Tasks:** Publish one sanitized terminal/attention event per semantic event; render searchable/filterable entries and unread state; add source controls, details, Open related, explicit snooze/Create Todo and local history actions. Preserve existing toast presentation as a separate channel.
- **Behavior:** Internal notifications work with no optional package; operation completion is not announced before success; actions use real current IDs.
- **Edge cases:** Stale reminder/job, changed file target, repeated render pass, muted presentation versus excluded retention, app restart and duplicate own-toast events.
- **Regression constraints:** Do not scrape `toast.log`, replace existing alarm scheduling, clear Windows notifications or emit one entry per progress update.
- **Verification:** Producer-event and action-deduplication tests; native timer/transfer/inbox interaction smoke with no external setup.
- **Acceptance:** C14's internal UX is complete and accurate, including keyboard/dock/pop behavior and privacy settings.
- **Commit boundary:** `feat(inbox): [M11-B] integrate internal outcomes and notification actions`.

### M11-C — Optional Windows setup and actual permission path

- **Objective:** Make the external listener's approved native route available through reversible opt-in setup.
- **Code ownership:** Selected platform listener adapter; optional identity/companion packaging assets and setup documentation; narrowly required Cargo Windows features only.
- **Tasks:** Implement the M0-B selected supported identity/capability/consent route. Supply build/register/unregister instructions or tooling with clear prerequisites; expose setup/access states and verify runtime identity. Keep data-directory and single-instance ownership stable. Request permission only from an explicit user action in a supported UI context.
- **Behavior:** Ordinary `cargo build --release` and internal inbox remain independent of SDK/packaging steps; absent/denied setup is a usable state, not a crash.
- **Edge cases:** Missing SDK/signing prerequisite, moved executable, changed identity, consent dismissed/denied/revoked, companion unavailable and uninstall while app runs.
- **Regression constraints:** No silent trust-store changes, OS-policy bypass, whole-app framework migration, fabricated access or packaged/unpackaged split data stores.
- **Verification:** Normal build with setup absent; manifest/setup validation and actual allowed/denied permission observation on a suitable authorized Windows test session. Log only metadata.
- **Acceptance:** The chosen route obtains real supported access when installed/permitted and leaves ordinary use intact. A mock or unsupported placeholder is not completion.
- **Commit boundary:** `feat(inbox-windows): [M11-C] add optional listener setup and permission flow`.

### M11-D — External capture, reconnection and source actions

- **Objective:** Complete Q1-A with real external entries in the inbox.
- **Code ownership:** Platform listener lifetime/snapshot adapter, notification domain dedupe, inbox source/status controls.
- **Tasks:** Read supported current toasts and observe changes; normalize bounded app/text metadata; dedupe updates/ID reuse; suppress own-toast loops; handle revoke/disconnect/reconnect and session-only retention. Offer validated source actions, plus separately labeled exact-item Windows dismissal where supported.
- **Behavior:** Coverage and status are visible; current snapshots are not called historical completeness; clearing a local filter/view does not clear the Windows notification center.
- **Edge cases:** Empty reads after revocation, malformed entry, source app gone, same ID reused, repeated snapshot, permission lost during callback, listener restart and stale dismissal target.
- **Regression constraints:** No database scraping, global clear, arbitrary body action replay, hidden collection after disabling, or duplicate internal completions.
- **Verification:** Adapter fixtures plus a real non-Multi-Launcher test toast captured after opt-in, subsequent removal/update observation, permission revocation and disabled-path idle check. Use nonprivate test text and avoid screenshots of unrelated notifications.
- **Acceptance:** C14 external capture is demonstrably functional on a documented supported setup; denied/off/unsupported paths remain honest. Record a native environment blocker separately if proof cannot run; never mark Q1-A complete from mocks alone.
- **Commit boundary:** `feat(inbox-windows): [M11-D] capture supported external notifications with explicit lifecycle`.

### M12-A — Existing ordinary utility keyboard gaps

- **Objective:** Extend C1 across the utility surfaces identified by M0-B.
- **Code ownership:** Existing Jump Mode scope/paint/target registry and the concrete ordinary utility dialogs listed in the coverage ledger.
- **Tasks:** Add missing semantic target registration, logical tab order, row/options navigation, focus visibility, keyboard actions menus and off-screen reveal. Group changes by established host/control patterns; retain stable identities and virtualized-list behavior.
- **Behavior:** File/data tools, settings and ordinary controls are usable without the mouse in both presentations; selecting a hint never authorizes an action.
- **Edge cases:** Hidden/disabled/clipped controls, table filtering/reordering, empty lists, small window, nested popup and rapid source/owner change.
- **Regression constraints:** No new global hook, repeated scroll-to-center, accidental Enter confirmation or stolen text input.
- **Verification:** Focused registry/ownership/key-routing tests and representative native utility flows from the ledger, not an exhaustive pixel matrix.
- **Acceptance:** Every ordinary control gap assigned to this checkpoint is closed; intentional exclusions are specific and justified.
- **Commit boundary:** `feat(keyboard): [M12-A] complete ordinary utility navigation coverage`. Split by coherent utility family only if needed, using additive checkpoint suffixes on the same branch.

### M12-B — Editors, new tools and final focus consistency

- **Objective:** Close remaining editor/outline/new-UI gaps and verify cross-tool focus behavior.
- **Code ownership:** Existing editor/authoring utility adapters, C1 coverage ledger, new dialogs and shared utility-host integration where directly necessary.
- **Tasks:** Finish ordinary controls around editors and pointer-only canvases; ensure typing remains authoritative inside text fields; check all newly delivered tools for reachable settings/footers and dock/pop parity. Fix actual omissions at the owning adapter, not by global synthetic input.
- **Behavior:** Users can navigate controls, move sections and use new tools with the keyboard while the main query remains interactable.
- **Edge cases:** Multiline Enter, selection shortcuts, open pop-out with root query click, Esc from nested menus, changed target generation and retained drafts.
- **Regression constraints:** Canvas drawing/dragging itself is not promised; no workspace features, new shortcuts that override established bindings, or focus loops.
- **Verification:** Directly affected editor/host tests; a batched native main-query→pop-out→Jump→typing→Escape flow and final ledger inspection.
- **Acceptance:** C1 is fulfilled across the declared ordinary surfaces, with no unjustified omissions or unverified claims disguised as coverage.
- **Commit boundary:** `feat(keyboard): [M12-B] finish editor and cross-tool focus consistency`.

### M13-A — Final targeted review, documentation and handoff

- **Objective:** Reconcile all 14 requirements with actual evidence and preserve the correct final branch tip.
- **Code ownership:** Cumulative task diff, affected tests/docs, one execution ledger; no unrelated production cleanup.
- **Tasks:** Have the configured reviewer inspect the final source/diff and requirement matrix; resolve feature-related findings with small remediation commits. Run the union of relevant tests once, one final first-party lint pass and one final ordinary application build/smoke where available. Finalize user guides and optional external setup instructions; record honest native limitations and Git ancestry.
- **Behavior:** Every feature has discovery/help, correct defaults and compatible persisted data; no mock capability is counted as shipped behavior.
- **Edge cases:** Pre-existing unrelated failures, unavailable native permission/printer environment, push rejection, dirty unrelated user files and newly discovered feature defect.
- **Regression constraints:** No full-suite escalation without a concrete cross-cutting reason, repeated release builds, deleted failing tests, automatic merge or scope expansion.
- **Verification:** Section H's final bounded pass; inspect diff/staged files and verify the parent/baseline ancestry and checkpoint history.
- **Acceptance:** Section J is satisfied or the final report explicitly identifies partial/blocked rows; final handoff has the literal `final_tip_sha` and actual push state.
- **Commit boundary:** `docs(productivity): [M13-A] finalize acceptance evidence and chained handoff`, after all necessary correction commits. Do not create a meaningless source change merely for a subject.

## G. Active Checkpoint Commit Cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** Section F supplies those boundaries; M0-A is mandatory and first.

Commit each coherent verified increment on the same task branch, with the prescribed checkpoint label and an accurate subject. Do not accumulate several unrelated features into one final commit, and do not commit every trivial line. Additional necessary splits/remediation use new explicit labels on this branch and are recorded in the ledger; they do not create new product milestones or branches automatically.

Before every commit inspect branch, HEAD, staged paths and diff. Stage only intended task changes, preserving pre-existing edits. Run checkpoint-appropriate checks, not an automatic full qualification suite. Attempt one ordinary non-force push after each commit; record failure and continue locally. Never force-push, squash, amend away checkpoint history or change ancestry to make a push succeed.

Use `baseline_sha` for implementation-diff tracking and `parent_tip_sha` for chain verification. The baseline's direct parent is the prior completed Goal's final tip or the legitimate first integration tip. No checkpoint gets its own branch and no intermediate merge/rebase/cherry-pick is used to advance work. This plan defines no internal category branch handoff.

At the last actual commit, resolve `final_tip_sha` and include it literally in the final response and any existing external queue/orchestrator record. A tracked ledger cannot contain its own commit hash without changing that hash: record the final checkpoint/subject and evidence there, then report the resolved literal tip after committing. Do not enter a self-referential amend/commit loop. The next Goal's required parent is that literal final tip; an unpushed local tip remains authoritative on this machine. A different machine missing it must block rather than branch from an older remote commit.

## H. Verification and Testing Strategy

### H1. Targeted automated checks

Use Cargo Nextest for relevant Rust tests. Inspect actual target names first (`cargo nextest list --lib` or the relevant integration target); do not invent existing names from this document. New test names may use a consistent `productivity_*` prefix when helpful. Examples of command shapes, with the discovered filter substituted, are:

```powershell
cargo check --lib
cargo nextest run --lib -E 'test(<actual-focused-test-or-module>)'
cargo nextest run --test <actual-affected-integration-target> -E 'test(<actual-filter>)'
cargo clippy -p multi_launcher --lib --bin multi_launcher --no-deps
cargo build --release --bin multi_launcher
```

These are selective tools, not a mandatory stack after every checkpoint. Use cached development builds during iteration. Run an ordinary production build once at the final appropriate boundary, plus a relevant optional listener package build when needed; do not repeatedly rebuild release for small changes. Check only first-party targets affected by this Goal; do not run vendored cleanup. Format touched Rust files without repository-wide churn.

For application fixtures, follow `AGENTS.md`: disable Paste Sequence Next and Cancel shortcuts before construction (`next_hotkey = None`, `cancel_hotkey = None`) while keeping production defaults unchanged. Use isolated temp data roots; never exercise file mutation against the user's actual Downloads, notes or application data. Avoid parallel tests sharing clipboard, native hooks, desktop state or process-wide configuration. Rerun an apparent interference failure individually once, preserve the evidence, and fix the real isolation/behavior cause rather than weakening an assertion.

Bound potentially hanging individual tests to approximately 100 seconds of **test execution after compilation**, using an existing Nextest profile or narrow supported runner setting. This is not a 100-second compilation deadline or a cap on an entire multi-target suite. Do not expand global production settings solely for this task.

### H2. Focused acceptance evidence matrix

| Requirement | Essential automated evidence | Native/user-visible evidence |
|---|---|---|
| L11 | Scope/generation validity, key ownership, focused scrolling, no hint execution | Existing utility plus editor and new-tool dock/pop flows; main query remains editable |
| P5 | All fixtures, input immutability, callback types/arity/budgets/rename | Script results, watch/copy and dependent chart update |
| P6 | Output length, empty/window/zero denominator/overflow cases | Raw versus aligned rolling series |
| P7 | Known numerical values, sample/population distinction, degenerate paired data | Reusable named outputs with diagnostics |
| P10/P11 | Bin counts/boundaries, CDF, transforms, persisted defaults, scene parity | Histogram/CDF, fixed/log/reference options and copied chart |
| CP-DOC | Coverage, executable examples, search ranking, safe insertion/undo | Find `pop` by name/concept and follow related functions |
| N5 | Source-preserving range moves and invalid/stale refusal | Nested move and Undo in Notes and Studio |
| N3 | Pagination/content/links, settings and output publication | Preview, real PDF inspection, direct test print submission |
| U13 | Rule/entropy/generator failures and all secret-retention boundaries | Generate/copy with history exclusion and no clipboard damage |
| ZIP-DEST | Exact derivation, accepted/cancelled picker, stale preview invalidation | No directory before extraction; expected destination after confirmed extraction |
| X9 | Planning, native/fake faults, pause/cancel, receipts/restart/guarded undo | Queue/conflict/progress/pause/reopen and actual output verification |
| X8 | Unknown origin, identity-bound review, partial scans, explicit actions | Review and handoff to transfer/ZIP/inspect from disposable downloads |
| X25 internal | Single event publication, retention, local/source action separation | Timer/transfer outcome→inbox→explicit action without optional setup |
| X25 external | Snapshot/dedupe/revoke adapter tests plus real listener execution | Optional setup, allowed real external capture, denied/revoked/off and no own-toast loop |

Capture a small number of representative screenshots or equivalent interaction evidence when the environment supports them. Screenshots are supporting evidence, not a substitute for observing results or native status. Do not introduce pixel-perfect automated harnesses. Mask/exclude private desktop content and generated secrets. For PDF output, inspect rendered representative pages as well as extracted text/page metadata; do not call a structurally valid but clipped PDF acceptable.

### H3. Final scope

Run the union of directly affected automated targets once after integration, plus focused reruns for corrections. A full `cargo nextest run` is **not automatic**; use it only if a concrete shared-boundary change leaves consumers inadequately covered and record that reason first. One final first-party Clippy pass should leave no new goal-related findings. Existing unrelated findings are recorded, not silently fixed or broadly allowed. Review the cumulative diff for accidental formatting/config/data changes.

Codex builds, launches, interacts and records evidence where available; the user is not an intermediate test operator. Missing desktop/printer/permission capability is explicitly Unverified or Environment blocked. Never substitute an API return value or mocked provider for a real promised native interaction. The user's own final review is supplementary, not an excuse to skip available verification.

## I. Risk Management and Bounded Validation

| Concrete concern | Required control |
|---|---|
| New vector callbacks change name binding or consume excessive work | Resolve through existing formula scope; explicit extra args; include callbacks in rename/dependencies and budgets; preserve immutable results. |
| Numerical-looking output is wrong for degenerate data | Fix interpolation/denominators/undefined rules in tests and docs; use stable arithmetic and honest diagnostics. |
| Chart settings restore incorrectly or copy stale output | Additive compatible defaults; definition/evaluation identity checks; one bounded screen/export scene. |
| Outline moves lose text or change hierarchy | Source-range prepared edits, explicit sibling boundaries, minimal separator edits, one undo and anchor warning. |
| PDF preview differs from print/export or hangs on long content | Shared page layout, bounded assets and pages/work, defined long-block splitting, real representative render inspection. |
| Passwords leak through ordinary infrastructure | Protect generation/result/action/history/clipboard/inbox boundaries before publication; restart-aware clipboard exclusion. |
| Copy/move damages unrelated or changed files | Exact reviewed intent, handle/identity checks, no-clobber publication, explicit replacement, verified move deletion and durable partial receipts. |
| Downloads suggestions are mistaken for verified facts | Unknown origin, suspected versus verified duplicate, receipt-bound extracted status and explicit actions. |
| External notification permission/setup cannot be proven in this host | Early native-boundary check, one selected documented route, honest setup states and a real external capture acceptance requirement. |
| Feature breadth causes endless remediation | One writer, per-checkpoint packets, targeted verification, no unrelated suite/benchmark/framework project. |

Normal compiler/test failures introduced by a checkpoint must be fixed; they are not a reason to hand routine debugging to the user. For a persistent failure with no new information, make at most **two materially different focused remediation attempts after the initial diagnosis** before reassessing. Do not repeat identical native capture/build loops. An additional attempt is justified only by concrete new evidence and recorded scope.

Distinguish: (1) a goal defect, which must be resolved or marks the feature incomplete; (2) an unrelated pre-existing issue, which is recorded without absorption; and (3) an optional improvement, which is deferred. Native environment constraints and signing/consent requirements are not permission to lower acceptance. Continue independent checkpoints on the same branch while retaining blocked statuses. Never label all 14 delivered when one has only a stub.

For a genuine stop, follow `AGENTS.md`'s concise report convention under `docs/plans/execution/standalone/productivity-toolkit-expansion/stalls/` and update the existing ledger, not a second operational tracker. Include the failing/unrun check, expected/actual result, evidence, attempts, affected checkpoints, preserved work, Git state and smallest next action. Do not include passwords, copied content, private notification text, credentials or uncontrolled logs. Safe report commits receive the same one best-effort push. No background promises or automatic OS permission bypass.

## J. Definition of Done

The Goal is **Complete only when** all of the following hold:

- All C1–C14 requirements and all 14 feature rows are implemented through actual application entry points, with no unsupported-placeholder substitution. The optional end-user external setup has a working supported capture path with recorded real evidence.
- New APIs and numerical conventions have passing focused tests and matching in-app/reference examples. Existing language semantics, 1,024-value vector limit, Manual calculation and old chart definitions remain compatible.
- Keyboard coverage inventory is reconciled; ordinary controls and new UIs have consistent ownership and dock/pop behavior, and root query focus remains available. Exclusions are specific, not blanket excuses.
- Notes/Studio preserve drafts and undo; section moves preserve content; PDF/print produces reviewed content through the shared page model with necessary native validation accurately recorded.
- ZIP destination convenience retains guarded extraction; transfer/triage actions are explicit and identity-safe; interrupted/partial outcomes and recovery are truthful.
- Secrets are not retained by app histories/inbox or diagnostics; external notification capture is off by default, consented, bounded, removable and clear about coverage.
- New settings/stores use validated compatible persistence, bounded resources, appropriate backup exclusions and safe shutdown. Ordinary launcher builds do not require optional external-listener setup.
- Targeted verification and available native UI checks have actually run. No known feature-related blocking defects or unaddressed substantive review findings remain. First-party lint results and unrelated pre-existing issues are accurately reported.
- Implementation is committed at active checkpoints on the correct fresh child branch; baseline's parent is the recorded required parent; pushes are accurately reported. No worktrees, auxiliary checkouts, independent branches, intermediate merges or history rewriting were used.
- The literal final branch `final_tip_sha` is captured after the final commit and handed off as the next Goal's required parent, with no uncommitted feature work silently transferred.

If mandatory native validation or a feature remains blocked, deliver the actual implemented/verified portions and label the Goal **Partial / Environment blocked** or **Blocked**, as appropriate. Do not present that outcome as Complete, and do not make the user responsible for an endless intermediate test cycle.

## K. Final Delivery Report

Provide a concise factual report containing:

1. **Outcome:** Complete or an explicit partial/blocked status, with a 14-row requirement table showing implemented, verified and remaining evidence separately.
2. **Milestones/checkpoints:** Completed IDs; blocked/deferred IDs and exact reasons; no invented pass counts.
3. **Important decisions:** Final API spellings/conventions, PDF/print dependency/path, transfer fidelity/recovery limits, external listener deployment/permission route, privacy/retention behavior and any justified departure from a routine planning default.
4. **Changed areas:** Meaningful source/documentation locations, not an unfiltered file dump.
5. **Verification:** Actual commands, selected targets/filters, results, individual reruns, build/lint findings and native UI/print/listener evidence locations. State which native capabilities were unavailable.
6. **User-visible entry points:** How to open each new utility/updated dialog and find its reference documentation; include optional external setup prerequisites and reversible enable/disable instructions.
7. **Git handoff:** `parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, significant checkpoint SHAs, literal `final_tip_sha`, actual upstream/push state, clean/dirty checkout state and preserved unrelated user edits. State explicitly: **the next Goal must create its fresh branch from this `final_tip_sha`, not from master/main**; no Git worktrees or intermediate branch integration were used.
8. **Remaining limitations:** Real platform/support limits and any unverified behavior. Brief final manual-review recommendations are acceptable, but do not transfer available intermediate verification to the user.

Do not merge to master, open an integration PR, publish a release or begin another feature unless separately authorized. Stop at the approved Goal boundary.
