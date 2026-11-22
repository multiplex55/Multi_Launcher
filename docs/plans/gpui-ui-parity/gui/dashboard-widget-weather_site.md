# Weather

G23 owns `dashboard-widget-weather_site`. The `WeatherSiteWidget::settings_ui` callback edits this slot's draft configuration through the Dashboard Editor. Runtime actions require the accepted current slot and dashboard pass.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-WEATHER-01 | Display configured location with the existing default. Retain editable location, default restore and URL template. An explicit widget action opens the generated external weather URL with the current query override. |

Use a synthetic location and a reserved URL for visual/setting checks. Shared registry and default-layout tests cover registration, and native/external action remains pending.
