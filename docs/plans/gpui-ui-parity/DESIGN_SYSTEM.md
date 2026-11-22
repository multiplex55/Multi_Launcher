# Visual design contract

Status: visual goals specified; actual component/style decisions and screenshots await G04.

## Preserve the application while improving presentation

Use a coherent native desktop style with readable typography, controlled spacing, aligned controls, consistent icons, clear panels and stable hover/selection/focus/error states. The user must recognize existing tools and find the same operations without learning a different workflow.

Preserve panel relationships, tool order where meaningful, labels, information density, command prefixes, advanced controls and supported compact/window modes. Do not replace a rich editor with a generic form or turn a multi-pane utility into a sparse demo.

Define reusable design tokens for font family/size/weight, theme colors, surface hierarchy, padding/gap, row/control heights, border/radius and semantic feedback. Pick concrete values from the existing UI’s density and supported user settings during G04; no arbitrary pixel specification is imposed by this package.

## Required decisions and checks

Record mapping of shared theme intent, font/text scale, accent/status colors, selection and contrast. Preserve effects of meaningful user preferences; different framework internals do not justify ignoring an existing text-size setting.

For a representative real GUI, inspect normal/populated, empty, loading, disabled, error, long content, narrow/large window, keyboard focus, and supported scaled display. Confirm custom fonts/icons load in the actual distribution, not only from a development directory.

Use Kit styles/components as the baseline. Extend only for recorded functional or presentation requirements. Accessible semantics and visible focus must be designed alongside control interaction, not added as a dependency badge at the end.

## Reference and evidence

Capture comparable egui and GPUI screenshots with synthetic data, equivalent theme, dimensions and scale. Record which differences are intentional polish and which are parity defects. Pixel identity is not required. Verify operation outcomes separately.

When a shared visual change affects earlier screens, inspect only representative impacted layouts and specific known edge cases; re-open affected acceptance criteria. Avoid a giant screenshot-diff infrastructure solely for this migration.
