# GPUI Kit component mapping

Status: required mapping categories, not assertions that a particular selected-release API already satisfies the application.

G01 pins Kit; G04 confirms exact API/module names and establishes reusable components. Each feature record links the appropriate row and documents any extension.

| Existing pattern | Preferred foundation to evaluate | Behavior that must survive |
| --- | --- | --- |
| Ordinary buttons/toggles/options | Kit styled controls | Enablement, shortcuts, focus, exactly-once operation |
| Text fields / multi-line editors | Kit input/editor and retained state | Selection/caret, undo/redo, Unicode/IME, validation, draft lifetime |
| Search/results | Kit lists/tables/command primitives as appropriate | Ranking, stable identity, virtualization, keyboard/numpad actions |
| Panels/tabs/sections | Kit layout/container/tab primitives | Original structure, labels, density, active-state persistence |
| Menus/context actions/popovers | Kit menu/overlay infrastructure | Deep Escape ownership, keyboard activation, focus return, no duplicate action |
| Dialogs/confirmations | Window-local Kit roots and dialog primitives | Modality, cancel/save, child lifetime, focus/unsaved state |
| Independent/popout utilities | GPUI native windows with per-window roots | Hidden main window, dock/pop semantics, geometry, lifetime |
| Tables/grids | Kit table/data primitives or narrow custom components | Full data semantics, selection, sorting, scale and row identity |
| Charts/custom canvas | GPUI drawing/custom elements plus shared model | Coordinate/selection/drag/zoom semantics, accessibility alternatives where applicable |
| Image previews/annotation | GPUI image/canvas presentation plus existing engines | Pixel/coordinate transforms, crop/compare/export, source identity |
| Notifications/progress | Kit presentation primitives plus existing job state | Relevant status/errors, cancellation, history and lifetime |
| Native overlays/capture tools | Existing independent Windows substrate + GPUI controls/adapters | No-activate/topmost/hit testing/capture/input restoration/DPI |
| Plugin/widget settings | Explicit frontend presentation contract | Every registered option and custom child, default/validation/save semantics |

## Required mapping evidence

For each recurring pattern record exact selected-release component/type, state owner, event/command boundary, focus and accessibility behavior, custom extension rationale, test strategy and known limitations. Custom code should handle a real mismatch; merely adding the Kit dependency while rebuilding every normal component defeats the selected foundation.

Reused code must not smuggle egui Ui/Context/texture assumptions into GPUI. A toolkit component’s default behavior is not authority to change Multi Launcher’s contract. An unsupported feature becomes tracked work or a genuine blocker, never a silent removed control.
