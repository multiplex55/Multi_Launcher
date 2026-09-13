# Screen Draw implementation ledger

This ledger tracks the approved Screen Draw initiative. A milestone is only
`complete` after its acceptance criteria and targeted verification pass.

| ID | State | Milestone | Verification gate |
|---|---|---|---|
| M01 | complete | Core Screen Draw types, settings, and signed geometry | Unit tests for defaults, serde, hotkeys, palette, coordinate conversion, crop planning, toolbar clamping |
| M02 | complete | Mouse Gesture runtime suppression tokens | Targeted Mouse Gesture service tests |
| M03 | complete | Shared native GDI segment primitive | Shared helper tests and existing Mouse Gesture overlay tests |
| M04 | complete | Annotation document, history, hit testing, and transient ink | Document/history/eraser/fade tests |
| M05 | complete | Shared raster kernel and Screenshot Editor parity | Raster tests and existing Screenshot Editor tests |
| M06 | complete | Typed commands, built-in plugin, controller state machine | Parser/bus/host/plugin/state tests |
| M07 | complete | Hide-before-capture coordinator | Ordering, cancellation, stale-generation, and failure-restoration tests |
| M08 | complete | Native worker protocol and fail-safe lifecycle | Protocol, teardown, hotkey-conflict, worker-failure tests |
| M09 | complete | Active canvas, exact Pen, and backgrounds | Input, signed mapping, dirty-region, cancellation, suppression tests |
| M10 | complete | Remaining tools, text, eraser, fading ink, local shortcuts | Tool/text/shortcut/eyedropper/fade tests |
| M11 | complete | Ghost overlay, visibility, and display-change safety | Overlay alpha/style, transitions, display-change tests |
| M12 | complete | Floating toolbar and preference persistence | Control mapping, teardown order, persistence/clamping tests |
| M13 | complete | Unified full export workflows | Synthetic compositor/destination/failure tests |
| M14 | complete | Region export and Screenshot Editor handoff | Signed crop, picker cancel/error, handoff-order tests |
| M15 | complete | Lifecycle hardening, documentation, full verification, independent review | Formatting, check, targeted Nextest, full Nextest, review/remediation |

## Architectural invariants

- `LauncherApp` owns one cohesive `ScreenDrawController`; native input and
  high-frequency rendering stay on the session worker.
- Capture begins only after the launcher root viewport is hidden and verified
  absent from the virtual desktop. Toolbar and overlay creation follow capture.
- The immutable snapshot and all annotation geometry use signed physical
  desktop coordinates and reuse `mkmacro::screen` capture composition.
- Screen Draw and Mouse Gesture Pen both call the same extracted native GDI
  `CreatePen`/`MoveToEx`/`LineTo` primitive.
- Runtime gesture suppression never mutates persisted `config.enabled`.
- Escape, emergency hotkey, Done, close, display change, errors, panic/drop,
  and application exit all converge on idempotent native input disarm.
- Ghost uses a distinct per-pixel-alpha, passive, click-through surface over
  the live desktop; returning to Drawing never recaptures.
- Export is one compositor over orthogonal scope, background, and destination.
- No idle capture, native worker, polling loop, fade timer, or full-frame clone
  is introduced while Screen Draw is unused.

## Commit checkpoints

1. M01-M09: `feat(screen-draw): add native annotation runtime`
2. M10-M14: `feat(screen-draw): add annotation tools and export workflows`
3. M15: `test(screen-draw): harden lifecycle and regression coverage`
