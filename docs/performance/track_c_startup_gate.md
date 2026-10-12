# C3-B startup gate — LOCAL ONLY

Decision: **SKIPPED (measured gate)**, matching the plan's **SKIPPED (readiness contract)** acceptance. No runtime candidate or startup reorder is introduced. Independent reviewer `c0_boundary_review` cleared the decision, exact fractions, source continuity, ownership and limitations; no blocking findings.

The C3-A evidence at source `0fbd2f0327badf899e2bdd662616ac6d9d1228d5` does not identify a concrete material wait outside the complete-catalog readiness barrier that can safely be shortened. At 16/1,000/10,000 indexed files, the median same-sample wait fractions are 96.885057%/99.386539%/99.426946%. Indexed-body cloning accounts for 0.395089%/0.335859%/0.318219%. These are distributions of per-sample ratios, not divisions of phase quantiles. Scan-step time overlaps wait and cannot be added to it.

`startup_indexed_actions` owns submit, matching generation/config wait, take and acknowledgment validation. `startup_action_catalog` produces the custom-first/indexed-tail contiguous catalog. `spawn_gui` supplies it to plugin registration before GUI construction; OmniSearch and VirtualDesktop initially depend on that complete immutable snapshot. GUI transfer retains the acknowledged coordinator without resubmitting. Removing readiness would require a separately approved complete plugin-update/parity design. None is established by these measurements.

The separate ready-catalog component's frame phase accounts for median 44.310616%/73.508275%/81.199278% of its owner. That aggregate does not identify removable query or geometry preparation. The real constructor search is retained, and no speculative cache or second index worker is justified. This gate does not prove startup optimality.

No new Cargo, benchmark or native execution was performed for this documentation-only decision. Reuse the independently accepted C3-A 12 distinct ordinary passes and 160 unique retained owner samples; all nine reviewed source hashes and source/Cargo continuity through evidence HEAD `827990c99ca36d087eb8737979ed79f931b7bef2` were checked. Full distributions, receipts and adverse tails remain in [C3-A results](track_c_startup/results.md) and [phase fractions](track_c_startup/phase_fractions.json). At 10,000 files catalogue p95/max changed from 922.7792/956.3962 to 958.9180/1103.8792 ms; no improvement is claimed.

Full process/hotkey/display latency, generic plugin registration, cold disk and allocator/RSS are NOT MEASURED; native is NOT RUN. Separate catalogue and ready-catalog clocks must not be summed. Existing singleton, recovery, tap/hold hotkeys, focus, error propagation and persisted formats receive no runtime changes. Future opportunities require new evidence identifying a specific safely removable operation.

See [machine-readable decision](track_c_startup_gate.json). Next: C3-C event variant/sender inventory and queue characterization; no coalescing or drain policy change is authorized before that review.
