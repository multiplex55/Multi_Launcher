# M4-A compact root query surface

Parent `76ac8786`. Query presentation now uses a deliberate semantic frame, compact padding/radius, aligned vector search icon, native focus stroke, empty placeholder and secondary autocomplete/error treatment. Native TextEditOutput/cursor storage, explicit ID, outer query scaling, modal input suppression and immediate first-pass routing remain unchanged. Existing font/text-style override precedence retained.

New root_query_presentation_stays_usable_when_narrow_and_scaled checks actual galley font/height under distinctive competing overrides, stable identity, positive editable clip, icon separation, retained prior focus and unchanged empty query. Seven existing caret/history/autocomplete/modal/numpad contracts remain unchanged.

Windows locked/offline fast-dev query-002 **8/8 passed**, native0,79.501s including compilation; source bindings unchanged. Scoped rustfmt/diff pass; independent source review no remaining substantive findings. Raw input/start/result/logs: target/egui-modernization/m4a/query-002/nextest. query-001 compile101 retained; two row_height calls migrated to fonts_mut without behavioral/assertion change. Review removed explicit Body font selection that bypassed local overrides; new actual-galley assertion guards it.

No new native/visual/performance claim; M3-D bounded observations and current capture/hotkey/manual limitations remain separately documented. Proceed immediately to M4-B shared result text/geometry hierarchy and preserved virtualization.
