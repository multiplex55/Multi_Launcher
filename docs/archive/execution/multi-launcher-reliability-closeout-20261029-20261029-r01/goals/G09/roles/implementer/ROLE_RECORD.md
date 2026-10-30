# G09 Implementer role record

Date: 2026-10-30. Role: Implementer, M1 proof checkpoint only. Requested `gpt-6-astra / xhigh`; configured `.codex/agents/implementer.toml` `gpt-6-luna / max`, unchanged; actual serving backend **UNVERIFIED**. Parent authorized one capacity retry after the reported `Selected model is at capacity. Please try a different model.`; the retry added no implementation or verification allowance.

## Work and ownership

Read the complete active Goal/Kickstart and approved [owner proof contract](../planner/OWNER_PROOF_CONTRACT.md), [M1 handoff](../planner/M1_HANDOFF.md), role record and actual owners/tests. Strengthened existing tests in `src/thread_reaper.rs`, `src/duplicate_finder/tests.rs`, `src/gui/screenshot_capture.rs`. No production fix, new API, dependency, feature, global manager, app fixture or native UI work. Parent owns all Git and acceptance; no parent/planner/reviewer/global record edited.

Reaper receipt assertions separate notifier drop from actual join and callback, prove callback outside its lock, and preserve the spawn-failure detached/no-receipt distinction. Duplicate Finder tests preserve cancel-only draining and add cancel/close/repeated-refusal/current-generation proof through real reaper receipts. Screenshot tests keep Draining admission blocked, discard the late injected image, and distinguish ordinary tick confirmation from shutdown's informational `Unavailable` row and separate reaped probe.

The single native case uses eight synthetic 128 KiB files under a unique `target/productivity_expansion/activity_retirement/g09-native-*` directory. Baseline and post-join probes use actual Windows `share_mode(0)` read opens; a held probe must specifically return OS 32. Gate, cancel/close, join/reopen and warm result each have separate assertions. No in-flight ReadFile, capture/apartment or latency guarantee is inferred.

## Evidence and disposition

See [verification](../../VERIFICATION.md), [source identities](../../SOURCE_REVALIDATION.md) and [risks](../../RISKS.md). Verification passed 12/12 on unchanged recorded inputs: preparation 211.037s/1200s, run 3.648s/180s (Nextest 0.732s), run exit0. No incremental preparation or rerun used. Initial preparation numeric exit unavailable/null is retained explicitly; complete inventory and exact-source passing run corroborate preparation. Final recorded process scan and fixture root are empty. Parent acceptance and independent review remain pending. No distinct new finding: **NO_ACTION**. Do not create a duplicate initiative for unclaimed native categories.
