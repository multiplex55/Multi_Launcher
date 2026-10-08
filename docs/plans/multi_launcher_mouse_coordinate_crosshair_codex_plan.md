# Multi Launcher — Plan C: Mouse Coordinate Inspector & Crosshair

**Codex implementation plan · Approved scope 

**Required working branch:** `mouse-improvements` (Git does not permit spaces in branch names)  
**Target:** Windows 10/11, Rust, eframe/egui, Win32, local-only

> **Required directive:** **Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**
>
> **Required remote rule:** After **every successful checkpoint commit**, immediately push the new commit to `origin/mouse-improvements`, confirm success, and only then start the next checkpoint. Never force-push, squash, amend, or rewrite checkpoint history without explicit user authorization.

## 0. Mission and execution contract

Deliver **one integrated coordinate utility** whose independent operating modes are:

1. **Coordinate Inspector** — `coord` opens/toggles a small, responsive, movable-by-cursor-offset, click-through HUD displaying live mouse position and selected coordinate space. The HUD supports compact and detailed presentation, freeze/unfreeze, copying, last-copied status, monitor indication, and contextual help.
2. **One-shot coordinate picker** — `coord pick` enters an **explicit active capture session**. A left click captures the pixel at the cursor and copies it to the clipboard **without passing the capture click into the underlying application**. Escape cancels and must not change clipboard contents. Successful capture ends Pick mode without leaving an overlay or lingering mouse/key interception.
3. **Independent crosshair** — `crosshair` toggles a native, passive, click-through crosshair independently of the HUD. Support a small centered crosshair, optional virtual-desktop-spanning horizontal/vertical guides, saved color/thickness/arm-length/opacity, and high-contrast visibility.

The HUD and crosshair must be usable **separately and simultaneously**. Multiple invocations must be deterministic and idempotent (toggle or focus as defined below), never create duplicate workers or windows. The tool must never produce significant continuous overhead when inactive.

### Hard rules