# Kickstart — File Search domain fixture sender migration

Read [GOAL.md](GOAL.md) fully and current `AGENTS.md` before acting. This is a proposed future task; execute only when the parent/user activates it. Do not resume the entire closeout program from this pair.

1. Parent identifies the current source, safe branch and user edits. Revalidate the `filenames` helper in `tests/domain_cases/file_search_e2e.rs` against `SearchEventSender` in `src/file_search/coordinator.rs` and `search_filenames_in_directory` in `src/file_search/walkdir.rs`.
2. Confirm the recorded E0308 still describes the actual mismatch. Do not reset to the evidence SHA or import a historical patch. If fixed, record the precise no-change disposition.
3. Assign one writer M1-A: adapt only the fixture sender using the existing conversion, retaining the receiver/result logic and all assertions. No production overloads, dependencies, manifests or new infrastructure.
4. Run the exact four-case Nextest inventory from GOAL.md using the domain target, offline cache and fast-dev profile; require four selected cases. Respect the 20-minute preparation/180-second execution limits and one bounded migration correction. Record commands, source identities, timings, exits and actual count. Stop on an independent compile failure rather than expanding the task.
5. Inspect the diff and obtain one narrow review. Parent commits the coherent test migration, attempts one normal push and records acceptance. No automatic merge or unrelated test expansion.

Return changed paths, current source/branch, exact test evidence, model provenance and any remaining target blocker. No native File Search, worker-retirement or feature-wide acceptance may be inferred from this fixture correction.
