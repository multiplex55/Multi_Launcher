# CS editor privacy execution ledger

Binding plan: [Plan A](multi_launcher_cs_editor_privacy_codex_plan.md).
Baseline: clean `snippet-update` branch, `70de1523`.
Current source matches the plan's relevant archive-described inventory.

| Stage | State | Checkpoint |
|---|---|---|
| M1-A | complete; `97eaf1a6` | Persist optional per-snippet content masking |
| M1-B | complete; `6a128a87` | Preserve masking through updates; shared previews |
| M2-A | complete; `8ca1fc17` | Responsive rows, filtering, sizing |
| M2-B | complete; `255dcc79` | Inline confirmation, exact GUI alias validation, stale-save guard |
| M3-A | complete; `b5e58094` | Privacy checkbox and concealed presentation |
| M3-B | complete; `4291c7aa`; review P1 resolved | Deliberate reveal and lifecycle; primary targeted gate |
| M4-A | complete; `3659f842`; four focused tests passed; final review clear | Dashboard and confirmed preview consumers |
| M4-B | complete; documentation and final scoped checks | Documentation, final focused verification and review |

Each stage uses the approved stage-tagged commit subject. Source writes are
sequential through the configured implementer. Reviews occur after M3-B and
after M4 integration. Expensive verification is batched at those gates.

## Verification

Primary gate: `cargo nextest run --test snippets_plugin` passed 11 tests.
The final `cargo nextest run --lib --no-fail-fast -E 'test(snippet)'` passed
38/38. The earlier fail-fast run had 14 passes, one test-fixture failure and
23 not run; after correcting the fixture, the exact undo regression and
complete filter passed. A test-only egui namespace compile error was also
corrected during that gate.

M4-A passed 4/4 with:

```text
cargo nextest run --lib -E 'test(hidden_snippet_dashboard_preview_and_tooltip) | test(visible_snippet_keeps_full_hover_text_and_normalizes_preview) | test(snippet_preview_limit_counts_unicode_characters) | test(generic_result_tooltips_hide_snippet_bodies_and_keep_clipboard_actions)'
```

Per-checkpoint touched-file formatting and whitespace checks passed.
Final `cargo fmt --all -- --check` passed. Relevant targets compiled through
Nextest; no redundant cargo check or full test suite was run. Three existing
render test warnings appeared during the M4 gate.
Native Windows app control is unavailable in this session; report manual GUI
acceptance separately from automated state/layout evidence.

## Manual acceptance still to perform

Use disposable data, without replacing irreplaceable live snippets:

- Resize with very long aliases and multiline bodies; check Edit/Remove and
  Save/Cancel stay reachable, and Clear Filter/count agree with rows.
- Check hidden editor, Dashboard and launcher row hovers; copy must retain
  the original multiline Unicode body.
- Open hidden entries through row Edit and `cs edit`; reveal, save/cancel,
  close/title-bar X, reopen and switch entries; concealment must reset.
- Check inline removal Confirm/Cancel, duplicate create/rename rejection,
  and malformed-file read-only rejection without changing the file.

## Architectural notes

- Keep privacy on `SnippetEntry`; execution and search use exact plaintext.
- Guard GUI snapshot replacement inside the existing `update_snippets`
  transaction; reject stale snapshots rather than overwriting newer records.
- Generic panel dismissal/reopen must participate in reveal reset, including
  close/reopen between UI frames.
- Primary review found cross-session egui body undo history could restore a
  previously revealed body in another draft. Distinct per-edit-session widget
  identities resolve this; a headless widget undo regression covers it.
- Launcher snippet tooltips are alias-only for all snippet results to avoid
  stale privacy metadata lookup; action payloads remain untouched. Other
  audited catalog/action-sheet/radial/history snippet labels were already
  alias-only and needed no changes.
- Final independent source review found no substantive findings. Native
  geometry remains unverified. Masking is display-only; saved bodies,
  action payloads, clipboard and history remain plaintext. Plan B is deferred.
