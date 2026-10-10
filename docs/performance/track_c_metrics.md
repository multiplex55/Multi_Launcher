# Track C metric definitions

Track C diagnostics are a fixed-cardinality addition in `performance::track_c`.
The historical `performance::Metric` collector remains at 15 entries and keeps
its existing frame-recording behavior. Track C has 12 named phase counters and
an 8-by-9 event class/origin matrix; it stores no payloads, paths, queries, or
producer-provided labels.

Diagnostics are enabled when `MULTI_LAUNCHER_PERF` is set to `1`, `true`,
`yes`, or `on` (case-insensitive, with surrounding whitespace ignored). The
setting is cached once per process. Timers do not call the clock when disabled.
The internal event channel similarly caches this setting when created: its
normal disabled send path does not classify the event, read the clock, or
maintain observed queue depth. `EventReceiver::queued_depth()` returns `None`
for such a channel. When observation is enabled, depth counts only envelopes
owned by that app channel, separate from registry wake bookkeeping and events
pending before an app registers.

`performance::track_c::snapshot()` returns every phase in enum order with call
count, work units, elapsed nanoseconds (total and maximum), and completed,
error, and abandoned outcomes. The search scoring phase also reports separate
`candidates_scored` and `action_clones` counters; those are zero for unrelated
phases. `snapshot_events()` returns every class/origin
pair, including zero-valued pairs. Concurrent snapshots are approximate and
may straddle updates. `reset()` is intended for controlled runs while writers
are quiescent; it resets both Track C collectors and leaves historical metrics
alone.

| Phase | Timed boundary and work units |
| --- | --- |
| `note.relationship_refresh` | Heavy derived-note refresh from snapshot acquisition through the relationship recomputation; work units are three times the todo-plus-note slice lengths traversed by its three backlink passes. Snapshot failures are errors. |
| `note.mentions_scan` | The mention backlink pass only; units are todo-plus-note slice lengths traversed, including the self-note check that skips the current note. |
| `action.publish_prepare` | Clone/preparation of the currently accepted indexed tail before action publication; units are indexed actions prepared. |
| `action.publish_commit` | Assemble and publish the custom prefix plus indexed tail, then update the action cache and request query refresh; units are resulting actions. |
| `search.score_and_clone_hits` | Current action scoring path, including its interleaved cloning of matching candidates; `work_units` is the examined action-catalog length, `candidates_scored` counts entries that passed filters and ran exact/fuzzy text matching, and `action_clones` counts local Actions copied into hits. Empty-query hit copies contribute clones but no scored candidates. |
| `search.move_results` | Final conversion of scored owned entries into the result vector. This is a move/collection boundary, not clone time; `work_units` is scored entries, and the clone counter remains zero. |
| `root.geometry_cold` | A root list/grid geometry cache miss through construction; units are result rows. Cache hits are excluded. |
| `notes.geometry_cold` | A Quick Notes geometry cache miss through measurement; units are filtered note indices. Cache hits are excluded. |
| `event.enqueue_age` | One sample from successful observed enqueue until dequeue or receiver teardown; work units are one envelope. Teardown before consumption is marked abandoned. |
| `event.drain` | One complete GUI event-drain invocation; work units are events dispatched. Static class/origin handler durations are available separately in the event matrix. |
| `startup.catalog_ready` | Startup indexed catalog completion and custom-first assembly; units are actions in the complete catalog. Failure is recorded as an error. |
| `hotkey.to_first_usable_frame` | A visibility invocation/revision from show publication to the first eligible, visible ROOT frame whose captured result generation is still current. Empty browse/dashboard frames qualify. |

The hotkey phase is CPU-side GUI completion: it ends after the normal root
render path and OCR surface call. It excludes the OCR early-return path and
does not claim that native window presentation or display scanout has occurred.
It is armed while the existing visibility publication gate is held, before
other observers can consume the show revision. Duplicate, stale, hidden,
changed-result-generation, changed-provider-revision, unstable or changed
mutable-catalog-version, provider-pending, refresh/loading, or parked frames do
not complete the sample. The accepted search outcome's mutable catalog
versions are retained with its installed results and compared with current
clipboard, todo, and note versions at frame end.

The event matrix attributes envelopes using a static class derived from the
`WatchEvent` variant and a static origin assigned at the internal producer or
registry fanout. For each pair it reports successful enqueues, dequeues, failed
sends, abandoned queued envelopes, queue-age total/maximum, and reducer-handler
call count plus handler-time total/maximum. Handler timing begins immediately
after dequeue and uses an RAII guard spanning the complete reducer arm, including
error and early-continue paths. Public raw sender registration retains its
existing `std::sync::mpsc::Sender<WatchEvent>` signature; the app-owned receiver
facade returns the original plain `WatchEvent` values and keeps envelope
metadata private. Native presentation remains outside these measurements.
