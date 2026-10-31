# Accepted source manifest

**New accepted Rust/test source files: 0.** All production source, Cargo manifests/lockfile and `.codex` remain exactly at run-start master `f28355df5e210cfc1633284b0b80c7f156090245`. The cumulative PR contains execution documentation and preserved evidence only. No feature gate was enabled.

Four candidates were preserved and excluded: G02 (5 owner/path entries), G05 (1), History (5), ZIP R1 (3): **14 entries across 12 physical paths**. Repeated `src/file_operations.rs` entries represent distinct candidates. Canonical blobs below identify unaccepted content, not carried Git source. Raw working bytes and SHA-256 values are in the linked preservation receipts/archives; CRLF raw hashes can differ from canonical Git blobs.

| Candidate | Source path | Excluded canonical blob | Current baseline blob |
|---|---|---|---|
|G02|src/file_operations.rs|4742aa14a51e1c27bdfbbdaa815e267ceb7e99b6|b7d67bf4d49f81a8c4492991a95892a8dda4835b|
|G02|src/table_utility_session.rs|83ecd7be538cbbe29985007de76ba0523599fc06|b7f5bb8b525d0786067fb30d6d25199c6fb5ce4d|
|G02|src/table_utility_session/publication.rs|3b87dc6ab0a7e1277a6307e4fe141dea1b4fdf0c|absent|
|G02|crates/multi_launcher_table_core/src/lib.rs|0e4dccc43056ac0a4e29b4329dac5626cf784d5b|0cba07df6636a0922ac7ad815c80918f54ce10d5|
|G02|crates/multi_launcher_table_core/src/resource.rs|b10c0b880b129a7902369c5af4490e21c1342a6d|74def7e006cffddff576bab08563bd284c52374b|
|G05|src/file_operations.rs|1b78081c87f2ead7c0930b63f21a334e82b93835|b7d67bf4d49f81a8c4492991a95892a8dda4835b|
|G08 History|src/mkmacro/runtime/summary.rs|871f7ba002a199660c55e693cf7799c9473632cd|269d50f56344620fd59b0d4976077a821e725e2a|
|G08 History|src/mkmacro/runtime.rs|6cd60c4ec58fbbdff72dff69c7b10c221ec106bd|3a454dfc77b4a5572bf0250ac6a4d1d661290ccd|
|G08 History|src/gui/mkmacro_dialog/history.rs|1dc0ab489a0e6bd3386c2f35914e405278ef879c|absent|
|G08 History|src/gui/mkmacro_dialog/mod.rs|715b089db6a88b65ad4ea2600c4bf1d21ae9b3cb|a9f2335418f69f72dc407a686f6ca4f8956a4960|
|G08 History|src/gui/mkmacro_dialog/toolbar.rs|03093248e4bb30f7b8de95b43887d7c17bfe3f67|4cf3875d7014fd443916987b34f97978035907f3|
|G07|src/file_operations.rs|0f362f2994c8decbf2f0c88b36ac6e9360f3b386|b7d67bf4d49f81a8c4492991a95892a8dda4835b|
|G07|src/archive/preparation.rs|8c657ec60557ac37252ff457184f7bb37d0c5d0f|absent|
|G07|src/archive/mod.rs|ba84fd5ca01d55bee053a1c927cc0165612efb6c|267a87a87552d63c5f8c67d76448a43c01eeccee|

[Machine index](stages/B09-final/EXCLUDED_SOURCE_INDEX.json). Every source isolation had an empty index, exact current/task/base identity checks, byte-preserving archives, reversible patch check and baseline restoration. No user edits were discarded.

- G02: [preservation](stages/G02/PRESERVATION_RECEIPT.json), [patch](stages/G02/UNACCEPTED_SOURCE.patch.gz), [new module copy](stages/G02/unaccepted/publication.rs.txt), [rollback](stages/G02/ROLLBACK_AND_CLEANUP.json).
- G05: [preservation](stages/G05/PRESERVATION_AND_ROLLBACK_PRECHECK.json), [raw tested source](stages/G05/roles/implementer/verification/n0-r1/candidate-file_operations.rs.gz), [rollback](stages/G05/ROLLBACK_AND_CLEANUP.json). Raw blob17373039280b9f597853f7c59c9d6ab74edc9ab2 differs from canonical1b78081c because of line endings; both were verified.
- History: [preservation](stages/G08-history/PRESERVATION_AND_ROLLBACK_PRECHECK.json), [all five raw source archives](stages/G08-history/unaccepted), [patch](stages/G08-history/UNACCEPTED_SOURCE.patch.gz), [rollback](stages/G08-history/ROLLBACK_AND_CLEANUP.json). Static correction confirmation did not establish test acceptance.
- ZIP R1: [preservation](stages/G07/PRESERVATION_AND_ROLLBACK_PRECHECK.json), [all three raw source archives](stages/G07/unaccepted), [patch](stages/G07/UNACCEPTED_SOURCE.patch.gz), [rollback](stages/G07/ROLLBACK_AND_CLEANUP.json). No current native test ran.

Historical donor prototypes remain evidence-only and were never merged/cherry-picked wholesale. Current code has no new Table publisher, History projection/UI or ZIP preparation/writer. Hot Folder condition refusals remain intact. The existing accepted editor, templates, invocation, screenshot composition and read-only ZIP catalog/planner retain baseline behavior; this run adds no new runtime acceptance for them.
