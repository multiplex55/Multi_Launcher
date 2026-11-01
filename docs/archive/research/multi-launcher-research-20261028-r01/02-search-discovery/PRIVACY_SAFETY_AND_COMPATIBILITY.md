# Privacy, safety and compatibility contract

M1-C proposed design, no production execution. [Current architecture](CURRENT_SEARCH_ARCHITECTURE.md), [runtime](RUNTIME_AND_INTEGRATION_DESIGN.md), [UX](KEYBOARD_AND_RESULTS_UX.md) and closed [audit handoff](../01-static-audit/SEARCH_HANDOFF.md) supply ownership/evidence. The model-authored [corpus](evaluation/queries.jsonl) contains synthetic strings/settings only; it was not loaded or transformed from user history, clipboard, files or runtime providers.

## Data and authority boundaries

| Data/class | Current fact / proposed discovery rule |
|---|---|
| Query text | Existing root owns raw query. Discovery borrows it for admitted generation, retains only current normalized tokens/explanations and drops them on edit/invalidation/hide. No query logs/MRU/telemetry or persistence. Protected syntax is never normalized by discovery. |
| Tool metadata | Source-owned fixed opener, identity and finite curated vocabulary; no `Plugin::search` or `commands()` invocation to collect it. Keep manager metadata revision/epoch/origin distinct from capability permission. |
| Usage | Existing action-string/count identity, no established recency timestamp. Research candidates ignore all usage; counts 0/1,000,000 perturbations must leave output unchanged. No new usage/history format or migration. |
| History/pins | History holds raw query/full Action/source/time; pins use action+args. Neither becomes relevance ground truth, safe training data or executable current authority. Do not feed them to this catalog/prototype. |
| Clipboard/Paste/OCR/capture | Raw clipboard/undo/run text, pixels/OCR and leases remain existing domain owners. No read, capture, background polling or preview sampling for discovery. Pasted query text is user input, not permission to fetch clipboard separately. |
| Paths/receipts/native identities | Metadata can itself be sensitive. No filesystem probe/canonicalization or receipt ingestion. Synthetic example paths use explicit fake roots/hosts; no actual file is opened. |
| Settings | One proposed opt-in boolean, false default, through owned SaveIntent/acknowledged live application. Draft toggle cannot enable rows; no generic watcher is assumed. No secret/private plugin settings copied into metadata. |
| Evaluation | Only frozen synthetic files and local research outputs. No shell/network/provider/clipboard/OS invocation from prototype; output may quote synthetic queries but never actual user records. Hashes describe canonical content, not permission or native provenance. |

Opening a tool is not executing an inferred operation. Exact Action+args=None is captured and revalidated before existing parser/dispatcher/confirmation. Supplied words never become paths, left/right Diff inputs, search text/root, JSON, regex, clipboard content, shell text or query override. Existing tools may retain previous drafts/work; no universal empty-session guarantee is made.

## Threat/failure disposition

| Mechanism / adverse sequence | Required behavior / limit of evidence |
|---|---|
| Malicious/untrusted metadata labels | Plain display data, never instructions/code. Bounds validate IDs/alias/document/projection size; reject malformed records/ambiguous origin. No dynamic-plugin owner inherits builtin metadata by name. Trusted explanation tags render finite text. |
| Oversized/repeated/Unicode/control input | Hard raw/token bounds and conservative protected grammar before relevance. Repetition cannot amplify score. Non-ASCII is excluded without folding. This loses valid prose/locales; no claim it detects all malicious language. |
| Unsafe English missed by finite verb rules | A missed paraphrase still cannot fabricate args or auto-activate. Every row remains a fixed opener requiring explicit user selection and existing domain checks. Corpus can nevertheless count an unwanted suggestion as failure; do not redefine labels after seeing it. |
| Disabled/capability-denied/missing/wrong-origin provider | Remove descriptor before ranking. Relevant-but-denied is not “a low score”; no enable button or alternate owner selected by name. Current root activation does not already universally provide this stronger provenance guard. |
| Dynamic load/reload or incomplete global grammar | Route revision/coverage invalidates all rows. Unknown includes disabled registered owners and suppresses discovery. Complete synthetic contexts do not establish a production manifest or default live coverage. |
| Rerank/selection changes just before Enter | Store immutable full payload/current stamp and owner; invalidate on changes. Never follow index or replace a stale selection with current top1. Existing input/focus/fresh-Enter owners remain authoritative. |
| Capability/setting save in flight | Suppress until acknowledged live policy. Timeout is not proof of no effect or permission for blind rollback. Existing save/reconciliation owner remains responsible. |
| Provider late completion / retired worker | Existing owner rejects/refreshes stale data and separately retires work. Ranker is synchronous and has no new worker. Discarding rows does not free provider capacity or join native work. |
| Unsupported tool name | No invented Extract ZIP/Macro run-history/full-unmaterialized export/delete-duplicates/AI-image action. Do not count a planning/summary core as a user capability. |
| Hash/corpus drift | Abort evaluation before candidates run; verify frozen canonical hashes and identities. Repair through one recorded correction cycle/new manifest version, never overwrite evidence silently. |

The finite intent classifier is not a general destructive-language security boundary; fixed payloads, explicit selection, live eligibility and existing execution authority are the stronger boundaries. Native file proof/confirmations/automation capabilities are not encoded as score terms. AUD-F002 remains a separate Diff authority spike; an empty opener and a synthetic relevance gain do not resolve it.

## Compatibility ledger carried into future tests

- Keep exact `app`/`g`, Literal matching-head and upstream ActionFilters behavior; odd terminal escape is existing AUD-F001, not a Goal 02 parser repair. Snippet helper-local preservation does not undo manager parsing.
- Preserve non-g empty input-provider fallback, no retry after exact post-filter empties a nonempty vector, no fallback during capacity deferral, and separate failed-provider reopening. Protected fixture success only proves the Python gate, not current Rust equivalence.
- Preserve command/cache/completion inputs and canonical `self.results` order/cardinality. Production Macro immediately uses zero/one/many without pending wait; PresentedForSelection is handoff success, not completed effect. Exported compatibility Macro path still picks first row with Enter; bounded source absence of production submit callers is not proof no external use.
- Preserve `queryexec:` first canonical row, programmatic/saved origin, radial lease/revision/selected target, dashboard explicit filter grammar, Hot Folder unattended eligibility and source-specific confirmation. Discovery relevance grants none of these capabilities.
- Keep actual ActivationSource distinct from ActionSurface; proposed FreshUserEdit must not be inferred from main-window visibility. Existing Jump/modal/native-picker/root-child focus ownership and fresh-Enter fences remain in force.
- Four extra research descriptors do not expand raw SEARCH-F001 shipping scope. Full 15/initial 11 metrics keep identical cases/labels/denominators and retain missing targets. UI admission must be usable before unused infrastructure is recommended for production.

Future checks target these owners/invariants only. This research runs no Cargo/rustc/Nextest/Clippy/native tests/benchmarks. A pure prototype cannot establish real Windows focus, native file identity, scheduler/retirement timing, current Rust provider behavior or human relevance. Default-like completeness remains unresolved for 43 global contracts; UI stays NOT_IMPLEMENTATION_READY until a concrete acceptance decision/proof resolves that gap.
