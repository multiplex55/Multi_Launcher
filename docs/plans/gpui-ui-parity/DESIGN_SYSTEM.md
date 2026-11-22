# Visual design contract

Status: G04 appearance mapping is specified from current source. Integration and visual validation remain pending.

## Preserve the application while improving presentation

Use a coherent native desktop style with readable typography, controlled spacing, aligned controls, consistent icons, clear panels and stable hover/selection/focus/error states. The user must recognize existing tools and find the same operations without learning a different workflow.

Preserve panel relationships, tool order where meaningful, labels, information density, command prefixes, advanced controls and supported compact/window modes. Do not replace a rich editor with a generic form or turn a multi-pane utility into a sparse demo.

Define reusable design tokens for font family/size/weight, theme colors, surface hierarchy, padding/gap, row/control heights, border/radius and semantic feedback. Pick concrete values from the existing UI’s density and supported user settings during G04; no arbitrary pixel specification is imposed by this package.

## Required decisions and checks

Record mapping of shared theme intent, font/text scale, accent/status colors, selection and contrast. Preserve effects of meaningful user preferences; different framework internals do not justify ignoring an existing text-size setting.

For a representative real GUI, inspect normal/populated, empty, loading, disabled, error, long content, narrow/large window, keyboard focus, and supported scaled display. Confirm custom fonts/icons load in the actual distribution, not only from a development directory.

Use Kit styles/components as the baseline. Extend only for recorded functional or presentation requirements. Accessible semantics and visible focus must be designed alongside control interaction, not added as a dependency badge at the end.

## G04 appearance mapping

Use the committed `Settings.theme` and existing framework-independent `gui::theme::resolve_theme` output. Resolve System, Light, Dark and Custom, including saved preset fallback, accent and normalized typography scale. GPUI Kit initializes Light. Apply the resolved light or dark mode first and its semantic slot and typography overrides separately, since changing the Kit mode reloads its defaults. Keep one root appearance observer for System. Both Hash mounts use the same Kit global theme. The pinned Windows backend owns system caption appearance.

Map the semantic window and panel surfaces, foreground, input border and focus ring to the corresponding Kit slots. Keep the accent, selection, link, and success, warning and error roles distinct and preserve saved alpha. Use the status slots on real Hash result, SHA-1 warning and error text. Retain Kit focus, disabled states and component styling where no product role applies.

At scale 1 the resolver yields 14-point body, 18-point heading, 12-point small text and 18-point line height. Compact density uses 6 × 4 item gaps, 8-point section/window padding, 6-point panel padding, 4-point corner radius and 28-point minimum row height. Comfortable scales gaps and row height by 1.25 and uses a 7-point radius. Base Kit rem on `16 × resolved typography scale` for its standard 14-point small controls. Apply the existing roles explicitly to application-owned labels and layout, and avoid applying the typography scale again to Kit rem dimensions. The global settings have no font family selector. Query and result-list scales belong to the actual launcher surface.

Verify fixed and System modes, Custom preset fallback, independent accent and selection with alpha, density and normalization bounds through the resolver and adapter. Exercise the same production Hash states docked and in a narrow popout with default and larger text. G06 owns the appearance editor and save/preview behavior.

## Reference and evidence

Capture comparable egui and GPUI screenshots with synthetic data, equivalent theme, dimensions and scale. Record which differences are intentional polish and which are parity defects. Pixel identity is not required. Verify operation outcomes separately.

When a shared visual change affects earlier screens, inspect only representative impacted layouts and specific known edge cases; re-open affected acceptance criteria. Avoid a giant screenshot-diff infrastructure solely for this migration.
