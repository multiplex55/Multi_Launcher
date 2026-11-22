# Media Controls

G23 owns `dashboard-widget-now_playing`. The `NowPlayingWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-MEDIA-01 | Preserve individual Play, Pause, Previous and Next controls and settings. The current slot renders transport controls. Windows Play and Pause use the same toggle media key. |

Use a fake sink or observe controls without activation. Shared descriptor and typed route tests cover registration.

Native and GPUI remain pending.
