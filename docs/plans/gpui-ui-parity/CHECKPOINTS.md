# Checkpoints and continuation

## P00 — Planning package prepared

Planning reference: master f7bc610a86bcad1c14b69dce65b1d804df1b7934, November 21, 2026. User requirements translated into a master Goal, orchestration Kickstart, 27 initial goal packets and execution record system.

No migration repository branch or source modification is claimed. No application tests, builds, runtime observations or GUI acceptance occurred in preparation.

**Next action:** G00-A: inspect current checkout/instructions, fetch origin/master, reconcile source changes and existing user work, create the dedicated branch safely, materialize the package, record execution baseline, commit the initial coherent ledger and attempt one normal push.

## Subsequent checkpoints

Use the continuation template. Include branch/HEAD, working-tree state, owner, exact current checkpoint, implemented vs accepted GUI IDs, partial changes, commands/evidence and limits, revalidation requirements, blockers/dependencies and exact next action. Keep historical entries factual and concise; do not overwrite the only resume state with a vague “continue migration.”

## G00-A — Execution baseline

The parent owns Git and this ledger. The branch is `feat/gpui-ui-parity` and the parent is `98bbc0ccc22a297e8eafae7e844c2f7e45854856`. The baseline commit introduces this entry. The checkout was clean before the milestone and this checkpoint touches only the execution records. No application source was modified and no GUIs have been implemented or accepted.

`git fetch origin master` succeeded with Git write access. `FETCH_HEAD`, `origin/master` and the starting `HEAD` agreed. Source and environment observations are in [SOURCE_BASELINE](SOURCE_BASELINE.md). G00-B discovery and the G01 real-binary feasibility handoff are next. Runtime baseline, build and test evidence remain pending.

The baseline commit is `3e67f7396f127d6dcb7bec432e9bb1684e9e6ae3`. The single `git push` attempt succeeded and local `HEAD` and `origin/feat/gpui-ui-parity` agreed immediately after it.

## G00-B/D — Discovery and review increment

Source discovery increment `e449f6f785ebbb1b466cda361d984214951fb599` was committed and pushed. The [review](reviews/G00-source-discovery.md) and [crosswalk](GUI_ROUTE_CROSSWALK.md) reconcile 77 Panel variants, 60 Jump Mode kinds and 110 persisted geometry keys. These registrations overlap. Coverage remains provisional for feature-specific controls, dynamic plug-ins and runtime confirmation.

G00-C still needs native query/navigation, an independent utility, and a persisted edit under an identified process. The first scope-limited smoke and the G01 same-binary observations are in [Validation log](VALIDATION_LOG.md). G01-C native probes and G02-B Hash Generator session extraction have bounded plans.

## G01-A — Same-binary foundation linked and reviewed

The G01-A checkpoint adds official Kit 0.10.0 and its aligned GPUI 0.3.12 family, early startup selection and a native foundation window. The existing brightness helper remains first. The canonical instance guard and startup recovery remain before frontend initialization. Option-free and explicit-egui startup took the established egui route. Explicit GPUI entered Kit on the main thread. The debug executable linked and all three focused selector tests passed. See [evidence](VALIDATION_LOG.md) and [review](reviews/G01-A-foundation.md).

GPUI foundation is a feasibility surface. No production GUI parity was implemented or accepted. G01-B needs a retained editable field with actual Windows input. G01-C and G01-D still need native lifecycle and release evidence. G03 owns actionable data-lock conflict and controlled restart. Continue from the G01-A commit on the same branch with the G01-B handoff.
