# AUD-F001 M1-A implementation analysis

Current source/base `59156d96adcc73939c7171487a0cb582e3286587`; branch `codex/query-terminal-escape`; M0-A accepted/pushed `42fe7d2a50c59f67e4ae628de845142c1cab9e54`. The current user authorized sequential catalog execution; this child executes only the selected M1-A. Parent owns Git/Goal/queue/review records. Other work is preserved.

## Change and ownership

The source scan already records each backslash in RawToken.raw; pending EOF must complete only decoded value. Removed the second raw append at that exact owner and added a short invariant comment. No consumer repair, signature/type/grammar/filter-policy/prefix/persistence/default/Action serialization/ranking/authority change.

Five new tests contain `terminal_escape`:

1. `common::query::tests::tokenize_raw_terminal_escape_preserves_spelling_and_decoding`: explicit decoded expectations for one/two/three/four terminal backslashes (one/one/two/two decoded), Unicode, completed/unclosed quote controls, nonterminal escaped space/quote. Every emitted raw token equals the original token spelling.
2. `common::query::tests::split_action_filters_terminal_escape_preserves_order_and_metadata`: one-through-four retained terminal runs with positive/negative mixed-case kind/id filters, exact normalized metadata, retained order/quotes and existing single-space reconstruction.
3. `plugin::tests::action_filters_terminal_escape_matches_synchronous_and_snapshot_input`: synthetic echo Action satisfies selected filters, exactly one nonempty result and identical sync/snapshot Action/received args for all four runs. Manager uses the `_at` inert constructor with a unique owned TempDir; no global macro store/window/native initialization.
4. `plugin::tests::matching_prefix_literal_terminal_escape_preserves_original_input`: explicit case-insensitive matching prefix plus Literal policy preserves original leading/inter-token whitespace, filter-looking payload, Unicode/quotes and terminal escape. Its id/kind-looking text would hide the observation if ordinary filtering were mistakenly applied.
5. `plugins::snippets::persistence_tests::snippet_tag_terminal_escape_preserves_reconstruction_and_no_tag_input`: pure helper, positive/negative normalized tags and retained terminal spelling; no-tag exact-whitespace control is helper-local, not an upstream manager exemption. No persistence fixture or global mutex.

Manager tests drop the manager before explicit TempDir.close and require cleanup success. No LauncherApp/app fixture is needed, so no Paste Sequences settings or production defaults are touched. Existing tests remain intact. Production code change is only EOF bookkeeping; other two source files change inside test modules only.

## Tooling and verification boundary

Parent confirmed installed Cargo/Nextest/Clippy/rustfmt support. Child inspected effective settings: no project/ancestor/user Cargo config found, no CARGO_BUILD_TARGET/CARGO_TARGET_DIR/RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS override, rustc 1.99.1 host x86_64-pc-windows-msvc. Therefore the selected default target is native host, edition 2024/default features, no extra target/features/profile/environment override. No tool installation or manifest/lockfile change.

The first touched-file rustfmt check parsed all three files without output; no source formatting change was needed. Exact standalone exit/status is recorded with final verification. Scoped cumulative source diff is 149 insertions/one deletion including meaningful tests and invariant comment; no unrelated source churn. Static callers confirm existing GUI/dashboard paths inherit shared correction without policy edits.

Nextest list starts the selected-target build only once. It uses locked/offline, with an owned cargo process/descendant tree, automatic 20-minute deadline and safe tree termination only if needed. Separate built selected run 120s and scoped Clippy 600s deadlines follow; no ceremonial cargo check/build. Logs/JSON process receipts are task-owned ignored outputs under target/aud-f001-verification, with actual commands/exit/timing and diagnostics summarized in VERIFICATION. Required selected inventory must be nonzero and include new provider/snippet tests before a pass is claimed.

Required scoped verification is complete: initial list/build exit 0 in 434.594s/1200s, nonzero 11-case inventory (eight parser/two inert provider/one pure snippet; all five regressions). Already-built selected run exit 0 in 2.266s/120s, 11/11 passed and 6440 skipped, actual tests 0.102s; unchanged build artifacts reused with no Compiling lines. Both explicit owned TempDir.close assertions pass within provider test durations 0.051s/0.044s, establishing cleanup below 30s with no external cleanup needed. Clippy --lib --no-deps exit 0 in 90.266s/600s, warnings only in unchanged code; no independent baseline warning comparison, no suppression or unrelated fix. Touched-file formatting standalone exit 0. Exact commands/timestamps/PIDs/hashes/inventory and bounded local receipts are in VERIFICATION.

Source-prefix comparison confirms the production delta is only EOF bookkeeping; plugin/snippet changes are test-only. Scoped whitespace check succeeds; manifest/lock/config unchanged. No routine correction/retest, timeout, extra build/check, full suite or native/app fixture was needed. Source was stable before initial compilation and remains frozen. No known implementation issue remains from this author's scoped checks; independent review is pending, and parent owns acceptance, any required bounded correction, commit and one normal push. Only AUD-F001 M1-A was executed; no next package/Git operation/agent started.
