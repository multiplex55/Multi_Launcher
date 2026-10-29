# AUD-002 — Diff folder mutation identity is metadata/path based

Status after M2-B: **NEEDS_RESEARCH_SPIKE — AUD-F002 accepted for proposed contract/proof decision only**. No production repair is accepted or authorized. Source baseline `59156d96adcc73939c7171487a0cb582e3286587`. Evidence type: static owner/caller/counterguard inspection; no destructive experiment or native reproduction. [Decision D02](../DECISION_RECORDS.md#d02--decide-diff-concurrent-writer-authority-before-choosing-a-repair).

## Mechanism

`diff/file_ops.rs::FileIdentity` stores kind, length and modification time; device/inode exist only on Unix. `same_root_identity` on non-Unix checks canonical path. `fresh` checks confirmed source/destination metadata, containment, path meaning and dirty destination, then `execute_copy` calls `fs::copy` on paths. `execute_delete` similarly checks then calls a path-based trash backend. Therefore the inspected Windows owner does not distinguish a replacement retaining those metadata values, and does not retain a source/destination native handle across validation and mutation. [F1], [F2], [F3]

The potential adverse sequence is an external writer replacing source/destination after confirmation while preserving compared metadata, or between the freshness check and path mutation. The plan may still be considered fresh while a different file object is used. This is a source-level authority limitation; native feasibility, occurrence and resulting user harm have not been demonstrated here. It is not evidence that ordinary confirmed copy/recycle is currently wrong.

## Counterguards and scope

The GUI executes the captured preview plan, captures dirty editor paths at execution, disables overlapping mutation via runtime state, plans GUI deletion as Recycle and calls `spawn_recycle_delete`, which rejects Permanent mode. Executors check containment/root/path meaning, expected metadata and dirty descendants; cancellation stops between items. Trash failure does not fall back to permanent deletion. These checks reject many ordinary stale/conflicting plans. [F2]–[F5]

Batch Rename/Replace have stronger native evidence, held handles and journal protocols in `file_operations.rs`; those guarantees are not implicitly shared by Diff's separate implementation. Their existence suggests a potential reuse boundary, not permission to replace every file operation or to add a universal filesystem framework. [Safety report](../PERSISTENCE_AND_FILE_SAFETY.md#diff-folder-mutations-have-a-different-guarantee).

## Confidence, severity and disposition

- Source distinction: **high confidence** for inspected Windows cfg and named call paths.
- Impact: potential wrong-object copy/overwrite/recycle under concurrent replacement; native incidence and exact failure sequence **unverified**. Severity is **conditional medium pending required concurrent-writer contract and proof**, not a reproduced data-loss assertion.
- Current disposition: M2-B accepts **AUD-F002** as a bounded decision/proof spike, not implementation-ready repair. Do not prioritize from historical incidents, source line count or lack of a runtime test alone.

The canonical [Goal](../follow-on/AUD-F002-diff-mutation-authority/GOAL.md)/[Kickstart](../follow-on/AUD-F002-diff-mutation-authority/KICKSTART.md) compares best-effort semantics, local-owner repair and narrowly reusable native primitives. It identifies source/destination/ancestor authority, overwrite consent, dirty/recycle/cancel/partial invariants, current tests, bounded abstract witness, later native proof obligations and migration if warranted. The selected spike may conclude no change. No framework, native campaign or repair is executed; [M3-A index](../FOLLOW_ON_INDEX.md) records canonical promotion and preserved raw author pair.

Minimum future proof: a pure injected filesystem boundary can deterministically change an identity between validation and mutation; a small isolated native fixture is needed only for the precise Windows handle/publication behavior being relied on. Verify ordinary copy/overwrite, same-path spelling, dirty-descendant refusal, stale source/destination and recycle-only behavior. Avoid a broad native campaign. Rejection condition: if the product contract explicitly accepts concurrent external replacement and evidence shows no unmet guarantee, document the limitation and take no action.

## Pinned sources

[F1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/diff/file_ops.rs#L30-L64
[F2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/diff/file_ops.rs#L507-L631
[F3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/diff/file_ops.rs#L804-L898
[F4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/diff_dialog/mod.rs#L1052-L1082
[F5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/gui/diff_dialog/mod.rs#L1146-L1178

Existing source tests are [ordinary copy/planning and recycle refusal](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/tests/domain_cases/diff_file_ops.rs#L7-L179). Their presence is not an executed result or proof of the concurrent native replacement case.
