# Performance and responsiveness

Status: metric definitions and comparison policy; no migration measurements exist yet.

## Baseline and candidate comparison

Measure equivalent synthetic/representative data and settings on the same Windows machine, display configuration, build profile and toolchain where possible. Identify cold vs warm startup/cache conditions and repeat enough to distinguish a material regression from ordinary noise. Reuse existing performance tools when they measure the relevant question.

| Metric | What to measure | Relevant goals |
| --- | --- | --- |
| Startup usability | Process start to first usable selected frontend | G01, G03, G05, G26 |
| Invocation | Actual hotkey to visible, focused, editable launcher | G01, G05, G26 |
| Typing/search | Query changes to current results; stale results never replace current | G05, G07 |
| Large content | List/table/editor scrolling and edits with representative data | G07–G17, G23 |
| Native transitions | Overlay/radial open/close and capture restoration | G18–G22 |
| Idle work | CPU, wakeups and unnecessary repainting while idle/hidden | G02–G05, G23, G26 |
| Repeated lifecycle | Windows/hooks/subscriptions/tasks/resources after open/close cycles | Shared host and all specialized lifecycle goals |
| Memory | Material steady-state/peak differences with equivalent workload | G26 and changed heavy tools |
| Distribution/iteration | Executable size, dependency/build/link cost | G01, G26 |

Set practical budgets from the measured baseline and the feature’s role as a fast launcher. Record the rationale; do not invent a universal frame-time promise or assume GPU use implies a speedup. No fixed percentage is prescribed before measurements.

## Acceptance

Resolve observed regressions at their owning layer. An explained, explicitly accepted tradeoff remains visible. Loading both frontend libraries is different from starting both graphics runtimes; only the selected frontend may run.

Do not solve missing change notifications with a permanent busy repaint, duplicate polling workers or arbitrary sleep-based focus hacks. Reuse targeted wakeups and existing service contracts.

G26 consolidates measurements and checks evidence applicability. It does not repeat every benchmark for harmless documentation or style changes.
