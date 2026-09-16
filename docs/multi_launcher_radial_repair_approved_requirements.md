# Radial repair — approved requirements

**Status:** Approved by the user. No additional questionnaire is required.  
**Source reviewed:** `multi_launcher(20260916-004931).zip`; implement against the current checkout and preserve newer work.  
**Historical baseline:** Retain the existing pinned branch baseline. Record a separate repair-start revision.  
**Master instructions:** `multi_launcher_radial_repair_codex_plan.md`.

## Superseding decisions

These choices intentionally replace earlier radial defaults:

- The shared hotkey now consistently distinguishes **tap -> toggle grid only** and **hold -> toggle radial only**, even while a radial is open. It no longer immediately dismisses an open radial on initial key-down.
- SameCenter replaces Cascade as the default. Existing submenu presentation receives a one-time, reported conversion with backup/undo; later explicit Cascade choices remain allowed.
- The editor becomes one independent native Radial Designer window rather than an embedded large launcher dialog.
- Long-job observation is **10 minutes initially, 15 minutes next, then every 20 minutes**, not the previous seconds-based schedule. Completion notifications may return sooner. These are observation intervals only.

## Runtime and navigation

Keep the current launcher chord and configured threshold; 350 ms remains its existing default. No mouse-wiggle, manual refresh, grid visibility, or foreground/unfocus dependency. A short tap must hide a focused visible grid. In sticky mode, the opening hold's release leaves the radial open; a later closing hold's release does not reopen it. Esc cancels radial promptly without affecting the underlying editor. Preserve Screen Draw recovery/emergency priority and existing alternate/direct-trigger modes outside this intentional shared-trigger change.

SameCenter uses the actual visible physical center and session monitor through child/grandchild/Back, including after clamping and deliberate dragging. Moving the cursor does not reposition navigation. Larger children fit/page within usable limits or leave the current menu usable with an explicit fallback; they do not silently jump elsewhere or lose cells. Runtime child center-click goes Back. Optional Cascade remains close/overlapping with obvious active level.

## Pointer, text, and warnings

Use the normal Windows pointer for ordinary radial hover; keep valid drag/resize feedback. Do not mask genuine stalls with cursor changes.

Full-label tooltips are enabled by default with configurable 300 ms delay, independent of the launcher hold threshold. Preserve original labels, include distinct custom descriptions, wrap to sensible monitor-aware bounds, and support runtime plus embedded/native previews. Tooltips must not use the small label width, move the wheel, steal focus, or block clicks/Back.

Expected truncation is quiet by default: no repeating inline warning list or toast/log flood. Keep optional deduplicated details in a collapsed Diagnostics section and a developer setting. Real asset, glyph/configuration, save, and runtime failures remain actionable.

## Independent Designer and compact layout

One ordinary resizable Radial Designer window in the same application; Menus and Skins share the authoring backend. Approximately 900x650 logical units initially, clamped to the work area, then remember user geometry. Not always-on-top by default. Grid hiding does not hide it. Closing it does not quit Multi Launcher; preserve dirty-close and pending-operation protection.

Narrow optional tree, bounded canvas, and hideable/manually resizable inspector. No equal-width columns. All current controls stay available, grouped into compact basic and collapsed advanced sections. Remember widths, visibility, expansion, zoom, and pan. Selection, search, diagnostics, and refresh do not auto-expand anything. Long content scrolls internally. Fit is explicit; preview zoom does not change actual menu size.

## Direct editing

Design mode is clearly non-executing. Show empty authored slots. Center '+' starts placing/creating a cell in an explicit ring/slot; empty-slot click can create/edit it. Single-click selects an existing cell, right-click opens compact properties, double-click a submenu opens it for design. Breadcrumb/Back navigates the visited path.

Drag with a visible destination and safe occupied-slot handling; never silently overwrite an action. Create-and-link submenu is one undoable operation. Ring shrink preserves data unless removal/relocation is explicitly chosen. Generated dynamic results retain source identity; selecting/dragging them does not silently make them static definitions. Testing is a separate explicit operation.

Preserve stable IDs, menus, skins, actions, confirmations, dynamic collections, imports/exports, transactions, undo/redo, and actual current Save/Apply/Cancel semantics.

## Execution and verification

Implement runtime invocation/position repairs first, then pointer/tooltips/diagnostics, then Designer work. No restart of the radial subsystem and no new renderer or unrelated feature family.

Write/refactor tests alongside coherent batches. Use focused gates and collect failures before rebuilding; keep durable logs. Only one Cargo/build/Nextest job per checkout/target. Reattach to it on the **10/15/20-minute** schedule instead of launching a replacement. Do not kill healthy builds, clean caches, or change test timeouts because progress is quiet.

Require the complete relevant Cargo Nextest suite before completion. Native acceptance must exercise hidden/focused hotkeys without mouse movement, stable visible menu centers, pointer/tooltips, and independent compact Designer behavior. Unperformed native checks remain explicitly unverified.
