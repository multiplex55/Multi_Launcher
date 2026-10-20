# Jump Mode GUI fixture harness

Run the current M01 baseline from the repository root with one command:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File dev/jump_mode/run.ps1
```

The runner requires Windows, PowerShell 7, Cargo, and an interactive desktop
session for a real GUI capture. It builds `multi_launcher` with the existing
`qa-framebuffer-capture` feature, creates a new profile below the evidence run,
sets `ML_NOTES_DIR` and `ML_NOTE_TEMPLATES_DIR` to directories inside that
profile, and starts only that
binary with the profile as its working directory. The app therefore resolves
`settings.json`, `actions.json`, `dashboard.json`, and its AppDataRoot within
the fresh fixture profile. The fixture enables only the built-in `notes`
plugin; clipboard modification is explicitly left disabled, and no macros,
network, system, or external-file actions are configured.

All test profiles disable **Enable Next Shortcut** and **Enable Cancel Shortcut**
for Paste Sequences. The verifier rejects profiles that enable either shortcut.

The fixture verifier canonicalizes and checks both isolated data directories
are contained by the fresh profile before setting either environment variable,
loading notes, or constructing the production Notes plugin. It parses each card with the
production pinned-query config type, runs the production note loader and
`NotePlugin::search` for the configured list/search/tag queries, and records
the validated IDs/counts in its output log. It also validates the fixture
launcher chord with the production hotkey-route validator before any GUI
launch. The fixture disables
`reduce_dashboard_work_when_unfocused` so the first unfocused launcher frame
can populate the dashboard cache without waiting for its ordinary refresh
interval.

Each invocation creates `dev/jump_mode/evidence/<run-id>/` with the generated
profile, capture request and receipt, a named baseline PNG, process lifecycle
proof in `process_lifecycle.json`, scenario and event records, build/app stdout
and stderr, and an incrementally updated `summary.md`. The fixture checker is a
dev-only Cargo binary enabled through the existing `dev-smoke-binaries`
feature, so it uses the repository lockfile and dependency patches. The root
framebuffer image comes from the running app's `ROOT/Startup` QA capture. Its
receipt describes the capture boundary as CPU UI completion before GPU
presentation. The baseline scenario remains `BLOCKED` until app-state
readiness confirms the fixture results are visible; capture metadata alone
cannot establish that content, and the runner does not use OCR or pixel-golden
assertions. The app is asked to
close through its main window; if it does not exit before the bounded timeout,
the runner uses the retained handle for that exact process and its descendants.
It never terminates processes by executable name.

The fixtures contain three safe `notes` query cards on the empty-query
dashboard and 32 local Markdown records with stable slugs for the later note
grid and dense-hint scenarios. Clicking a dashboard card only fills the
launcher query. Opening a note stays in the application; external note opening
is set to `Neither`. The launcher chord is isolated from Screen Draw's default
emergency chord. Screenshot auto-save and editor launch are disabled, with any
configured screenshot directory below the fixture profile. `actions.json` is
an empty typed catalog.

For fixture-only validation without launching the app:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File dev/jump_mode/run.ps1 -FixturesOnly
```

If the runner detects a non-interactive Windows session, it still records the
fixture and environment result, marks the GUI scenario `NOT_RUN_ENVIRONMENT`,
and does not create a synthetic screenshot. A failed build, capture, or cleanup
is recorded in the evidence folder and returns a nonzero exit code.

Run the bounded M05 query-only native probe with a separate explicitly
Jump-enabled fixture:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File dev/jump_mode/run.ps1 -Scenario QueryProbe
```

This opt-in request records only retained ROOT UI state, waits for the actual
query hint, and binds its screenshot receipt to that same retained pass. The
runner verifies the owned app is foreground and no modifier is already held
before sending Shift+Space, the observed query hint, and `X`. It checks
that the hint handoff focused the real query field, the query stayed empty
until that handoff, and the action count stayed zero. The default baseline
request does not enable the observer or send native input.

Phase 1 fixtures use the application's existing global grid configuration
(`respect_plugin_capability = false`) with no explicit plugin opt-outs. The
Notes plugin currently declares search capability only. This fixture choice
exercises the existing grid renderer; production capability and opt-out
selection remains unchanged. Fixture error notifications stay enabled so
unexpected failures remain visible. Paste Sequence Next/Cancel shortcuts
remain disabled independently.
Run the complete Phase 1 native catalog (dashboard, note grid, query editing,
cancellation, ownership, dense hints, and the separate disabled-feature case):

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File dev/jump_mode/run.ps1 -Scenario Phase1
```

`Phase1Enabled` and `Phase1Disabled` run the respective child session directly.
The aggregate runs them sequentially, checks their status and owned cleanup,
and copies only receipt-bound, hash-matching PNGs into its screenshot index.
Enabled captures use actual F11 input and the retained ROOT observation from
that exact frame/pass. The runner derives hint letters from the observed
registered targets; a completed hint must preserve the query and activation
count before a later Enter uses the existing handler. Every screenshot is
checked against its own captured observation rather than a later state file.

The Phase 1 fixture has four dashboard slots and 32 local notes with distinct
slugs, including duplicate titles. Its larger window and explicit 4-by-8 grid
allow the dense case to assert more than 26 actually visible result targets.
A separate negative profile changes the Jump Mode gate and must fail typed
validation before launch; both Paste Sequence shortcuts remain disabled.
For a focused enabled smoke, use `-Scenario Phase1Enabled -Screen Dashboard`
(catalog 01–05) or `-Scenario Phase1Enabled -Screen Grid` (grid, query,
cancellation, dense-map and ownership cases). Both filters reuse the same
scenario implementation and still perform fixture checks and owned cleanup.
`-CaptureTimeoutSeconds` bounds each observation/capture step;
`-OverallTimeoutSeconds` supplies the shared run deadline. See each run's
manifest/events for the exact environment, window/DPI metadata and deadline
outcome. Dashboard and Grid filters passed native M08 qualification; see JUMP_MODE_RESULTS.md for the accepted run IDs.
