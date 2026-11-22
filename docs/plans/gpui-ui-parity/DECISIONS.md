# Decisions

## D001 — Full duplicate frontend, shared authoritative behavior

User-directed. Keep complete egui and complete GPUI implementations over shared domain/services. One frontend per session. No mixed windows or hidden egui host.

## D002 — GPUI Kit is the chosen foundation

User-directed. Use gpui-kit.com / longbridge/gpui-kit. Pin a compatible exact release and matching dependency family at G01. 0.10.0 is the inspected documentation version, not a claim of a successful project build.

## D003 — Ordinary launch remains egui

The new explicit user parenthetical controls older generic saved-preference prose. No selector resolves to egui even after a remembered GPUI choice. Remembered choice can drive an explicit restart/shortcut. Do not change default at completion. See FRONTEND_SELECTION.

## D004 — One branch across the program

User-directed. Create a new dedicated branch from latest fetched master at kickoff; use it across all goals. Historical per-goal branch chains and worktrees are not adopted. Resume existing program state rather than restart from master.

## D005 — Scoped verification, not repeated full nextest

User-directed and consistent with current AGENTS. Use affected targets/filters, baseline comparison, production-view tests and appropriate native evidence. Full-suite runs require a concrete reason; no whole-Cargo 100-second timeout.

## D006 — Native substrate reuse is permitted

Architectural interpretation. Independent Windows capture/overlay/file-dialog/service code can be shared if it requires no egui runtime and preserves the contract. All existing egui presentation/control surfaces still need GPUI equivalents. This avoids rewriting already independent Windows behavior solely for branding.

## D007 — Goal packets are expandable coverage families

Planning decision. Initial G00–G26 goals cover known source families. G00 source/runtime discovery assigns real stable GUI/child IDs. Additional baseline surfaces expand scope; they cannot be hidden to preserve a count.

## Future decisions

Record date, source/evidence, alternatives only when material, chosen owner/contract, compatibility impact, affected validation and reversal implications. Ordinary helper naming and equivalent Rust details do not require decision paperwork or user approval.
