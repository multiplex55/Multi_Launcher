# Universal Action Model initiative

This document is the execution ledger for the Universal Action Model migration.
Milestone state is one of `pending`, `in_progress`, `complete`, or `blocked`.

The migration keeps the persisted/search `actions::Action` and `Plugin` APIs as
compatibility boundaries. Universal Actions are resolved lazily above those
types only when an action-oriented surface requests them.

## Milestones

### 1. Universal Action domain foundation — `complete`

- Add stable semantic action identifiers and surface-independent presentation,
  availability, safety, target, persistence-reference, and operation types.
- Keep the model independent of egui and launcher-hotkey behavior.
- Add focused model tests and expose the module from the library crate.
- Acceptance: model tests, `cargo fmt --all --check`, and `cargo check` pass;
  legacy `Action` and `Plugin` definitions remain unchanged.

### 2. Target resolver — `complete`

- Resolve legacy search actions into typed runtime targets using typed command
  parsing and existing in-memory catalogs.
- Cover all existing result-context-menu identities plus Window, MkMacro, and
  Browser Tab, with a safe generic fallback.

### 3. Provider registry and built-in providers — `complete`

- Add pure, read-only providers that discover semantic actions lazily.
- Preserve current context-menu capabilities and add only currently supported
  Window, MkMacro, and Browser Tab capabilities.

### 4. Execution bridge and UI intents — `complete`

- Execute primary actions through the existing activation path, typed secondary
  commands through the command bus, and GUI-owned work through typed UI intents.
- Preserve secondary-action interaction behavior and centralized destructive
  confirmation, including confirmation context.

### 5. Launcher context-menu migration — `complete`

- Render launcher-result context menus from Universal Actions with behavioral
  parity in List and Grid modes.
- Remove the legacy `ResultContextMenuKind` normal path after parity is proven.

### 6. Keyboard Action Sheet — `complete`

- Add the searchable Ctrl+Enter Action Sheet with shared selected/sole-result
  targeting, independent filter state, keyboard navigation, and focus safety.

### 7. Compatibility hardening and full verification — `in_progress`

- Add resolver/provider/executor/UI regression coverage, confirm persistence and
  launcher behavior compatibility, complete independent review, and pass the
  full Nextest suite.

## Architectural boundary

The intended flow is:

```text
Legacy/Search Action
        -> ActionTargetResolver
        -> ActionTarget
        -> UniversalActionProvider
        -> UniversalAction
        -> ActionSurface presentation
        -> Universal Action Executor
```

`ActionSurface` says where an action is presented; `ActivationSource` continues
to say how an invocation was triggered. They are deliberately independent.

Radial Menu is future work. Universal Actions intentionally contain no radial
geometry, wedge placement, hotkey, gesture-duration, or hold-threshold behavior.
The existing launcher trigger may eventually be wrapped by a gesture resolver
that routes a tap to the launcher and a hold to a radial surface, while custom
radial menus may use unrelated triggers. Neither change should require provider
or executor redesign, and current hotkey behavior is unchanged by this plan.
