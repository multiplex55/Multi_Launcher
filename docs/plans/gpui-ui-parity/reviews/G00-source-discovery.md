# G00 source discovery review

Configured read-only reviewer inspected `e449f6f785ebbb1b466cda361d984214951fb599` and the pending discovery records. Source samples confirmed the 17 built-in plugin overrides, 39 dashboard registrations and 36 settings callbacks. No build or test was run during review.

The review required a crosswalk from actual registrations to stable GUI IDs, explicit launcher completion and history criteria, complete launcher menu handoffs, two separately owned MkMacro image decisions, and explicit Hash Generator close, host movement and feedback behavior. The parent reconciled those in the [crosswalk](../GUI_ROUTE_CROSSWALK.md), [inventory](../GUI_INVENTORY.md), and [Hash Generator specification](../gui/hash-generator.md).

G00-C remains pending for observed launcher query/navigation, a utility popout, and a persisted edit from an identified fixture artifact. The source inventory is provisional. Dynamic-library controls and remaining per-feature inline criteria retain their named owners and next actions in the inventory.
