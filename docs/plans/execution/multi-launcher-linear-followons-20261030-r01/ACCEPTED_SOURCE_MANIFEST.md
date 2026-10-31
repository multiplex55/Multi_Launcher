# Accepted source manifest

Run-start accepted baseline:f28355df5e210cfc1633284b0b80c7f156090245. New accepted production/test files:0. git diff --exit-code80455ffa --src crates Cargo.toml Cargo.lock .codex passed after G02 rollback; M0 differs from run-start only in documentation.

## G02 candidate excluded from the advancing production tree

All5paths remain UNACCEPTED. Exact working bytes tested are in [run receipt](stages/G02/roles/implementer/verification/m1a-current/corrected-run.receipt.json). [Exact compressed binary patch](stages/G02/UNACCEPTED_SOURCE.patch.gz) covers4tracked paths; [new module copy](stages/G02/unaccepted/publication.rs.txt) covers the created fifth. [Preservation](stages/G02/PRESERVATION_RECEIPT.json) and [rollback](stages/G02/ROLLBACK_AND_CLEANUP.json) record hashes and reconstructibility. Patch/text are evidence, not compiled production source.

| Path | Unaccepted tested Git blob | Current production state |
|---|---|---|
| src/file_operations.rs |4742aa14a51e1c27bdfbbdaa815e267ceb7e99b6|baseline b7d67bf4d49f81a8c4492991a95892a8dda4835b|
| src/table_utility_session.rs |83ecd7be538cbbe29985007de76ba0523599fc06|baseline b7f5bb8b525d0786067fb30d6d25199c6fb5ce4d|
| src/table_utility_session/publication.rs |3b87dc6ab0a7e1277a6307e4fe141dea1b4fdf0c|absent|
| crates/multi_launcher_table_core/src/lib.rs |0e4dccc43056ac0a4e29b4329dac5626cf784d5b|baseline 0cba07df6636a0922ac7ad815c80918f54ce10d5|
| crates/multi_launcher_table_core/src/resource.rs |b10c0b880b129a7902369c5af4490e21c1342a6d|baseline 74def7e006cffddff576bab08563bd284c52374b|

No historical donor branch was modified/imported wholesale. G07 historical prototype remains excluded as recorded in the archived closeout manifest; current program has not attempted G07 source yet. Current G08 attempt is recorded below and remains excluded.

## G05 N0 candidate excluded

One additional owner/path entry, src/file_operations.rs test-only candidate. Raw tested blob17373039280b9f597853f7c59c9d6ab74edc9ab2; canonical LF blob1b78081c87f2ead7c0930b63f21a334e82b93835; working SHA2568703eec28b47679fd04bbee28e892dc7984db80247ce73e19ef2305e0b12c29f. No source accepted. [Exact raw source and patch preservation](stages/G05/PRESERVATION_AND_ROLLBACK_PRECHECK.json) and [rollback](stages/G05/ROLLBACK_AND_CLEANUP.json) record gzip hashes and baseline restoration. Production blob restored to b7d67bf4d49f81a8c4492991a95892a8dda4835b. G05 four accepted foundation paths and every refusal remain unchanged. Current run exclusions so far:6owner/path entries across5physical paths; accepted new source files0.

## G08 History candidate excluded

Five additional owner/path entries, all UNACCEPTED after final preparation TIMEOUT and no15case run. [Preservation precheck](stages/G08-history/PRESERVATION_AND_ROLLBACK_PRECHECK.json) binds final raw SHA256/canonical blobs/baseline blobs to [five exact raw gzip copies](stages/G08-history/unaccepted) and [reversible patch](stages/G08-history/UNACCEPTED_SOURCE.patch.gz). [Rollback](stages/G08-history/ROLLBACK_AND_CLEANUP.json) proves baseline source parity and patch reapply. Static named R1 correction confirmation is not test/source acceptance.

| Path | Excluded canonical blob |
|---|---|
|src/mkmacro/runtime/summary.rs|871f7ba002a199660c55e693cf7799c9473632cd|
|src/mkmacro/runtime.rs|6cd60c4ec58fbbdff72dff69c7b10c221ec106bd|
|src/gui/mkmacro_dialog/history.rs|1dc0ab489a0e6bd3386c2f35914e405278ef879c|
|src/gui/mkmacro_dialog/mod.rs|715b089db6a88b65ad4ea2600c4bf1d21ae9b3cb|
|src/gui/mkmacro_dialog/toolbar.rs|03093248e4bb30f7b8de95b43887d7c17bfe3f67|

Run exclusions so far11owner/path entries over10physical paths; new accepted source0. No History/recipe UI or runtime source carried.
