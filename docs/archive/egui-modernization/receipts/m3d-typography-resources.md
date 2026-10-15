# M3-D typography and cached UI resources

Parent9c56978b. Context theme runtime retains one bundled brand texture; simple chrome symbols are immutable vector painting in native text-labeled buttons. Existing egui fonts/families/fallbacks remain, with semantic role mapping to existing TextStyles and absolute theme sizes. Root header/theme-editor headings/footer/reset controls are initial consumers. Query/default result fallback preserves override precedence, independent scales and geometry contracts; no result-row redesign.

Source review findings resolved: explicit result/query text styles bypassed existing overrides, so native resolution restored; new role test runs a first UI pass before reading fonts. No remaining substantive finding.

Windows locked/offline fast-dev resources-001 ran35tests:34passed,1failed only exact literal25.2 vs18f32*1.4 representation. resources-002 reran that exact role test:1/1passed, retaining exact equality with expected scaling expressions. Production unchanged between these runs; all required35unique contracts have passing evidence. Normal Windows binary build resources-002 also passed/native0 in121.8295s. Source hashes unchanged during commands; scoped format/diff pass. Existing ROOT/list/grid overrides/cache consumers, Notes, preview/session persistence, resource reuse/secondary viewport and native button accessibility/activation tests remain meaningful.

Raw input/start/result/logs and runners under target/egui-modernization/m3d/resources-001/002. No broad suite/benchmark activated. Bounded normal native run is documented in WINDOWS_FUNCTIONAL_QUALIFICATION.md: accessibility typing/local search/Enter/Escape observed; screenshot and hotkey restoration unverified; one post-Escape idle sample/no growth, process closed normally. No visual/parity/full-native claim.

Proceed to M4-A query surface, M4-B result hierarchy/geometry, shared primitives and full settings/plugin rollout. SURFACE_COVERAGE.md inventories54 presentation owners and pending group qualification.
