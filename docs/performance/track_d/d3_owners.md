# D3-A idle ownership inspection

Source as of D2 closure cd03ae95; no idle source edits. Inspection only, not
measured process CPU/wakeup evidence. Native containment blocker and its one
diagnostic retry are recorded in native_capability.json; unchanged here.

| Owner | Actual scheduling boundary | Relevant invariant |
|---|---|---|
| Hot Folder `driver_main`, src/gui/hotfolder_runtime.rs | Request channel recv_timeout: enabled 80 ms, disabled 500 ms; snapshot publication after loop work | Requests, execution readiness, admission and shutdown/reaping belong to this driver |
| Mouse gesture worker, src/mouse_gestures/service.rs | Timeout uses min of configured trail/recognition intervals, bounded at >=1 ms; Windows message pump and hook events share loop | Gesture/hint/trail response and stop processing cannot be weakened |
| Cursor/click worker, src/coordinate_tool/controller.rs | Timed sampling while cursor modes or click animation active; blocking recv otherwise | Disabled/idle click-only path already blocks; cursor sampling and balanced native cleanup stay owned here |
| Radial deadline scheduler, src/radial/controller.rs | Blocking recv with no deadlines, timed recv to earliest deadline otherwise | Handoff/dwell/tooltip deadlines and cancellation share scheduler |

Hot Folder publishes snapshots even after timeout; loaded store/watcher/engine
snapshots and acknowledgement/preview clones may be worth attribution **if**
owned whole-process measurements show meaningful cost. An 80 ms configured
wait is not a measured wakeup rate and does not establish the >=25% plus
absolute D3 gate. Do not change it based on inspection alone.

External probes intended for an owned PID: process TotalProcessorTime / elapsed
wall, PrivateMemorySize64, WorkingSet64, HandleCount and thread count. Wakeups
require supported OS counters or bounded owner counters; never infer them from
CPU. No new counters or polling thread installed because required controlled
GUI scenarios cannot be safely launched under the established containment
restriction. No system-wide/user process sampling performed.
