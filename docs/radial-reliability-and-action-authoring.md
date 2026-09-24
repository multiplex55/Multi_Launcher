# Radial reliability and action authoring — execution ledger

This is a mutable execution ledger. All initial statuses are pending. Creation of this file is not application implementation or validation.

## Authority

Starting archive: `multi_launcher(20260924-210943).zip`  
Archive SHA-256: `f21ca0ee92d1b7572f779b936bb3ce1de7e685314f685ae8711d1734952d2544`

Approved behavior: `../multi_launcher_radial_reliability_approved_requirements.md`  
Implementation tasks: `../multi_launcher_radial_reliability_codex_plan.md`  
Acceptance: `../multi_launcher_radial_reliability_acceptance_matrix.md`  
Source evidence: `../multi_launcher_radial_reliability_source_notes.md`

Critical override of older plans: a short launcher tap toggles the grid AND dismisses the runtime radial. Designer and its draft remain independent.

## Baseline identity — fill from actual checkout/tools

| Field | Actual value |
|---|---|
| Branch / starting HEAD | Not recorded |
| Dirty diff / source manifest identity | Not recorded |
| Archive comparison / intervening changes | Not recorded |
| Historical branch point / first feature commit (optional, verified only) | Not established; no Git history in archive |
| Toolchain / Cargo / Nextest versions | Not recorded |
| Target / profile / target directory | Not recorded |
| Candidate application path / SHA-256 | Not built/recorded |
| Acceptance runner path / SHA-256 | Not built/recorded |
| Build command and source-to-artifact record | Not recorded |
| Native environment / monitor-DPI / integrity | Not recorded |
| Fixture / profile hashes | Not recorded |
| Authorized consistent copied profile available | Not established |

## Milestones

| ID | Scope | Code status | Verification status | Commit/diff | Evidence / blocker |
|---|---|---|---|---|---|
| M0 | Source baseline and fixture/test map | pending | pending | — | — |
| M1 | Hotkey/grid/runtime reliability and exact-chord runner | pending | pending | — | — |
| M2 | Typed bindings and persistence migration | pending | pending | — | — |
| M3 | Shared query resolution and execution/handoff | pending | pending | — | — |
| M4 | Shared authoring editor and Add to radial | pending | pending | — | — |
| M5 | Multi-select/bulk editing/navigation | pending | pending | — | — |
| M6 | Presets/gallery/simple controls/density | pending | pending | — | — |
| M7 | Full regression/native acceptance/review | pending | pending | — | — |

Use `pending`, `in_progress`, `code_complete`, `complete`, or `blocked` explicitly. A milestone is `complete` only when its required acceptance passed. Record a known failure separately from unavailable environment evidence.

## Gates

| Gate | Meaning | Status | Exact source/candidate identity | Report/exit/counts |
|---|---|---|---|---|
| H | Rapid exact-chord native hotkey reliability | pending | — | — |
| P | Model/migration/store/package compatibility | pending | — | — |
| Q | Same-ranked query resolution and correct execution | pending | — | — |
| C | Core editor and insertion, H/Q rerun | pending | — | — |
| D | Bulk/nav usability | pending | — | — |
| S | Skin/geometry/appearance compatibility | pending | — | — |
| R | Final full suite/native/review | pending | — | — |

## Long-running jobs

| Job ID/PID | Command + working directory | Source identity | Started | Log/metadata path | Last observed state | Exit code |
|---|---|---|---|---|---|---|
| None launched | — | — | — | — | — | — |

One expensive job at a time. Check actual process state before starting another. Use completion notification or the user's 10–20 minute observation cadence for long jobs. Keep native test timing independent.

## Milestone record template

### Mx — title

Objective/requirement IDs:  
Starting and ending source/commit/diff:  
Changed files/architectural owners:  
Important decisions and intentional behavior changes:  
Tasks completed:  
Tests added/migrated and why:  
Commands, discovered counts, pass/fail/skip counts, exit codes:  
Candidate/runner/profile identities:  
Native cases and report path:  
Source-backed root-cause evidence versus remaining hypotheses:  
Performance measurements, if any:  
Unresolved blockers/limitations:  
Next bounded milestone:

## Review findings

| Finding | Severity | Source location | Required remediation | Status | Retest evidence |
|---|---|---|---|---|---|
| Review not run | — | — | — | pending | — |

## Final report checklist

Approved behavior implemented; intentional old-test changes explained; final Nextest results; applicable doctests; final candidate and runner hashes/source manifest; mandatory exact-chord/native cases; query/UI/no-flash/confirmation checks; Designer/persistence/skin checks; cleanup; copied-profile result or honest absence; independent review/remediation; remaining environment limitations; no unsupported “no regressions” claim.
