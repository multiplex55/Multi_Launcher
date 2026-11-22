# Recent Notes

G23 owns `dashboard-widget-recent_notes`. The `RecentNotesWidget::settings_ui` callback edits this slot's draft configuration through the Dashboard Editor. Runtime actions require the accepted current slot and dashboard pass.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-RECENT-NOTES-01 | Retain configurable count, tag, snippet and Panel/Dialog/Query modes. Use current modified order, title or alias, slug and current cache version. Existing source can leave a literal prefix in Query mode and can slice a Unicode snippet at a byte boundary. Verify both before accepting. |

Use one short ASCII test note and explicit Panel or visual inspection. Shared registry tests cover descriptor registration. Native and GPUI remain pending.
