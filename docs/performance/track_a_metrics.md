# Track A metric reference

Enable with `MULTI_LAUNCHER_PERF=1` before starting the process. Existing truthy parsing and process-cached enablement are retained. New owner metrics aggregate into a fixed atomic array; they do not emit per-operation logs or create a reporter thread. No labels, paths, queries, note/history bodies, clipboard data or HWNDs are stored.

`snapshot_metrics()` returns bounded cumulative observations. Concurrent fields are individually atomic, not a transactional snapshot. `reset_metrics()` is for controlled measurement boundaries with quiescent producers. Durations are nanoseconds; totals/max are aggregates. Exact p50/p95/max operation samples are the harness's responsibility, not inferred from totals.

| Metric | Boundary and work units |
| --- | --- |
| `note.refresh_check` | Heavy-derived decision check; requested refresh count, excluding invoked heavy recompute duration |
| `note.snapshot` | Snapshot attempt; estimated cloned bytes; elapsed is lock-held estimation/cloning, with lock acquisition wait separately reported |
| `note.alias_hash` | Actual alias gathering/sort/hash; primary alias/slug pairs hashed |
| `note.heavy_recompute` | Actual enabled derived rebuilding after no-op guards; rebuild count |
| `history.prepare` | History acquisition through resolved/filter/count preparation, excluding painting; normal history records cloned |
| `history.resolve` | One candidate resolver invocation; candidate count |
| `history.catalog_build` | Actual plugin command enumeration needed by history; command actions enumerated |
| `launcher.rows_built` | Active root results painting; actual selectable action widgets (grid cells count individually) |
| `quick_notes.rows_built` | Active browse projection/preparation/painting; matching note rows constructed |
| `actions.reload` | One Actions watcher handling attempt, including early returns; actions loaded/reused/published as applicable |
| `index.scan` | Each actual iterator `next` traversal call, excluding consumer waits; constructed actions (including an error-discarded partial batch), with mutually exclusive completed/error/abandoned scan outcomes |
| `coordinate.sample` | Live worker sampler call or disjoint click-time point sample; attempts, including failures |
| `effects.refresh_source` | Native refresh dispatch; attempts, including halo fallback |
| `effects.present_source` | Native live presentation dispatch; attempts, including halo fallback |
| `hud.gdi_create` | Brush/font creation API call; attempts |

Clone byte counts estimate owned structs/string/vector payloads, including note content, rather than allocator traffic or retained capacities. Computing estimates adds opt-in measurement overhead inside the snapshot boundary. Do not label these as exact allocation bytes. Submission attempts do not establish successful presentation, display latency, or GPU cost. Native outcomes require actual Windows checks. The index metric's `calls` is traversal invocations, not scans; terminal outcomes identify scan counts.

The disabled path checks the existing cached flag before clock reads, byte estimation or collector mutation. Fixed metric storage has no dynamic cardinality. Telemetry changes no owner algorithms, publication ordering, cadence, persistence or formats.

