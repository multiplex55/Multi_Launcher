# G05 B1 R1 — Preserve terminal failure classification

Status RESOLVED_IN_G05 at944f9d0ef4ed5ceee54c9a3d255c40e9a220494d; independently confirmed. Historical contingency, do not execute or schedule. Discovery independent review at bab309689e5acf0b1924fb20842fe06c53b37fec. High-confidence direct control-flow counterexample; no native defect/unsafe execution alleged. Companion [KICKSTART](KICKSTART.md).

## Objective, evidence and scope

Correct LiteralScan::feed/finish in src/hotfolder/conditions.rs so supplied bytes rejected before parser processing do not falsely establish truncation. Preserve Invalid > Changed > Unavailable > NoMatch > Match.

Admitted65537-byte input supplied in one valid chunk exceeds65536-byte chunk bound. feed returns ContentChunk before counting; genuineEOF final length65537 falsely Changed from zero observed counter. Entry cancel/deadline before last chunk similarly affected. Existing test uses ReadFailed, not genuine EOF.

Own scanner accounting/finalization, directly relevant inline tests and own G05 receipts only. No reader/native API/model redesign/gate removal/B2. Keep bounded arithmetic/state/limits.

## Required behavior and proof

Distinguish received-stream length from successful processing. Genuine EOF after resource rejection or entry cancel/deadline retains Unavailable when received bytes/final length consistent. Actual overrun, premature EOF, changed final length or source binding remain Changed even with competing Unavailable. No failure becomes completed evidence/Match.

Add deterministic cases for both modes. One focused correction selection covering scanner/composition under current approved budget. Preserve first20/20 evidence and exact corrected source/inventory/timings/cleanup. No native or broad suite claim.

## Roles, checkpoints, acceptance and stop

Parent authorizes one writer and owns Git/integration. Implementer requested Astra/xhigh, reviewer Astra/max; configured pins/backendUNVERIFIED separately. One coherent correction checkpoint, normal push attempt, R1-only confirmation. Done requires correct classifications, focused pass and named review. If correction fails within bounded pass, preserve fail-closed behavior and defer B1, no B2 expansion. Correction independently reversible, preserve user changes. Mark RESOLVED_IN_G05 when accepted; no separate future queue then. No automatic dispatch or budget renewal from this package.
Resolution2026-10-30: one scanner-owner correction, exact22/22; named reviewer confirmation accepts corrected cumulative B1. Original finding preserved above; no unresolved future work from this pair.
