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

No historical donor branch was modified/imported wholesale. G07/G08 historical prototypes remain excluded as recorded in the archived closeout manifest; current program has not attempted those source stages yet.
