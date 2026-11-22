# Virtual Desktop Plugin Settings

G24 owns `plugin-settings-virtual-desktop`. Settings → Plugins → Virtual Desktop opens the built-in callback. It uses checked native desktop IDs and a retained snapshot. Virtual desktop commands in launcher results remain a separate dispatch path.

| Criterion | Source-backed behavior |
| --- | --- |
| VD-SETTINGS-01 | Preserve launch timeout, opt-in auto-switch rules, enabled/executable/path/title/class fields, exact target GUID and unresolved, empty and failure states. Load the desktop list on demand and refresh explicitly. |
| VD-SETTINGS-02 | Empty or disabled rules start no worker. Applied valid settings and enabled plugin reconcile one runtime. Native effect commands revalidate desktop or workspace identity. Use a fake desktop source for effect tests. |

Select `settings_desktop_dropdown_is_on_demand_not_polled_each_frame`, `rule_runtime_requires_plugin_enablement_and_stops_on_disable_and_drop`, `workspace_binding_actions_use_stable_workspace_id_and_resolved_desktop_guid` and `stale_binding_never_retargets_by_cached_name`. Native and GPUI remain pending.
