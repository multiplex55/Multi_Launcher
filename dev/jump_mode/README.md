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
