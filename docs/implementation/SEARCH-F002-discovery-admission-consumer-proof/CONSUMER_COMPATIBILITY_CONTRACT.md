# SEARCH-F002-consumer-v1

Status: **REJECTED_CANDIDATE / H0 NO_CHANGE**. Authored by `/root/contract_author`, 2026-10-29. This preserves a concrete, narrow source-plausible candidate that the bounded decision declines to recommend. It is not an implemented or accepted capability. Production source/base is `59156d96adcc73939c7171487a0cb582e3286587`. Its canonical UTF-8/LF content hash is recorded in the separate [decision](ADMISSION_DECISION.md), avoiding a self-referential hash.

## Consumer and evidence boundary

The candidate consumer is a **separate, explicitly selected XML Toolkit suggestion in the main LauncherList or LauncherGrid**, for freshly typed ordinary text such as `format xml`, after current canonical search has definitively returned no actions. The suggestion opens the existing XML Toolkit with its source-owned Action; it neither formats anything nor infers an XML document. This contract contains one fixed opener and one illustrative natural-language request. It defines no ranking algorithm, aliases, learned policy, relevance improvement or additional surface. It does not establish a consumer for every descriptor in the historical eleven-descriptor proposal.

The potential benefit is helping a person who knows the operation but not the `xml` shortcut find the existing panel. That value is a hypothesis, particularly in the restricted profile below. Maintaining 81 reserved envelopes for this one opener is a material cost. The positive source witness alone does not establish enough value to recommend infrastructure. The existing direct `xml` route already opens this exact tool; C2 adds a plain-language way to find one opener only after disabling the other 80 providers and choosing exact mode. It still needs all 81 reserved-envelope contracts, correct origin/finality/lifetime policy and separate selection authority. No measured usability benefit or default-like reachability offsets that maintenance in this bounded record. **H0 keeps production unchanged and rejects proceeding on this candidate.** This is a cost/benefit disposition, not a claim that the source witness is impossible or unsafe.

Evidence labels throughout this contract:

- **S — source-established:** directly read current control flow/declarations and fixed Action fields.
- **C — configuration/condition:** specified possible state, not an observation of live settings, providers or user data.
- **R — reasoned consequence:** follows from S when all stated C conditions hold; no provider was invoked.
- **U — unrun:** live/provider/native/interaction behavior still requiring focused evidence if this candidate is accepted.

No synthetic Complete dictionary, data-empty fixture, optional-model pass or historical holdout is runtime evidence. Optional admission model: **NOT_NEEDED**, zero states and invocations. The decisive issue is source scope and consumer value, not an unresolved finite transition truth table.

## Two configurations and concrete query sequences

Both configurations use exactly the same admission, origin, binding and automatic-consumer rules below. Settings defaults are S at `src/settings/model.rs:1137–1170`: enabled providers/capabilities and plugin directories are None; exact matching is false. Defaults do not reveal configured directory contents or actual registration.

| Candidate | Concrete configuration and sequence | Source result and disposition |
|---|---|---|
| C1, ordinary/default-like | Source defaults for enablement/capabilities/exact mode; no live roster assumed. Person opens the main list or grid, types `format xml`, waits for ordinary canonical processing, then would explicitly select the separate XML opener if admitted. | All 81 builtin outer envelopes can be reviewed, but actual external registrations/tombstones, provider health/finality and fuzzy command-cache fallback emptiness were not established. Current default-like reachability/utility is **UNESTABLISHED**. No C1 positive admission claim. |
| C2, XML-focused restricted profile | `plugin_dirs=Some(Vec::new())`; fresh manager lifecycle with exactly the 81 reviewed unique Builtin registrations and no unresolved external/removed owner; `enabled_plugins=Some({"xml_toolkit"})`; `enabled_capabilities=None`; `match_exact=true`; current XML command cache, with no folder/bookmark alias matching `format xml` for `query:xml`. No pending provider work, capacity deferral or concurrent policy/catalog change. Person types `format xml` directly in the main query editor, canonical search finishes empty, then explicitly selects the separate XML Toolkit suggestion. | A **source-plausible** empty canonical path is derived below. All 80 disabled builtin owners remain in coverage and reserve their syntax. Restriction reduces canonical work; it does not erase UNKNOWN owners. C2 is a retained positive source witness but a **rejected infrastructure candidate**: its one-opener benefit over `xml` does not justify the all-owner maintenance and eleven-descriptor proposal within the available evidence. |

C2 is an intentionally narrow tool profile, not a recommendation to disable users' plugins or rewrite their settings. No profile was created or read. An enabled-provider allowlist alone is insufficient: an unknown dynamic/Registered owner, changed builtin or unresolved unloaded identity, even disabled, rejects C2 entirely. The fresh builtin-only lifecycle must be established from authoritative registration/lifecycle facts, not inferred from an empty directory setting or a plugin-name string. No third configuration was investigated.

### C2 empty-path derivation

1. S: `reload_from_dirs` registers the 81 builtins before iterating configured directories (`src/plugin.rs:877–1008`). The E/C manager gate precedes provider search; XML declares head `xml` (`src/plugins/xml_toolkit.rs:6–45`). C: the exact roster, source and policies above have been established, all disabled syntax remains accounted for, and no unresolved owner exists.
2. S: `format xml` is outside every conservative reserved envelope in the [manifest](SOURCE_ADMISSION_MANIFEST.md). It has no filter, quote, path, URL, calculator or shell shape. The enabled XML provider does not match head `format`, so its search is not called; disabled owners cannot supply rows for this query under E/C. This does not assert provider locks/lifecycles are healthy in an arbitrary running app.
3. S: non-g provider-vector-empty fallback scans `command_cache` (`src/gui/search.rs:1245–1300`). `commands_filtered` filters by enabled provider names, **not configured search capabilities** (`src/plugin.rs:1175–1188`). Thus C2 uses provider enablement as well as explicit exact mode; denying search capability alone would not create this cache.
4. S: the XML command shortcut is exactly label `xml`, desc `Open the bounded offline XML Toolkit`, action `query:xml`, args None (`xml_toolkit.rs:37–45`). Exact fallback checks label/desc/action substrings plus folder/bookmark aliases (`search.rs:539–560,1245–1275`). None of the three literal fields contains `format xml`; C excludes a matching alias. This derivation uses strings and control flow, not an executed parser/matcher or data-empty stand-in.
5. S: application catalog actions are searched only for the explicit app route in the inspected shared outcome path (`search.rs:1579–1590`), so arbitrary application catalog contents need not be empty for this example. C excludes g/app routes, pending tickets, capacity deferral, a revision change and unstable mutable-result catalogs. R: with those C facts, canonical actions are empty and the source outcome selects NoResults (`search.rs:1611–1624`).

This is a source-level witness under explicit configuration and lifecycle conditions. **U:** no running manager, command cache, provider tickets, actual settings, native edit or XML panel was observed. Existing outcome fields alone are not an implemented complete admission receipt, nor a general proof that silent provider failure means no match.

## Exact insertion and presentation boundary

Current canonical ownership remains in `LauncherApp::search` and the shared `search_read_only_outcome*` functions. `search` installs `self.results` and records provider revision, pending, deferral and mutable result-catalog versions (`src/gui/search.rs:806–958`). Command cache/completion remain at `707–767`. The existing performance observer is evidence/telemetry machinery, **not query-origin authority**.

If separately accepted and implemented, the GUI reads a coherent current canonical outcome **after** installation and computes a separate presentation projection. Admission requires all of:

- Exact current raw query, user-edit generation and canonical query generation agree; empty input is excluded.
- Canonical state is NoResults and `self.results` is empty; results are current, stable and valid. `last_results_valid` alone is insufficient because it can coexist with pending state.
- No provider pending ticket, failure/unknown outcome, capacity deferral, suppressed failed-query provider path or unstable/start-versus-end revision. Inability to establish finality rejects admission.
- Reviewed coverage identity matches the current complete registered roster, including disabled owners and retained unresolved removed-owner cost. Coverage is distinct from enabled descriptor eligibility. All required identity/policy/source facts are available; any unknown rejects.
- The raw query passes conservative protected-form guards before normalization, and the consumer owns fresh direct user input on the main list/grid.
- The explicit prospective opt-in is committed and enabled. Its default is false; this spike adds no field.

The future projection has separate selection identity and storage; it is never inserted into `self.results`, `command_cache`, `command_search_cache`, completion index or the canonical read-only query/broker return values. The current list/grid branches read `self.results` (`src/gui/render.rs:7546–7549,7637,7811`); a prospective separate suggestion section would be coordinated there without changing canonical row order, cardinality or indexing. No existing canonical index may identify a discovery row. Zero canonical rows stay zero for every existing caller.

Explicit user selection is required: a suggestion must not become a default canonical first result or activate merely because Enter was pressed before selecting it. A separate current binding must exist and be focused/selected. The actual input source remains Enter or Click as applicable; list versus grid remains a separate surface fact.

## Raw route protection and corrected search distinctions

The manifest's reserved families apply even to malformed or empty-result commands. All 36 manager heads, all 45 reviewed global envelopes, `csfoo`, partial `br`/`bri`/`brig`/`brigh`, and nonempty partial `check missing` forms stay protected. Do not replace these with enabled-prefix lookup (`src/plugin.rs:1310–1327`) or a descriptor-owner allowlist.

For this candidate, restrict possible discovery input to plain alphabetic words separated by ordinary ASCII spaces; case folding may be used only after raw protection. Reject all known heads/envelopes and any string outside that narrow shape. In particular reject g/app heads, action filters including negated kind/id forms, quoted input, punctuation/payload delimiters, paths/drive/UNC separators, URLs/schemes, regex syntax, leading `=`, numeric arithmetic and shell forms. Alphabetic `calc`/`sh` families remain protected as raw heads. Unknown shapes abstain rather than guessing intent. This is a proposed conservative exclusion, not a new parser or a claim all such strings currently execute.

Current `plugin_query_plan` owns Literal/filter precedence (`src/plugin.rs:1363–1408`); QR's declared raw Literal head and payload must remain intact. `split_action_filters` at `src/common/query.rs:230–263` can strip kind/id tokens before ordinary provider routing. Discovery cannot strip those tokens to turn a protected request into plain intent.

Preserve these distinct paths:

- **Non-g fallback:** command-cache fallback starts when the provider vector supplied to `search_plugins_from_results` is empty, not only when a later display-filtered list is empty. The vector may already reflect manager action filters.
- **g:** the g branch returns before ordinary command-cache fallback (`search.rs:1167–1243`); its provider selection has existing special behavior. A disabled web plugin or empty filtered g result never licenses discovery.
- **Post-filter empty:** a nonempty provider vector can become empty after exact display filtering without entering the command fallback branch. That is not proof the original command was unclaimed.
- **Capacity:** `ProviderSearchDeferral::Capacity` explicitly constructs pending/deferred evidence (`1420–1432`). It is never completed no-match.
- **Failed-query reopening:** `search_read_only_outcome_without_providers` deliberately suppresses retry and feeds pending=true even if capacity has become free (`1451–1471`). It stays excluded; preserve worker retirement and fallback behavior.

## Query origin and invalidation ownership

Query origin is **proposed policy**, distinct from existing `commands::ActivationSource` and `universal_actions::ActionSurface`. The GUI query-input coordinator would own it; no rendering-time string comparison can infer it. The real editor seam is `show_root_query_editor`/`query_text_edited` (`src/gui/render.rs:6743–6754`) and first-pass edit handling (`6943–6951`, `handle_query_text_changed_after_intent` at `1747`). `response.changed()` alone is not sufficient evidence of a fresh direct edit if the change was programmatic.

| Transition | Proposed origin / admission consequence |
|---|---|
| Actual enabled main-query editor receives direct user text insertion/deletion, with owned input/focus and a new query generation | FreshUserEdit for that generation; still must pass every other guard. Unattributed input/paste/IME automation remains unknown until a concrete source policy can distinguish it; no such native claim is made here. |
| Programmatic Set/Show/query/queryexec, Macro/broker, saved query, history navigation, completion acceptance, radial/dashboard/hotfolder/gesture/hotkey update | Programmatic/History/Completion as applicable; invalidate suggestion/selection; never promote just because text is displayed. |
| Ordinary render, repaint, async completion, focus restoration or unchanged query refresh | Preserve origin identity only; cannot create FreshUserEdit. Revalidate outcome and policy before any new projection. |
| User genuinely edits previously programmatic text in the owned query editor | New FreshUserEdit generation may begin; the resulting raw string must independently pass protected syntax and finality. Merely moving caret or choosing a result does not qualify. |
| Query, source, surface, editor ownership, opt-in, enablement, capability, coverage/source version, origin/name/epoch, result catalog or provider generation changes | Invalidate projection and immutable selected binding. Old rows cannot remain actionable while a new projection is built. |
| Modal/action sheet intercepts input, launcher hides, or query provenance is unavailable | No admission/activation. Restoring the window alone does not restore an old selection's authority. |

Existing `track_c_query_observer::Origin::TextEdit` is not reused as authorization. History already has a distinct source seam (`render.rs:1758–1788`); the prospective policy must cover all programmatic mutations, not only the two inspected examples.

## Action identity, activation and Settings

The only selected opener in v1 must use the **entire exact source Action** from `src/plugins/xml_toolkit.rs:12–17`:

```text
label = Open XML Toolkit
desc = Browse, query, validate, format, and save bounded local UTF-8 XML
action = xml:open
args = None
```

A later implementation must obtain that value through the builtin's pure owned constructor, not copy a wire command from this document or invoke search to enumerate openers. No XML text/path, filename, clipboard contents, selected file, query suffix or inferred argument is appended. Opening the existing tool can retain its current state; the contract does not promise a blank panel or no work inside its existing opener.

The immutable selection binds contract/descriptor identity, full Action including args, Builtin origin, exact name `xml_toolkit`, registration epoch, raw-query/edit generation, main surface, reviewed coverage version/source identity, registration/catalog revision and current enablement/capability/opt-in policy identity. Re-read owner-authoritative facts immediately before dispatch. Replacement, disappearance, disabling, same-name dynamic registration, changed source, policy or query invalidates it. Discard stale selection; never retarget a list index or silently resolve a same-name replacement. If fresh facts cannot be obtained, activation is unavailable.

Existing `activate_action` parses exact Action plus query override and ActivationSource, then performs current destructive confirmation/dispatch (`src/gui/actions.rs:113–187`). It does **not** already provide universal provider-origin revalidation. The prospective GUI/domain boundary must perform that check first, then use this authority with `query_override=None`. Actual surface is LauncherList/LauncherGrid; actual source is Enter/Click. `UniversalActionInvocationContext` (`src/universal_actions/model.rs:125–145`) carries those distinct dimensions when that existing path is used; do not invent a parallel dispatcher or label a Macro as a Click.

The prospective false-default opt-in belongs in existing Settings model/editor ownership, via `SettingsEditor::save_settings` → owned `SettingsSaveIntent::general` → acknowledged apply (`src/settings_editor/save.rs:12–66`; `src/settings_save.rs:199–249,830–1007`). Provider/capability selection uses the existing dedicated ownership. Do not publish draft policy early or write a parallel JSON file. Save pending/failed/rollback/reconciliation states invalidate admission; publish a new policy identity only after authoritative committed application. Turning off clears suggestions immediately. Rollback restores only owned settings and preserves concurrent unrelated changes. No persistence, UI controls or rollback implementation is added here.

## Automatic and non-main consumers

`handle_macro_launcher_command` sets query, calls the shared `search()`, and **immediately** interprets `self.results` as zero → NoResults, one → normal Macro activation, many → PresentedForSelection (`src/gui/render.rs:3098–3140`). It does not await finality. Discovery must not change those instantaneous branches, even when a provider is pending. The legacy query broker's existing first-result behavior at `3171–3190` also stays canonical.

`queryexec:` is ExecuteFirst at `src/commands/parser.rs:531`; handler `launcher_query.rs:46` preserves the source in `activate_first_result`. The source test `src/gui/actions.rs:1513–1554` asserts recursive first-result activation with the original Dashboard source. These automatic paths cannot acquire the new suggestion. Radial query resolution uses the same shared ordering/cache/filter outcome; dashboard, hotfolder, saved/programmatic queries and non-main surfaces receive no discovery projection. Their existing commands and confirmation policies remain unchanged.

Relevant existing test source was read, not run: Macro cardinality test (`render.rs:47177`), queryexec source preservation (`actions.rs:1513`), and the Literal/capacity/failed-query test locators in `search.rs`. No tests were added or changed in this documentation milestone.

## Focused remaining evidence, rejection and rollback

Before any separately accepted consumer implementation could claim compatibility, it would need focused evidence for: a real C2 fresh builtin-only roster with no unresolved owner; actual canonical empty/current completion under the specified cache/alias policy; direct user input provenance and every programmatic invalidation seam; selected Action/epoch/policy invalidation on replacement/disable/save; separate list/grid selection and existing XML activation; and preserved Macro zero/one/many, broker/queryexec, Literal/g/app/filter, capacity and failed-provider fallback. Use isolated app-test settings with Paste Sequence next/cancel hotkeys None if such future tests are authorized. No broad native campaign is a condition of finishing this source decision.

Reject admission whenever any guard lacks evidence. The current decision rejects H1 because C2 utility is too narrow to justify the 81-owner maintenance burden in the available evidence. A later runtime/native contradiction would additionally invalidate the source witness assumptions, but no such contradiction is claimed here. C1 remains unestablished; C2 does not prove ordinary-profile utility. No live proof, numerical relevance claim or product acceptance exists here.

Current rollback is scoped supersession/removal of these task documents only. A prospective accepted feature would use the false-default committed Settings opt-out to remove the projection while canonical search remains untouched. This contract authorizes neither production work nor a new future project. The separate [acceptance block](ADMISSION_DECISION.md) is authoritative for its status.
