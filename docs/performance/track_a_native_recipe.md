# Track A native measurement and smoke recipe

Status: procedure prepared; native observations are NOT RUN until explicitly recorded in baseline/results. Headless test success does not establish desktop composition, exclusion correctness or latency.

Use the same Windows host, build profile, power state, monitor/DPI arrangement and display refresh for before/after. Record those facts and the exact commit. Use only a synthetic animated scene; do not commit desktop readbacks, foreground titles, raw paths or private profile data.

1. Build/run the existing production fixture: `cargo run --bin coordinate_tool_smoke -- --cursor-effects`. It uses the production controller/backend. `--cursor-effects-auto` advances its existing 16 scene stages and records diagnostics; automatic API success alone does not prove visible composition.
2. Observe halo alone, zoom alone and both, first moving over the scene, then stationary over its changing marker. Include HUD/crosshair/guides to verify exclusion ordering and absence of recursive capture. Repeat each case using the same duration and note sample/submission counts from opt-in metrics.
3. Check effect disable/re-enable, clipping pause/recovery, supported display/DPI changes, and all-four-off worker shutdown. Coordinate copy must remain fresh while the same foreground window moves/resizes.
4. For quantitative timing, capture bounded sample-to-source-submission windows and report p50/p95/max with sample count. Label this CPU/API timing; actual pointer-to-display latency requires an appropriate external observation tool. CPU/GPU and GDI-handle observations must state their tool, duration and mode.
5. The standalone feasibility fixture described in `tools/cursor_effects_smoke/README.md` can supplement observations, but it does not use the production renderer and cannot substitute for the production smoke.
6. Keep all native outputs in a new isolated directory under ignored `target/` or a temporary working directory. Do not overwrite historical smoke evidence. Report PASS, FAIL, NOT RUN and NOT MEASURED separately for API execution, visible composition/exclusion, stationary content, geometry/copy, resource stability and latency.

M6-B/M6-C require supporting measurements. If native timing/resource evidence is unavailable or marginal, skip the optional caching implementations explicitly rather than claiming a measured decision.
