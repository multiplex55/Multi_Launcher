# M1-A compiler API error inventory

Status: **graph resolved; all-target check failed on expected source migration errors.** No application APIs were adapted in M1-A.

## Check record

- Command: direct stable Cargo 1.99.0 `cargo check --all-targets` on Windows MSVC, after the manifest update.
- Rust compiler: `rustc 1.99.0 (b940084d7 2026-09-28)`; Cargo: `cargo 1.99.0 (5f94df478 2026-08-27)`.
- Exit status: `101`.
- Cargo summaries: library compile reported 621 previous errors; library-test compile reported 1,269 previous errors. Both reported eight existing `Atomic::fetch_update` deprecation warnings.
- Full raw output: `target/egui-modernization/m1a/cargo-check.log` (ignored, retained locally).

The log contains repeated diagnostics across application and test compilation contexts. The raw log is authoritative; error-code counts below count diagnostic headers as captured and are not a unique-callsite count:

| Error code | Count | Main shape |
|---|---:|---|
| E0599 | 972 | Removed or renamed methods on egui `Context`, `Ui`, widgets, and related types |
| E0308 | 87 | Type/signature mismatches |
| E0061 | 74 | Changed argument counts |
| E0063 | 42 | Missing fields in initializers, including `egui_toast::Toast::style` |
| E0433 | 33 | Removed or renamed egui types/modules, including `SelectableLabel`, `Rounding`, and popup APIs |
| E0609 | 23 | Removed struct fields |
| E0624 | 9 | Newly private items |
| E0614 | 8 | Dereference mismatches |
| E0560 | 8 | Removed struct fields in initializers |
| E0277 | 7 | Trait-bound mismatches |
| E0631 | 6 | Closure/signature mismatches |
| E0507 | 5 | Move/ownership mismatches exposed by changed APIs |
| E0425 | 4 | Missing functions or values |
| E0596 | 2 | Mutability mismatches |
| E0782 | 1 | Trait/type syntax mismatch |
| E0432 | 1 | Unresolved import (`egui::popup`) |
| E0407 | 1 | `eframe::App::update` is no longer a trait member |
| E0271 | 1 | Associated-type mismatch |
| E0046 | 1 | Missing trait items |

## High-frequency API families

- **Context integration:** `Context::run` (205 diagnostics), `begin_frame` (97), and `end_frame` (103) are no longer available; target harnesses use the new `run_ui` path. `eframe::App::update` also changed and is not part of this checkpoint's source work.
- **Widget APIs:** `DragValue::clamp_range` (135), `Ui::close_menu` (82), `ScrollArea::id_source` (70), `ComboBox::from_id_source` (46), and `Ui::allocate_ui_at_rect` (12) are changed or removed.
- **Style and menus:** `Context::style` (25), `set_style` (17), `is_context_menu_open` (19), menu-bar/popup/tooltip methods, `SelectableLabel`, and `Rounding` require scoped API migration.
- **Toast integration:** 41 diagnostics identify `Toast` initializers missing the new `style` field.
- **Other direct changes:** compiler output includes `Frame`, panels, viewport callbacks, text/layout, painter, and AccessKit API shape changes. These remain assigned to later API/integration checkpoints.

No diagnostics were fixed, suppressed, or used to change assertions in M1-A. A successful/runnable application claim is not supported until the assigned source migration checkpoints and their verification complete.
