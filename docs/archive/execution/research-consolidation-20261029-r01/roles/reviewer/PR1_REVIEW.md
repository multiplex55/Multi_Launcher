# M1-A — PR 1 integration review

Authored by `/root/integration_review` on 2026-10-29; persisted by `/root`.

**ACCEPTED — no substantive findings or required corrections.**

Reviewed branch `codex/integrate-query-escape-20261029-r01`, M0-A HEAD `762ce52d5aabe52fc6006dff30430a63d97cccc6`, staged imports and accompanying integration receipts. Baseline: `59156d96adcc73939c7171487a0cb582e3286587`. Source: `2db6c8a8e3c7b9c6de8c1f2c1dccd3ca0c2a77c5`.

- Independently compared Git objects: all three indexed Rust files and all nine original evidence documents exactly match SOURCE_QUERY. Imported paths have no unstaged differences. Local master, origin/master and the source merge-base equal the recorded baseline. The original source has no other production changes.
- The sole production correction belongs in `tokenize_query_with_raw`: scanning already records the source backslash, so pending-escape EOF must append only the deferred decoded value. Quote handling, token emission, filter grammar and existing normalization remain unchanged.
- The five added tests provide meaningful protection: explicit raw/decoded expectations for odd/even terminal runs; complete filter metadata and retained-token order; nonempty synchronous/snapshot provider results with independently expected payloads; matching-prefix Literal preservation; and pure snippet tag reconstruction with a helper-local no-tag control. Existing tests remain intact.
- Provider fixtures use the existing explicitly rooted inert manager, empty macro store without a watcher, and inert window catalog. They register only the synthetic provider and require temporary-directory cleanup. No application fixture, shortcut registration or production policy change is introduced.
- Cumulative tracked scope contains only the three permitted Rust paths and authorized documentation. The obsolete query-branch queue ledger is excluded. Configuration, manifests, native behavior and future features are untouched. The immutable integration Goal prefix and complete Kickstart match the invocation source.

The original verification and independent review record **11/11 selected Nextest PASS**, scoped library Clippy PASS and touched-file formatting PASS. Exact source/baseline parity supports their reuse as historical evidence. The new scope receipt and PR body clearly distinguish those results from integration checks; this reviewer did not inspect original raw logs or rerun the commands.

Reviewer checks passed: staged `git diff --check`, source/evidence blob comparisons, scope inspection, and cumulative `git -c core.whitespace=-blank-at-eol diff --check <baseline>`. The latter respects the disclosed preserved Markdown hard breaks; it is not a claim that an ordinary cumulative whitespace check passed.

No Cargo, rustc, Nextest, Clippy, rustfmt, application, native or prototype execution occurred. Commit, publication and actual GitHub merge gates remain parent-owned subsequent steps. This acceptance covers M1-A only.
