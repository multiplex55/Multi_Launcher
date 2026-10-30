# G09 risks and proof limits

Retirement remains owner-specific. Twelve selected tests passed; no production defect or ownership contradiction was reproduced. Production behavior is unchanged, and independent review/parent acceptance remains pending.

- The native receipt proves one actual pre-read file-handle acquire/hold/release sequence through Duplicate Finder on synthetic local files. It does not prove cancellation of an in-flight ReadFile, native screenshot capture, COM/apartment teardown or worst-case latency; those categories are NOT_RUN and unclaimed.
- A retained Duplicate Finder loop may be alive after normal scan completion. Its Activity `Unavailable` row is intentional. Screenshot shutdown transfers ownership without an Activity after-join callback; that row also stays `Unavailable` even when the separate real reaper probe establishes join.
- Reaper supervisor-spawn failure deliberately detaches. Capacity release on notifier drop is not a join receipt. No callback or successful reaped probe is fabricated for that path.
- Initial preparation's numeric Cargo exit was unavailable in its PowerShell receipt. Compiler completion, a complete nonzero inventory, identical input hashes and the serial run's explicit exit0/12 passes are retained; no exit was invented and no redundant build was added.
- No production polling, manager, FFI, dependency, feature, hotkey or user-data change. Activity visibility, exact-key cancellation, history and sanitized-copy ownership remain intact. Full-app, full-suite, domain and native UI verification are outside this checkpoint.

The fixture directory was removed after joins; final process/fixture scan is empty. Distinct new actionable finding: **NO_ACTION**. Unclaimed native categories do not create a new initiative. Any later claim must bring its own explicit bounded contract and actual owner evidence.
